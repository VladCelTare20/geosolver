#!/usr/bin/env bash
# Starts the three servers the browser suites need and prints the environment
# for each suite. Stops them again with `start-servers.sh stop <dir>`.
#   start-servers.sh <agstudio binary> <first port> [dir]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"

if [ "${1:-}" = "stop" ]; then
  dir="${2:?start-servers.sh stop <dir>}"
  for f in "$dir"/*.pid; do
    [ -e "$f" ] || continue
    kill "$(cat "$f")" 2>/dev/null || true
    rm -f "$f"
  done
  exit 0
fi

bin="${1:?start-servers.sh <agstudio binary> <first port> [dir]}"
port="${2:?start-servers.sh <agstudio binary> <first port> [dir]}"
dir="${3:-$(mktemp -d)}"
pw="${BASIC_PASSWORD:-pw-for-tests-123}"
mkdir -p "$dir"
log="$dir/fake-claude.log"

start() {
  local name=$1 p=$2
  shift 2
  env AGSTUDIO_DB="$dir/$name.db" AGSTUDIO_CLAUDE_BIN="$here/fake-claude.sh" CLAUDE_CODE_OAUTH_TOKEN=dummy \
    FAKE_CLAUDE_LOG="$log" AGSTUDIO_RATE_PER_MIN=0 AGSTUDIO_TRANSLATE_PER_MIN=0 AGSTUDIO_AUTH_PER_MIN=0 AGSTUDIO_GUEST_AI_PER_DAY=0 \
    "$@" "$bin" serve --port "$p" > "$dir/$name.log" 2>&1 &
  echo $! > "$dir/$name.pid"
}

start guest "$port" AGSTUDIO_BASIC_AUTH=":$pw" AGSTUDIO_GUEST_MODE=1
start accounts-pw "$((port + 1))" AGSTUDIO_BASIC_AUTH=":$pw" AGSTUDIO_GUEST_MODE=0
start accounts-open "$((port + 2))" AGSTUDIO_GUEST_MODE=0

for p in "$port" "$((port + 1))" "$((port + 2))"; do
  for _ in $(seq 1 100); do
    curl -fs -o /dev/null "http://127.0.0.1:$p/healthz" && break
    sleep 0.1
  done
done

g="http://127.0.0.1:$port"
cat <<EOF
servers in $dir (stop: $0 stop $dir)
common:  BASIC_PASSWORD=$pw FAKE_CLAUDE_LOG=$log EXPECT_HUMAN=1
iphone-layout, human-shots, helper-leaks:
         BASE=$g
android-layout:
         BASE=$g ACCOUNTS_BASE=http://127.0.0.1:$((port + 2))
iphone-e2e, compat-robustness:
         GUEST_BASE=$g ACCOUNTS_BASE=http://127.0.0.1:$((port + 1))
desktop-e2e, android-e2e:
         GUEST_BASE=$g ACCOUNTS_BASE=http://127.0.0.1:$((port + 2))
EOF
