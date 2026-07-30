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

first_result="$(cutout_create_ios_ui_test_result_bundle "$tmp/derived-data")"
second_result="$(cutout_create_ios_ui_test_result_bundle "$tmp/derived-data")"

if [[ "$first_result" == "$second_result" ]]; then
  echo "expected each UI test invocation to own a distinct result bundle" >&2
  exit 1
fi
if [[ -e "$first_result" || -e "$second_result" ]]; then
  echo "xcodebuild result bundle paths must not exist before the run" >&2
  exit 1
fi

assert_equal "Result.xcresult" "$(basename "$first_result")"
assert_equal "$tmp/derived-data/TestResults" "$(dirname "$(dirname "$first_result")")"
