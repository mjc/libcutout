#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
crate="$root/crates/cutout-mobile-ffi"
stage="$crate/CutoutMobileFFI"
package="$(cutout_swift_ffi_package_dir "$root")"
mkdir -p "$(dirname "$package")"
cd "$crate"
cutout_use_xcode_developer_dir "/Applications/Xcode.app/Contents/Developer"

lock="$(dirname "$package")/.generation.lock"
while ! ln -s "$$" "$lock" 2>/dev/null; do
  owner="$(readlink "$lock" 2>/dev/null || true)"
  if [[ "$owner" =~ ^[0-9]+$ ]] && ! kill -0 "$owner" 2>/dev/null; then
    rm -f -- "$lock"
    continue
  fi
  sleep 0.1
done
cleanup_lock() {
  [[ "$(readlink "$lock" 2>/dev/null || true)" == "$$" ]] && rm -f -- "$lock"
}
trap cleanup_lock EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

if cutout_validate_swift_ffi_build_input "$root" 2>/dev/null; then
  exit 0
fi

if [[ "${RUSTC_WRAPPER+x}" != x || -n "$RUSTC_WRAPPER" ||
      "${RUSTC_WORKSPACE_WRAPPER+x}" != x || -n "$RUSTC_WORKSPACE_WRAPPER" ]]; then
  echo "Swift FFI regeneration requires nix develop with Cargo wrappers disabled" >&2
  exit 1
fi

generate_stage() {
  cargo swift package \
    --platforms ios@18 macos@15 \
    --release \
    --name CutoutMobileFFI \
    --lib-type static \
    --skip-toolchains-check \
    --accept-all \
    --swift-tools-version 6.0 \
    --silent

  python3 - "$stage/cutout_mobile_ffiFFI.xcframework/Info.plist" <<'PY'
import plistlib
import sys

path = sys.argv[1]
with open(path, "rb") as source:
    plist = plistlib.load(source)
plist["AvailableLibraries"].sort(key=lambda library: library["LibraryIdentifier"])
with open(path, "wb") as destination:
    plistlib.dump(plist, destination, sort_keys=False)
PY

  if [[ "$(uname -s)" == Darwin ]]; then
    shopt -s nullglob
    for proc_macro in serde_derive thiserror_impl uniffi_internal_macros; do
      dylibs=("$root"/target/release/deps/lib"$proc_macro"-*.dylib)
      if [[ ${#dylibs[@]} -eq 0 ]]; then
        echo "Missing generated $proc_macro proc-macro dylib" >&2
        exit 1
      fi
      for dylib in "${dylibs[@]}"; do
        python3 - "$dylib" <<'PY'
import ctypes
import sys

ctypes.CDLL(sys.argv[1])
PY
      done
    done
  fi

  find "$stage" \
    -type f \( -name '*.swift' -o -name '*.h' -o -name 'module.modulemap' \) \
    -exec perl -pi -e 's/[ \t]+$//' {} +

  cutout_swift_ffi_source_fingerprint "$root" \
    >"$stage/.cutout-source.sha256"
}

cutout_replace_generated_directory "$stage" generate_stage
cutout_replace_generated_directory "$package" mv "$stage" "$package"
