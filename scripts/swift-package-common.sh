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

cutout_macosx_sdk_path() {
  xcrun --sdk macosx --show-sdk-path
}

cutout_use_xcode_developer_dir() {
  export DEVELOPER_DIR="${CUTOUT_DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}"
  unset SDKROOT
}

cutout_prepare_swift_package_workspace() {
  local root package_dir
  root="$(cutout_repo_root)"
  package_dir="$1"
  CUTOUT_SWIFT_SOURCEKIT_PACKAGE_DIR="$package_dir" "$root/scripts/prepare-swift-sourcekit-workspace.sh" >/dev/null
}

cutout_iphoneos_sdk_path() {
  xcrun --sdk iphoneos --show-sdk-path
}

cutout_iphoneos_clang_path() {
  xcrun --sdk iphoneos --find clang
}

cutout_build_ios_rust_ffi() {
  local root
  root="$(cutout_repo_root)"

  env \
    SDKROOT="$(cutout_iphoneos_sdk_path)" \
    CC_aarch64_apple_ios="$(cutout_iphoneos_clang_path)" \
    CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="$(cutout_iphoneos_clang_path)" \
    cargo build -p cutout-mobile-ffi --target aarch64-apple-ios
}

cutout_xcode_auth_args() {
  if [[ -n "${CUTOUT_APPSTORE_AUTH_KEY_PATH:-}" ]]; then
    printf '%s\0' \
      -authenticationKeyPath "${CUTOUT_APPSTORE_AUTH_KEY_PATH}" \
      -authenticationKeyID "${CUTOUT_APPSTORE_AUTH_KEY_ID:-}" \
      -authenticationKeyIssuerID "${CUTOUT_APPSTORE_AUTH_KEY_ISSUER_ID:-}"
  fi
}

cutout_connected_ios_device_udid() {
  local device_json
  cutout_use_xcode_developer_dir

  device_json="$(mktemp "${TMPDIR:-/tmp}/cutout-devicectl.XXXXXX.json")"
  trap 'rm -f "$device_json"' RETURN

  xcrun devicectl --quiet list devices --json-output "$device_json" >/dev/null

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

cutout_build_ios_app_bundle() {
  local root package_dir project scheme destination derived_data rust_lib product
  root="$(cutout_repo_root)"
  package_dir="${CUTOUT_IOS_APP_PACKAGE_DIR:-target/swift-sourcekit/CutoutMobile}"
  project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
  destination="${CUTOUT_IOS_APP_BUILD_DESTINATION:-platform=macOS,id=00008103-001935121A8A001E}"
  derived_data="${CUTOUT_IOS_APP_DERIVED_DATA:-$root/target/xcode-designed-for-iphone}"
  rust_lib="$root/target/aarch64-apple-ios/debug/libcutout_mobile_ffi.a"
  product="$derived_data/Build/Products/Debug-iphoneos/CutoutApp.app"

  cutout_use_xcode_developer_dir

  cutout_prepare_swift_package_workspace "$package_dir"

  cutout_build_ios_rust_ffi

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

cutout_build_ios_device_app_bundle() {
  local root package_dir project scheme device_udid destination derived_data rust_lib product
  local development_team bundle_id
  root="$(cutout_repo_root)"
  package_dir="${CUTOUT_IOS_APP_PACKAGE_DIR:-target/swift-sourcekit/CutoutMobile}"
  project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
  device_udid="${CUTOUT_IOS_DEVICE_UDID:-$(cutout_connected_ios_device_udid)}"
  destination="${CUTOUT_IOS_DEVICE_DESTINATION:-id=$device_udid}"
  derived_data="${CUTOUT_IOS_DEVICE_DERIVED_DATA:-$root/target/xcode-device-signed}"
  rust_lib="$root/target/aarch64-apple-ios/debug/libcutout_mobile_ffi.a"
  product="$derived_data/Build/Products/Debug-iphoneos/CutoutApp.app"
  development_team="${CUTOUT_IOS_DEVELOPMENT_TEAM:-}"
  bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-}"

  cutout_use_xcode_developer_dir

  if [[ -z "$development_team" ]]; then
    echo "CUTOUT_IOS_DEVELOPMENT_TEAM is required for iPhone signing" >&2
    echo "Set CUTOUT_IOS_DEVELOPMENT_TEAM and optionally CUTOUT_IOS_APP_BUNDLE_ID, then rerun scripts/run-ios-app-on-phone.sh" >&2
    return 1
  fi

  cutout_prepare_swift_package_workspace "$package_dir"

  cutout_build_ios_rust_ffi

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

cutout_archive_ios_release_testing_app() {
  local root package_dir project scheme archive_path rust_lib
  local development_team bundle_id
  local -a auth_args=()

  root="$(cutout_repo_root)"
  package_dir="${CUTOUT_IOS_APP_PACKAGE_DIR:-target/swift-sourcekit/CutoutMobile}"
  project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
  archive_path="${CUTOUT_IOS_AD_HOC_ARCHIVE_PATH:-$root/target/xcode-ad-hoc/CutoutApp.xcarchive}"
  rust_lib="$root/target/aarch64-apple-ios/debug/libcutout_mobile_ffi.a"
  development_team="${CUTOUT_IOS_DEVELOPMENT_TEAM:-}"
  bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-}"

  cutout_use_xcode_developer_dir

  if [[ -z "$development_team" ]]; then
    echo "CUTOUT_IOS_DEVELOPMENT_TEAM is required for ad hoc export" >&2
    echo "Set CUTOUT_IOS_DEVELOPMENT_TEAM and optionally CUTOUT_IOS_APP_BUNDLE_ID, then rerun scripts/export-ios-ad-hoc.sh" >&2
    return 1
  fi

  if [[ -n "${CUTOUT_APPSTORE_AUTH_KEY_PATH:-}" ]]; then
    if [[ -z "${CUTOUT_APPSTORE_AUTH_KEY_ID:-}" || -z "${CUTOUT_APPSTORE_AUTH_KEY_ISSUER_ID:-}" ]]; then
      echo "CUTOUT_APPSTORE_AUTH_KEY_ID and CUTOUT_APPSTORE_AUTH_KEY_ISSUER_ID are required when CUTOUT_APPSTORE_AUTH_KEY_PATH is set" >&2
      return 1
    fi
    while IFS= read -r -d '' arg; do
      auth_args+=("$arg")
    done < <(cutout_xcode_auth_args)
  fi

  cutout_prepare_swift_package_workspace "$package_dir"
  cutout_build_ios_rust_ffi

  rm -rf "$archive_path"

  if ! (
    cd "$package_dir"
    env -u SDKROOT -u LD -u CC -u CXX -u AR -u RANLIB \
      PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
      xcodebuild \
      -project "$root/$project" \
      -scheme "$scheme" \
      -destination "generic/platform=iOS" \
      -archivePath "$archive_path" \
      -allowProvisioningUpdates \
      "${auth_args[@]}" \
      CODE_SIGNING_ALLOWED=YES \
      CODE_SIGNING_REQUIRED=YES \
      CODE_SIGN_STYLE=Automatic \
      DEVELOPMENT_TEAM="$development_team" \
      ${bundle_id:+PRODUCT_BUNDLE_IDENTIFIER="$bundle_id"} \
      OTHER_LDFLAGS="$rust_lib -liconv" \
      archive
  ) >&2; then
    rm -rf "$archive_path"
    return 1
  fi

  if [[ ! -d "$archive_path" ]]; then
    echo "expected archive not found: $archive_path" >&2
    return 1
  fi

  printf '%s\n' "$archive_path"
}

cutout_export_ios_ad_hoc_ipa() {
  local root archive_path export_path options_plist bundle_id ipa_path team_id
  local profile_specifier signing_certificate signing_style
  local -a auth_args=()

  root="$(cutout_repo_root)"
  archive_path="${CUTOUT_IOS_AD_HOC_ARCHIVE_PATH:-$root/target/xcode-ad-hoc/CutoutApp.xcarchive}"
  export_path="${CUTOUT_IOS_AD_HOC_EXPORT_PATH:-$root/target/xcode-ad-hoc/export}"
  archive_path="${CUTOUT_IOS_AD_HOC_ARCHIVE:-$archive_path}"
  bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-io.cutout.cutoutapp}"
  team_id="${CUTOUT_IOS_DEVELOPMENT_TEAM:-}"
  profile_specifier="${CUTOUT_IOS_AD_HOC_PROFILE:-}"
  signing_certificate="${CUTOUT_IOS_AD_HOC_CERTIFICATE:-Apple Distribution}"
  signing_style="${CUTOUT_IOS_AD_HOC_SIGNING_STYLE:-automatic}"

  if [[ -n "$profile_specifier" ]]; then
    signing_style="manual"
  fi

  if [[ ! -d "$archive_path" ]]; then
    archive_path="$(cutout_archive_ios_release_testing_app)"
  fi

  if [[ -n "${CUTOUT_APPSTORE_AUTH_KEY_PATH:-}" ]]; then
    while IFS= read -r -d '' arg; do
      auth_args+=("$arg")
    done < <(cutout_xcode_auth_args)
  fi

  rm -rf "$export_path"
  mkdir -p "$export_path"

  options_plist="$(mktemp "${TMPDIR:-/tmp}/cutout-ad-hoc-export.XXXXXX.plist")"
  trap 'rm -f "$options_plist"' RETURN

  python3 - "$options_plist" "$bundle_id" "$team_id" "$signing_style" "$profile_specifier" "$signing_certificate" <<'PY'
import plistlib
import sys

path, bundle_id, team_id, signing_style, profile_specifier, signing_certificate = sys.argv[1:7]
options = {
    "destination": "export",
    "method": "release-testing",
    "signingStyle": signing_style,
    "stripSwiftSymbols": True,
    "teamID": team_id,
    "thinning": "<none>",
}
if bundle_id:
    options["distributionBundleIdentifier"] = bundle_id
if signing_style == "manual":
    options["provisioningProfiles"] = {bundle_id: profile_specifier}
    options["signingCertificate"] = signing_certificate

with open(path, "wb") as fh:
    plistlib.dump(options, fh)
PY

  if ! env -u SDKROOT -u LD -u CC -u CXX -u AR -u RANLIB \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    xcodebuild \
    -exportArchive \
    -archivePath "$archive_path" \
    -exportPath "$export_path" \
    -exportOptionsPlist "$options_plist" \
    -allowProvisioningUpdates \
    "${auth_args[@]}" >&2; then
    return 1
  fi

  ipa_path="$(find "$export_path" -maxdepth 1 -name '*.ipa' -print -quit)"
  if [[ -z "$ipa_path" ]]; then
    echo "expected ipa not found in export path: $export_path" >&2
    return 1
  fi

  printf '%s\n' "$ipa_path"
}
