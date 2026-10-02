#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

log_directory="$root/target/test-logs"
mkdir -p "$log_directory"
log_file="$log_directory/rust-workspace-$(date +%Y%m%dT%H%M%S)-$$.log"

printf 'Full Rust workspace test log: %s\n' "$log_file"

run_logged() {
  local command_status
  local tee_status
  local summary_status
  local -a pipeline_status

  {
    printf '$'
    printf ' %q' "$@"
    printf '\n'
  } | tee -a "$log_file"

  set +e
  "$@" 2>&1 \
    | tee -a "$log_file" \
    | awk '
        /Summary \[/ || /^test result: / || /^Doc-tests / ||
        /error:/ || /^FAIL / {
          print
          fflush()
        }
      '
  pipeline_status=("${PIPESTATUS[@]}")
  command_status="${pipeline_status[0]}"
  tee_status="${pipeline_status[1]}"
  summary_status="${pipeline_status[2]}"
  set -e

  if [[ "$tee_status" -ne 0 || "$summary_status" -ne 0 ]]; then
    printf 'Could not reliably log Rust test output: %s\n' "$log_file" >&2
    return 2
  fi

  return "$command_status"
}

if run_logged cargo nextest run --workspace --locked; then
  :
else
  status=$?
  printf 'Rust workspace tests failed. Full log: %s\n' "$log_file" | tee -a "$log_file" >&2
  exit "$status"
fi

if run_logged cargo test --workspace --doc --locked; then
  :
else
  status=$?
  printf 'Rust doc tests failed. Full log: %s\n' "$log_file" | tee -a "$log_file" >&2
  exit "$status"
fi

printf 'Rust workspace tests and doc tests passed. Full log: %s\n' "$log_file" \
  | tee -a "$log_file"
