#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

case "$(cutout_host_os)" in
  Darwin) lib_name="libcutout_mobile_ffi.dylib" ;;
  Linux) lib_name="libcutout_mobile_ffi.so" ;;
  *)
    echo "unsupported host OS for Swift SourceKit workspace: $(cutout_host_os)" >&2
    exit 1
    ;;
esac

swift_cmd=($(cutout_swift_runtime_command))

package_dir="${CUTOUT_SWIFT_SOURCEKIT_PACKAGE_DIR:-swift/CutoutMobile}"
generated_dir="$package_dir/Sources/CutoutMobile/Generated"
system_target_dir="$package_dir/Sources/cutout_mobile_ffiFFI"
smoke_dir="$package_dir/Tests/CutoutMobileSmoke"
in_place_package_dir="swift/CutoutMobile"

SDKROOT="$(cutout_macosx_sdk_path)" cargo build -p cutout-mobile-ffi

case "$package_dir" in
  "$in_place_package_dir" | "$root/$in_place_package_dir")
    rm -rf "$generated_dir" "$system_target_dir" "$smoke_dir"
    ;;
  *)
    rm -rf "$package_dir"
    mkdir -p "$package_dir"
    cp swift/CutoutMobile/Package.swift "$package_dir/Package.swift"
    cp -R swift/CutoutMobile/Apps "$package_dir/Apps"
    cp -R swift/CutoutMobile/Sources "$package_dir/Sources"
    cp -R swift/CutoutMobile/Tests "$package_dir/Tests"
    ;;
esac

mkdir -p "$generated_dir" "$system_target_dir" "$smoke_dir"

SDKROOT="$(cutout_macosx_sdk_path)" cargo run -p cutout-uniffi-bindgen -- generate \
  --library "target/debug/$lib_name" \
  --language swift \
  --no-format \
  --out-dir "$generated_dir"
mv "$generated_dir/cutout_mobile_ffiFFI.h" "$system_target_dir/cutout_mobile_ffiFFI.h"
mv "$generated_dir/cutout_mobile_ffiFFI.modulemap" "$system_target_dir/module.modulemap"

cp tests/mobile-ffi/swift-package-smoke.swift "$smoke_dir/main.swift"

"${swift_cmd[@]}" package describe --package-path "$package_dir" >/dev/null
printf '%s\n' "$package_dir"
