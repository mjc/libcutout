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

help_output="$("$root/scripts/run-ios-ui-tests.sh" --help)"
if ! grep -q -- "--smoke" <<<"$help_output"; then
  echo "expected UI test runner help to document the smoke lane" >&2
  exit 1
fi
if ! grep -q -- "--verbose" <<<"$help_output"; then
  echo "expected UI test runner help to document verbose diagnostics" >&2
  exit 1
fi
for option in --appearance --increase-contrast --content-size; do
  if ! grep -q -- "$option" <<<"$help_output"; then
    echo "expected UI test runner help to document $option" >&2
    exit 1
  fi
  if "$root/scripts/run-ios-ui-tests.sh" "$option" >"$tmp/missing-option-value.log" 2>&1; then
    echo "expected $option without a value to be rejected" >&2
    exit 1
  fi
  if ! grep -q "requires" "$tmp/missing-option-value.log"; then
    cat "$tmp/missing-option-value.log" >&2
    exit 1
  fi
done

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
