# Contributing to Deku

Repository:
[github.com/frankievalentine/deku](https://github.com/frankievalentine/deku)

This document covers the recommended workflow for contributing code, docs, and fixes to Deku.

## Goals

Contributions should improve:

- Correctness
- Operator experience
- User-facing clarity
- Maintainability of the CLI, daemon, dashboard, docs, and installer

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
2. Check the current worktree and avoid reverting unrelated changes.
3. Understand the affected area before editing.
4. Make the smallest coherent change that solves the problem.
5. Run targeted verification for the area you touched.
6. Run broader checks for cross-cutting changes before opening or updating a pull request.
7. Update docs and operator guidance in the same change when behavior changes.

## Verification

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

The local CI script mirrors the main CI workflow in `.github/workflows/ci.yml`: Rust formatting, clippy, tests, installer smoke coverage, dashboard checks, and docs checks.

## Pull Requests

- Describe the problem and the behavioral change clearly.
- Mention important tradeoffs, assumptions, and follow-up work.
- Call out changes to command output, API shapes, install behavior, or deploy behavior.
- Include the exact verification you ran.
- Keep unrelated refactors out of the same PR unless they are required for correctness.

## Documentation

When behavior changes, update the relevant docs in the same change. Common files to keep aligned:

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
