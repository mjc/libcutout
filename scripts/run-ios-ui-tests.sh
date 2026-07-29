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
elif [[ "${1:-}" == "--no-build" || "${CUTOUT_IOS_UI_TEST_SKIP_BUILD:-}" == "1" ]]; then
  mode="test-without-building"
  if [[ "${1:-}" == "--no-build" ]]; then
    shift
  fi
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
    DEVELOPMENT_TEAM="$(cutout_ios_development_team)"
  )
fi
xcodebuild_args+=("$@")

if [[ -n "${CUTOUT_IOS_UI_TEST_RUN_TIMEOUT:-}" ]]; then
  ui_test_run_timeout="$CUTOUT_IOS_UI_TEST_RUN_TIMEOUT"
elif [[ " ${xcodebuild_args[*]} " == *" -only-testing:"* ]]; then
  ui_test_run_timeout=150
else
  ui_test_run_timeout=1800
fi

if [[ "$mode" != "test-without-building" ]]; then
  /usr/bin/xcrun xcodebuild "${xcodebuild_args[@]}" build-for-testing
fi

if [[ "$mode" == "test" || "$mode" == "test-without-building" ]]; then
  timeout --foreground --kill-after=30 "$ui_test_run_timeout" \
    /usr/bin/xcrun xcodebuild \
    "${xcodebuild_args[@]}" \
    -parallel-testing-enabled NO \
    -test-timeouts-enabled YES \
    -default-test-execution-time-allowance 120 \
    -maximum-test-execution-time-allowance 120 \
    test-without-building
fi
