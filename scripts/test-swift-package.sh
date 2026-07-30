#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cutout_require_swift_ffi_build_input "$root"
exec swift test --package-path "$root/swift/CutoutMobile" "$@"
