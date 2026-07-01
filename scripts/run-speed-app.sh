#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "CutoutMobileSpeedApp requires macOS/CoreBluetooth" >&2
  exit 1
fi

./scripts/smoke-swift-package.sh

export DYLD_LIBRARY_PATH="$root/target/debug:${DYLD_LIBRARY_PATH:-}"
exec env -u SDKROOT -u DEVELOPER_DIR swift run \
  --package-path target/swift-package-smoke/CutoutMobile \
  -Xlinker -L -Xlinker "$root/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  CutoutMobileSpeedApp
