#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "CutoutApp iOS-on-Mac requires Darwin" >&2
  exit 1
fi

if [[ "$(uname -m)" != arm64 ]]; then
  echo "CutoutApp iOS-on-Mac requires Apple Silicon" >&2
  exit 1
fi

export DEVELOPER_DIR="${CUTOUT_DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}"
unset SDKROOT

destination="${CUTOUT_IOS_ON_MAC_DESTINATION:-platform=macOS,id=00008103-001935121A8A001E}"
export CUTOUT_IOS_APP_BUILD_DESTINATION="$destination"
product="${CUTOUT_IOS_ON_MAC_PRODUCT:-$(cutout_build_ios_app_bundle)}"

echo "ios_app_product=$product"
echo "ios_app_destination=$destination"
echo "ios_app_note=real iPhoneOS app bundle built; use scripts/run-cutout-app.sh --launch-smoke for local Mac launch proof"
