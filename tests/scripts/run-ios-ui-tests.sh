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

mkdir -p "$tmp/derived-data/.run-ios-ui-tests.lock"
if CUTOUT_IOS_UI_TEST_DERIVED_DATA="$tmp/derived-data" \
  "$root/scripts/run-ios-ui-tests.sh" --no-build >"$tmp/no-build.log" 2>&1
then
  echo "expected --no-build to be rejected" >&2
  exit 1
fi
if ! grep -q "an incremental test build is required" "$tmp/no-build.log"; then
  cat "$tmp/no-build.log" >&2
  exit 1
fi
rmdir "$tmp/derived-data/.run-ios-ui-tests.lock"

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
