#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
project="$root/swift/CutoutMobile/CutoutApp.xcodeproj"

if [[ "$(cutout_host_os)" != Darwin ]]; then
  echo "CutoutApp Xcode bootstrap requires Darwin/Xcode" >&2
  exit 1
fi

cutout_use_xcode_developer_dir
cutout_ensure_swift_ffi_build_input "$root"
exec /usr/bin/open "$project"
