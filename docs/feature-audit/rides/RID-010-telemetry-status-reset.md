# RID-010: Fresh map telemetry status is reset to no telemetry on notifications

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Ride with both valid GPS and vehicle packets. A location projection can show fresh telemetry, then the next vehicle notification publishes a map snapshot and resets the app's status to associatedNoTelemetry.

## Evidence

CutoutSessionCore.swift:1977-1984 observes telemetry and publishes a snapshot. CutoutAppModel.swift:698-704 overwrites rideMapLiveTelemetryState using association alone, choosing associatedNoTelemetry for every associated ride. Rust tracks real freshness independently in recording.rs:1003-1058.

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1977`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:698`
- `crates/cutout-ride-maps/src/recording.rs:1003`

## Expected behavior and smallest remedy

Publish or query the Rust freshness state with the snapshot, or preserve the existing point/projection provenance until a genuine freshness transition.

## Acceptance / reproduction

Feed fresh telemetry, an accepted GPS point, then another telemetry notification. The status must not regress to 'no telemetry'; later staleness should still display correctly.

## Comparison and limitations

Competitor status wording unverified; this contradicts the same app's confirmed telemetry.

The exact visible flicker depends on notification and projection ordering. Source identifies the unconditional incorrect assignment.

