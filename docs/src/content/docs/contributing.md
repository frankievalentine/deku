---
title: Contributing
description: Contributing guidance for the Deku repository.
---

Contributions should prioritize correctness, compatibility, and clear user-facing behavior.

## Recommended Workflow

1. Confirm the current repository state and existing worktree changes.
2. Make changes in the smallest coherent slice possible.
3. Run targeted checks for the affected area.

## Code Areas

- `crates/dekud`: daemon, API, deploy pipeline, SSH, proxy integration
- `crates/deku`: CLI
- `dashboard/`: Astro dashboard
- `docs/`: Starlight docs
- `plugins/`: first-party plugin crates

## Expectations

- Do not revert unrelated dirty-worktree changes.
- Keep API and CLI request/response shapes aligned.
- Verify browser-facing work with `astro check` and `astro build`.
- Verify Rust-facing changes with workspace tests or targeted checks.

## Documentation

If command surfaces or operating workflows change, update:

- `AGENTS.md` when the supported coding-agent workflow changes
- `docs/`
- `.codex/deku-ops.md` when local run/test instructions change
