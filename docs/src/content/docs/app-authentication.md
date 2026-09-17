---
title: App authentication
description: Put HTTP basic auth, an external identity provider, or Cloudflare Access in front of an app.
---

Deku protects an app at the proxy layer, so your application code does not need to change. Auth is
configured per app and applies to every request before it reaches the container.

Two modes are supported:

- **Basic auth** — one shared username and password, stored only as a hash, checked by Angie.
- **Forward auth** — every request is authorized by an external endpoint (Cloudflare Access, Authelia,
  Authentik, oauth2-proxy, and similar).

Deku deliberately does not implement user accounts, sessions, MFA, or roles. It gives you a one-line
gate, and delegates real identity to a system that is built for it.

## Basic auth

```bash
deku auth enable my-app --user alice
# prompts for a password (or pass --password for automation)
deku auth status my-app
deku auth disable my-app
```

When enabled, Deku writes an htpasswd file next to the app's Angie config (mode `0600`) and adds
`auth_basic` + `auth_basic_user_file` to the vhost. The password is stored as a SHA-512 `crypt()`
hash (`$6$…`), which Angie/NGINX accept; the plaintext is never written anywhere.

Basic auth protects a route with **one shared credential**. It does not replace application
accounts, role-based access, multi-factor authentication, or single sign-on. Use it for staging
sites, dashboards, and internal tools — not as the only gate on sensitive data.

## Forward auth

```bash
deku auth forward my-app --url https://auth.example/verify
```

Deku adds an Angie `auth_request` subrequest: for each request, Angie calls your endpoint with
`X-Original-URI` and allows the request only when it returns `2xx`. Point it at:

- **Cloudflare Access JWT validation** (see below), or
- a self-hosted IdP gateway such as **Authelia**, **Authentik**, or **oauth2-proxy**.

The app itself stays unaware; identity is enforced before the proxy forwards anything.

## Recommended: Cloudflare Access

For teams that want SSO/MFA without running an identity gateway, front the app with
[Cloudflare Access](https://developers.cloudflare.com/cloudflare-one/policies/access/):

1. Put the app's domain behind Cloudflare (proxied DNS or a Cloudflare Tunnel to the Deku host).
2. Create a Zero Trust **Access application** for the hostname and an **Access policy** (email OTP,
   Google, GitHub, Okta, Entra, or any OIDC/SAML IdP), with MFA enforced by the IdP.
3. Optionally add a **service token** for CI or health checks that must bypass the login.

Cloudflare Access evaluates identity before traffic reaches Deku, so no per-app configuration is
required on the host. The Cloudflare Zero Trust free plan covers up to 50 users.

Trade-offs:

- It requires a Cloudflare account and a domain using Cloudflare DNS.
- Traffic transits Cloudflare's network, which is unsuitable for fully offline or air-gapped hosts.
- It is a third-party dependency on the request path.

If you prefer to validate Access tokens locally, or to use a self-hosted IdP, use `deku auth
forward` with your endpoint instead. For hosts with no external dependency, `deku auth enable`
(basic auth) is the fallback.

## Choosing a mode

| Situation | Use |
| --- | --- |
| Quick gate on staging or an internal tool | Basic auth |
| Team access with SSO/MFA, already on Cloudflare | Cloudflare Access |
| Self-hosted identity gateway (Authelia, Authentik, oauth2-proxy) | Forward auth |
| Air-gapped host, no external identity provider | Basic auth |

Auth is reapplied automatically on every deploy and routing change, so it survives redeploys.
