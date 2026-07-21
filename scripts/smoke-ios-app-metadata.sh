#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(cutout_host_os)" != Darwin ]]; then
  echo "CutoutApp metadata smoke requires Darwin/Xcode" >&2
  exit 1
fi

export DEVELOPER_DIR="${CUTOUT_DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}"
unset SDKROOT

product="${CUTOUT_IOS_METADATA_PRODUCT:-$(cutout_build_ios_app_bundle)}"

python3 - "$product/Info.plist" <<'PY'
import plistlib
import sys

with open(sys.argv[1], "rb") as fh:
    plist = plistlib.load(fh)

expected_scalars = {
    "CFBundleDisplayName": "CutOut",
    "NSBluetoothAlwaysUsageDescription": "CutOut uses Bluetooth to read live vehicle telemetry.",
}
for key, expected in expected_scalars.items():
    actual = plist.get(key)
    if actual != expected:
        raise SystemExit(f"metadata mismatch for {key}: expected {expected!r}, got {actual!r}")

expected_orientations = [
    "UIInterfaceOrientationPortrait",
    "UIInterfaceOrientationLandscapeLeft",
    "UIInterfaceOrientationLandscapeRight",
]
if plist.get("UIDeviceFamily") != [1]:
    raise SystemExit(f"metadata mismatch for UIDeviceFamily: expected [1], got {plist.get('UIDeviceFamily')!r}")
for key in ("UISupportedInterfaceOrientations",):
    actual = plist.get(key)
    if actual != expected_orientations:
        raise SystemExit(f"metadata mismatch for {key}: expected {expected_orientations!r}, got {actual!r}")
PY

echo "ios_app_metadata=ok"
echo "ios_app_product=$product"
