#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$root/scripts/swift-package-common.sh"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-build-scripts.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

assert_equal() {
  [[ "$1" == "$2" ]] || { printf 'expected <%s>, got <%s>\n' "$1" "$2" >&2; exit 1; }
}

export CUTOUT_IOS_APP_DERIVED_DATA="$tmp/derived data"
export CUTOUT_IOS_AD_HOC_ARCHIVE_PATH="$tmp/default archive.xcarchive"
export CUTOUT_IOS_AD_HOC_EXPORT_PATH="$tmp/export"
export CUTOUT_SPOTIFY_CLIENT_ID=test-client
unset CUTOUT_IOS_AD_HOC_ARCHIVE CUTOUT_APPSTORE_AUTH_KEY_PATH
product="$CUTOUT_IOS_APP_DERIVED_DATA/Build/Products/Debug-iphoneos/CutoutApp.app"
mkdir -p "$product" "$CUTOUT_IOS_AD_HOC_ARCHIVE_PATH" "$tmp/explicit archive.xcarchive"
touch "$product/keep" "$CUTOUT_IOS_AD_HOC_ARCHIVE_PATH/keep"
build_status=0

# Exercise the real build/export helpers with only external build and bundle
# validation replaced. No Rust, Swift, signing, or device operations run.
cargo() {
  assert_equal cutout "$1"
  if [[ "$2" == xcodebuild ]]; then
    assert_equal -- "$3"
    printf '%s\n' "${@: -1}" >>"$tmp/events"
    return "$build_status"
  fi
  assert_equal 'ios verify-app' "$2 $3"
}
cutout_verify_embedded_spotify_client_id() { :; }
function /usr/bin/xcrun {
  assert_equal 'xcodebuild -exportArchive -archivePath' "$1 $2 $3"
  assert_equal "$expected_archive" "$4"
  assert_equal -exportPath "$5"
  assert_equal "$CUTOUT_IOS_AD_HOC_EXPORT_PATH" "$6"
  printf 'export\n' >>"$tmp/events"
  touch "$6/CutoutApp.ipa"
}

: >"$tmp/events"
assert_equal "$product" "$(cutout_build_ios_app_bundle)"
assert_equal build "$(<"$tmp/events")"
[[ -f "$product/keep" ]]
build_status=42
if cutout_build_ios_app_bundle >"$tmp/result"; then
  echo 'failed build returned an existing product' >&2
  exit 1
fi
[[ ! -s "$tmp/result" && -f "$product/keep" ]]

build_status=0
for override in CUTOUT_IOS_APP_BUILD_DESTINATION CUTOUT_IOS_APP_SCHEME CUTOUT_IOS_APP_PROJECT; do
  case "$override" in
    CUTOUT_IOS_APP_BUILD_DESTINATION) value='platform=iOS Simulator,id=simulator' ;;
    CUTOUT_IOS_APP_SCHEME) value=OtherApp ;;
    CUTOUT_IOS_APP_PROJECT) value=OtherApp.xcodeproj ;;
  esac
  export "$override=$value"
  : >"$tmp/events"
  if cutout_build_ios_app_bundle >"$tmp/result" 2>"$tmp/error"; then
    echo "unsupported $override returned an existing product" >&2
    exit 1
  fi
  [[ ! -s "$tmp/result" && -s "$tmp/error" && ! -s "$tmp/events" ]]
  unset "$override"
done
export CUTOUT_IOS_APP_BUILD_DESTINATION='platform=macOS'
: >"$tmp/events"
assert_equal "$product" "$(cutout_build_ios_app_bundle)"
assert_equal build "$(<"$tmp/events")"
unset CUTOUT_IOS_APP_BUILD_DESTINATION

expected_archive="$CUTOUT_IOS_AD_HOC_ARCHIVE_PATH"
: >"$tmp/events"
assert_equal "$CUTOUT_IOS_AD_HOC_EXPORT_PATH/CutoutApp.ipa" "$(cutout_export_ios_ad_hoc_ipa)"
assert_equal $'archive\nexport' "$(<"$tmp/events")"
[[ -f "$CUTOUT_IOS_AD_HOC_ARCHIVE_PATH/keep" ]]

build_status=42
: >"$tmp/events"
if cutout_export_ios_ad_hoc_ipa >"$tmp/result"; then
  echo 'failed archive build fell through to export' >&2
  exit 1
fi
assert_equal archive "$(<"$tmp/events")"
[[ ! -s "$tmp/result" && -f "$CUTOUT_IOS_AD_HOC_ARCHIVE_PATH/keep" ]]

# Explicit reuse succeeds even when a new build would fail.
export CUTOUT_IOS_AD_HOC_ARCHIVE="$tmp/explicit archive.xcarchive"
expected_archive="$CUTOUT_IOS_AD_HOC_ARCHIVE"
: >"$tmp/events"
assert_equal "$CUTOUT_IOS_AD_HOC_EXPORT_PATH/CutoutApp.ipa" "$(cutout_export_ios_ad_hoc_ipa)"
assert_equal export "$(<"$tmp/events")"

export CUTOUT_IOS_AD_HOC_ARCHIVE="$tmp/missing.xcarchive"
: >"$tmp/events"
if cutout_export_ios_ad_hoc_ipa >"$tmp/result" 2>"$tmp/error"; then
  echo 'missing explicit archive was accepted' >&2
  exit 1
fi
[[ ! -s "$tmp/result" && ! -s "$tmp/events" ]]
grep -q 'CUTOUT_IOS_AD_HOC_ARCHIVE does not exist' "$tmp/error"
printf 'Build failure and archive selection checks passed\n'
