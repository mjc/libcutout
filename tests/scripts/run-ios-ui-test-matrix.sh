#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
matrix_runner="$root/scripts/run-ios-ui-test-matrix.sh"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-ui-test-matrix.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

help_output="$($matrix_runner --help)"
grep -q -- "--plan-from" <<<"$help_output"
grep -q -- "--only-group" <<<"$help_output"
grep -q -- "--smoke" <<<"$help_output"

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

dark_plan="$($matrix_runner \
  --plan-from "$tmp/enumeration.json" \
  --only-group "dark disabled accessibility-extra-extra-extra-large")"
grep -q "dark disabled accessibility-extra-extra-extra-large: 1 test" <<<"$dark_plan"
grep -q "1 test across 1 simulator-settings group" <<<"$dark_plan"
if grep -q "light " <<<"$dark_plan"; then
  echo "--only-group retained an unrelated settings group" >&2
  exit 1
fi

if "$matrix_runner" \
  --plan-from "$tmp/enumeration.json" \
  --only-group "dark enabled large" >/dev/null 2>&1; then
  echo "--only-group accepted a settings group with no compiled tests" >&2
  exit 1
fi

printf '%s\n' '{
  "errors": [],
  "values": [{
    "enabledTests": [
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testPickerExposesAccessibleCaptureControls()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testVescUseShowsConnectingBeforeRide()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testVescRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testEucRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testEucBmsDiagnosticsExposeStableAccessibleDataRows()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testVescCriticalLiveActivityAutoFixtureStartsAnAccessibleRide()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testVescCriticalLiveActivityLockScreenPreservesSafetySemantics()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testVescLiveActivityContinuesUpdatingWhileBackgrounded()"},
      {"identifier": "CutoutAppUITests/CutoutAppUITests/testUnrelated()"}
    ]
  }]
}' >"$tmp/smoke-enumeration.json"

smoke_plan="$($matrix_runner --plan-from "$tmp/smoke-enumeration.json" --smoke)"
grep -q "light disabled accessibility-extra-extra-extra-large: 2 tests" <<<"$smoke_plan"
grep -q "light disabled large: 5 tests" <<<"$smoke_plan"
grep -q "7 tests across 2 simulator-settings groups" <<<"$smoke_plan"
if grep -q "testUnrelated" <<<"$smoke_plan"; then
  echo "--smoke retained a test outside the smoke lane" >&2
  exit 1
fi
