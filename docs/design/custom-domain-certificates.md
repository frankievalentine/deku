# Automatic certificates for custom domains

Status: approved and implemented 2026-10-15; not yet released. Remaining: an end-to-end run against
Let's Encrypt staging on a real host.

## Problem

Deku issues and renews one certificate: the wildcard for `<global_domain>` and `*.<global_domain>`,
through Angie's ACME module and a Cloudflare DNS token. An app's own domains are served over HTTPS
only when an operator installs `/etc/angie/ssl/deku_<app>.{crt,key}` and runs
`deku letsencrypt enable`. Deku never issues or renews those certificates.

## Goal

Add a domain, point its DNS at the server, and it is served over HTTPS with a Let's Encrypt
certificate that renews itself. Operator-provided certificates keep working.

## Decisions

| Question | Decision | Why |
| --- | --- | --- |
| Who speaks ACME | Angie's ACME module, HTTP-01 | It already renews the wildcard; no ACME client in Deku |
| Certificate scope | One ACME client per domain | A Let's Encrypt order fails as a whole, so one bad domain must not block the others |
| Account | One account key shared by every client (`account_key=`) | Avoids the new-account rate limit when many domains are added |
| Before requesting | An HTTP self-check through the domain | Protects the failed-validation limit; works behind a CDN, where an A-record check would not |
| Default | On for new installs; opt-in (`deku acme custom-domains on`) for existing ones | Turning it on changes port 80 behavior for existing domains |
| Account email | Not set on per-domain clients | Let's Encrypt stopped sending expiry email in 2025, and the clients share one account |
| HTTP to HTTPS | Redirect automatically once a domain's certificate is active | |
| HSTS | Opt-in per app | Hard to undo once a browser has seen it |
| CLI | `deku domains tls`, `deku domains retry`; `deku letsencrypt` deprecated | The old name never used Let's Encrypt |

## Domain certificate states

| State | Meaning | Served as |
| --- | --- | --- |
| `checking` | Reachability self-check pending | HTTP |
| `issuing` | Client written; Angie is requesting | HTTP, challenges answered |
| `active` | Valid certificate | HTTPS; HTTP redirects |
| `failed` | Angie reported a failure; `cert_details` says why | HTTP |
| `manual` | Operator-provided certificate files | HTTPS |

`tls_mode` per domain is `auto`, `manual`, or `off`. Existing domains of apps with `tls_enabled`
migrate to `manual`; the rest to `auto`, which does nothing until `custom_domains` is on.

## Mechanism

- **Rendering.** Each auto domain gets `acme_client deku_d_<hash> ... challenge=http
  account_key=... retry_after_error=15m` in its app's config file, and server blocks of its own,
  because Angie certifies every name of every block that references a client. The location rules
  are repeated in each block rather than moved to an include, which keeps every app in one file.
  The upstream is rendered once per environment. The port 80 block answers challenges and the
  reachability check, and redirects only `active` domains.
- **Reload discipline.** Angie re-requests every missing certificate on each reload, ignoring its
  retry delay, so client blocks are written only when the set changes, and a failing client is set
  to `enabled=off` after two failures.
- **Reachability.** Angie answers `/.well-known/deku-check` on each domain's port-80 server with
  a random identifier stored in `<data_dir>/host-id`; Deku fetches it through the domain. This
  replaced the planned nonce endpoint: no internal route or per-check state is needed.
- **Watcher.** A daemon task reads `/status/http/acme_clients/<client>` from Angie's API over a
  Unix socket every 60 seconds and moves domains between states: an issued certificate rewrites
  the app config and emits `certificate.issued`; a failure records Angie's `details` and schedules
  a recheck at 1 h, 4 h, and 24 h. The same watcher applies the wildcard as soon as it is issued.
- **Global domain.** A domain exactly one level below the global domain uses the wildcard when it
  is active, instead of requesting another certificate.
- **Out of scope.** Wildcard custom domains (need DNS-01) and IP addresses stay manual.

## Phases

0. Verify in a container: challenge handling against basic auth and maintenance mode, the status
   API in the Angie version the installer installs, and a shared `account_key`.
1. Schema, config, and state model; migrate existing domains. No behavior change.
2. Rendering, status socket, and watcher, behind `custom_domains`.
3. Reachability check, backoff, events, alerts, and doctor checks.
4. API, CLI, dashboard, and docs.
5. Pebble integration tests in CI; an end-to-end run against Let's Encrypt staging.

## Phase 0 findings

Verified 2026-10-15 against Angie 1.12.3, built from source with `--with-http_acme_module`, and
Pebble with its challenge test DNS server.

- **Challenges bypass access checks.** Domains behind `auth_basic`, a `location` returning 503,
  and a server returning `301` to HTTPS all validated. No exempt location is needed.
- **`account_key` is shared and created on demand.** Six clients pointed at one missing file
  registered a single ACME account.
- **`enabled=off` is safe.** The config stays valid and Angie logs "renewal disabled".
- **A reload re-requests at once.** A client scheduled to retry in two hours retried on the next
  reload. The two-failures-then-`enabled=off` rule is needed.
- **The first attempt after a reload can lose a race.** One domain failed because an exiting
  worker answered the challenge with the app's response; the next attempt passed. Set
  `retry_after_error=15m` and treat only a second failure as real.
- **The status API is reliable but terse.** `/status/http/acme_clients/<client>` reports `state`
  (`ready`, `requesting`, `failed`, `disabled`) and `certificate` (`valid`, `missing`, ...), but
  on failure `details` only says to see the logs. The reason has to come from the
  `acme_client: "<name>"` lines in Angie's error log and from Deku's own reachability check.
- **A failed client leaves an empty `certificate.pem`.** Serving it fails every TLS handshake
  ("cannot load certificate"). File existence is therefore not proof of issuance, which is also a
  bug in the current wildcard code (`derived_certificate`): a failed wildcard request would move
  generated hostnames to HTTPS and redirect HTTP to a handshake that cannot succeed. Fixed in
  phase 1 by requiring a non-empty file that parses as a certificate.
- **The packaged Angie includes the API module**, and the installer installs the latest 1.12.x,
  which has the status endpoint.
