#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

case "$(uname -s)" in
  Darwin)
    export DYLD_LIBRARY_PATH="$root/target/debug:${DYLD_LIBRARY_PATH:-}"
    swift_cmd=(env -u SDKROOT -u DEVELOPER_DIR swift)
    ;;
  Linux)
    export LD_LIBRARY_PATH="$root/target/debug:${LD_LIBRARY_PATH:-}"
    swift_cmd=(swift)
    ;;
  *)
    echo "unsupported host OS for Swift package smoke: $(uname -s)" >&2
    exit 1
    ;;
esac

package_dir="target/swift-package-smoke/CutoutMobile"
CUTOUT_SWIFT_SOURCEKIT_PACKAGE_DIR="$package_dir" ./scripts/prepare-swift-sourcekit-workspace.sh >/dev/null

"${swift_cmd[@]}" run \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$root/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  CutoutMobileSmoke

if [[ "$(uname -s)" == Darwin ]]; then
  "${swift_cmd[@]}" build \
    --package-path "$package_dir" \
    -Xlinker -L -Xlinker "$root/target/debug" \
    -Xlinker -lcutout_mobile_ffi \
    --target CutoutMobileSpeedApp
  "${swift_cmd[@]}" run \
    --package-path "$package_dir" \
    -Xlinker -L -Xlinker "$root/target/debug" \
    -Xlinker -lcutout_mobile_ffi \
    CutoutMobileSpeedApp \
    --smoke
  "${swift_cmd[@]}" build \
    --package-path "$package_dir" \
    -Xlinker -L -Xlinker "$root/target/debug" \
    -Xlinker -lcutout_mobile_ffi \
    --target CutoutMobileLiveValidator
fi
