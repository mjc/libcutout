#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: scripts/run-ios-ui-test-matrix.sh [--plan-from enumeration.json]"
  echo "  With no arguments, enumerate the compiled UI tests and run every test in settings-compatible groups."
  echo "  --plan-from prints the groups in an existing Xcode enumeration without running tests."
}

if [[ "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

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
  done < <(matrix_rows "$enumeration")
  echo "$previous_key: $count tests"
  groups=$((groups + 1))
  echo "$total tests across $groups simulator-settings groups"
}

if [[ "${1:-}" == "--plan-from" ]]; then
  [[ $# -eq 2 ]] || { echo "--plan-from requires one Xcode enumeration JSON path" >&2; exit 2; }
  validate_enumeration "$2"
  print_plan "$2"
  exit 0
fi
if [[ $# -ne 0 ]]; then
  usage >&2
  exit 2
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
done < <(matrix_rows "$enumeration")
run_group
