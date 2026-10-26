#!/usr/bin/env bash
# Build the wasm pkg from the Rust engine and stage the static site in dist/.
# (The GitHub Action runs the same commands; this script is for local builds.)
set -euo pipefail
cd "$(dirname "$0")"

# wasm-bindgen-cli version must match the wasm-bindgen crate version exactly
# (pinned in Cargo.toml) or wasm-bindgen fails with a version-mismatch error.
BINDGEN_VERSION=$(grep -m1 '^wasm-bindgen' Cargo.toml | sed -E 's/.*"=([0-9.]+)".*/\1/')
if [ -z "$BINDGEN_VERSION" ]; then
  echo "error: cannot read pinned wasm-bindgen version from Cargo.toml" >&2
  exit 1
fi
echo "==> wasm-bindgen crate version: $BINDGEN_VERSION"
actual=$(wasm-bindgen --version | awk '{print $2}')
if [ "$actual" != "$BINDGEN_VERSION" ]; then
  echo "error: wasm-bindgen-cli is $actual, crate pin is $BINDGEN_VERSION — install the matching cli:" >&2
  echo "  cargo install wasm-bindgen-cli@$BINDGEN_VERSION --locked" >&2
  exit 1
fi

cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --out-dir web/pkg --target web \
  target/wasm32-unknown-unknown/release/ledger_reports_wasm.wasm

rm -rf dist
mkdir -p dist
cp web/index.html web/style.css web/app.js dist/
cp -r web/pkg dist/pkg

echo "==> dist/"
find dist -type f | sort
du -sh dist