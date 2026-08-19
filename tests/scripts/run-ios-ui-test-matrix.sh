#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
matrix_runner="$root/scripts/run-ios-ui-test-matrix.sh"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-ui-test-matrix.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

help_output="$($matrix_runner --help)"
grep -q -- "--plan-from" <<<"$help_output"

printf '%s\n' '{
  "errors": [],
  "values": [{
    "enabledTests": [
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testDefault()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testDarkInDarkAppearanceAtAccessibilityDynamicType()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testRTLInRightToLeftLayoutWithIncreasedContrast()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testLargeAtExtraExtraExtraLargeType()"}
    ]
  }]
}' >"$tmp/enumeration.json"

plan="$($matrix_runner --plan-from "$tmp/enumeration.json")"
grep -q "dark disabled accessibility-extra-extra-extra-large: 1 test" <<<"$plan"
grep -q "light enabled accessibility-extra-extra-extra-large: 1 test" <<<"$plan"
grep -q "light disabled extra-extra-extra-large: 1 test" <<<"$plan"
grep -q "light disabled large: 1 test" <<<"$plan"
grep -q "4 tests across 4 simulator-settings groups" <<<"$plan"
