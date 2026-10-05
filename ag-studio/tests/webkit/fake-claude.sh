#!/bin/sh
D=${FAKE_CLAUDE_LOG:-${TMPDIR:-/tmp}/agstudio-fake-claude}
mkdir -p "$D"
case "$1" in
  --version) echo "2.1.0 (Claude Code)"; exit 0 ;;
  auth) echo '{"loggedIn": true, "authMethod": "claude.ai"}'; exit 0 ;;
esac
printf '%s\0' "$@" > "$D/last-argv"
ls -A > "$D/last-ls"
for f in problem.*; do [ -f "$f" ] && cp "$f" "$D/$f"; done
PROMPT=$2
sleep 1
case "$PROMPT" in
  *"translate it into a .geo program"*|*"Translate this geometry problem"*)
    printf '```geo\n# Isosceles base angles\nB C = segment\nA = point: dist(A, B) = dist(A, C)\nprove eqangle(B, C, B, A, C, A, C, B)\n```\n'
    exit 0 ;;
  Problema:*)
    printf 'Deoarece AB = AC, triunghiul ABC este isoscel, deci unghiurile de la baza B si C sunt egale. Aceasta este o explicatie de proba scrisa de stub.\n'
    exit 0 ;;
  *)
    printf 'Since AB = AC, triangle ABC is isosceles, so its base angles at B and C are equal. This explanation came from the test stub.\n'
    exit 0 ;;
esac
