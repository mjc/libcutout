#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

case "$(uname -s)" in
  Darwin)
    lib_name="libcutout_mobile_ffi.dylib"
    swift_cmd=(env -u SDKROOT -u DEVELOPER_DIR swift)
    ;;
  Linux)
    lib_name="libcutout_mobile_ffi.so"
    swift_cmd=(swift)
    ;;
  *)
    echo "unsupported host OS for Swift SourceKit workspace: $(uname -s)" >&2
    exit 1
    ;;
esac

package_dir="${CUTOUT_SWIFT_SOURCEKIT_PACKAGE_DIR:-target/swift-sourcekit/CutoutMobile}"
generated_dir="$package_dir/Sources/CutoutMobile/Generated"
system_target_dir="$package_dir/Sources/cutout_mobile_ffiFFI"
smoke_dir="$package_dir/Tests/CutoutMobileSmoke"

cargo build -p cutout-mobile-ffi

rm -rf "$package_dir"
mkdir -p "$package_dir"
cp swift/CutoutMobile/Package.swift "$package_dir/Package.swift"
cp -R swift/CutoutMobile/Apps "$package_dir/Apps"
cp -R swift/CutoutMobile/Sources "$package_dir/Sources"
cp -R swift/CutoutMobile/Tests "$package_dir/Tests"
mkdir -p "$generated_dir" "$system_target_dir" "$smoke_dir"

cargo run -p cutout-uniffi-bindgen -- generate \
  --library "target/debug/$lib_name" \
  --language swift \
  --no-format \
  --out-dir "$generated_dir"
mv "$generated_dir/cutout_mobile_ffiFFI.h" "$system_target_dir/cutout_mobile_ffiFFI.h"
mv "$generated_dir/cutout_mobile_ffiFFI.modulemap" "$system_target_dir/module.modulemap"

cp tests/mobile-ffi/swift-package-smoke.swift "$smoke_dir/main.swift"

"${swift_cmd[@]}" package describe --package-path "$package_dir" >/dev/null
printf '%s\n' "$package_dir"
