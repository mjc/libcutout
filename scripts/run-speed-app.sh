#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "CutoutMobileSpeedApp requires macOS/CoreBluetooth" >&2
  exit 1
fi

./scripts/smoke-swift-package.sh

package_dir="target/swift-package-smoke/CutoutMobile"
swift_cmd=(env -u SDKROOT -u DEVELOPER_DIR swift)
export DYLD_LIBRARY_PATH="$root/target/debug:${DYLD_LIBRARY_PATH:-}"
exec "${swift_cmd[@]}" run \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$root/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  CutoutMobileSpeedApp
