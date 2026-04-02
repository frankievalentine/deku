#!/usr/bin/env bash
set -euo pipefail

DEKU_VERSION="${DEKU_VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

echo "==> Installing Deku ${DEKU_VERSION}"

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

OS=$(uname -s | tr '[:upper:]' '[:lower:]')

echo "==> Detected ${OS}/${ARCH}"

# Install Angie (Ubuntu/Debian)
if command -v apt-get &>/dev/null; then
  echo "==> Installing Angie"
  curl -sSL https://angie.software/keys/angie-signing.gpg | \
    gpg --dearmor -o /usr/share/keyrings/angie-signing.gpg
  echo "deb [signed-by=/usr/share/keyrings/angie-signing.gpg] \
    https://download.angie.software/angie/$(. /etc/os-release && echo "$ID/$VERSION_CODENAME") free main" \
    > /etc/apt/sources.list.d/angie.list
  apt-get update -qq
  apt-get install -y angie
fi

echo "==> Deku installation placeholder — binary download not yet implemented"
echo "    Build from source: cargo build --release"
