# RID-011: Automatically started rides ignore the saved listening-history preference

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Enable listening history in Setup, then begin the normal ride by connecting a wheel. The Rust-created ride starts with history disabled; the saved preference is applied only when manually starting a GPS-only ride.

## Evidence

lib.rs:9961 resets music_history_policy during every new ride. The connection route at CutoutSessionCore.swift:1969 calls ensureRecordingForVehicle. CutoutAppModel.swift:1255-1261 applies musicHistoryPolicyStore.policy only in startGpsOnlyRide, while the automatic snapshot callback at 698-704 does not. MusicIntegration.swift:760 sends events to the Rust policy.

- `crates/cutout-mobile-ffi/src/lib.rs:9961`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1969`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:1255`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:698`
- `swift/CutoutMobile/Sources/CutoutMobile/MusicIntegration.swift:760`

## Expected behavior and smallest remedy

Apply the saved default once at the shared new-ride creation boundary, without overwriting an existing ride's deletion or retention choice.

## Acceptance / reproduction

Enable history, connect through the production connection path, change tracks, stop/save and reopen. Verify the expected metadata exists; a disabled default still records nothing.

## Comparison and limitations

Competitor music history behavior unverified. The expected behavior follows docs/music-integration.md and Setup's future-ride preference.

Existing testMusicHistoryDefaultIsLoadedForFutureRides covers a manual start, not automatic vehicle connection. No provider/phone session was exercised.

