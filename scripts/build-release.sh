#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"

targets=(
  "x86_64-unknown-linux-musl"
  "aarch64-unknown-linux-musl"
)

release_artifacts=(
  "deku-dashboard.tar.gz"
  "deku-linux-amd64"
  "deku-linux-arm64"
  "dekud-linux-amd64"
  "dekud-linux-arm64"
  "install.sh"
  "deku.service"
  "angie-deku.conf"
)

log() {
  printf '==> %s\n' "$*"
}

build_target_dir() {
  printf '%s/target/release-build/%s\n' "${ROOT_DIR}" "$1"
}

checksum_file() {
  local file="$1"

  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file"
    return
  fi

  shasum -a 256 "$file"
}

artifact_arch() {
  case "$1" in
    x86_64-unknown-linux-musl) echo "amd64" ;;
    aarch64-unknown-linux-musl) echo "arm64" ;;
    *) printf 'unknown target: %s\n' "$1" >&2; exit 1 ;;
  esac
}

build_dashboard() {
  log "Building dashboard assets"
  (
    cd "${ROOT_DIR}/dashboard"
    bun install --frozen-lockfile
    bun run build
  )

  tar -czf "${DIST_DIR}/deku-dashboard.tar.gz" \
    -C "${ROOT_DIR}/dashboard/dist" .
}

build_binaries() {
  local builder=(cargo build)

  if command -v cross >/dev/null 2>&1; then
    builder=(cross build)
  fi

  for target in "${targets[@]}"; do
    log "Building ${target}"
    CARGO_TARGET_DIR="$(build_target_dir "$target")" \
      "${builder[@]}" --release --target "$target" -p dekud -p deku
  done
}

package_binaries() {
  for target in "${targets[@]}"; do
    local arch
    local target_dir
    arch="$(artifact_arch "$target")"
    target_dir="$(build_target_dir "$target")"

    cp "${target_dir}/${target}/release/dekud" "${DIST_DIR}/dekud-linux-${arch}"
    cp "${target_dir}/${target}/release/deku" "${DIST_DIR}/deku-linux-${arch}"
  done
}

package_support_files() {
  cp "${ROOT_DIR}/scripts/install.sh" "${DIST_DIR}/install.sh"
  cp "${ROOT_DIR}/scripts/package/deku.service" "${DIST_DIR}/deku.service"
  cp "${ROOT_DIR}/scripts/package/angie-deku.conf" "${DIST_DIR}/angie-deku.conf"
  chmod 0755 "${DIST_DIR}/install.sh"
  chmod 0644 "${DIST_DIR}/deku.service"
  chmod 0644 "${DIST_DIR}/angie-deku.conf"
}

write_checksums() {
  (
    cd "${DIST_DIR}"
    : > SHA256SUMS
    for artifact in "${release_artifacts[@]}"; do
      [[ -f "$artifact" ]] || {
        printf 'missing release artifact: %s\n' "$artifact" >&2
        exit 1
      }
      checksum_file "$artifact" >> SHA256SUMS
    done
  )
}

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

build_dashboard
build_binaries
package_binaries
package_support_files
write_checksums

log "Artifacts written to ${DIST_DIR}"
