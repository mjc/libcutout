#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(cutout_host_os)" != Darwin ]]; then
  echo "CutoutApp ad hoc export requires Darwin" >&2
  exit 1
fi

archive_path="${CUTOUT_IOS_AD_HOC_ARCHIVE:-}"
if [[ -n "$archive_path" && ! -d "$archive_path" ]]; then
  echo "CUTOUT_IOS_AD_HOC_ARCHIVE does not exist: $archive_path" >&2
  exit 1
fi

ipa_path="${CUTOUT_IOS_AD_HOC_IPA:-$(cutout_export_ios_ad_hoc_ipa)}"
export_path="$(dirname "$ipa_path")"
archive_path="${archive_path:-${CUTOUT_IOS_AD_HOC_ARCHIVE_PATH:-$root/target/xcode-ad-hoc/CutoutApp.xcarchive}}"

echo "ios_ad_hoc_archive=$archive_path"
echo "ios_ad_hoc_export_path=$export_path"
echo "ios_ad_hoc_ipa=$ipa_path"
echo "ios_ad_hoc_method=release-testing"
