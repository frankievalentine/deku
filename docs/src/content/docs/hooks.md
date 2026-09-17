---
title: Lifecycle hooks
description: Send deploy and app lifecycle events to an HTTP endpoint, and optionally gate a deploy on the response.
---

Hooks let an external service react to what Deku is doing: notify chat, record an audit trail, run a
compliance check, or block a rollout that should not ship. Deku sends a JSON `POST` to each
configured URL when a lifecycle event fires.

This is the supported integration point. The in-process plugin runtime is off by default and
experimental — see [In-process plugins](#in-process-plugins) below.

## Configure a hook

Add one or more `[[hooks]]` entries to the daemon's `config.toml`:

```toml
[[hooks]]
url = "https://ci.example.com/deku/hooks"
secret = "a-shared-secret"                  # optional; signs every request
events = ["pre_deploy", "deploy.failed"]    # optional; omit to receive all events
blocking = true                             # optional; default false
```

| Field | Meaning |
| --- | --- |
| `url` | Endpoint that receives the event. |
| `secret` | When set, each request carries an HMAC-SHA256 signature of the body. |
| `events` | Events to deliver. Omit to deliver all of them. |
| `blocking` | When true, a failing hook fails a `pre_build` or `pre_deploy` event. |

Config is re-read on every deploy, so changing hooks does not require restarting `dekud`.

## Events

| Event | When | Can block |
| --- | --- | --- |
| `pre_build` | Before the image build starts | Yes |
| `post_build` | After the image is built | No |
| `pre_deploy` | Before containers are replaced | Yes |
| `post_deploy` | After the new version is serving | No |
| `deploy.succeeded` | The deploy finished | No |
| `deploy.failed` | The deploy failed | No |
| `app.created` | An app was created | No |
| `app.destroyed` | An app was deleted | No |
| `alert.fired` | An alert opened; `detail.alert` describes it | No |
| `alert.resolved` | A previously fired alert cleared | No |

`blocking` only applies to `pre_build` and `pre_deploy`. Those run before anything irreversible
happens, so a failure there fails the deploy and the previous version keeps serving. For every other
event, a failed delivery is logged and ignored — the deploy outcome never depends on a notification.

## Request

```http
POST /deku/hooks HTTP/1.1
Content-Type: application/json
X-Deku-Event: pre_deploy
X-Deku-Delivery: 8b0d0b1e-7c0a-4a4f-9f2a-9a1f9c2f0a11
X-Deku-Signature: sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843

{
  "event": "pre_deploy",
  "timestamp": "2026-09-17T20:33:42.512Z",
  "deku_version": "0.1.12",
  "app": { "id": "…", "name": "my-app" },
  "deployment": { "id": "…", "image_tag": "deku/my-app:492165515d72", "status": "deploying" },
  "detail": { "image_tag": "deku/my-app:492165515d72" }
}
```

`detail` carries event-specific fields: `builder` and `deploy_id` on build events, `image_tag` on
deploy events, and `error` on `deploy.failed`. `deployment` is `null` for app and build events.

Return any `2xx` to accept. A non-`2xx` response, a connection error, or a timeout (10 seconds)
counts as a failure.

## Verify the signature

With `secret` set, compute HMAC-SHA256 over the **raw request body** and compare it to the
`sha256=` value in `X-Deku-Signature`. Verify before parsing, and use a constant-time comparison.

```python
import hmac, hashlib

def verified(secret: bytes, body: bytes, header: str) -> bool:
    expected = "sha256=" + hmac.new(secret, body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, header)
```

## Blocking example

A migration or schema check that must pass before the new version starts:

```python
@app.post("/deku/hooks")
def hook(request):
    payload = json.loads(request.body)
    if payload["event"] == "pre_deploy" and not schema_is_compatible():
        # A non-2xx response fails the deploy while the old version keeps serving.
        return Response(status_code=409)
    return Response(status_code=204)
```

## In-process plugins

Deku can also load `.so`/`.dylib` plugins into the daemon. That runtime is **compiled out by
default**, because a plugin built against a different Rust toolchain can abort the process. Build
with `--features dynamic-plugins` to enable it:

```bash
cargo build -p dekud --features dynamic-plugins
```

`deku plugins list` reports whether the running daemon has the runtime, and
`deku plugins install <path>` returns a clear error when it does not. Prefer hooks: they are
language-agnostic, cannot crash the daemon, and survive a `dekud` upgrade.
