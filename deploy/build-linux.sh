#!/usr/bin/env bash
# Build a release binary of GeoSolver on a Linux host (no Docker).
# Builds with the host-CPU tuning of .cargo/config.toml (target-cpu=native):
# run it on the machine that will serve, not on a different CPU.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust/cargo not found. Install it: https://rustup.rs" >&2
  exit 1
fi

echo "Building (release)…"
cargo build --release --locked -p ag-studio

bin="${CARGO_TARGET_DIR:-target}/release/agstudio"
case "$bin" in /*) ;; *) bin="./$bin" ;; esac
echo
echo "Built: $bin"
echo "Try it locally:   $bin serve"
# shellcheck disable=SC2016
echo 'On the LAN:       AGSTUDIO_BIND=0.0.0.0:8787 AGSTUDIO_BASIC_AUTH=":$(openssl rand -hex 16)"' "$bin serve"
echo "Public:           see deploy/DEPLOY.md (systemd unit + TLS proxy)"
