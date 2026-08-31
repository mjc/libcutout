#!/usr/bin/env bash
set -euo pipefail

cutout_repo_root() {
  cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd
}

cutout_host_os() {
  uname -s
}

cutout_use_xcode_developer_dir() {
  local default_developer_dir
  default_developer_dir="${1:-/Applications/Xcode-beta.app/Contents/Developer}"
  export DEVELOPER_DIR="${CUTOUT_DEVELOPER_DIR:-$default_developer_dir}"
  unset SDKROOT
}

cutout_ios_development_team() {
  printf '%s\n' "${CUTOUT_IOS_DEVELOPMENT_TEAM:-2RH32Y5HM5}"
}

cutout_swift_ffi_package_dir() {
  printf '%s\n' "$1/target/swift-ffi/CutoutMobileFFI"
}

cutout_replace_generated_directory() {
  local target parent name backup_root backup status
  target="$1"
  shift
  parent="$(dirname "$target")"
  name="$(basename "$target")"
  if [[ "$target" != /* || "$target" == "/" || "$name" == "." || "$name" == ".." || ! -d "$parent" ]]; then
    echo "refusing unsafe generated-directory target: $target" >&2
    return 2
  fi

  backup_root="$(mktemp -d "$parent/.$name.backup.XXXXXX")"
  backup="$backup_root/$name"
  if [[ -e "$target" ]]; then
    mv "$target" "$backup"
  fi

  if "$@"; then
    rm -rf -- "$backup_root"
    return 0
  else
    status=$?
  fi

  rm -rf -- "$target"
  if [[ -e "$backup" ]]; then
    mv "$backup" "$target"
  fi
  rmdir "$backup_root"
  return "$status"
}

cutout_swift_ffi_source_fingerprint() {
  local root
  root="$1"

  (
    cd "$root"
    {
      printf '%s\n' \
        Cargo.lock Cargo.toml rust-toolchain.toml flake.lock flake.nix \
        scripts/regenerate-swift-ffi.sh
      printf '%s\n' \
        crates/cutout-core/Cargo.toml \
        crates/cutout-mobile-ffi/Cargo.toml \
        crates/cutout-protocols/Cargo.toml \
        crates/cutout-ride-maps/Cargo.toml \
        crates/libcutout-persistence/Cargo.toml
      find \
        crates/cutout-core/src \
        crates/cutout-mobile-ffi/src \
        crates/cutout-protocols/src \
        crates/cutout-ride-maps/src \
        crates/libcutout-persistence/src \
        -type f -print
      [[ ! -d crates/cutout-protocols/registry ]] \
        || find crates/cutout-protocols/registry -type f -print
      [[ ! -f crates/cutout-protocols/build.rs ]] || printf '%s\n' crates/cutout-protocols/build.rs
      [[ ! -f crates/cutout-mobile-ffi/uniffi.toml ]] || printf '%s\n' crates/cutout-mobile-ffi/uniffi.toml
    } \
      | LC_ALL=C sort -u \
      | while IFS= read -r file; do
          printf '%s  ' "$file"
          sha256sum "$file"
        done
  ) | sha256sum | cut -d ' ' -f 1
}

cutout_require_current_swift_ffi() {
  local root stamp expected actual
  root="$1"
  stamp="$(cutout_swift_ffi_package_dir "$root")/.cutout-source.sha256"

  if [[ -f "$stamp" ]]; then
    expected="$(tr -d '[:space:]' <"$stamp")"
  else
    expected=""
  fi
  actual="$(cutout_swift_ffi_source_fingerprint "$root")"
  if [[ "$expected" == "$actual" ]]; then
    return
  fi

  echo "Swift FFI artifact is missing or stale for the current Rust sources." >&2
  echo "Run: nix develop -c ./scripts/regenerate-swift-ffi.sh" >&2
  return 1
}

cutout_validate_swift_ffi_build_input() {
  local root package
  local -a required
  root="$1"
  package="$(cutout_swift_ffi_package_dir "$root")"
  required=(
    "$package/Package.swift"
    "$package/Sources/CutoutMobileFFI/cutout_mobile_ffi.swift"
    "$package/cutout_mobile_ffiFFI.xcframework/Info.plist"
    "$package/cutout_mobile_ffiFFI.xcframework/ios-arm64/libcutout_mobile_ffi.a"
    "$package/cutout_mobile_ffiFFI.xcframework/ios-arm64/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"
    "$package/cutout_mobile_ffiFFI.xcframework/ios-arm64/Headers/cutout_mobile_ffiFFI/module.modulemap"
    "$package/cutout_mobile_ffiFFI.xcframework/ios-arm64_x86_64-simulator/libcutout_mobile_ffi.a"
    "$package/cutout_mobile_ffiFFI.xcframework/ios-arm64_x86_64-simulator/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"
    "$package/cutout_mobile_ffiFFI.xcframework/ios-arm64_x86_64-simulator/Headers/cutout_mobile_ffiFFI/module.modulemap"
    "$package/cutout_mobile_ffiFFI.xcframework/macos-arm64_x86_64/libcutout_mobile_ffi.a"
    "$package/cutout_mobile_ffiFFI.xcframework/macos-arm64_x86_64/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"
    "$package/cutout_mobile_ffiFFI.xcframework/macos-arm64_x86_64/Headers/cutout_mobile_ffiFFI/module.modulemap"
  )

  if ! cutout_require_current_swift_ffi "$root"; then
    return 1
  fi
  for input in "${required[@]}"; do
    if [[ ! -f "$input" ]]; then
      echo "missing Swift FFI build input: $input" >&2
      echo "Run: nix develop -c ./scripts/regenerate-swift-ffi.sh" >&2
      return 1
    fi
  done

  if [[ "$(uname -s)" == Darwin ]]; then
    if ! /usr/bin/lipo "${required[3]}" -verify_arch arm64 >/dev/null 2>&1 \
      || ! /usr/bin/lipo "${required[6]}" -verify_arch arm64 >/dev/null 2>&1 \
      || ! /usr/bin/lipo "${required[6]}" -verify_arch x86_64 >/dev/null 2>&1 \
      || ! /usr/bin/lipo "${required[9]}" -verify_arch arm64 >/dev/null 2>&1 \
      || ! /usr/bin/lipo "${required[9]}" -verify_arch x86_64 >/dev/null 2>&1
    then
      echo "Swift FFI XCFramework contains a missing or wrong-architecture slice." >&2
      echo "Run: nix develop -c ./scripts/regenerate-swift-ffi.sh" >&2
      return 1
    fi
  fi
}

cutout_require_swift_ffi_build_input() {
  cutout_validate_swift_ffi_build_input "$@"
}

cutout_ensure_swift_ffi_build_input() {
  local root generator
  root="$1"
  generator="${2:-$root/scripts/regenerate-swift-ffi.sh}"
  if cutout_validate_swift_ffi_build_input "$root" 2>/dev/null; then
    return 0
  fi
  "$generator"
  cutout_validate_swift_ffi_build_input "$root"
}

cutout_create_ios_ui_test_result_bundle() {
  local derived_data result_directory
  derived_data="$1"

  mkdir -p "$derived_data/TestResults"
  result_directory="$(mktemp -d "$derived_data/TestResults/run.XXXXXX")"
  printf '%s\n' "$result_directory/Result.xcresult"
}

cutout_require_complete_ios_ui_test_summary() {
  local summary_json test_count skipped_count
  summary_json="$1"
  test_count="$(jq -er '.totalTestCount' <<<"$summary_json")" || {
    echo "iOS UI test result has no total test count" >&2
    return 1
  }
  skipped_count="$(jq -er '.skippedTests // 0' <<<"$summary_json")" || {
    echo "iOS UI test result has no skipped test count" >&2
    return 1
  }
  if ! [[ "$test_count" =~ ^[1-9][0-9]*$ ]]; then
    echo "iOS UI test completed without executing a test; refusing to report a green result" >&2
    return 1
  fi
  if ! [[ "$skipped_count" =~ ^[0-9]+$ ]] || [[ "$skipped_count" -ne 0 ]]; then
    echo "iOS UI test result skipped $skipped_count tests; refusing to report complete coverage" >&2
    return 1
  fi
  printf '%s\n' "$test_count"
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
    if hardware.get("reality") != "physical":
        continue
    if state.get("bootState") != "booted":
        continue

    udid = properties.get("hardware", {}).get("udid")
    if udid:
        print(udid)
        raise SystemExit(0)

raise SystemExit("no connected booted physical iOS device found")
PY
}

cutout_build_ios_app_bundle() {
  local root project scheme destination derived_data product configuration
  root="$(cutout_repo_root)"
  project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
  destination="${CUTOUT_IOS_APP_BUILD_DESTINATION:-platform=macOS,id=00008103-001935121A8A001E}"
  derived_data="${CUTOUT_IOS_APP_DERIVED_DATA:-$root/target/xcode-designed-for-iphone}"
  configuration="${1:-Debug}"
  case "$configuration" in
    Debug|Release) ;;
    *)
      echo "iOS app build configuration must be Debug or Release" >&2
      return 2
      ;;
  esac
  product="$derived_data/Build/Products/$configuration-iphoneos/CutoutApp.app"

  cutout_use_xcode_developer_dir
  cutout_ensure_swift_ffi_build_input "$root"

  rm -rf "$product"

  if ! /usr/bin/xcrun xcodebuild \
      -project "$root/$project" \
      -scheme "$scheme" \
      -destination "$destination" \
      -derivedDataPath "$derived_data" \
      -configuration "$configuration" \
      build >&2; then
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
  local root project scheme device_udid destination derived_data product
  local development_team bundle_id
  root="$(cutout_repo_root)"
  project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
  device_udid="${CUTOUT_IOS_DEVICE_UDID:-$(cutout_connected_ios_device_udid)}"
  destination="${CUTOUT_IOS_DEVICE_DESTINATION:-platform=iOS,id=$device_udid}"
  derived_data="${CUTOUT_IOS_DEVICE_DERIVED_DATA:-$root/target/xcode-device-signed}"
  product="$derived_data/Build/Products/Debug-iphoneos/CutoutApp.app"
  development_team="$(cutout_ios_development_team)"
  bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-}"

  cutout_use_xcode_developer_dir
  cutout_ensure_swift_ffi_build_input "$root"

  rm -rf "$product"

  if ! /usr/bin/xcrun xcodebuild \
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
      build >&2; then
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
  local root project scheme archive_path
  local development_team bundle_id
  local -a auth_args=()

  root="$(cutout_repo_root)"
  cutout_ensure_swift_ffi_build_input "$root"
  project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
  archive_path="${CUTOUT_IOS_AD_HOC_ARCHIVE_PATH:-$root/target/xcode-ad-hoc/CutoutApp.xcarchive}"
  development_team="$(cutout_ios_development_team)"
  bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-}"

  cutout_use_xcode_developer_dir

  if [[ -n "${CUTOUT_APPSTORE_AUTH_KEY_PATH:-}" ]]; then
    if [[ -z "${CUTOUT_APPSTORE_AUTH_KEY_ID:-}" || -z "${CUTOUT_APPSTORE_AUTH_KEY_ISSUER_ID:-}" ]]; then
      echo "CUTOUT_APPSTORE_AUTH_KEY_ID and CUTOUT_APPSTORE_AUTH_KEY_ISSUER_ID are required when CUTOUT_APPSTORE_AUTH_KEY_PATH is set" >&2
      return 1
    fi
    while IFS= read -r -d '' arg; do
      auth_args+=("$arg")
    done < <(cutout_xcode_auth_args)
  fi

  rm -rf "$archive_path"

  if ! /usr/bin/xcrun xcodebuild \
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
      archive >&2; then
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

  if ! /usr/bin/xcrun xcodebuild \
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
