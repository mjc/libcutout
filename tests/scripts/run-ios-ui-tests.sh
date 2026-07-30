#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$root/scripts/swift-package-common.sh"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-ui-test-runner.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

assert_equal() {
  if [[ "$1" != "$2" ]]; then
    echo "expected '$1', got '$2'" >&2
    exit 1
  fi
}

logs="$tmp/Logs/Test"
old_result="$logs/Test-Old.xcresult"
current_result="$logs/Test-Current.xcresult"
marker="$tmp/current-run"
mkdir -p "$old_result" "$current_result"
touch "$old_result/Info.plist" "$current_result/Info.plist" "$marker"
touch -t 202001010000 "$old_result"
touch -t 202101010000 "$marker"
touch -t 202201010000 "$current_result"

assert_equal \
  "$current_result" \
  "$(cutout_latest_complete_xcresult_since "$logs" "$marker")"

rm -rf "$current_result"
assert_equal "" "$(cutout_latest_complete_xcresult_since "$logs" "$marker")"
