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
  "$fake/crates/cutout-ride-maps/src" \
  "$fake/crates/libcutout-persistence/src" \
  "$fake/crates/cutout-mobile-ffi/src" \
  "$fake/target/swift-ffi/CutoutMobileFFI" \
  "$fake/scripts"
printf '[workspace]\n' >"$fake/Cargo.toml"
printf 'version = 4\n' >"$fake/Cargo.lock"
printf 'channel = "stable"\n' >"$fake/rust-toolchain.toml"
printf '{}\n' >"$fake/flake.lock"
printf 'inputs = {};\n' >"$fake/flake.nix"
printf '#!/usr/bin/env bash\n' >"$fake/scripts/regenerate-swift-ffi.sh"
cp "$root/scripts/swift-package-common.sh" "$fake/scripts/swift-package-common.sh"
printf 'pub struct Core;\n' >"$fake/crates/cutout-core/src/lib.rs"
printf 'pub struct Protocol;\n' >"$fake/crates/cutout-protocols/src/lib.rs"
printf 'pub struct RideMaps;\n' >"$fake/crates/cutout-ride-maps/src/lib.rs"
printf 'pub struct Persistence;\n' >"$fake/crates/libcutout-persistence/src/lib.rs"
printf 'pub struct Mobile;\n' >"$fake/crates/cutout-mobile-ffi/src/lib.rs"
for crate in cutout-core cutout-protocols cutout-ride-maps cutout-mobile-ffi libcutout-persistence; do
  printf '[package]\nname = "%s"\n' "$crate" >"$fake/crates/$crate/Cargo.toml"
done

stamp="$fake/target/swift-ffi/CutoutMobileFFI/.cutout-source.sha256"
cutout_swift_ffi_source_fingerprint "$fake" >"$stamp"
cutout_require_current_swift_ffi "$fake"

if cutout_require_swift_ffi_build_input "$fake" 2>"$tmp/missing.log"; then
  echo "expected a missing XCFramework slice to fail before the Swift build" >&2
  exit 1
fi
grep -q "missing Swift FFI build input" "$tmp/missing.log"

if [[ "$(uname -s)" == Darwin ]]; then
  package="$fake/target/swift-ffi/CutoutMobileFFI"
  for slice in ios-arm64 ios-arm64_x86_64-simulator macos-arm64_x86_64; do
    mkdir -p "$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI"
    printf 'generated\n' >"$package/cutout_mobile_ffiFFI.xcframework/$slice/libcutout_mobile_ffi.a"
    printf 'generated\n' >"$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"
    printf 'generated\n' >"$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI/module.modulemap"
  done
  mkdir -p "$package/Sources/CutoutMobileFFI"
  printf 'let package = Package(\n    name: "CutoutMobileFFI"\n)\n' >"$package/Package.swift"
  printf 'generated\n' >"$package/Sources/CutoutMobileFFI/cutout_mobile_ffi.swift"
  printf '%s\n' \
    '<?xml version="1.0" encoding="UTF-8"?>' \
    '<plist version="1.0"><dict><key>AvailableLibraries</key><array/></dict></plist>' \
    >"$package/cutout_mobile_ffiFFI.xcframework/Info.plist"
  if cutout_require_swift_ffi_build_input "$fake" 2>"$tmp/architecture.log"; then
    echo "expected wrong-architecture XCFramework slices to fail before the Swift build" >&2
    exit 1
  fi
  if ! grep -q "wrong-architecture slice" "$tmp/architecture.log"; then
    cat "$tmp/architecture.log" >&2
    exit 1
  fi
fi

printf 'pub struct Changed;\n' >>"$fake/crates/cutout-core/src/lib.rs"
if cutout_require_current_swift_ffi "$fake" 2>"$tmp/stale.log"; then
  echo "expected changed Rust input to invalidate the Swift FFI artifact" >&2
  exit 1
fi
grep -q "scripts/regenerate-swift-ffi.sh" "$tmp/stale.log"

write_complete_fake_package() {
  local package slice
  package="$(cutout_swift_ffi_package_dir "$fake")"
  mkdir -p \
    "$package/Sources/CutoutMobileFFI" \
    "$package/cutout_mobile_ffiFFI.xcframework"
  printf 'let package = Package(\n    name: "CutoutMobileFFI"\n)\n' >"$package/Package.swift"
  printf 'generated\n' >"$package/Sources/CutoutMobileFFI/cutout_mobile_ffi.swift"
  printf '%s\n' \
    '<?xml version="1.0" encoding="UTF-8"?>' \
    '<plist version="1.0"><dict><key>AvailableLibraries</key><array/></dict></plist>' \
    >"$package/cutout_mobile_ffiFFI.xcframework/Info.plist"
  for slice in ios-arm64 ios-arm64_x86_64-simulator macos-arm64_x86_64; do
    mkdir -p "$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI"
    printf 'generated\n' >"$package/cutout_mobile_ffiFFI.xcframework/$slice/libcutout_mobile_ffi.a"
    printf 'generated\n' >"$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"
    printf 'generated\n' >"$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI/module.modulemap"
  done
  cutout_swift_ffi_source_fingerprint "$fake" >"$package/.cutout-source.sha256"
}

(
  # The focused ensure contract exercises package state; architecture validation
  # remains covered separately above on Darwin.
  uname() {
    printf 'Linux\n'
  }

  generation_count=0
  fake_regenerate() {
    generation_count=$((generation_count + 1))
    write_complete_fake_package
  }

  rm -rf -- "$(cutout_swift_ffi_package_dir "$fake")"
  cutout_ensure_swift_ffi_build_input "$fake" fake_regenerate
  [[ "$generation_count" -eq 1 ]] || {
    echo "expected an absent package to trigger exactly one generation" >&2
    exit 1
  }

  cutout_ensure_swift_ffi_build_input "$fake" fake_regenerate
  [[ "$generation_count" -eq 1 ]] || {
    echo "expected an unchanged package to skip generation" >&2
    exit 1
  }

  : >"$(cutout_swift_ffi_package_dir "$fake")/Package.swift"
  cutout_ensure_swift_ffi_build_input "$fake" fake_regenerate
  [[ "$generation_count" -eq 2 ]] || {
    echo "expected an empty generated file to trigger repair" >&2
    exit 1
  }

  rm -f -- "$(cutout_swift_ffi_package_dir "$fake")/cutout_mobile_ffiFFI.xcframework/ios-arm64/Headers/cutout_mobile_ffiFFI/module.modulemap"
  cutout_ensure_swift_ffi_build_input "$fake" fake_regenerate
  [[ "$generation_count" -eq 3 ]] || {
    echo "expected a missing required file to trigger repair" >&2
    exit 1
  }

  printf 'pub struct ChangedAgain;\n' >>"$fake/crates/cutout-ride-maps/src/lib.rs"
  cutout_ensure_swift_ffi_build_input "$fake" fake_regenerate
  [[ "$generation_count" -eq 4 ]] || {
    echo "expected a covered dependency change to trigger regeneration" >&2
    exit 1
  }
)

(
  uname() {
    printf 'Linux\n'
  }

  fakebin="$tmp/fake-bin"
  counter="$tmp/generation-count"
  mkdir -p "$fakebin"
  cp "$root/scripts/swift-package-common.sh" "$fake/scripts/swift-package-common.sh"
  cp "$root/scripts/regenerate-swift-ffi.sh" "$fake/scripts/regenerate-swift-ffi.sh"
  chmod +x "$fake/scripts/regenerate-swift-ffi.sh"
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'printf "%s\\n" "$$" >>"$CUTOUT_FAKE_COUNTER"' \
    'sleep 0.2' \
    'if [[ "${CUTOUT_FAKE_MUTATE:-0}" == 1 ]]; then printf "mutated\\n" >>"$PWD/../cutout-core/src/lib.rs"; fi' \
    'package="$PWD/CutoutMobileFFI"' \
    'mkdir -p "$package/Sources/CutoutMobileFFI" "$package/cutout_mobile_ffiFFI.xcframework"' \
    'printf "let package = Package(\\n    name: \\\"CutoutMobileFFI\\\"\\n)\\n" >"$package/Package.swift"' \
    'printf "generated\\n" >"$package/Sources/CutoutMobileFFI/cutout_mobile_ffi.swift"' \
    'printf "%s\\n" "<plist><dict><key>AvailableLibraries</key><array/></dict></plist>" >"$package/cutout_mobile_ffiFFI.xcframework/Info.plist"' \
    'for slice in ios-arm64 ios-arm64_x86_64-simulator macos-arm64_x86_64; do' \
    '  mkdir -p "$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI"' \
    '  printf "generated\\n" >"$package/cutout_mobile_ffiFFI.xcframework/$slice/libcutout_mobile_ffi.a"' \
    '  printf "generated\\n" >"$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI/cutout_mobile_ffiFFI.h"' \
    '  printf "generated\\n" >"$package/cutout_mobile_ffiFFI.xcframework/$slice/Headers/cutout_mobile_ffiFFI/module.modulemap"' \
    'done' >"$fakebin/cargo"
  printf '%s\n' '#!/usr/bin/env bash' 'printf "Linux\\n"' >"$fakebin/uname"
  chmod +x "$fakebin/cargo" "$fakebin/uname"
  rm -rf -- "$(cutout_swift_ffi_package_dir "$fake")" "$fake/crates/cutout-mobile-ffi/CutoutMobileFFI"

  env -u RUSTC_WRAPPER -u RUSTC_WORKSPACE_WRAPPER \
    CUTOUT_FAKE_COUNTER="$counter" PATH="$fakebin:$PATH" \
    "$fake/scripts/regenerate-swift-ffi.sh" >"$tmp/generation-one.log" 2>&1 &
  first_pid=$!
  env -u RUSTC_WRAPPER -u RUSTC_WORKSPACE_WRAPPER \
    CUTOUT_FAKE_COUNTER="$counter" PATH="$fakebin:$PATH" \
    "$fake/scripts/regenerate-swift-ffi.sh" >"$tmp/generation-two.log" 2>&1 &
  second_pid=$!
  if ! wait "$first_pid"; then
    cat "$tmp/generation-one.log" >&2
    exit 1
  fi
  if ! wait "$second_pid"; then
    cat "$tmp/generation-two.log" >&2
    exit 1
  fi

  [[ "$(wc -l <"$counter")" -eq 1 ]] || {
    echo "expected concurrent regeneration to invoke cargo exactly once" >&2
    cat "$tmp/generation-one.log" "$tmp/generation-two.log" >&2
    exit 1
  }
  cutout_require_swift_ffi_build_input "$fake"

  rm -rf -- "$(cutout_swift_ffi_package_dir "$fake")"
  if env -u RUSTC_WRAPPER -u RUSTC_WORKSPACE_WRAPPER \
      CUTOUT_FAKE_COUNTER="$counter" CUTOUT_FAKE_MUTATE=1 PATH="$fakebin:$PATH" \
      "$fake/scripts/regenerate-swift-ffi.sh" >"$tmp/generation-mutated.log" 2>&1; then
    echo "expected source changes during generation to reject the package" >&2
    exit 1
  fi
  grep -q "inputs changed during generation" "$tmp/generation-mutated.log"

  lock="$(cutout_swift_ffi_lock_path "$fake")"
  ln -s 999999999 "$lock"
  if ! CUTOUT_FAKE_COUNTER="$counter" PATH="$fakebin:$PATH" \
      "$fake/scripts/regenerate-swift-ffi.sh" >"$tmp/stale-lock.log" 2>&1; then
    cat "$tmp/stale-lock.log" >&2
    exit 1
  fi
  test ! -e "$lock"
  cutout_require_swift_ffi_build_input "$fake"

  printf 'malformed\n' >"$lock"
  if "$fake/scripts/regenerate-swift-ffi.sh" >"$tmp/malformed-lock.log" 2>&1; then
    echo "expected a malformed lock to fail instead of hanging" >&2
    exit 1
  fi
  grep -q "malformed Swift FFI generation lock" "$tmp/malformed-lock.log"
  rm -f -- "$lock"

  ln -s "$$" "$lock"
  if CUTOUT_SWIFT_FFI_LOCK_TIMEOUT_SECONDS=1 \
      "$fake/scripts/regenerate-swift-ffi.sh" >"$tmp/timeout-lock.log" 2>&1; then
    echo "expected a live lock to time out" >&2
    exit 1
  fi
  grep -q "timed out waiting for Swift FFI generation lock" "$tmp/timeout-lock.log"
  rm -f -- "$lock"
)

generated="$tmp/generated-package"
mkdir -p "$generated"
printf 'old\n' >"$generated/original"

if cutout_replace_generated_directory relative-package true 2>"$tmp/unsafe.log"; then
  echo "expected a relative replacement target to be rejected" >&2
  exit 1
fi
grep -q "refusing unsafe generated-directory target" "$tmp/unsafe.log"

failed_generation() {
  mkdir -p "$generated"
  printf 'partial\n' >"$generated/partial"
  return 1
}

if cutout_replace_generated_directory "$generated" failed_generation; then
  echo "expected failed regeneration to preserve the prior package" >&2
  exit 1
fi
test -f "$generated/original"
test ! -e "$generated/partial"

successful_generation() {
  mkdir -p "$generated"
  printf 'new\n' >"$generated/replacement"
}

cutout_replace_generated_directory "$generated" successful_generation
test ! -e "$generated/original"
test -f "$generated/replacement"

atomic_source="$tmp/atomic-source"
mkdir -p "$atomic_source"
printf 'atomic-new\n' >"$atomic_source/replacement"
cutout_atomic_replace_generated_directory "$atomic_source" "$generated"
test ! -e "$atomic_source"
test ! -e "$generated/original"
test -f "$generated/replacement"
grep -q '^atomic-new$' "$generated/replacement"
