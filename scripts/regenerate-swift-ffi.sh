#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root/crates/cutout-mobile-ffi"
cutout_use_xcode_developer_dir "/Applications/Xcode.app/Contents/Developer"

if [[ "${RUSTC_WRAPPER+x}" != x || -n "$RUSTC_WRAPPER" ||
      "${RUSTC_WORKSPACE_WRAPPER+x}" != x || -n "$RUSTC_WORKSPACE_WRAPPER" ]]; then
  echo "Swift FFI regeneration requires nix develop with Cargo wrappers disabled" >&2
  exit 1
fi

cargo swift package \
  --platforms ios@18 macos@15 \
  --release \
  --name CutoutMobileFFI \
  --lib-type static \
  --skip-toolchains-check \
  --accept-all \
  --swift-tools-version 6.0 \
  --silent

python3 - "$root/crates/cutout-mobile-ffi/CutoutMobileFFI/cutout_mobile_ffiFFI.xcframework/Info.plist" <<'PY'
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

find "$root/crates/cutout-mobile-ffi/CutoutMobileFFI" \
  -type f \( -name '*.swift' -o -name '*.h' -o -name 'module.modulemap' \) \
  -exec perl -pi -e 's/[ \t]+$//' {} +

cutout_swift_ffi_source_fingerprint "$root" \
  >"$root/crates/cutout-mobile-ffi/CutoutMobileFFI/.cutout-source.sha256"
