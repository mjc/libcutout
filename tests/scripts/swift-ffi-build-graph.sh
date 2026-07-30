#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$root/scripts/swift-package-common.sh"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/cutout-swift-ffi-graph.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

fake="$tmp/repository"
mkdir -p \
  "$fake/crates/cutout-core/src" \
  "$fake/crates/cutout-protocols/src" \
  "$fake/crates/cutout-mobile-ffi/src" \
  "$fake/crates/cutout-mobile-ffi/CutoutMobileFFI"
printf '[workspace]\n' >"$fake/Cargo.toml"
printf 'version = 4\n' >"$fake/Cargo.lock"
printf 'channel = "stable"\n' >"$fake/rust-toolchain.toml"
printf 'pub struct Core;\n' >"$fake/crates/cutout-core/src/lib.rs"
printf 'pub struct Protocol;\n' >"$fake/crates/cutout-protocols/src/lib.rs"
printf 'pub struct Mobile;\n' >"$fake/crates/cutout-mobile-ffi/src/lib.rs"
for crate in cutout-core cutout-protocols cutout-mobile-ffi; do
  printf '[package]\nname = "%s"\n' "$crate" >"$fake/crates/$crate/Cargo.toml"
done

stamp="$fake/crates/cutout-mobile-ffi/CutoutMobileFFI/.cutout-source.sha256"
cutout_swift_ffi_source_fingerprint "$fake" >"$stamp"
cutout_require_current_swift_ffi "$fake"

if cutout_require_swift_ffi_build_input "$fake" 2>"$tmp/missing.log"; then
  echo "expected a missing XCFramework slice to fail before the Swift build" >&2
  exit 1
fi
grep -q "missing Swift FFI build input" "$tmp/missing.log"

if [[ "$(uname -s)" == Darwin ]]; then
  package="$fake/crates/cutout-mobile-ffi/CutoutMobileFFI"
  for slice in ios-arm64 ios-arm64_x86_64-simulator macos-arm64_x86_64; do
    mkdir -p "$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI"
    touch \
      "$package/cutout_mobile_ffiFFI.xcframework/$slice/libcutout_mobile_ffi.a" \
      "$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h" \
      "$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI/module.modulemap"
  done
  mkdir -p "$package/Sources/CutoutMobileFFI"
  touch \
    "$package/Package.swift" \
    "$package/Sources/CutoutMobileFFI/cutout_mobile_ffi.swift" \
    "$package/cutout_mobile_ffiFFI.xcframework/Info.plist"
  if cutout_require_swift_ffi_build_input "$fake" 2>"$tmp/architecture.log"; then
    echo "expected wrong-architecture XCFramework slices to fail before the Swift build" >&2
    exit 1
  fi
  grep -q "wrong-architecture slice" "$tmp/architecture.log"
fi

printf 'pub struct Changed;\n' >>"$fake/crates/cutout-core/src/lib.rs"
if cutout_require_current_swift_ffi "$fake" 2>"$tmp/stale.log"; then
  echo "expected changed Rust input to invalidate the Swift FFI artifact" >&2
  exit 1
fi
grep -q "scripts/regenerate-swift-ffi.sh" "$tmp/stale.log"
