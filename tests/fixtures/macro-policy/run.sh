#!/usr/bin/env bash
set -euo pipefail
fixture="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$fixture/../../.." && pwd)"
[[ "${DEVENV_ROOT:-}" == "$root" ]] || { echo 'Run from the project devenv shell' >&2; exit 1; }
scratch="$(mktemp -d "${TMPDIR:-/tmp}/cutout-macro-policy.XXXXXX")"
export CLIPPY_CONF_DIR="$root"

# The fixture uses the same test-only exemption as the workspace crate roots.
# Check the central deny too, so removing it cannot leave this regression green.
python3 - "$root/Cargo.toml" <<'PY'
import sys
import re
with open(sys.argv[1]) as manifest:
    section = re.search(r'(?ms)^\[workspace\.lints\.clippy\]\s*\n(.*?)(?=^\[|\Z)', manifest.read())
assert section is not None
assert re.search(r'(?m)^disallowed_macros\s*=\s*"deny"\s*$', section[1])
PY

if clippy-driver --edition=2024 --crate-type=lib --emit=metadata \
    -D clippy::disallowed_macros --error-format=json \
    "$fixture/probe.rs" -o "$scratch/production.rmeta" 2> "$scratch/production.jsonl"; then
    echo 'FAIL: production matches! was accepted' >&2
    exit 1
fi
python3 - "$scratch/production.jsonl" <<'PY'
import json
import sys
with open(sys.argv[1]) as log:
    diagnostics = [json.loads(line) for line in log]
errors = [item for item in diagnostics if (item.get('code') or {}).get('code') == 'clippy::disallowed_macros']
assert len(errors) == 3, diagnostics
assert all(item['level'] == 'error' for item in errors), errors
PY

clippy-driver --edition=2024 --test -D clippy::disallowed_macros \
    "$fixture/probe.rs" -o "$scratch/test-probe"
"$scratch/test-probe"
echo 'PASS: production rejects unqualified/core/std matches!; test builds allow all three'
