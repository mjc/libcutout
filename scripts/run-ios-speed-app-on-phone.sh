#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "CutoutApp iPhone launch requires Darwin" >&2
  exit 1
fi

export DEVELOPER_DIR="${CUTOUT_DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}"
unset SDKROOT

device_udid="${CUTOUT_IOS_DEVICE_UDID:-$(cutout_connected_ios_device_udid)}"
product="${CUTOUT_IOS_DEVICE_PRODUCT:-$(CUTOUT_IOS_DEVICE_UDID="$device_udid" cutout_build_ios_device_speed_app_bundle)}"
bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-$(cutout_ios_app_bundle_identifier "$product")}"

xcrun devicectl device install app --device "$device_udid" "$product"
xcrun devicectl device process launch --device "$device_udid" --terminate-existing --activate "$bundle_id"

echo "ios_device_udid=$device_udid"
echo "ios_app_bundle_id=$bundle_id"
echo "ios_speed_app_product=$product"
