---
title: Automatic certificates
description: Request and renew one wildcard certificate for the generated hostnames, with Deku answering the DNS challenge.
---

Environment and per-deployment hostnames are served over **HTTP** by default, because no certificate
covers a name like `demo-staging.apps.test`. Deku can have Angie obtain a single wildcard
certificate for `<global_domain>` and `*.<global_domain>`, which covers every generated hostname of
every app.

## What it needs

- A `global_domain` in the daemon config. Without one there is no name to certify.
- A DNS provider API token. Only **Cloudflare** is built in, and the token needs `Zone:Read` and
  `DNS:Edit` for the zone.
- Angie built with its ACME module. Angie's own packages include it; a build from source needs
  `--with-http_acme_module`. `deku doctor` reports which one you have.

## Enabling it

In the dashboard, **Routing → Automatic certificates**: turn it on, switch the wildcard on, paste
the token, and save. Or write the daemon config directly:

```toml
[acme]
enabled = true
wildcard = true
directory = "https://acme-v02.api.letsencrypt.org/directory"
provider = "cloudflare"
# Read on every use, so the secret stays out of this file.
api_token_file = "/var/lib/deku/acme-api-token"
client_path = "/var/lib/angie/acme"
```

The token itself is written with mode `0600` and only its path is recorded in the config. `Use
staging` in the dashboard points `directory` at Let's Encrypt's staging environment, which issues
untrusted certificates with much higher rate limits — the right thing to test with.

The ACME account's contact address is the host's certificate email, the same setting an app's own
certificate uses:

```bash
deku letsencrypt config --email ops@example.com
```

## What happens

Deku writes `deku+acme.conf` next to the app configs. It names the client, the names the certificate
has to cover, and a hook location Angie calls during validation:

```nginx
acme_client deku_wildcard https://acme-v02.api.letsencrypt.org/directory
    challenge=dns
    email=ops@example.com;

server {
    listen unix:/var/lib/angie/acme/collector.sock;
    server_name apps.test *.apps.test;
    acme deku_wildcard;

    location @acme_dns_hook {
        acme_hook deku_wildcard uri=/internal/acme/dns-hook;
        proxy_pass http://unix:/run/deku/deku.sock;
        # ...
    }
}
```

A wildcard can only be validated over DNS, so Angie asks the hook instead of the certificate
authority directly. For each name, Deku writes the `_acme-challenge` TXT record through the provider's
API and answers `200`, which is what Angie waits for. The certificate and its key are kept under
`client_path`:

```
/var/lib/angie/acme/deku_wildcard/
  account.key  certificate.pem  private.key
```

## Serving it

A vhost cannot name a certificate that does not exist yet, so the generated hostnames stay HTTP-only
until the certificate has been issued. Once `certificate.pem` is there, the next thing that rewrites
the app's config puts it in front of them — the next deploy, or a domain or upstream change. Renewal
needs nothing: the vhosts reference the certificate by variable, so what they serve is replaced
without them being written again.

## Checking it

```bash
deku acme status     # settings, token source, and whether a certificate was issued
deku doctor          # angie_acme_module, acme, and acme_certificate rows
```

`deku acme status` exits non-zero when the certificate exists but is expired or unreadable, since
Angie will re-request it and nothing is being served meanwhile.

## Cautions

- **A reload re-requests an invalid certificate immediately**, ignoring Angie's retry delay. A host
  that reloads repeatedly while issuance is failing will keep asking the certificate authority. Deku
  writes the request file only when it changes, so an unchanged configuration causes no reload, but
  any deploy does.
- **The challenge record may not be visible yet.** Deku answers the hook as soon as the provider
  accepts the write, and Angie asks the certificate authority to validate immediately after. If the
  record has not reached the provider's nameservers, that attempt fails and is retried later. A first
  attempt that fails and a later one that succeeds is the expected shape of that race.
- **The wildcard covers generated hostnames only.** An app's own domains use the per-app certificate
  from `deku letsencrypt enable <app>`, which is issued for those domains.
