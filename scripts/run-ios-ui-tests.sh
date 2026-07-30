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
clean=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --clean)
      clean=true
      shift
      ;;
    --build-only)
      if [[ "$mode" != "test" ]]; then
        echo "--build-only and --no-build cannot be combined" >&2
        exit 1
      fi
      mode="build-for-testing"
      shift
      ;;
    --no-build)
      if [[ "$mode" != "test" ]]; then
        echo "--build-only and --no-build cannot be combined" >&2
        exit 1
      fi
      mode="test-without-building"
      shift
      ;;
    *) break ;;
  esac
done

if [[ "${CUTOUT_IOS_UI_TEST_SKIP_BUILD:-}" == "1" && "$mode" == "test" ]]; then
  mode="test-without-building"
fi
destination="${CUTOUT_IOS_TEST_DESTINATION:-${CUTOUT_IOS_SIMULATOR_DESTINATION:-platform=iOS Simulator,name=Cutout iPhone 15 iOS 27,OS=latest}}"
project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
derived_data="$(cutout_ios_ui_test_derived_data "$root" "$destination")"
lock_directory="$derived_data/.run-ios-ui-tests.lock"
result_marker=""

mkdir -p "$derived_data"
if ! mkdir "$lock_directory" 2>/dev/null; then
  echo "iOS UI tests are already running for $derived_data" >&2
  exit 1
fi
cleanup() {
  [[ -z "$result_marker" ]] || rm -f "$result_marker"
  rmdir "$lock_directory"
}
trap cleanup EXIT

xcodebuild_args=(
  -project "$root/$project"
  -scheme "$scheme"
  -destination "$destination"
  -derivedDataPath "$derived_data"
)

if [[ "$destination" == platform=iOS,* ]]; then
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

if [[ "$clean" == true ]]; then
  /usr/bin/xcrun xcodebuild "${xcodebuild_args[@]}" clean
fi

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
  result_marker="$(mktemp "$derived_data/.run-ios-ui-tests-result.XXXXXX")"
  test_status=0
  if timeout --foreground --kill-after=30 "$ui_test_run_timeout" \
    /usr/bin/xcrun xcodebuild \
    "${xcodebuild_args[@]}" \
    -parallel-testing-enabled NO \
    -collect-test-diagnostics never \
    -test-timeouts-enabled YES \
    -default-test-execution-time-allowance 120 \
    -maximum-test-execution-time-allowance 120 \
    test-without-building
  then
    :
  else
    test_status=$?
  fi

  latest_result_bundle="$(cutout_latest_complete_xcresult_since "$derived_data/Logs/Test" "$result_marker")"
  if [[ -z "$latest_result_bundle" ]]; then
    echo "iOS UI test failed without a complete .xcresult; do not treat this run as product evidence" >&2
    exit 1
  fi

  if [[ "$test_status" -eq 0 ]]; then
    test_count="$(
      /usr/bin/xcrun xcresulttool get test-results summary --path "$latest_result_bundle" \
        | /usr/bin/plutil -extract totalTestCount raw -
    )"
    if ! [[ "$test_count" =~ ^[1-9][0-9]*$ ]]; then
      echo "iOS UI test completed without executing a test; refusing to report a green result" >&2
      exit 1
    fi
  fi

  exit "$test_status"
fi
