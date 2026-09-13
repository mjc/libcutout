# RID-004: Reconnect starts a new ride before the stopped ride is saved or discarded

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Tap Stop while connected, then reconnect before choosing Save or Discard. The connection handler replaces the stopped active projection with a fresh recording. The old stopped row remains in history, but its terminal decision controls disappear.

## Evidence

CutoutSessionCore.swift:1959-1973 runs ensureRecordingForVehicle on each connection transition. lib.rs:10151-10162 treats Stopped as a reason to start a new ride, and 9910-9974 replaces active_ride_id. The UI explicitly presents Save/Discard for stopped at RideMapControlsView.swift:53-70.

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1959`
- `crates/cutout-mobile-ffi/src/lib.rs:10151`
- `crates/cutout-mobile-ffi/src/lib.rs:9910`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapControlsView.swift:53`

## Expected behavior and smallest remedy

Preserve an unresolved stopped ride across reconnect. Require an explicit new recording decision or settle the prior ride under a documented automatic-save policy.

## Acceptance / reproduction

Stop, reconnect, and verify the same ride ID and terminal controls remain until the user settles it. Also verify saved rides can begin a later ride deliberately.

## Comparison and limitations

Competitor automatic logging policy is configurable in WheelLog; see ../comparison-baseline.md. No universal reconnect rule is asserted.

Existing auto-start tests cover first connection and interrupted recovery, not stopped-pending-decision reconnect. This does not claim the old database row is deleted.

