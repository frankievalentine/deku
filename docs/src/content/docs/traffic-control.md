---
title: Traffic control
description: Take an app out of service, or send specific paths elsewhere, without touching its containers.
---

Maintenance mode and redirects are enforced by Angie before a request reaches your containers. Both survive redeploys, because Deku reapplies them every time it writes the app's vhost.

## Maintenance mode

Serve a `503` to visitors while the containers keep running:

```bash
deku maintenance on my-app
deku maintenance on my-app --message "Back at 14:00 UTC"
deku maintenance status my-app
deku maintenance off my-app
```

With `--message`, the text is returned as the response body, so visitors learn why the site is closed. Without it, they get an empty `503`.

Maintenance mode replaces the proxy location. It does not stop or scale the containers, and deploys still work while it is on, which is what you want when you are preparing a fix. Turn it off to resume serving.

## Redirects

Send an individual path to another URL or path:

```bash
deku redirects add my-app /old https://example.com/new
deku redirects add my-app /docs /documentation --code 301
deku redirects list my-app
deku redirects remove my-app <id>
```

`--code` accepts `301`, `302`, `307`, and `308`, and defaults to `302`.

A redirect matches one exact path. `/old` matches `/old` and not `/old/page` or `/old-ish`, so there is no prefix or pattern matching. `deku redirects list` prints the id that `remove` expects. Adding a redirect for a path that already has one is rejected; remove the existing entry first.

Because redirects live in the vhost, a request for a redirected path never reaches your containers. Use them for renamed pages, retired endpoints, and paths that now live on another host.
