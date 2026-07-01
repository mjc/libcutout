#!/usr/bin/env bash
set -euo pipefail

cutout_repo_root() {
  cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd
}

cutout_host_os() {
  uname -s
}

cutout_swift_runtime_command() {
  case "$(cutout_host_os)" in
    Darwin)
      printf '%s\n' "env -u SDKROOT -u DEVELOPER_DIR swift"
      ;;
    Linux)
      printf '%s\n' "swift"
      ;;
    *)
      echo "unsupported host OS for Swift package tooling: $(cutout_host_os)" >&2
      return 1
      ;;
  esac
}

cutout_prepare_swift_package_workspace() {
  local root package_dir
  root="$(cutout_repo_root)"
  package_dir="$1"
  CUTOUT_SWIFT_SOURCEKIT_PACKAGE_DIR="$package_dir" "$root/scripts/prepare-swift-sourcekit-workspace.sh" >/dev/null
}

cutout_connected_ios_device_udid() {
  local device_json
  device_json="$(mktemp "${TMPDIR:-/tmp}/cutout-devicectl.XXXXXX.json")"
  trap 'rm -f "$device_json"' RETURN

  xcrun devicectl list devices --json-output "$device_json" >/dev/null

  python3 - "$device_json" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)

devices = data.get("result", {}).get("devices", [])
for device in devices:
    hardware = device.get("hardwareProperties", {})
    properties = device.get("properties", {})
    state = properties.get("state", {})

    if hardware.get("platform") != "iOS":
        continue
    if state.get("bootState") != "booted":
        continue

    udid = properties.get("hardware", {}).get("udid")
    if udid:
        print(udid)
        raise SystemExit(0)

raise SystemExit("no connected booted iOS device found")
PY
}

cutout_build_ios_speed_app_bundle() {
  local root package_dir project scheme destination derived_data rust_lib product
  root="$(cutout_repo_root)"
  package_dir="${CUTOUT_IOS_SPEED_PACKAGE_DIR:-target/swift-sourcekit/CutoutMobile}"
  project="${CUTOUT_IOS_SPEED_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_SPEED_SCHEME:-CutoutApp}"
  destination="${CUTOUT_IOS_SPEED_BUILD_DESTINATION:-platform=macOS,arch=arm64,variant=Designed for iPad,name=My Mac}"
  derived_data="${CUTOUT_IOS_SPEED_DERIVED_DATA:-$root/target/xcode-designed-for-ipad}"
  rust_lib="$root/target/aarch64-apple-ios/debug/libcutout_mobile_ffi.a"
  product="$derived_data/Build/Products/Debug-iphoneos/CutoutApp.app"

  cutout_prepare_swift_package_workspace "$package_dir"

  env -u SDKROOT \
    CC_aarch64_apple_ios="$(xcrun --sdk iphoneos --find clang)" \
    CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="$(xcrun --sdk iphoneos --find clang)" \
    cargo build -p cutout-mobile-ffi --target aarch64-apple-ios

  rm -rf "$product"

  if ! (
    cd "$package_dir"
    env -u SDKROOT -u LD -u CC -u CXX -u AR -u RANLIB \
      PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
      xcodebuild \
      -project "$root/$project" \
      -scheme "$scheme" \
      -destination "$destination" \
      -derivedDataPath "$derived_data" \
      OTHER_LDFLAGS="$rust_lib -liconv" \
      build
  ) >&2; then
    rm -rf "$product"
    return 1
  fi

  if [[ ! -d "$product" ]]; then
    echo "expected installed product not found: $product" >&2
    return 1
  fi

  printf '%s\n' "$product"
}

cutout_build_ios_device_speed_app_bundle() {
  local root package_dir project scheme device_udid destination derived_data rust_lib product
  local development_team bundle_id
  root="$(cutout_repo_root)"
  package_dir="${CUTOUT_IOS_SPEED_PACKAGE_DIR:-target/swift-sourcekit/CutoutMobile}"
  project="${CUTOUT_IOS_SPEED_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_SPEED_SCHEME:-CutoutApp}"
  device_udid="${CUTOUT_IOS_DEVICE_UDID:-$(cutout_connected_ios_device_udid)}"
  destination="${CUTOUT_IOS_DEVICE_DESTINATION:-id=$device_udid}"
  derived_data="${CUTOUT_IOS_DEVICE_DERIVED_DATA:-$root/target/xcode-device-signed}"
  rust_lib="$root/target/aarch64-apple-ios/debug/libcutout_mobile_ffi.a"
  product="$derived_data/Build/Products/Debug-iphoneos/CutoutApp.app"
  development_team="${CUTOUT_IOS_DEVELOPMENT_TEAM:-}"
  bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-}"

  cutout_prepare_swift_package_workspace "$package_dir"

  env -u SDKROOT \
    CC_aarch64_apple_ios="$(xcrun --sdk iphoneos --find clang)" \
    CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="$(xcrun --sdk iphoneos --find clang)" \
    cargo build -p cutout-mobile-ffi --target aarch64-apple-ios

  rm -rf "$product"

  if ! (
    cd "$package_dir"
    env -u SDKROOT -u LD -u CC -u CXX -u AR -u RANLIB \
      PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
      xcodebuild \
      -project "$root/$project" \
      -scheme "$scheme" \
      -destination "$destination" \
      -derivedDataPath "$derived_data" \
      -allowProvisioningUpdates \
      CODE_SIGNING_ALLOWED=YES \
      CODE_SIGNING_REQUIRED=YES \
      CODE_SIGN_STYLE=Automatic \
      CODE_SIGN_IDENTITY="Apple Development" \
      ${development_team:+DEVELOPMENT_TEAM="$development_team"} \
      ${bundle_id:+PRODUCT_BUNDLE_IDENTIFIER="$bundle_id"} \
      OTHER_LDFLAGS="$rust_lib -liconv" \
      build
  ) >&2; then
    rm -rf "$product"
    return 1
  fi

  if [[ ! -d "$product" ]]; then
    echo "expected installed product not found: $product" >&2
    return 1
  fi

  printf '%s\n' "$product"
}

cutout_ios_app_bundle_identifier() {
  local product
  product="$1"
  /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$product/Info.plist"
}
