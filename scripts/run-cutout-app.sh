#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "CutoutApp requires macOS/CoreBluetooth" >&2
  exit 1
fi

package_dir="$root/swift/CutoutMobile"
cutout_prepare_swift_package_workspace

swift_cmd=($(cutout_swift_runtime_command))
export DYLD_LIBRARY_PATH="$root/target/debug:${DYLD_LIBRARY_PATH:-}"
exec "${swift_cmd[@]}" run \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$root/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  CutoutApp \
  "$@"
