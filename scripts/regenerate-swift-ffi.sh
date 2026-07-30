#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root/crates/cutout-mobile-ffi"
cutout_use_xcode_developer_dir "/Applications/Xcode.app/Contents/Developer"

cargo swift package \
  --platforms ios@18 macos@15 \
  --release \
  --name CutoutMobileFFI \
  --lib-type static \
  --skip-toolchains-check \
  --accept-all \
  --swift-tools-version 6.0 \
  --silent

find "$root/crates/cutout-mobile-ffi/CutoutMobileFFI" \
  -type f \( -name '*.swift' -o -name '*.h' -o -name 'module.modulemap' \) \
  -exec perl -pi -e 's/[ \t]+$//' {} +

cutout_swift_ffi_source_fingerprint "$root" \
  >"$root/crates/cutout-mobile-ffi/CutoutMobileFFI/.cutout-source.sha256"
