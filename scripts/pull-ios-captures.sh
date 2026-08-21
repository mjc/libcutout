#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/swift-package-common.sh"

root="$(cutout_repo_root)"
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "iOS capture pull requires Darwin" >&2
  exit 1
fi

cutout_use_xcode_developer_dir

device_udid="${CUTOUT_IOS_DEVICE_UDID:-$(cutout_connected_ios_device_udid)}"
bundle_id="${CUTOUT_IOS_APP_BUNDLE_ID:-io.cutout.cutoutapp}"
destination="${CUTOUT_IOS_CAPTURE_DESTINATION:-$root/target/ios-captures}"
limit="${CUTOUT_IOS_CAPTURE_LIMIT:-5}"
listing_json="$(mktemp "${TMPDIR:-/tmp}/cutout-ios-captures.XXXXXX.json")"
trap 'rm -f "$listing_json"' EXIT

mkdir -p "$destination"

xcrun devicectl --quiet device info files \
  --device "$device_udid" \
  --domain-type appDataContainer \
  --domain-identifier "$bundle_id" \
  --subdirectory Documents \
  --json-output "$listing_json" >/dev/null

capture_names=()
while IFS= read -r capture_name; do
  capture_names+=("$capture_name")
done < <(python3 - "$listing_json" "$limit" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)

limit = int(sys.argv[2])
files = [
    f for f in data.get("result", {}).get("files", [])
    if f.get("name", "").startswith("cutout-btle-capture-") and f.get("name", "").endswith(".jsonl")
]
files.sort(key=lambda f: f.get("metadata", {}).get("lastModDate", ""), reverse=True)
for file in files[:limit]:
    print(file["name"])
PY
)

if [[ "${#capture_names[@]}" -eq 0 ]]; then
  echo "no cutout-btle-capture JSONL files found in $bundle_id Documents" >&2
  exit 1
fi

for capture_name in "${capture_names[@]}"; do
  xcrun devicectl --quiet device copy from \
    --device "$device_udid" \
    --domain-type appDataContainer \
    --domain-identifier "$bundle_id" \
    --source "Documents/$capture_name" \
    --destination "$destination/$capture_name"
done

find "$destination" -maxdepth 1 -name 'cutout-btle-capture-*.jsonl' -print
