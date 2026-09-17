---
title: Resource limits
description: Cap the memory and CPU an app's processes can use.
---

Resource limits cap what each container can consume. They protect the host from one heavy app and keep a runaway process from starving its neighbors.

## Show current limits

```bash
deku ps limits my-app
```

## Set limits

```bash
deku ps limits my-app --memory 512m
deku ps limits my-app --cpu 0.5
deku ps limits my-app --process worker --memory 1g --cpu 500m
```

- `--memory` accepts `k`, `m`, and `g` suffixes, or a plain byte count: `512m`, `1g`, `268435456`.
- `--cpu` accepts decimal cores or millicores: `0.5`, `500m`.
- `--process` scopes the limit to one process type. Omit it to set the default for every process.

Values are checked when you set them. An unparseable `--memory` or `--cpu` is rejected, and a request with neither is refused.

## When limits apply

Limits are applied when containers are created, so a change takes effect on the next deploy:

```bash
deku ps limits my-app --memory 512m
deku deploy run my-app --path .
```

A limit set with `--process` applies to that process only. The default applies to every process that has no limit of its own.
