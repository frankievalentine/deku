#!/usr/bin/env bash
set -euo pipefail

INSTALLER_URL="${DEKU_INSTALLER_URL:-https://raw.githubusercontent.com/frankievalentine/deku/main/scripts/install.sh}"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

curl --fail --location --silent --show-error --retry 3 \
  "$INSTALLER_URL" \
  -o "$tmp"

bash "$tmp" "$@"
