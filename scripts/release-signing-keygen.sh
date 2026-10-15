#!/usr/bin/env bash
# Generate the key pair that signs each release's SHA256SUMS.
#
# The private key is written outside the repository and never committed: add it
# to the repository's Actions secrets as DEKU_RELEASE_SIGNING_KEY, then keep an
# offline copy and delete it from this machine. The public key is written into
# scripts/install.sh, which verifies every release it installs against it.
#
# Usage: scripts/release-signing-keygen.sh [output-dir]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALLER="${REPO_ROOT}/scripts/install.sh"
OUT_DIR="${1:-${HOME}/.deku-release-signing}"
PRIVATE_KEY="${OUT_DIR}/release-signing.key"
PUBLIC_KEY="${OUT_DIR}/release-signing.pub"

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

command -v openssl >/dev/null 2>&1 || fail "openssl is required"

case "$(cd "$(dirname "$OUT_DIR")" 2>/dev/null && pwd)/$(basename "$OUT_DIR")" in
  "${REPO_ROOT}"/*) fail "write the key outside the repository, not under ${REPO_ROOT}" ;;
esac

[[ -e "$PRIVATE_KEY" ]] && fail "${PRIVATE_KEY} already exists; refusing to replace a signing key"

grep -q '^# BEGIN RELEASE SIGNING KEY$' "$INSTALLER" \
  || fail "no release signing key block found in ${INSTALLER}"

mkdir -p "$OUT_DIR"
chmod 700 "$OUT_DIR"

(
  umask 077
  openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "$PRIVATE_KEY"
)
openssl pkey -in "$PRIVATE_KEY" -pubout -out "$PUBLIC_KEY"

# Replace everything between the markers with the new public key.
awk -v key_file="$PUBLIC_KEY" '
  /^# BEGIN RELEASE SIGNING KEY$/ {
    print
    printf "RELEASE_SIGNING_PUBLIC_KEY=\x27"
    first = 1
    while ((getline line < key_file) > 0) {
      if (!first) printf "\n"
      printf "%s", line
      first = 0
    }
    print "\x27"
    skipping = 1
    next
  }
  /^# END RELEASE SIGNING KEY$/ { skipping = 0 }
  !skipping { print }
' "$INSTALLER" > "${INSTALLER}.tmp"
mv "${INSTALLER}.tmp" "$INSTALLER"
chmod 755 "$INSTALLER"

cat <<EOF
Release signing key generated.

  Private key: ${PRIVATE_KEY}
  Public key:  ${PUBLIC_KEY} (also written into scripts/install.sh)

Next:
  1. Add the private key as an Actions secret:
       gh secret set DEKU_RELEASE_SIGNING_KEY < "${PRIVATE_KEY}"
  2. Store an offline copy of the private key, then delete it here:
       rm -P "${PRIVATE_KEY}"    # shred -u on Linux
  3. Commit scripts/install.sh with the new public key.
EOF
