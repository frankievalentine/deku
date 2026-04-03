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

The plugin architecture is present and first-party plugin crates exist, but the broader plugin surface is still under active implementation. Treat the current API as milestone-stage and subject to change.

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
