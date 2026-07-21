#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

package_dir="$root/swift/CutoutMobile"

swift run \
  --package-path "$package_dir" \
  CutoutMobileSmoke

if [[ "$(uname -s)" == Darwin ]]; then
  swift build \
    --package-path "$package_dir" \
    --target CutoutMobileLiveValidator
fi
