#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

log() {
  printf '==> %s\n' "$*"
}

run_in_dir() {
  local dir="$1"
  shift
  (
    cd "${ROOT_DIR}/${dir}"
    "$@"
  )
}

log "Checking Rust formatting"
cargo fmt --all -- --check

log "Running Rust clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

log "Running Rust tests"
cargo test --all

log "Auditing Rust dependencies"
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  log "cargo-audit is not installed; skipping (cargo install cargo-audit)"
fi

log "Running installer smoke test"
"${ROOT_DIR}/scripts/test-install-smoke.sh"

log "Installing dashboard dependencies"
run_in_dir dashboard bun install --frozen-lockfile

log "Linting dashboard"
run_in_dir dashboard bun run lint

log "Type checking dashboard"
run_in_dir dashboard bun run check

log "Building dashboard"
run_in_dir dashboard bun run build

log "Installing docs dependencies"
run_in_dir docs bun install --frozen-lockfile

log "Type checking docs"
run_in_dir docs bun run check

log "Building docs"
run_in_dir docs bun run build

log "Local CI checks passed"
