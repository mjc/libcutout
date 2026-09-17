#!/usr/bin/env bash
set -euo pipefail

fixture="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$fixture/../../.." && pwd)"
[[ "${DEVENV_ROOT:-}" == "$root" ]] || { echo 'Run from the project devenv shell' >&2; exit 1; }
[[ "$(uname -sm)" == 'Darwin arm64' ]] || { echo 'Requires native macOS arm64 and Xcode' >&2; exit 1; }

source_file="$root/crates/cutout-core/src/lib.rs"
git -C "$root" diff --quiet -- "$source_file" || {
  echo "Refusing to modify already-dirty $source_file" >&2
  exit 1
}
backup="$(mktemp)"
cp "$source_file" "$backup"
cleanup() {
  cp "$backup" "$source_file"
  rm -f "$backup"
}
trap cleanup EXIT

run_production_test() {
  cargo cutout swift -- test \
    --package-path "$root/swift/CutoutMobile" \
    --filter AeroSettingsSimulatorTests
}

generation() {
  sed -n 's#// cutout-generation: ##p' "$root/target/swift-ffi/Package.swift"
}

echo 'Preparing and testing production FFI generation A'
run_production_test
generation_a="$(generation)"
[[ "$generation_a" =~ ^[0-9a-f]{64}$ ]]

printf '\n// Production FFI A/B fixture implementation change.\n' >> "$source_file"
echo 'Preparing and testing production FFI generation B'
run_production_test
generation_b="$(generation)"
[[ "$generation_b" =~ ^[0-9a-f]{64}$ && "$generation_a" != "$generation_b" ]]

echo "PASS: production SwiftPM A -> B through cutout-dev and VerifyRustArtifact ($generation_a -> $generation_b)"
