#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
cd "$root"

case "$(cutout_host_os)" in
  Darwin)
    export DYLD_LIBRARY_PATH="$target_dir/debug:${DYLD_LIBRARY_PATH:-}"
    ;;
  Linux)
    export LD_LIBRARY_PATH="$target_dir/debug:${LD_LIBRARY_PATH:-}"
    ;;
  *)
    echo "unsupported host OS for Swift package tests: $(cutout_host_os)" >&2
    exit 1
    ;;
esac

swift_cmd=($(cutout_swift_runtime_command))
package_dir="${CUTOUT_SWIFT_TEST_PACKAGE_DIR:-target/swift-package-test/CutoutMobile}"

cutout_prepare_swift_package_workspace "$package_dir"

"${swift_cmd[@]}" test \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$target_dir/debug" \
  -Xlinker -lcutout_mobile_ffi \
  "$@"
