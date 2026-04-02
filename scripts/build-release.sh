#!/usr/bin/env bash
set -euo pipefail

echo "==> Building Deku release binaries"

targets=(
  "x86_64-unknown-linux-musl"
  "aarch64-unknown-linux-musl"
)

for target in "${targets[@]}"; do
  echo "==> Building for $target"
  cargo build --release --target "$target" -p dekud -p deku
done

echo "==> Packaging"
mkdir -p dist

for target in "${targets[@]}"; do
  arch="${target%%-*}"
  cp "target/$target/release/dekud" "dist/dekud-linux-$arch"
  cp "target/$target/release/deku" "dist/deku-linux-$arch"
done

echo "==> Build complete. Artifacts in dist/"
