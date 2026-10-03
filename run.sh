#!/usr/bin/env bash
# Build (release) and launch the GeoSolver web app.  ./run.sh [PORT]
set -euo pipefail
cd "$(dirname "$0")"
port="${1:-8787}"
cargo build --release -p ag-studio
echo "Serving on http://127.0.0.1:$port"
exec ./target/release/agstudio serve --port "$port"
