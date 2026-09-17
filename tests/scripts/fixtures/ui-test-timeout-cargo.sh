#!/usr/bin/env bash
set -euo pipefail

# Stand in for a coordinator that spawned a build process which ignores TERM.
bash -c '
  trap '\'' '\'' TERM
  touch "$CUTOUT_TIMEOUT_TEST_DIR/child-started"
  while :; do sleep 0.1; done
' &
child=$!
printf '%s\n' "$child" >"$CUTOUT_TIMEOUT_TEST_DIR/child.pid"
wait "$child"
