#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
package="$root/swift/CutoutMobile"

if [[ "$(cutout_host_os)" != Darwin ]]; then
  echo "CutoutMobile SourceKit preparation requires Darwin/Xcode" >&2
  exit 1
fi

cutout_use_xcode_developer_dir
cutout_ensure_swift_ffi_build_input "$root"
swift package describe --package-path "$package" >/dev/null
printf '%s\n' "$package"
