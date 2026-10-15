//! Certificates issued end to end: Deku's rendered configuration, real Angie,
//! and Pebble, Let's Encrypt's test certificate authority.
//!
//! Runs only with `DEKU_ACME_IT=1` and Docker available. Angie is built from its
//! published source by `tests/acme/Dockerfile.angie` unless `DEKU_ANGIE_IMAGE`
//! names an image that already has the ACME module.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::acme::domains::client_name;
use crate::acme::status::ClientStatus;
use crate::config::AcmeConfig;
use crate::proxy::writer::{self, AcmeClient, AppVhostPolicy, CertSource, VhostConfig};

const NETWORK: &str = "deku-acme-it";
const SUBNET: &str = "172.31.240.0/24";
const DNS_IP: &str = "172.31.240.3";
const PEBBLE_IP: &str = "172.31.240.4";
const ANGIE_IP: &str = "172.31.240.10";
const UNREACHABLE_IP: &str = "172.31.240.99";
const CLIENT_PATH: &str = "/var/lib/angie/acme";
const CONF_DIR: &str = "/etc/angie/http.d";
const DIRECTORY: &str = "https://pebble:14000/dir";

fn enabled() -> bool {
    std::env::var("DEKU_ACME_IT").is_ok_and(|value| value == "1")
}

fn docker(args: &[&str]) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("docker should run");
    assert!(
        output.status.success(),
        "docker {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn docker_quiet(args: &[&str]) {
    let _ = Command::new("docker").args(args).output();
}

fn cleanup() {
    docker_quiet(&[
        "rm",
        "-f",
        "deku-acme-it-angie",
        "deku-acme-it-pebble",
        "deku-acme-it-dns",
    ]);
    docker_quiet(&["network", "rm", NETWORK]);
}

fn angie_image() -> String {
    if let Ok(image) = std::env::var("DEKU_ANGIE_IMAGE") {
        return image;
    }
    let image = "deku-angie-acme:it";
    let context = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/acme");
    docker(&[
        "build",
        "-q",
        "-f",
        &context.join("Dockerfile.angie").display().to_string(),
        "-t",
        image,
        &context.display().to_string(),
    ]);
    image.to_string()
}

fn wait_for<T>(what: &str, timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn statuses() -> Option<std::collections::HashMap<String, ClientStatus>> {
    let socket = format!("{CLIENT_PATH}/status.sock");
    let output = Command::new("docker")
        .args([
            "exec",
            "deku-acme-it-angie",
            "curl",
            "-si",
            "--unix-socket",
            &socket,
            "http://localhost/acme/",
        ])
        .output()
        .ok()?;
    let raw = String::from_utf8_lossy(&output.stdout)
        .replace("\r\n", "\n")
        .replace('\n', "\r\n");
    crate::acme::status::parse_response(raw.as_bytes()).ok()
}

fn curl_in_network(args: &[&str]) -> String {
    let mut full = vec![
        "run",
        "--rm",
        "--network",
        NETWORK,
        "curlimages/curl:latest",
        "-s",
    ];
    full.extend_from_slice(args);
    docker(&full)
}

fn policy() -> AppVhostPolicy<'static> {
    AppVhostPolicy {
        auth: None,
        maintenance: false,
        maintenance_message: None,
        redirects: &[],
    }
}

fn upstream() -> Vec<deku_core::types::Upstream> {
    // Nothing listens here; the checks only need Angie's own answers.
    vec![deku_core::types::Upstream {
        host: "127.0.0.1".to_string(),
        port: 9,
    }]
}

fn client(name: &str, enabled: bool) -> AcmeClient {
    AcmeClient {
        name: name.to_string(),
        directory: DIRECTORY.to_string(),
        account_key: PathBuf::from(format!("{CLIENT_PATH}/deku-account.key")),
        enabled,
    }
}

fn vhost(domain: &str, client: &str, cert: Option<CertSource>) -> VhostConfig {
    VhostConfig {
        key: "shop".to_string(),
        domains: vec![domain.to_string()],
        upstreams: upstream(),
        cert,
        acme_client: Some(client.to_string()),
    }
}

#[tokio::test]
async fn per_domain_certificates_are_issued_by_angie() {
    if !enabled() {
        return;
    }
    cleanup();
    let image = angie_image();

    let conf = tempfile::tempdir().expect("conf dir");
    let acme = AcmeConfig {
        enabled: true,
        custom_domains: true,
        directory: DIRECTORY.to_string(),
        client_path: PathBuf::from(CLIENT_PATH),
        ..AcmeConfig::default()
    };
    std::fs::write(
        conf.path().join(crate::acme::file::CONFIG_FILE_NAME),
        crate::acme::file::render(&crate::acme::file::FileConfig {
            wildcard_domain: None,
            acme: &acme,
            email: None,
            deku_socket: Path::new("/run/deku.sock"),
        })
        .expect("render the ACME file"),
    )
    .expect("write the ACME file");

    let shop = client_name("shop.test");
    let bad = client_name("bad.test");
    let paused = client_name("paused.test");
    let clients = [
        client(&shop, true),
        client(&bad, true),
        client(&paused, false),
    ];
    let issuing = [
        vhost("shop.test", &shop, None),
        vhost("bad.test", &bad, None),
        vhost("paused.test", &paused, None),
    ];
    writer::write_app_config(conf.path(), "shop", &clients, &issuing, &policy())
        .expect("write the app config");

    let main_conf = conf.path().join("angie.conf");
    std::fs::write(
        &main_conf,
        format!("events {{}}\nhttp {{\n    include {CONF_DIR}/*.conf;\n}}\n"),
    )
    .expect("main config");
    // Angie loads its own file from the include directory too; keep it apart.
    let main_dir = tempfile::tempdir().expect("main dir");
    std::fs::rename(&main_conf, main_dir.path().join("angie.conf")).expect("move main config");

    // The Pebble image has no shell, so its CA is copied out of a container
    // that is created but never started.
    let ca_file = main_dir.path().join("pebble.crt");
    docker_quiet(&["rm", "-f", "deku-acme-it-ca"]);
    docker(&[
        "create",
        "--name",
        "deku-acme-it-ca",
        "ghcr.io/letsencrypt/pebble:latest",
    ]);
    docker(&[
        "cp",
        "deku-acme-it-ca:/test/certs/pebble.minica.pem",
        &ca_file.display().to_string(),
    ]);
    docker_quiet(&["rm", "-f", "deku-acme-it-ca"]);
    let pebble_config = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/acme/pebble.json");

    docker(&["network", "create", "--subnet", SUBNET, NETWORK]);
    docker(&[
        "run",
        "-d",
        "--name",
        "deku-acme-it-dns",
        "--network",
        NETWORK,
        "--ip",
        DNS_IP,
        "ghcr.io/letsencrypt/pebble-challtestsrv:latest",
        "-defaultIPv4",
        ANGIE_IP,
        "-defaultIPv6",
        "",
        "-dnsserver",
        ":8053",
        "-http01",
        "",
        "-https01",
        "",
        "-tlsalpn01",
        "",
    ]);
    docker(&[
        "run",
        "-d",
        "--name",
        "deku-acme-it-pebble",
        "--network",
        NETWORK,
        "--ip",
        PEBBLE_IP,
        "--network-alias",
        "pebble",
        "-e",
        "PEBBLE_VA_NOSLEEP=1",
        "-v",
        &format!("{}:/test/config/it.json:ro", pebble_config.display()),
        "ghcr.io/letsencrypt/pebble:latest",
        "-config",
        "/test/config/it.json",
        "-dnsserver",
        &format!("{DNS_IP}:8053"),
    ]);
    // bad.test resolves somewhere nothing answers.
    wait_for("the test DNS server", Duration::from_secs(20), || {
        let output = Command::new("docker")
            .args([
                "run",
                "--rm",
                "--network",
                NETWORK,
                "curlimages/curl:latest",
                "-sf",
                "-X",
                "POST",
                "-d",
                &format!("{{\"host\":\"bad.test\",\"addresses\":[\"{UNREACHABLE_IP}\"]}}"),
                &format!("http://{DNS_IP}:8055/add-a"),
            ])
            .output()
            .ok()?;
        output.status.success().then_some(())
    });

    docker(&[
        "run",
        "-d",
        "--name",
        "deku-acme-it-angie",
        "--network",
        NETWORK,
        "--ip",
        ANGIE_IP,
        "-v",
        &format!(
            "{}:/etc/angie/angie.conf:ro",
            main_dir.path().join("angie.conf").display()
        ),
        "-v",
        &format!("{}:{CONF_DIR}", conf.path().display()),
        "-v",
        &format!(
            "{}:/usr/local/share/ca-certificates/pebble.crt:ro",
            ca_file.display()
        ),
        &image,
        "sh",
        "-c",
        "update-ca-certificates >/dev/null 2>&1; angie -t && exec angie -g 'daemon off;'",
    ]);

    // The reachability answer is served before any certificate exists.
    let answer = wait_for(
        "Angie to answer on port 80",
        Duration::from_secs(30),
        || {
            let output = Command::new("docker")
                .args([
                    "run",
                    "--rm",
                    "--network",
                    NETWORK,
                    "curlimages/curl:latest",
                    "-sf",
                    "--resolve",
                    &format!("shop.test:80:{ANGIE_IP}"),
                    &format!("http://shop.test{}", crate::acme::reachability::CHECK_PATH),
                ])
                .output()
                .ok()?;
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        },
    );
    assert_eq!(answer.trim(), crate::acme::reachability::host_id());

    // The reachable domain is issued. The first attempt after start can lose a
    // race with Angie's workers, which the short retry recovers from.
    let issued = wait_for("shop.test to be issued", Duration::from_secs(150), || {
        let statuses = statuses()?;
        let status = statuses.get(&shop)?;
        status.has_valid_certificate().then(|| status.clone())
    });
    assert_eq!(issued.state, "ready");

    let bad_status = wait_for("bad.test to fail", Duration::from_secs(90), || {
        let statuses = statuses()?;
        let status = statuses.get(&bad)?;
        status.failed().then(|| status.clone())
    });
    assert_eq!(bad_status.certificate, "missing", "{bad_status:?}");
    let statuses = statuses().expect("statuses");
    assert_eq!(
        statuses.get(&paused).expect("paused client").state,
        "disabled"
    );

    // With the certificate issued, the same domain is served over HTTPS.
    let active = [
        vhost(
            "shop.test",
            &shop,
            Some(CertSource::Acme {
                client: shop.clone(),
            }),
        ),
        vhost("bad.test", &bad, None),
        vhost("paused.test", &paused, None),
    ];
    writer::write_app_config(conf.path(), "shop", &clients, &active, &policy())
        .expect("rewrite with the certificate");
    docker(&["exec", "deku-acme-it-angie", "angie", "-s", "reload"]);
    let https = wait_for("shop.test over HTTPS", Duration::from_secs(20), || {
        let output = Command::new("docker")
            .args([
                "run",
                "--rm",
                "--network",
                NETWORK,
                "curlimages/curl:latest",
                "-sk",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code} %{ssl_verify_result}",
                "--resolve",
                &format!("shop.test:443:{ANGIE_IP}"),
                "https://shop.test/",
            ])
            .output()
            .ok()?;
        let line = String::from_utf8_lossy(&output.stdout).into_owned();
        line.split_whitespace()
            .next()
            .is_some_and(|code| code != "000")
            .then_some(line)
    });
    assert!(
        !https.starts_with("000"),
        "the TLS handshake succeeds: {https}"
    );
    let redirect = curl_in_network(&[
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "--resolve",
        &format!("shop.test:80:{ANGIE_IP}"),
        "http://shop.test/",
    ]);
    assert_eq!(
        redirect.trim(),
        "301",
        "HTTP redirects once the domain is active"
    );

    cleanup();
}
