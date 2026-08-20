#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: scripts/run-ios-ui-test-matrix.sh [--smoke] [--plan-from enumeration.json] [--only-group \"appearance contrast content-size\"]"
  echo "  With no arguments, enumerate the compiled UI tests and run every test in settings-compatible groups."
  echo "  --smoke selects the eight production-root smoke tests and runs them in compatible settings groups."
  echo "  --plan-from prints the groups in an existing Xcode enumeration without running tests."
  echo "  --only-group runs or prints one exact settings group from the compiled enumeration."
}

plan_from=""
only_group=""
smoke=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help)
      usage
      exit 0
      ;;
    --plan-from)
      [[ $# -ge 2 ]] || { echo "--plan-from requires one Xcode enumeration JSON path" >&2; exit 2; }
      plan_from="$2"
      shift 2
      ;;
    --smoke)
      smoke=true
      shift
      ;;
    --only-group)
      [[ $# -ge 2 ]] || { echo "--only-group requires one exact settings group" >&2; exit 2; }
      only_group="$2"
      shift 2
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runner="$root/scripts/run-ios-ui-tests.sh"

matrix_rows() {
  jq -r '
    [.values[].enabledTests[].identifier
      | {
          appearance: (if contains("InDarkAppearance") then "dark" else "light" end),
          contrast: (if contains("IncreasedContrast") then "enabled" else "disabled" end),
          content_size: (
            if contains("AccessibilityDynamicType") or contains("RightToLeft")
            then "accessibility-extra-extra-extra-large"
            elif contains("ExtraExtraExtraLarge")
            then "extra-extra-extra-large"
            else "large"
            end
          ),
          identifier: sub("\\(\\)$"; "")
        }
    ]
    | sort_by(.appearance, .contrast, .content_size, .identifier)
    | .[]
    | [.appearance, .contrast, .content_size, .identifier]
    | @tsv
  ' "$1"
}

is_smoke_test() {
  case "$1" in
    */testPickerExposesAccessibleCaptureControls | \
      */testVescUseShowsConnectingBeforeRide | \
      */testVescRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType | \
      */testEucRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType | \
      */testEucBmsDiagnosticsExposeStableAccessibleDataRows | \
      */testVescCriticalLiveActivityAutoFixtureStartsAnAccessibleRide | \
      */testVescCriticalLiveActivityLockScreenPreservesSafetySemantics | \
      */testVescLiveActivityContinuesUpdatingWhileBackgrounded)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

selected_matrix_rows() {
  local appearance contrast content_size identifier key
  while IFS=$'\t' read -r appearance contrast content_size identifier; do
    key="$appearance $contrast $content_size"
    if { [[ -z "$only_group" ]] || [[ "$key" == "$only_group" ]]; } \
      && { [[ "$smoke" == false ]] || is_smoke_test "$identifier"; }
    then
      printf '%s\t%s\t%s\t%s\n' "$appearance" "$contrast" "$content_size" "$identifier"
    fi
  done < <(matrix_rows "$1")
}

validate_enumeration() {
  if ! jq -e '([.errors[]?] | length) == 0 and ([.values[].enabledTests[]] | length) > 0' "$1" >/dev/null; then
    echo "Xcode UI test enumeration contains errors or no enabled tests: $1" >&2
    exit 1
  fi
}

print_plan() {
  local enumeration="$1"
  local appearance contrast content_size identifier key previous_key="" count=0 total=0 groups=0
  while IFS=$'\t' read -r appearance contrast content_size identifier; do
    key="$appearance $contrast $content_size"
    if [[ -n "$previous_key" && "$key" != "$previous_key" ]]; then
      echo "$previous_key: $count tests"
      groups=$((groups + 1))
      count=0
    fi
    previous_key="$key"
    count=$((count + 1))
    total=$((total + 1))
  done < <(selected_matrix_rows "$enumeration")
  if [[ $total -eq 0 ]]; then
    echo "No compiled UI tests match settings group: $only_group" >&2
    return 1
  fi
  echo "$previous_key: $count tests"
  groups=$((groups + 1))
  local test_word="tests" group_word="groups"
  [[ $total -eq 1 ]] && test_word="test"
  [[ $groups -eq 1 ]] && group_word="group"
  echo "$total $test_word across $groups simulator-settings $group_word"
}

if [[ -n "$plan_from" ]]; then
  validate_enumeration "$plan_from"
  print_plan "$plan_from"
  exit 0
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-ui-test-matrix.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
enumeration="$tmp/enumeration.json"
"$runner" --enumerate-tests "$enumeration"
validate_enumeration "$enumeration"
print_plan "$enumeration"

appearance=""
contrast=""
content_size=""
group_key=""
group_tests=()
run_group() {
  [[ ${#group_tests[@]} -gt 0 ]] || return
  local timeout_seconds=$((180 + 120 * ${#group_tests[@]}))
  local selectors=()
  local identifier
  for identifier in "${group_tests[@]}"; do
    selectors+=("-only-testing:$identifier")
  done
  echo "Running ${#group_tests[@]} tests with $group_key"
  "$runner" \
    --timeout "$timeout_seconds" \
    --appearance "$appearance" \
    --increase-contrast "$contrast" \
    --content-size "$content_size" \
    "${selectors[@]}"
}

while IFS=$'\t' read -r next_appearance next_contrast next_content_size identifier; do
  next_key="$next_appearance $next_contrast $next_content_size"
  if [[ -n "$group_key" && "$next_key" != "$group_key" ]]; then
    run_group
    group_tests=()
  fi
  appearance="$next_appearance"
  contrast="$next_contrast"
  content_size="$next_content_size"
  group_key="$next_key"
  group_tests+=("$identifier")
done < <(selected_matrix_rows "$enumeration")
run_group
