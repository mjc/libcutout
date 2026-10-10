#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-swift-test-runner.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
export CUTOUT_RUNNER_TEST_CALLS="$tmp/calls"
export CUTOUT_RUNNER_TEST_ARCH=arm64

uname() {
  case "$1" in
    -s) printf 'Darwin\n' ;;
    -m) printf '%s\n' "$CUTOUT_RUNNER_TEST_ARCH" ;;
  esac
}
cargo() {
  printf '%s\n' "$@" >"$CUTOUT_RUNNER_TEST_CALLS"
}
export -f uname cargo

bash "$root/scripts/run-swift-package-tests.sh" --filter MusicIntegrationTests >"$tmp/output" 2>&1
if ! grep -qx -- --arch "$CUTOUT_RUNNER_TEST_CALLS" \
  || ! grep -qx arm64 "$CUTOUT_RUNNER_TEST_CALLS"; then
  echo 'Swift package test runner must explicitly select arm64' >&2
  exit 1
fi
grep -qx MusicIntegrationTests "$CUTOUT_RUNNER_TEST_CALLS"
bash "$root/scripts/run-swift-package-tests.sh" --arch arm64 --arch=arm64 >"$tmp/output" 2>&1
[[ "$(grep -c -- '^--arch$' "$CUTOUT_RUNNER_TEST_CALLS")" -eq 1 ]]
[[ "$(grep -c '^arm64$' "$CUTOUT_RUNNER_TEST_CALLS")" -eq 1 ]]

for arguments in '--arch x86_64' '--arch=x86_64' '--triple x86_64-apple-macosx15.0' '--destination intel.json' '--experimental-swift-sdk intel' '--experimental-swift-sdk=intel'; do
  rm -f "$CUTOUT_RUNNER_TEST_CALLS"
  if bash "$root/scripts/run-swift-package-tests.sh" $arguments >"$tmp/output" 2>&1; then
    echo "Swift package runner accepted target override: $arguments" >&2
    exit 1
  fi
  [[ ! -e "$CUTOUT_RUNNER_TEST_CALLS" ]]
  grep -q arm64 "$tmp/output"
done

export CUTOUT_RUNNER_TEST_ARCH=x86_64
if bash "$root/scripts/run-swift-package-tests.sh" >"$tmp/output" 2>&1; then
  echo 'Swift package runner accepted an Intel or Rosetta host' >&2
  exit 1
fi
[[ ! -e "$CUTOUT_RUNNER_TEST_CALLS" ]]
grep -q 'Apple Silicon' "$tmp/output"
printf 'Swift package runner ARM64 checks passed\n'
