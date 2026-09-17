#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
wrapper="$root/scripts/swift.sh"
package="$root/swift/CutoutMobile"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-swift-wrapper.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/other package/Sources"
touch "$tmp/other package/Package.swift"
ln -s "$package" "$tmp/package link"

# Source in a fresh process to fake only the external commands, including
# the absolute exec boundary. The real shared helper still runs; no compiler
# or generator is invoked. Do not call this in an `if` (it disables errexit).
invoke() (
  cd "$test_cwd"
  export DEVELOPER_DIR="/chosen Xcode/Contents/Developer"
  export CUTOUT_DEVELOPER_DIR="/must not override chosen Xcode"
  cargo() {
    [[ "$PWD" == "$root" && "$*" == 'cutout swift-ffi' ]] || exit 90
    [[ "$DEVELOPER_DIR" == '/chosen Xcode/Contents/Developer' ]] || exit 91
    printf 'ensure\n' >>"$tmp/events"
    return "${ensure_status:-0}"
  }
  exec() {
    [[ "$1" == /usr/bin/xcrun && "$2" == swift ]] || exit 92
    [[ "$DEVELOPER_DIR" == '/chosen Xcode/Contents/Developer' ]] || exit 93
    [[ "$PWD" == "$test_cwd" ]] || exit 94
    shift 2
    printf '%s\0' "$@" >"$tmp/actual-args"
    printf 'native\n' >>"$tmp/events"
    exit "${native_status:-0}"
  }
  source "$wrapper" "$@"
)

check() {
  local expected_events="$1" expected_status="$2" status
  shift 2
  : >"$tmp/events"
  : >"$tmp/actual-args"
  printf '%s\0' "$@" >"$tmp/expected-args"
  set +e
  (set -e; invoke "$@") 2>"$tmp/stderr"
  status=$?
  set -e
  if [[ "$status" != "$expected_status" || "$(<"$tmp/events")" != "$expected_events" ]]; then
    printf 'FAIL (%s): status=%s events=%s\n' "$*" "$status" "$(<"$tmp/events")" >&2
    cat "$tmp/stderr" >&2
    exit 1
  fi
  if [[ "$expected_events" == *native ]]; then
    cmp "$tmp/expected-args" "$tmp/actual-args"
  else
    [[ ! -s "$tmp/actual-args" ]]
  fi
  checks=$((checks + 1))
}

checks=0
test_cwd="$root"
for command in build test run; do
  check $'ensure\nnative' 0 "$command" --package-path swift/CutoutMobile
  check $'ensure\nnative' 0 --package-path=swift/CutoutMobile -v "$command"
done
check $'ensure\nnative' 0 --package-path "$tmp/package link" test --filter 'A test with spaces' ''
check $'ensure\nnative' 0 run -c release --package-path "$package" Validator --help --package-path /unrelated --skip-build
check $'ensure\nnative' 0 run --package-path "$package" -- Validator --help
test_cwd="$package/Sources"
check $'ensure\nnative' 0 test
check $'ensure\nnative' 0 build --package-path ..
for command in test run; do
  check '' 2 "$command" --skip-build
  [[ "$(<"$tmp/stderr")" == *'cannot use --skip-build'* ]]
  check '' 2 "$command" --version --skip-build
  check '' 2 --version "$command" --skip-build
done
check '' 2 run --build-system swiftbuild --skip-build CutoutMobileLiveValidator
check '' 2 run --traits defaults --skip-build CutoutMobileLiveValidator
check '' 2 test --specifier help --skip-build
# Operand-taking options advertised by Swift 6.4 run/test/build --help.
# Use command-shaped operands to catch early exits as well as missing shifts.
for option in \
  --cache-path --config-path --security-path --scratch-path --swift-sdks-path \
  --toolset --pkg-config-path --manifest-cache --netrc-file \
  --resolver-fingerprint-checking --resolver-signing-entity-checking \
  --default-registry-url -c --configuration -Xcc -Xswiftc -Xlinker -Xcxx \
  --triple --sdk --toolchain --swift-sdk --sanitize -j --jobs \
  --explicit-target-dependency-import-check --build-system -debug-info-format \
  --experimental-codesize-profile-output-dir --traits; do
  check '' 2 run "$option" help --skip-build Validator
  check '' 2 run --skip-build "$option" package Validator
  check $'ensure\nnative' 0 run "$option" help --package-path "$package" Validator --skip-build
done
for option in --attachments-path --num-workers -s --specifier --filter --skip --xunit-output; do
  check '' 2 test "$option" help --skip-build
  check '' 2 test --skip-build "$option" package
done
for option in --sbom-spec --sbom-output-dir --sbom-filter --target --product; do
  check $'ensure\nnative' 0 build "$option" help
done
check '' 2 run --build-system=swiftbuild --traits=defaults --skip-build Validator
for listing in list -l --list-tests; do
  check '' 2 test "$listing" --skip-build
  check '' 2 test --skip-build "$listing"
  check $'ensure\nnative' 0 test "$listing"
done
check $'ensure\nnative' 0 test --version
check $'ensure\nnative' 0 --version build
ensure_status=37
check ensure 37 build
unset ensure_status
native_status=42
check $'ensure\nnative' 42 test
unset native_status
check native 0 --version
check native 0 -typecheck build
check native 0 -module-name build -typecheck example.swift
check native 0 example.swift build
check native 0 package clean
check native 0 package --package-path "$package" describe
check native 0 help build
check native 0 test --help --skip-build
check native 0 run --help
check native 0 run -help --skip-build
check native 0 build -h
check native 0 test --package-path "$tmp/other package"
check native 0 run --package-path="$tmp/other package" --skip-build
check native 0 build --package-path "$tmp/missing"
test_cwd="$tmp/other package/Sources"
check native 0 build
test_cwd="$root"
check native 0 test
check native 0
printf '%s Swift wrapper checks passed\n' "$checks"
