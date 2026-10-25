# Account-deletion Simulator harness

Run the two account-deletion UI tests on an Apple Silicon Mac from this
checkout's Devenv shell. Choose one available iOS Simulator UDID; the harness
uses it for both sequential settings runs:

```console
devenv shell -- python3 scripts/run-account-deletion-ui-tests.py \
  --simulator-udid YOUR_SIMULATOR_UDID
```

Find available UDIDs with `xcrun simctl list devices available`. The harness
checks the active Devenv Rust compiler against `rust-toolchain.toml`, the
Simulator, free ports `127.0.0.1:45986` and `127.0.0.1:45987`, and available
disk space before it builds anything. It refuses an occupied port and never
signals its owner.

Each run creates a fresh SQLite database and evidence directory under
`~/Library/Logs/Cutout/account-deletion-ui-tests/`, outside disposable build
caches. Use `--evidence-root PATH` to select another location. It reuses the
repository Cargo target and the UI runner's existing `target/xcode-ui-tests`
Derived Data cache. The
harness initializes the database with `cutout-server`, seeds the three fixed
test contributor IDs using SHA-256 token hashes, and creates their public
points through the real local HTTP API. The UI and the harness both use
`http://127.0.0.1:45987/`. A proxy on that port sets `Host: rides.cutout.lol`
and one `X-Real-IP`, then forwards to the server on `127.0.0.1:45986`. The
server is started with `--trusted-proxy 127.0.0.1` and a fixture SoundCloud
credential file. The harness stops the server and the proxy on success or
failure.

Builds use two Cargo jobs with incremental compilation disabled and two Xcode
jobs. The UI runner disables parallel testing. Coordinate one existing Simulator
and build checkout with other active Libcutout work before starting; do not
create another full target cache for this harness. Shut down the owned Simulator
after both settings cells finish.

The harness runs these existing tests through
`scripts/run-ios-ui-tests.sh`, using the same Simulator for each settings cell:

- `testPickerSurfaceHomeMapRouteAccountDeletionRetainsLocalData` with Large
  Dynamic Type
- `testPickerSurfaceHomeMapRouteAccountDeletionAtAccessibilityDynamicTypeRetainsLocalData`
  with Accessibility XXXL Dynamic Type

Both use Light appearance. The UI runner owns Simulator settings, per-run
timeouts (1,200 seconds), arm64 build configuration, build work, and `.xcresult`
output. Its standard Derived Data and Cargo caches remain reusable by later
runs.

After both UI tests pass, the harness checks SQLite and public HTTP readback:
the two tested contributor accounts are gone, there are exactly two matching
deletion receipts, and their old tokens receive HTTP 401 on the moderation
endpoint. Their public points retain the submitted body and published status
under the generic Anonymous owner. The unrelated contributor and point remain
intact, and its token receives HTTP 403.

The evidence directory is retained. It contains the fresh database, Cargo and
server logs, separate UI-run logs, `run.json` with the source revision and
server binary SHA-256, `seeded-public-spots.json`, and `result.json`. The shared
UI runner stores Derived Data and `.xcresult` bundles in its normal cache.
