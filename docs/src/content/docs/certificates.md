---
title: Automatic certificates
description: How Deku serves HTTPS — certificates Angie requests and renews for app domains, one wildcard for generated hostnames, or certificates you install.
---

Deku serves a domain over HTTPS in one of three ways:

| Way | Covers | Who renews it |
| --- | --- | --- |
| [Automatic, per domain](#app-domains) | Each domain you add to an app | Angie, on its own |
| [Wildcard](#generated-hostnames-wildcard) | Every generated hostname under `global_domain` | Angie, on its own |
| [Installed](#installed-certificates) | An app's domains, with files you provide | You |

## App domains

Add a domain to an app, point its DNS at the server, and Angie requests a Let's Encrypt certificate
for it over the HTTP-01 challenge, then renews it 30 days before it expires. New installs have this
on. On a host installed earlier, turn it on with:

```bash
deku acme custom-domains on
```

Each domain gets its own certificate, so one domain whose DNS is wrong never holds up the others.
Port 80 has to be open to the internet; the challenge is answered there.

### What happens after you add a domain

1. **Checking.** Deku fetches `http://<domain>/.well-known/deku-check` and expects this server to
   answer. Until it does, the domain is served over HTTP and nothing is asked of Let's Encrypt.
   The check repeats every few minutes at first, then hourly.
2. **Issuing.** Deku adds the domain's ACME client to the app's Angie config and Angie requests the
   certificate. This usually takes seconds.
3. **Active.** Once the certificate is issued, the domain is served over HTTPS and plain HTTP
   redirects to it. Renewals replace the certificate without any config change.

If two requests in a row fail, Deku stops Angie from asking again (Let's Encrypt allows only a few
failed validations per hour) and marks the domain **failed** with the reason. It checks again after
an hour, then four hours, then daily. After fixing the cause, retry at once:

```bash
deku domains list shop            # each domain's certificate state, and why it is waiting
deku domains retry shop shop.example.com
```

Behind a CDN or proxy, the check and the challenge both pass as long as HTTP requests for
`/.well-known/` reach this server.

### Choosing per domain

```bash
deku domains tls shop shop.example.com auto     # Angie requests and renews (the default)
deku domains tls shop shop.example.com manual   # serve the certificate you installed
deku domains tls shop shop.example.com off      # HTTP only
```

A wildcard domain such as `*.example.com` cannot be validated over HTTP, and neither can an IP
address; use `manual` for those. A domain one label below `global_domain` uses the wildcard
certificate when one is issued, instead of requesting its own.

### Watching it

- `deku domains list <app>` shows each domain's state, expiry, and the last reason it is waiting.
- `deku doctor` reports domains that are paused or not reachable yet.
- The alert watcher raises `certificate_failed` for a paused domain, and `certificate_expiring` only
  when an automatic certificate is inside 14 days of expiry, which means renewal has been failing for
  two weeks.
- Hooks receive `certificate.issued` and `certificate.failed`.

## Generated hostnames (wildcard)

Environment and per-deployment hostnames are served over **HTTP** by default, because no certificate
covers a name like `demo-staging.apps.test`. Deku can have Angie obtain a single wildcard
certificate for `<global_domain>` and `*.<global_domain>`, which covers every generated hostname of
every app.

### What it needs

- A `global_domain` in the daemon config. Without one there is no name to certify.
- A DNS provider API token. Only **Cloudflare** is built in, and the token needs `Zone:Read` and
  `DNS:Edit` for the zone.
- Angie built with its ACME module. Angie's own packages include it; a build from source needs
  `--with-http_acme_module`. `deku doctor` reports which one you have.

### Enabling it

In the dashboard, open **Settings → Certificates**, paste the token under **Wildcard certificate**,
and choose **Verify and save**. Deku checks the token against Cloudflare before storing it, then
turns the wildcard on. Or write the daemon config directly:

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

The token itself is written with mode `0600` and only its path is recorded in the config.

To test a new setup, set **Certificate authority** to **Let's Encrypt staging** in the same section.
That points `directory` at Let's Encrypt's staging environment, which issues certificates browsers do
not trust, with much higher rate limits. It applies to app-domain certificates and the wildcard
alike; switch back once issuance works.

The ACME account's contact address is the host's certificate email, the same setting an app's own
certificate uses:

```bash
deku letsencrypt config --email ops@example.com
```

### What happens

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

### Serving it

A vhost cannot name a certificate that does not exist yet, so the generated hostnames stay HTTP-only
until the certificate has been issued. The daemon's certificate watcher looks every minute, and once
`certificate.pem` holds a certificate it rewrites every app's config to serve it. Renewal needs
nothing: the vhosts reference the certificate by variable, so what they serve is replaced without
them being written again.

### Checking it

```bash
deku acme status     # settings, token source, and whether a certificate was issued
deku doctor          # angie_acme_module, acme, and acme_certificate rows
```

`deku acme status` exits non-zero when the certificate exists but is expired or unreadable, since
Angie will re-request it and nothing is being served meanwhile.

### Cautions

- **A reload re-requests an invalid certificate immediately**, ignoring Angie's retry delay. A host
  that reloads repeatedly while issuance is failing will keep asking the certificate authority. Deku
  writes the request file only when it changes, so an unchanged configuration causes no reload, but
  any deploy does.
- **The challenge record may not be visible yet.** Deku answers the hook as soon as the provider
  accepts the write, and Angie asks the certificate authority to validate immediately after. If the
  record has not reached the provider's nameservers, that attempt fails and is retried later. A first
  attempt that fails and a later one that succeeds is the expected shape of that race.
- **The wildcard covers generated hostnames only.** An app's own domains each get their own
  certificate (see [App domains](#app-domains)), or serve one you install (see
  [Installed certificates](#installed-certificates)).

## Installed certificates

To serve a certificate you obtain yourself, put the certificate and key at
`/etc/angie/ssl/deku_<app>.crt` and `/etc/angie/ssl/deku_<app>.key`, then:

```bash
deku domains tls shop shop.example.com manual
```

Deku serves those files but does not renew them. It warns before they expire: see [Certificate
expiry](/diagnostics/#certificate-expiry). `deku letsencrypt enable <app>` still works and puts every
domain of the app in `manual`, but it is deprecated.
