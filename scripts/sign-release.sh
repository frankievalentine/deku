#!/usr/bin/env bash
# Sign a release's SHA256SUMS with the key in DEKU_RELEASE_SIGNING_KEY.
#
# Writes SHA256SUMS.sig next to it, then checks the signature against the public
# key embedded in scripts/install.sh, so a secret that does not match the
# installer fails the release instead of publishing one nobody can install.
#
# Usage: DEKU_RELEASE_SIGNING_KEY="$(cat key.pem)" scripts/sign-release.sh <dist-dir>
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${1:?usage: sign-release.sh <dist-dir>}"
CHECKSUMS="${DIST_DIR}/SHA256SUMS"
SIGNATURE="${DIST_DIR}/SHA256SUMS.sig"

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

[[ -n "${DEKU_RELEASE_SIGNING_KEY:-}" ]] || fail "DEKU_RELEASE_SIGNING_KEY is not set"
[[ -f "$CHECKSUMS" ]] || fail "${CHECKSUMS} does not exist"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
chmod 700 "$work"

(
  umask 077
  printf '%s\n' "$DEKU_RELEASE_SIGNING_KEY" > "${work}/release-signing.key"
)
openssl dgst -sha256 -sign "${work}/release-signing.key" -out "$SIGNATURE" "$CHECKSUMS"

# shellcheck source=/dev/null
source <(sed -n '/^# BEGIN RELEASE SIGNING KEY$/,/^# END RELEASE SIGNING KEY$/p' "${REPO_ROOT}/scripts/install.sh")
[[ -n "${RELEASE_SIGNING_PUBLIC_KEY:-}" ]] || fail "scripts/install.sh has no release signing key"
printf '%s\n' "$RELEASE_SIGNING_PUBLIC_KEY" > "${work}/release-signing.pub"

openssl dgst -sha256 -verify "${work}/release-signing.pub" -signature "$SIGNATURE" "$CHECKSUMS" >/dev/null \
  || fail "the signing key does not match the public key in scripts/install.sh"

printf 'Signed %s\n' "$CHECKSUMS"
