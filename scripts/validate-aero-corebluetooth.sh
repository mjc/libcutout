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
  *)
    echo "Aero CoreBluetooth validation requires Darwin/CoreBluetooth" >&2
    exit 1
    ;;
esac

timeout_seconds="${1:-45}"
echo "libcutout_commit=$(git rev-parse HEAD)"

cargo build -p cutout-mobile-ffi

package_dir="target/aero-corebluetooth-live/CutoutMobile"
generated_dir="$package_dir/Sources/CutoutMobile/Generated"
system_target_dir="$package_dir/Sources/cutout_mobile_ffiFFI"
smoke_dir="$package_dir/Tests/CutoutMobileSmoke"
rm -rf "$package_dir"
mkdir -p "$generated_dir" "$system_target_dir" "$smoke_dir"

cp -R swift/CutoutMobile/. "$package_dir/"
cp tests/mobile-ffi/swift-package-smoke.swift "$smoke_dir/main.swift"
cargo run -p cutout-uniffi-bindgen -- generate \
  --library "target/debug/$lib_name" \
  --language swift \
  --no-format \
  --out-dir "$generated_dir"
mv "$generated_dir/cutout_mobile_ffiFFI.h" "$system_target_dir/cutout_mobile_ffiFFI.h"
mv "$generated_dir/cutout_mobile_ffiFFI.modulemap" "$system_target_dir/module.modulemap"

"${swift_cmd[@]}" run \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$root/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  CutoutMobileAeroLive \
  "$timeout_seconds"
