#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "MELK CoreBluetooth validation requires Darwin/CoreBluetooth" >&2
  exit 1
fi

timeout_seconds="${1:-60}"
platform_identifier="${2:-}"
if ! [[ "$timeout_seconds" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  echo "usage: $(basename "$0") [non-negative-timeout-seconds] [platform-identifier]" >&2
  exit 2
fi
cutout_ensure_swift_ffi_build_input "$root"

echo "libcutout_commit=$(git rev-parse HEAD)"

args=("$timeout_seconds")
if [[ -n "$platform_identifier" ]]; then
  args+=("$platform_identifier")
fi

exec swift run \
  --package-path "$root/swift/CutoutMobile" \
  MelkLightingLiveValidator \
  "${args[@]}"
