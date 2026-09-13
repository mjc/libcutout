# RID-015: Discard hides the ride but keeps its route and listening metadata in storage

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Stop a ride and confirm Discard. The ride vanishes from ordinary history, but the transition only changes the lifecycle row; it does not delete route points, segments or music records.

## Evidence

lifecycle.rs:37 calls Discard permanent. storage.rs:6006-6031 updates ride state/timing only. History at 6463 and 6574 excludes discarded rows. lib.rs:10306-10314 drops in-memory projection but does not delete durable content.

- `crates/cutout-ride-maps/src/lifecycle.rs:37`
- `crates/libcutout-persistence/src/storage.rs:6006`
- `crates/libcutout-persistence/src/storage.rs:6463`
- `crates/cutout-mobile-ffi/src/lib.rs:10306`
- `swift/CutoutMobile/Apps/CutoutApp/Localizable.xcstrings:1751`

## Expected behavior and smallest remedy

Decide whether Discard means deletion or recoverable trash. Implement scoped durable deletion for the former, or disclose retained trash and provide purge/recovery for the latter.

## Acceptance / reproduction

Create a ride with location and music, discard it, reopen the database and inspect all dependent tables. The behavior must match the visible wording, including storage reclamation policy.

## Comparison and limitations

Competitor deletion retention unverified. App localization describes the confirmation as deleting a stopped ride.

Logical row retention is confirmed; this is not a claim about secure erasure, external exports or recoverability through the current UI.

