#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
log_directory="$root/target/test-logs"
mkdir -p "$log_directory"
log_file="$log_directory/swift-package-$(date +%Y%m%dT%H%M%S)-$$.log"

printf 'Full Swift package test log: %s\n' "$log_file"
{
  printf '$ cargo cutout swift -- test --package-path %q' "$root/swift/CutoutMobile"
  printf ' %q' "$@"
  printf '\n'
} >"$log_file"

set +e
cargo cutout swift -- test --package-path "$root/swift/CutoutMobile" "$@" 2>&1 \
  | tee -a "$log_file" \
  | awk '
      /error:/ || /Test Case .* failed/ || /Test Suite .* (passed|failed)/ ||
      /Executed [0-9]+ tests, with [1-9][0-9]* failures/ ||
      /Some test targets reported failures/ || /✘/ {
        print
        fflush()
      }
    '
pipeline_status=("${PIPESTATUS[@]}")
test_status="${pipeline_status[0]}"
log_status="${pipeline_status[1]}"
summary_status="${pipeline_status[2]}"
set -e

if [[ "$log_status" -ne 0 || "$summary_status" -ne 0 ]]; then
  printf 'Could not reliably log Swift package test output: %s\n' "$log_file" >&2
  exit 2
fi

if [[ "$test_status" -eq 0 ]]; then
  printf 'Swift package tests passed. Full log: %s\n' "$log_file" | tee -a "$log_file"
else
  printf 'Swift package tests failed (exit %s). Full log: %s\n' "$test_status" "$log_file" \
    | tee -a "$log_file" >&2
fi

exit "$test_status"
