#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

case "$(uname -s)" in
  Darwin)
    lib_name="libcutout_mobile_ffi.dylib"
    export DYLD_LIBRARY_PATH="$root/target/debug:${DYLD_LIBRARY_PATH:-}"
    swift_cmd=(env -u SDKROOT -u DEVELOPER_DIR swift)
    ;;
  Linux)
    lib_name="libcutout_mobile_ffi.so"
    export LD_LIBRARY_PATH="$root/target/debug:${LD_LIBRARY_PATH:-}"
    swift_cmd=(swift)
    ;;
  *)
    echo "unsupported host OS for Swift package smoke: $(uname -s)" >&2
    exit 1
    ;;
esac

cargo build -p cutout-mobile-ffi

package_dir="target/swift-package-smoke/CutoutMobile"
generated_dir="$package_dir/Sources/CutoutMobile/Generated"
system_target_dir="$package_dir/Sources/cutout_mobile_ffiFFI"
smoke_dir="$package_dir/Tests/CutoutMobileSmoke"
rm -rf "$package_dir"
mkdir -p "$generated_dir" "$system_target_dir" "$smoke_dir"

cp -R swift/CutoutMobile/. "$package_dir/"
cargo run -p cutout-uniffi-bindgen -- generate \
  --library "target/debug/$lib_name" \
  --language swift \
  --no-format \
  --out-dir "$generated_dir"
mv "$generated_dir/cutout_mobile_ffiFFI.h" "$system_target_dir/cutout_mobile_ffiFFI.h"
mv "$generated_dir/cutout_mobile_ffiFFI.modulemap" "$system_target_dir/module.modulemap"

cp tests/mobile-ffi/swift-package-smoke.swift "$smoke_dir/main.swift"

"${swift_cmd[@]}" run \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$root/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  CutoutMobileSmoke
