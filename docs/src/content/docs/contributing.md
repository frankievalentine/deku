---
title: Contributing
description: Contributing guidance for the Deku repository.
---

Deku is still in active milestone buildout. Contributions should prioritize correctness and milestone completion over polish-only changes.

## Recommended Workflow

1. Read `.codex/deku-build-plan.md`.
2. Confirm the current milestone and existing worktree state.
3. Make changes in the smallest coherent slice possible.
4. Run targeted checks for the affected area.

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

If command surfaces or milestone status change, update:

- `AGENTS.md` when the supported coding-agent workflow changes
- `.codex/deku-build-plan.md`
- `docs/`
- `.codex/deku-ops.md` when local run/test instructions change
