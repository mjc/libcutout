#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cutout_use_xcode_developer_dir "/Applications/Xcode.app/Contents/Developer"
(cd "$root" && cargo run --quiet -p cutout-dev -- swift-ffi)
