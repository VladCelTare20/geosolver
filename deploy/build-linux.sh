#!/usr/bin/env bash
# Build a release binary of GeoSolver on a Linux host (no Docker).
# Building on the target host uses that host's CPU tuning, which is what you want.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust/cargo not found. Install it: https://rustup.rs" >&2
  exit 1
fi

echo "Building (release)…"
cargo build --release -p ag-studio

echo
echo "Built: target/release/agstudio"
echo "Try it locally:   ./target/release/agstudio serve"
echo "Expose publicly:  AGSTUDIO_BIND=0.0.0.0:8787 AGSTUDIO_BASIC_AUTH=user:pass ./target/release/agstudio serve"
