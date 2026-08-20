#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-ios-metadata.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

help_output="$("$root/scripts/smoke-ios-app-metadata.sh" --help)"
grep -q -- "--configuration Debug|Release" <<<"$help_output"

if "$root/scripts/smoke-ios-app-metadata.sh" --configuration >"$tmp/missing.log" 2>&1; then
  echo "expected a missing configuration value to be rejected" >&2
  exit 1
fi
grep -q -- "--configuration requires Debug or Release" "$tmp/missing.log"

if "$root/scripts/smoke-ios-app-metadata.sh" --configuration Profile >"$tmp/invalid.log" 2>&1; then
  echo "expected an invalid configuration to be rejected" >&2
  exit 1
fi
grep -q -- "--configuration must be Debug or Release" "$tmp/invalid.log"

if "$root/scripts/smoke-ios-app-metadata.sh" --unknown >"$tmp/unknown.log" 2>&1; then
  echo "expected an unknown option to be rejected" >&2
  exit 1
fi
grep -q -- "unknown option: --unknown" "$tmp/unknown.log"

product="$tmp/CutoutApp.app"
mkdir -p "$product"
python3 - "$product/Info.plist" <<'PY'
import plistlib
import sys

with open(sys.argv[1], "wb") as fh:
    plistlib.dump(
        {
            "CFBundleDisplayName": "CutOut",
            "NSBluetoothAlwaysUsageDescription": "CutOut uses Bluetooth to read live vehicle telemetry.",
            "UIDeviceFamily": [1],
            "UISupportedInterfaceOrientations": [
                "UIInterfaceOrientationPortrait",
                "UIInterfaceOrientationLandscapeLeft",
                "UIInterfaceOrientationLandscapeRight",
            ],
        },
        fh,
    )
PY

release_output="$(CUTOUT_IOS_METADATA_PRODUCT="$product" \
  "$root/scripts/smoke-ios-app-metadata.sh" --configuration Release)"
grep -q "ios_app_metadata=ok" <<<"$release_output"
grep -q "ios_app_configuration=Release" <<<"$release_output"
grep -q "ios_app_product=$product" <<<"$release_output"
