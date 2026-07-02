#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

case "$(cutout_host_os)" in
  Darwin)
    export DYLD_LIBRARY_PATH="$root/target/debug:${DYLD_LIBRARY_PATH:-}"
    ;;
  Linux)
    export LD_LIBRARY_PATH="$root/target/debug:${LD_LIBRARY_PATH:-}"
    ;;
  *)
    echo "unsupported host OS for Swift package smoke: $(cutout_host_os)" >&2
    exit 1
    ;;
esac

swift_cmd=($(cutout_swift_runtime_command))

package_dir="target/swift-package-smoke/CutoutMobile"
cutout_prepare_swift_package_workspace "$package_dir"

"${swift_cmd[@]}" run \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$root/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  CutoutMobileSmoke

if [[ "$(uname -s)" == Darwin ]]; then
  cutout_build_ios_cutout_app_bundle >/dev/null
  "${swift_cmd[@]}" build \
    --package-path "$package_dir" \
    -Xlinker -L -Xlinker "$root/target/debug" \
    -Xlinker -lcutout_mobile_ffi \
    --target CutoutMobileLiveValidator
fi
