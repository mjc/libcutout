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

cutout_spotify_client_id() {
  local config_file client_id
  if [[ -n "${CUTOUT_SPOTIFY_CLIENT_ID:-}" ]]; then
    printf '%s\n' "$CUTOUT_SPOTIFY_CLIENT_ID"
    return
  fi
  if [[ -n "${SPOTIFY_CLIENT_ID:-}" ]]; then
    printf '%s\n' "$SPOTIFY_CLIENT_ID"
    return
  fi

  config_file="${CUTOUT_SPOTIFY_CLIENT_ID_FILE:-${XDG_CONFIG_HOME:-$HOME/.config}/libcutout/spotify-client-id}"
  if [[ -r "$config_file" ]]; then
    IFS= read -r client_id <"$config_file" || true
    printf '%s\n' "$client_id"
  fi
}

cutout_require_spotify_client_id() {
  local client_id
  client_id="$(cutout_spotify_client_id)"
  if [[ -z "$client_id" ]]; then
    echo "Spotify client ID is required for an installable iOS build" >&2
    echo "Set CUTOUT_SPOTIFY_CLIENT_ID or write it to ~/.config/libcutout/spotify-client-id" >&2
    return 2
  fi
  printf '%s\n' "$client_id"
}

cutout_verify_embedded_spotify_client_id() {
  local product expected actual
  product="$1"
  expected="$2"
  actual="$(/usr/libexec/PlistBuddy -c 'Print :SpotifyClientID' "$product/Info.plist" 2>/dev/null || true)"
  if [[ -z "$expected" || "$actual" != "$expected" ]]; then
    echo "built app does not contain the configured Spotify client ID: $product" >&2
    return 1
  fi
}

cutout_ensure_swift_ffi_build_input() {
  (cd "$1" && cargo cutout swift-ffi)
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

cutout_build_ios_app_bundle() {
  local root project scheme destination derived_data product configuration spotify_client_id
  root="$(cutout_repo_root)"
  project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
  destination="${CUTOUT_IOS_APP_BUILD_DESTINATION:-platform=macOS,id=00008103-001935121A8A001E}"
  derived_data="${CUTOUT_IOS_APP_DERIVED_DATA:-$root/target/xcode-designed-for-iphone}"
  configuration="${1:-Debug}"
  spotify_client_id="$(cutout_spotify_client_id)"
  case "$configuration" in
    Debug|Release) ;;
    *)
      echo "iOS app build configuration must be Debug or Release" >&2
      return 2
      ;;
  esac
  product="$derived_data/Build/Products/$configuration-iphoneos/CutoutApp.app"

  cutout_use_xcode_developer_dir
  cutout_ensure_swift_ffi_build_input "$root" || return

  rm -rf "$product"

  if ! /usr/bin/xcrun xcodebuild \
      -project "$root/$project" \
      -scheme "$scheme" \
      -destination "$destination" \
      -derivedDataPath "$derived_data" \
      -configuration "$configuration" \
      ${spotify_client_id:+SPOTIFY_CLIENT_ID="$spotify_client_id"} \
      build >&2; then
    rm -rf "$product"
    return 1
  fi

  if [[ ! -d "$product" ]]; then
    echo "expected installed product not found: $product" >&2
    return 1
  fi
  (cd "$root" && cargo cutout ios verify-app "$product") || return
  if [[ -n "$spotify_client_id" ]]; then
    cutout_verify_embedded_spotify_client_id "$product" "$spotify_client_id" || return
  fi

  printf '%s\n' "$product"
}

cutout_archive_ios_release_testing_app() {
  local root project scheme archive_path
  local development_team bundle_id spotify_client_id
  local -a auth_args=()

  root="$(cutout_repo_root)"
  cutout_ensure_swift_ffi_build_input "$root" || return
  project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
  scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
  archive_path="${CUTOUT_IOS_AD_HOC_ARCHIVE_PATH:-$root/target/xcode-ad-hoc/CutoutApp.xcarchive}"
  development_team="$(cutout_ios_development_team)"
  bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-}"
  spotify_client_id="$(cutout_require_spotify_client_id)"

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
      SPOTIFY_CLIENT_ID="$spotify_client_id" \
      archive >&2; then
    rm -rf "$archive_path"
    return 1
  fi

  if [[ ! -d "$archive_path" ]]; then
    echo "expected archive not found: $archive_path" >&2
    return 1
  fi
  (cd "$root" && cargo cutout ios verify-app "$archive_path/Products/Applications/CutoutApp.app") || return
  cutout_verify_embedded_spotify_client_id \
    "$archive_path/Products/Applications/CutoutApp.app" \
    "$spotify_client_id" || return

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
