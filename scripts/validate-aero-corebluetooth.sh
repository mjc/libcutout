#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

case "$(uname -s)" in
  Darwin)
    export DYLD_LIBRARY_PATH="$root/target/debug:${DYLD_LIBRARY_PATH:-}"
    swift_cmd=(env -u SDKROOT -u DEVELOPER_DIR swift)
    ;;
  *)
    echo "Aero CoreBluetooth validation requires Darwin/CoreBluetooth" >&2
    exit 1
    ;;
esac

timeout_seconds="${1:-45}"
echo "libcutout_commit=$(git rev-parse HEAD)"

package_dir="$root/swift/CutoutMobile"
cutout_prepare_swift_package_workspace

"${swift_cmd[@]}" run \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$root/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  CutoutMobileLiveValidator \
  "$timeout_seconds"
