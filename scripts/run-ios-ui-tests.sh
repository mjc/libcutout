#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
  echo "CutoutApp iOS UI tests require Apple Silicon Darwin" >&2
  exit 1
fi

export DEVELOPER_DIR="${CUTOUT_DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}"
unset SDKROOT

mode="test"
if [[ "${1:-}" == "--build-only" ]]; then
  mode="build-for-testing"
  shift
fi
xcodebuild_args=("$@")

destination="${CUTOUT_IOS_TEST_DESTINATION:-${CUTOUT_IOS_SIMULATOR_DESTINATION:-platform=iOS Simulator,name=iPhone 17 Pro,OS=latest}}"
package_dir="${CUTOUT_IOS_APP_PACKAGE_DIR:-target/swift-sourcekit/CutoutMobile}"
project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
is_device_destination=0
derived_data="${CUTOUT_IOS_UI_TEST_DERIVED_DATA:-${CUTOUT_IOS_SIMULATOR_DERIVED_DATA:-$root/target/xcode-simulator-tests}}"
signing_args=()
provisioning_args=()

assert_rust_ffi_is_static() {
  local binary
  local binaries=()

  shopt -s nullglob
  binaries=(
    "$derived_data"/Build/Products/*-iphone*/CutoutApp.app/CutoutApp
    "$derived_data"/Build/Products/*-iphone*/CutoutApp.app/PlugIns/CutoutLiveActivityExtension.appex/CutoutLiveActivityExtension
  )
  shopt -u nullglob

  if (( ${#binaries[@]} == 0 )); then
    echo "No Cutout app executable was produced for Rust FFI linkage validation" >&2
    return 1
  fi

  for binary in "${binaries[@]}"; do
    if /usr/bin/otool -L "$binary" | /usr/bin/grep -q 'libcutout_mobile_ffi'; then
      echo "$binary dynamically links libcutout_mobile_ffi; link the static archive instead" >&2
      return 1
    fi
  done
}

if [[ "$destination" == platform=iOS,* ]]; then
  is_device_destination=1
  derived_data="${CUTOUT_IOS_DEVICE_DERIVED_DATA:-$root/target/xcode-device-tests}"
  signing_args=(
    CODE_SIGNING_ALLOWED=YES
    CODE_SIGNING_REQUIRED=YES
    CODE_SIGN_STYLE=Automatic
    CODE_SIGN_IDENTITY="Apple Development"
    DEVELOPMENT_TEAM="${CUTOUT_IOS_DEVELOPMENT_TEAM:-2RH32Y5HM5}"
  )
  provisioning_args=(-allowProvisioningUpdates)
fi

cutout_prepare_swift_package_workspace "$package_dir"
if (( is_device_destination )); then
  cutout_build_ios_rust_ffi
else
  cutout_build_ios_simulator_rust_ffi
fi
rm -rf "$derived_data"

(
  cd "$package_dir"
  env -u SDKROOT -u LD -u CC -u CXX -u AR -u RANLIB \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    xcodebuild \
    -project "$root/$project" \
    -scheme "$scheme" \
    -destination "$destination" \
    -derivedDataPath "$derived_data" \
    ONLY_ACTIVE_ARCH=YES \
    ARCHS=arm64 \
    "${signing_args[@]}" \
    "${provisioning_args[@]}" \
    "${xcodebuild_args[@]}" \
    build-for-testing

  if [[ "$mode" == "test" ]]; then
    env -u SDKROOT -u LD -u CC -u CXX -u AR -u RANLIB \
      PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
      xcodebuild \
      -project "$root/$project" \
      -scheme "$scheme" \
      -destination "$destination" \
      -derivedDataPath "$derived_data" \
      -parallel-testing-enabled NO \
      ONLY_ACTIVE_ARCH=YES \
      ARCHS=arm64 \
      "${xcodebuild_args[@]}" \
      test-without-building
  fi
)

assert_rust_ffi_is_static
