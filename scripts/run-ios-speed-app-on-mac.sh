#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "CutoutMobileSpeedApp iOS-on-Mac requires Darwin" >&2
  exit 1
fi

if [[ "$(uname -m)" != arm64 ]]; then
  echo "CutoutMobileSpeedApp iOS-on-Mac requires Apple Silicon" >&2
  exit 1
fi

export DEVELOPER_DIR="${CUTOUT_DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}"
unset SDKROOT

destination="${CUTOUT_IOS_ON_MAC_DESTINATION:-platform=macOS,arch=arm64,variant=Designed for iPad,name=My Mac}"
package_dir="${CUTOUT_IOS_ON_MAC_PACKAGE_DIR:-target/swift-sourcekit/CutoutMobile}"
derived_data="${CUTOUT_IOS_ON_MAC_DERIVED_DATA:-$root/target/xcode-designed-for-ipad}"
rust_lib="$root/target/aarch64-apple-ios/debug/libcutout_mobile_ffi.a"

CUTOUT_SWIFT_SOURCEKIT_PACKAGE_DIR="$package_dir" ./scripts/prepare-swift-sourcekit-workspace.sh >/dev/null

env -u SDKROOT \
  CC_aarch64_apple_ios="$(xcrun --sdk iphoneos --find clang)" \
  CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="$(xcrun --sdk iphoneos --find clang)" \
  cargo build -p cutout-mobile-ffi --target aarch64-apple-ios

(
  cd "$package_dir"
  env -u SDKROOT -u LD -u CC -u CXX -u AR -u RANLIB \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    xcodebuild \
    -scheme CutoutMobileSpeedApp \
    -destination "$destination" \
    -derivedDataPath "$derived_data" \
    OTHER_LDFLAGS="$rust_lib -liconv" \
    build
)

product="$derived_data/Build/Products/Debug-iphoneos/CutoutMobileSpeedApp"
if [[ ! -x "$product" ]]; then
  echo "expected installed product not found: $product" >&2
  exit 1
fi

echo "ios_speed_app_product=$product"
echo "ios_speed_app_note=SwiftPM/Xcode currently produces a bare iOS executable; foreground launch still needs an app bundle or Xcode launch automation"
