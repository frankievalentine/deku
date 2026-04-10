---
title: Contributing
description: How to contribute code, docs, and fixes to the Deku repository.
---

Contributions should improve correctness, operator experience, and user-facing clarity.

Repository:
[github.com/frankievalentine/deku](https://github.com/frankievalentine/deku)

Use the GitHub repository for issues, pull requests, and release history.

## Before You Start

- Read the [Architecture](/architecture/) page if your change touches the CLI, daemon, dashboard, or deploy pipeline.
- Check the current worktree before editing and avoid reverting unrelated changes.
- Keep changes in the smallest coherent slice you can review and verify.
- Prefer updating tests and docs in the same change when behavior changes.

## Repository Areas

- `crates/deku`: CLI commands and client behavior
- `crates/dekud`: daemon, HTTP API, deploy pipeline, routing, and host integration
- `crates/deku-core`: shared types and auth utilities
- `dashboard/`: Astro + React dashboard
- `docs/`: Starlight documentation site
- `plugins/`: first-party plugin crates
- `scripts/`: installer, release packaging, smoke tests, and local CI helpers

## Recommended Workflow

1. Start from an up-to-date local branch.
2. Understand the affected area before editing.
3. Make the smallest change that fully addresses the problem.
4. Run targeted checks for the area you touched.
5. Run broader checks before opening or updating a pull request when the change is cross-cutting.
6. Update docs and operator guidance when command surfaces or workflows change.

## Verification Expectations

Rust changes:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --all
```

Dashboard changes:

```bash
cd dashboard
bun install --frozen-lockfile
bun run lint
bun run check
bun run build
```

Docs changes:

```bash
cd docs
bun install --frozen-lockfile
bun run check
bun run build
```

Installer and packaging changes:

```bash
./scripts/test-install-smoke.sh
```

Full local CI pass:

```bash
./scripts/ci-local.sh
```

`./scripts/ci-local.sh` matches the main CI expectations in `.github/workflows/ci.yml`: Rust formatting, clippy, tests, installer smoke coverage, dashboard checks, and docs checks.

## Pull Request Recommendations

- Explain the user-facing problem and the behavioral change.
- Mention important tradeoffs, assumptions, and follow-up work.
- Call out any changes to command output, API shapes, install behavior, or deploy behavior.
- Include the exact verification you ran.
- Keep unrelated refactors out of the same PR unless they are required for correctness.

## Documentation Expectations

If behavior changes, update the relevant documentation in the same change. Common files to keep aligned:

- `README.md`
- `docs/`
- `AGENTS.md` when coding-agent workflow guidance changes
- `CONTRIBUTING.md` when contributor workflow guidance changes
- `.codex/deku-ops.md` when local run or test instructions change

## Practical Recommendations

- Keep CLI and daemon behavior aligned. If a CLI command depends on a daemon response shape, update both sides together.
- Treat installation and deploy changes as high-risk. Run the installer smoke test when touching packaging, release artifacts, or setup flow.
- Preserve established dashboard and docs patterns unless the change is intentionally redesigning them.
- Prefer deterministic, non-interactive commands in scripts and docs.
- Do not silently change operator-facing workflows without updating the related docs.
