#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
  echo "CutoutApp iOS UI tests require Apple Silicon Darwin" >&2
  exit 1
fi

cutout_use_xcode_developer_dir

mode="test"
if [[ "${1:-}" == "--build-only" ]]; then
  mode="build-for-testing"
  shift
fi
destination="${CUTOUT_IOS_TEST_DESTINATION:-${CUTOUT_IOS_SIMULATOR_DESTINATION:-platform=iOS Simulator,name=Cutout iPhone 15 iOS 27,OS=latest}}"
project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
derived_data="${CUTOUT_IOS_UI_TEST_DERIVED_DATA:-${CUTOUT_IOS_SIMULATOR_DERIVED_DATA:-$root/target/xcode-simulator-tests}}"
xcodebuild_args=(
  -project "$root/$project"
  -scheme "$scheme"
  -destination "$destination"
  -derivedDataPath "$derived_data"
)

if [[ "$destination" == platform=iOS,* ]]; then
  derived_data="${CUTOUT_IOS_DEVICE_DERIVED_DATA:-$root/target/xcode-device-tests}"
  xcodebuild_args=(
    -project "$root/$project"
    -scheme "$scheme"
    -destination "$destination"
    -derivedDataPath "$derived_data"
    -allowProvisioningUpdates
    CODE_SIGNING_ALLOWED=YES
    CODE_SIGNING_REQUIRED=YES
    CODE_SIGN_STYLE=Automatic
    CODE_SIGN_IDENTITY="Apple Development"
    DEVELOPMENT_TEAM="${CUTOUT_IOS_DEVELOPMENT_TEAM:-2RH32Y5HM5}"
  )
fi
xcodebuild_args+=("$@")

/usr/bin/xcrun xcodebuild "${xcodebuild_args[@]}" build-for-testing

if [[ "$mode" == "test" ]]; then
  /usr/bin/xcrun xcodebuild \
    "${xcodebuild_args[@]}" \
    -parallel-testing-enabled NO \
    test-without-building
fi
