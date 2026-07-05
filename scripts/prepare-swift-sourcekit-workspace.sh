#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
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

source_package_dir="$root/swift/CutoutMobile"
package_dir="${CUTOUT_SWIFT_SOURCEKIT_PACKAGE_DIR:-$source_package_dir}"
package_dir="${package_dir%/}"
package_parent="$(dirname "$package_dir")"
package_base="$(basename "$package_dir")"
mkdir -p "$package_parent"
package_dir="$(cd "$package_parent" && pwd -P)/$package_base"
generated_dir="$package_dir/Sources/CutoutMobile/Generated"
system_target_dir="$package_dir/Sources/cutout_mobile_ffiFFI"
smoke_dir="$package_dir/Tests/CutoutMobileSmoke"

SDKROOT="$(cutout_macosx_sdk_path)" cargo build -p cutout-mobile-ffi

if [[ "$package_dir" != "$source_package_dir" ]]; then
  rm -rf "$package_dir"
  mkdir -p "$package_dir"
  cp "$source_package_dir/Package.swift" "$package_dir/Package.swift"
  cp -R "$source_package_dir/Apps" "$package_dir/Apps"
  cp -R "$source_package_dir/Sources" "$package_dir/Sources"
  cp -R "$source_package_dir/Tests" "$package_dir/Tests"
fi

rm -rf "$generated_dir" "$system_target_dir" "$smoke_dir"

mkdir -p "$generated_dir" "$system_target_dir" "$smoke_dir"

SDKROOT="$(cutout_macosx_sdk_path)" cargo run -p cutout-uniffi-bindgen -- generate \
  --library "$target_dir/debug/$lib_name" \
  --language swift \
  --no-format \
  --out-dir "$generated_dir"
mv "$generated_dir/cutout_mobile_ffiFFI.h" "$system_target_dir/cutout_mobile_ffiFFI.h"
mv "$generated_dir/cutout_mobile_ffiFFI.modulemap" "$system_target_dir/module.modulemap"

cp tests/mobile-ffi/swift-package-smoke.swift "$smoke_dir/main.swift"

"${swift_cmd[@]}" package describe --package-path "$package_dir" >/dev/null
printf '%s\n' "$package_dir"
