---
title: Plugin API
description: Overview of Deku's current plugin model.
---

Deku plugins are compiled native libraries loaded by `dekud` at runtime from `~/.deku/plugins/`.

## Export Contract

Each plugin exports a constructor symbol:

```rust
#[no_mangle]
pub extern "C" fn deku_plugin_create() -> *mut dyn PluginDescriptor {
    Box::into_raw(Box::new(MyPlugin))
}
```

## Descriptor Surface

The current plugin descriptor model provides:

- `name()`
- `version()`
- optional hook factories for lifecycle integration

Current hook categories in the daemon:

- `pre_build`
- `post_build`
- `pre_deploy`
- `post_deploy`
- `app_create`
- `app_destroy`

## Current State Note

The plugin architecture is present and first-party plugin crates exist, but the broader plugin surface is still evolving. Treat the current API as subject to change.

For current first-party platform features, Deku uses a split model:

- Critical built-in workflows such as Postgres, Redis, MySQL, object store management, and release/install support ship through the daemon, HTTP API, dashboard, and CLI directly.
- The dynamic plugin runtime remains available for evolving lifecycle hooks and future third-party extensions.

This means the native plugin API is real, but it is not the only or primary delivery path for first-party functionality today.

## Loading Model

`dekud` scans `~/.deku/plugins/` on startup and can also load/unload plugins through API and CLI flows.

## First-Party Plugin Areas

The repository currently contains first-party plugin crates for:

- postgres
- redis
- mysql
- letsencrypt
- checks
- storage
- network
- cron
- git
- domains
