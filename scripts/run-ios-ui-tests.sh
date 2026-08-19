#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: scripts/run-ios-ui-tests.sh [--clean] [--build-only] [--smoke] [--verbose] [--enumerate-tests output.json] [--appearance light|dark] [--increase-contrast enabled|disabled] [--content-size size] [xcodebuild options]"
  echo "  --smoke    Run the seven production-root transition and accessibility checks."
  echo "  --verbose  Show full xcodebuild output instead of the bounded default."
  echo "  --enumerate-tests writes Xcode's compiled test graph without executing tests."
  echo "  --appearance, --increase-contrast, and --content-size apply and verify simulator settings for this run, then restore them."
  echo "  Test runs require --smoke or at least one -only-testing selector; an unqualified run mixes incompatible settings cells."
}

if [[ "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"
cutout_require_swift_ffi_build_input "$root"

if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
  echo "CutoutApp iOS UI tests require Apple Silicon Darwin" >&2
  exit 1
fi

cutout_use_xcode_developer_dir

mode="test"
enumeration_output=""
clean=false
smoke=false
quiet=true
requested_appearance="${CUTOUT_IOS_SIMULATOR_APPEARANCE:-}"
requested_increase_contrast="${CUTOUT_IOS_SIMULATOR_INCREASE_CONTRAST:-}"
requested_content_size="${CUTOUT_IOS_SIMULATOR_CONTENT_SIZE:-}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --clean)
      clean=true
      shift
      ;;
    --build-only)
      mode="build-for-testing"
      shift
      ;;
    --smoke)
      smoke=true
      shift
      ;;
    --verbose)
      quiet=false
      shift
      ;;
    --enumerate-tests)
      [[ $# -ge 2 ]] || { echo "--enumerate-tests requires an output JSON path" >&2; exit 2; }
      mode="enumerate-tests"
      enumeration_output="$2"
      shift 2
      ;;
    --appearance)
      [[ $# -ge 2 ]] || { echo "--appearance requires light or dark" >&2; exit 2; }
      requested_appearance="$2"
      shift 2
      ;;
    --increase-contrast)
      [[ $# -ge 2 ]] || { echo "--increase-contrast requires enabled or disabled" >&2; exit 2; }
      requested_increase_contrast="$2"
      shift 2
      ;;
    --content-size)
      [[ $# -ge 2 ]] || { echo "--content-size requires a simctl content-size value" >&2; exit 2; }
      requested_content_size="$2"
      shift 2
      ;;
    --no-build)
      echo "--no-build is unsupported; an incremental test build is required" >&2
      exit 2
      ;;
    *) break ;;
  esac
done

if [[ "${CUTOUT_IOS_UI_TEST_SKIP_BUILD:-}" == "1" ]]; then
  echo "CUTOUT_IOS_UI_TEST_SKIP_BUILD is unsupported; an incremental test build is required" >&2
  exit 2
fi

selected_tests="$*"
has_selected_test=false
for argument in "$@"; do
  if [[ "$argument" == -only-testing || "$argument" == -only-testing:* ]]; then
    has_selected_test=true
    break
  fi
done
if [[ "$mode" == test && "$smoke" == false && "$has_selected_test" == false ]]; then
  echo "iOS UI test runs require --smoke or at least one -only-testing selector; an unqualified run mixes incompatible simulator settings" >&2
  exit 2
fi
if [[ "$selected_tests" == *InLightAppearance* && "$selected_tests" == *InDarkAppearance* ]]; then
  echo "Light and Dark UI tests require separate runner invocations" >&2
  exit 2
fi
if [[ "$selected_tests" == *InLightAppearance* ]]; then
  [[ -z "$requested_appearance" || "$requested_appearance" == light ]] || {
    echo "the selected Light test conflicts with --appearance $requested_appearance" >&2
    exit 2
  }
  requested_appearance=light
elif [[ "$selected_tests" == *InDarkAppearance* ]]; then
  [[ -z "$requested_appearance" || "$requested_appearance" == dark ]] || {
    echo "the selected Dark test conflicts with --appearance $requested_appearance" >&2
    exit 2
  }
  requested_appearance=dark
fi

if [[ "$selected_tests" == *AccessibilityDynamicType* || "$selected_tests" == *RightToLeft* || "$smoke" == true ]]; then
  [[ -z "$requested_content_size" || "$requested_content_size" == accessibility-extra-extra-extra-large ]] || {
    echo "the selected Accessibility Dynamic Type test conflicts with --content-size $requested_content_size" >&2
    exit 2
  }
  requested_content_size=accessibility-extra-extra-extra-large
elif [[ "$selected_tests" == *ExtraExtraExtraLarge* ]]; then
  [[ -z "$requested_content_size" || "$requested_content_size" == extra-extra-extra-large ]] || {
    echo "the selected XXXL test conflicts with --content-size $requested_content_size" >&2
    exit 2
  }
  requested_content_size=extra-extra-extra-large
fi

if [[ "$selected_tests" == *IncreasedContrast* ]]; then
  [[ -z "$requested_increase_contrast" || "$requested_increase_contrast" == enabled ]] || {
    echo "the selected Increased Contrast test conflicts with --increase-contrast $requested_increase_contrast" >&2
    exit 2
  }
  requested_increase_contrast=enabled
fi

destination="${CUTOUT_IOS_TEST_DESTINATION:-${CUTOUT_IOS_SIMULATOR_DESTINATION:-platform=iOS Simulator,name=Cutout iPhone 15 iOS 27,OS=latest}}"
project="${CUTOUT_IOS_APP_PROJECT:-swift/CutoutMobile/CutoutApp.xcodeproj}"
scheme="${CUTOUT_IOS_APP_SCHEME:-CutoutApp}"
derived_data="${CUTOUT_IOS_UI_TEST_DERIVED_DATA:-$root/target/xcode-ui-tests}"
lock_directory="$derived_data/.run-ios-ui-tests.lock"
simulator_device=""
prior_appearance=""
prior_increase_contrast=""
prior_content_size=""

mkdir -p "$derived_data"
if ! mkdir "$lock_directory" 2>/dev/null; then
  echo "iOS UI tests are already running for $derived_data" >&2
  exit 1
fi
cleanup() {
  cleanup_status=$?
  restore_status=0
  trap - EXIT
  set +e
  if [[ -n "$simulator_device" ]]; then
    [[ -z "$prior_appearance" ]] || /usr/bin/xcrun simctl ui "$simulator_device" appearance "$prior_appearance" >/dev/null || restore_status=$?
    [[ -z "$prior_increase_contrast" ]] || /usr/bin/xcrun simctl ui "$simulator_device" increase_contrast "$prior_increase_contrast" >/dev/null || restore_status=$?
    [[ -z "$prior_content_size" ]] || /usr/bin/xcrun simctl ui "$simulator_device" content_size "$prior_content_size" >/dev/null || restore_status=$?
  fi
  rmdir "$lock_directory" || restore_status=$?
  if (( cleanup_status == 0 && restore_status != 0 )); then
    echo "failed to restore simulator UI settings or release the test lock" >&2
    cleanup_status=$restore_status
  fi
  exit "$cleanup_status"
}
trap cleanup EXIT

if [[ -n "$requested_appearance$requested_increase_contrast$requested_content_size" ]]; then
  if [[ "$destination" != platform=iOS\ Simulator,* ]]; then
    echo "simulator UI settings require an iOS Simulator destination" >&2
    exit 2
  fi
  if [[ "$destination" =~ ,id=([^,]+) ]]; then
    simulator_device="${BASH_REMATCH[1]}"
  elif [[ "$destination" =~ ,name=([^,]+) ]]; then
    simulator_device="${BASH_REMATCH[1]}"
  else
    echo "simulator UI settings require a destination with id= or name=" >&2
    exit 2
  fi

  case "$requested_appearance" in
    ""|light|dark) ;;
    *) echo "CUTOUT_IOS_SIMULATOR_APPEARANCE must be light or dark" >&2; exit 2 ;;
  esac
  case "$requested_increase_contrast" in
    ""|enabled|disabled) ;;
    *) echo "CUTOUT_IOS_SIMULATOR_INCREASE_CONTRAST must be enabled or disabled" >&2; exit 2 ;;
  esac

  /usr/bin/xcrun simctl boot "$simulator_device" 2>/dev/null || true
  /usr/bin/xcrun simctl bootstatus "$simulator_device" -b

  if [[ -n "$requested_appearance" ]]; then
    prior_appearance="$(/usr/bin/xcrun simctl ui "$simulator_device" appearance)"
    /usr/bin/xcrun simctl ui "$simulator_device" appearance "$requested_appearance"
    [[ "$(/usr/bin/xcrun simctl ui "$simulator_device" appearance)" == "$requested_appearance" ]]
  fi
  if [[ -n "$requested_increase_contrast" ]]; then
    prior_increase_contrast="$(/usr/bin/xcrun simctl ui "$simulator_device" increase_contrast)"
    /usr/bin/xcrun simctl ui "$simulator_device" increase_contrast "$requested_increase_contrast"
    [[ "$(/usr/bin/xcrun simctl ui "$simulator_device" increase_contrast)" == "$requested_increase_contrast" ]]
  fi
  if [[ -n "$requested_content_size" ]]; then
    prior_content_size="$(/usr/bin/xcrun simctl ui "$simulator_device" content_size)"
    /usr/bin/xcrun simctl ui "$simulator_device" content_size "$requested_content_size"
    [[ "$(/usr/bin/xcrun simctl ui "$simulator_device" content_size)" == "$requested_content_size" ]]
  fi
fi

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
if [[ "$quiet" == true ]]; then
  xcodebuild_args+=(-quiet)
fi

if [[ "$smoke" == true ]]; then
  smoke_tests=(
    testPickerExposesAccessibleCaptureControls
    testVescUseShowsConnectingBeforeRide
    testVescRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType
    testEucRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType
    testEucBmsDiagnosticsExposeStableAccessibleDataRows
    testVescCriticalLiveActivityLockScreenPreservesSafetySemantics
    testVescLiveActivityContinuesUpdatingWhileBackgrounded
  )
  for test_name in "${smoke_tests[@]}"; do
    xcodebuild_args+=("-only-testing:CutoutAppUITests/CutoutAppUITests/$test_name")
  done
fi

if [[ "$clean" == true ]]; then
  /usr/bin/xcrun xcodebuild "${xcodebuild_args[@]}" clean
fi

if [[ "$mode" == "enumerate-tests" ]]; then
  if [[ -e "$enumeration_output" ]]; then
    echo "test enumeration output already exists: $enumeration_output" >&2
    exit 2
  fi
  mkdir -p "$(dirname "$enumeration_output")"
  /usr/bin/xcrun xcodebuild \
    "${xcodebuild_args[@]}" \
    -parallel-testing-enabled NO \
    -enumerate-tests \
    -test-enumeration-style flat \
    -test-enumeration-format json \
    -test-enumeration-output-path "$enumeration_output" \
    test
  enumeration_errors="$(jq '[.errors[]?] | length' "$enumeration_output")"
  enumerated_test_count="$(jq '[.values[].enabledTests | length] | add // 0' "$enumeration_output")"
  if [[ "$enumeration_errors" -ne 0 || "$enumerated_test_count" -eq 0 ]]; then
    echo "Xcode test enumeration returned $enumerated_test_count tests and $enumeration_errors errors" >&2
    exit 1
  fi
  echo "Enumerated $enumerated_test_count iOS UI tests ($enumeration_output)"
  exit 0
fi

if [[ -n "${CUTOUT_IOS_UI_TEST_RUN_TIMEOUT:-}" ]]; then
  ui_test_run_timeout="$CUTOUT_IOS_UI_TEST_RUN_TIMEOUT"
elif [[ "$smoke" == true ]]; then
  ui_test_run_timeout=420
elif [[ " ${xcodebuild_args[*]} " == *" -only-testing:"* ]]; then
  ui_test_run_timeout=150
else
  ui_test_run_timeout=1800
fi

if [[ "$mode" == "test" ]]; then
  result_bundle="$(cutout_create_ios_ui_test_result_bundle "$derived_data")"
  test_status=0
  test_started_at=$SECONDS
  if timeout --foreground --kill-after=30 "$ui_test_run_timeout" \
    /usr/bin/xcrun xcodebuild \
    "${xcodebuild_args[@]}" \
    -parallel-testing-enabled NO \
    -collect-test-diagnostics never \
    -test-timeouts-enabled YES \
    -default-test-execution-time-allowance 120 \
    -maximum-test-execution-time-allowance 120 \
    -resultBundlePath "$result_bundle" \
    test
  then
    :
  else
    test_status=$?
  fi

  if [[ ! -f "$result_bundle/Info.plist" ]]; then
    echo "iOS UI test failed without a complete .xcresult; do not treat this run as product evidence" >&2
    exit 1
  fi

  if [[ "$test_status" -eq 0 ]]; then
    test_count="$(
      /usr/bin/xcrun xcresulttool get test-results summary --path "$result_bundle" \
        | /usr/bin/plutil -extract totalTestCount raw -
    )"
    if ! [[ "$test_count" =~ ^[1-9][0-9]*$ ]]; then
      echo "iOS UI test completed without executing a test; refusing to report a green result" >&2
      exit 1
    fi
    if [[ "$smoke" == true && "$test_count" -ne "${#smoke_tests[@]}" ]]; then
      echo "iOS UI smoke lane executed $test_count of ${#smoke_tests[@]} tests" >&2
      exit 1
    fi
    echo "iOS UI tests passed: $test_count in $((SECONDS - test_started_at))s ($result_bundle)"
  fi

  exit "$test_status"
fi

/usr/bin/xcrun xcodebuild "${xcodebuild_args[@]}" build-for-testing
