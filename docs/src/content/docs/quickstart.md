---
title: Quick Start
description: Run Deku locally for current milestone testing.
---

This quick start is aimed at the current repository state, not a packaged release.

## 1. Build the Dashboard

```bash
cd /Users/frankie/Projects/deku/dashboard
bun run build
mkdir -p ~/.deku/dashboard
cp -R /Users/frankie/Projects/deku/crates/dekud/assets/dashboard/. ~/.deku/dashboard/
```

## 2. Start the Daemon

```bash
cd /Users/frankie/Projects/deku
RUST_LOG=info cargo run -p dekud
```

## 3. Read the API Token

```bash
cat ~/.deku/cli-token
```

## 4. Open the Dashboard

Visit:

```text
http://127.0.0.1:2810
```

Paste the token from `~/.deku/cli-token` into the connect screen.

## 5. Smoke Test

Recommended checks:

1. Create an app from the dashboard.
2. Open the app detail page.
3. Add a domain and config variable.
4. Open the SSH Keys and Plugins pages.
5. Use the CLI to confirm the same data:

```bash
deku apps list
deku apps info <app>
deku config list <app>
deku domains list <app>
```

## Current Limitation

At the current milestone state, dashboard assets are not yet automatically staged into the runtime directory. Local testing still requires the manual copy step above.
