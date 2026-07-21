#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "Aero CoreBluetooth validation requires Darwin/CoreBluetooth" >&2
  exit 1
fi

timeout_seconds="${1:-45}"
echo "libcutout_commit=$(git rev-parse HEAD)"

swift run \
  --package-path "$root/swift/CutoutMobile" \
  CutoutMobileLiveValidator \
  "$timeout_seconds"
