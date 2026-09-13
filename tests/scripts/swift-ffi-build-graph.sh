#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$root/scripts/swift-package-common.sh"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-swift-ffi-graph.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

fake="$tmp/repository"
fakebin="$tmp/bin"
counter="$tmp/cargo-invocations"
mkdir -p "$fakebin" "$fake/target/swift-ffi"

printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'if [[ ${CUTOUT_FAKE_FAIL:-} == 1 ]]; then exit 17; fi' \
  'printf "called\n" >>"$CUTOUT_FAKE_COUNTER"' \
  'package="$PWD/target/swift-ffi/CutoutMobileFFI"' \
  'mkdir -p "$package/Sources/CutoutMobileFFI" "$package/cutout_mobile_ffiFFI.xcframework"' \
  'printf "let package = Package(name: \"CutoutMobileFFI\")\n" >"$package/Package.swift"' \
  'printf "generated\n" >"$package/Sources/CutoutMobileFFI/cutout_mobile_ffi.swift"' \
  'printf "plist\n" >"$package/cutout_mobile_ffiFFI.xcframework/Info.plist"' \
  'for slice in ios-arm64 ios-arm64-simulator macos-arm64; do' \
  '  headers="$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI"' \
  '  mkdir -p "$headers"' \
  '  printf "archive\n" >"$package/cutout_mobile_ffiFFI.xcframework/$slice/libcutout_mobile_ffi.a"' \
  '  printf "header\n" >"$headers/cutout_mobile_ffiFFI.h"' \
  '  printf "module\n" >"$headers/module.modulemap"' \
  'done' >"$fakebin/cargo"
chmod +x "$fakebin/cargo"

CUTOUT_FAKE_COUNTER="$counter" PATH="$fakebin:$PATH" \
  cutout_ensure_swift_ffi_build_input "$fake"
CUTOUT_FAKE_COUNTER="$counter" PATH="$fakebin:$PATH" \
  cutout_ensure_swift_ffi_build_input "$fake"

if CUTOUT_FAKE_FAIL=1 CUTOUT_FAKE_COUNTER="$counter" PATH="$fakebin:$PATH" \
  cutout_ensure_swift_ffi_build_input "$fake"; then
  echo "expected cargo failure to propagate from Swift FFI ensure" >&2
  exit 1
fi

[[ "$(wc -l <"$counter")" -eq 2 ]] || {
  echo "expected every shell ensure to delegate freshness and locking to cutout-dev" >&2
  exit 1
}

package="$(cutout_swift_ffi_package_dir "$fake")"
rm -f -- "$package/cutout_mobile_ffiFFI.xcframework/ios-arm64/Headers/cutout_mobile_ffiFFI/module.modulemap"
if cutout_validate_swift_ffi_build_input "$fake" 2>"$tmp/missing.log"; then
  echo "expected structural validation to reject an incomplete generated package" >&2
  exit 1
fi
grep -q "missing Swift FFI build input" "$tmp/missing.log"

echo "Swift FFI build graph checks passed"
