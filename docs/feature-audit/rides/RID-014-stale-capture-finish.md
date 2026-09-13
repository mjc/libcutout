# RID-014: An old capture's late finish can replace the new recording status

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Finish/disconnect one capture and start another before the first writer finishes. Its delayed .finished or .failed event overwrites the newer capture's filename/status because events carry no session identity check.

## Evidence

CutoutSessionCore.swift:2168-2181 finishes the old writer on a global queue and later publishes to BLE/main. CutoutAppModel.applyCaptureEvent at 3187-3213 unconditionally assigns captureFileName and captureStatus for terminal events. The new capture's .started event uses that same model state.

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2168`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:3187`

## Expected behavior and smallest remedy

Carry capture identity through started/progress/terminal events and apply terminal results only to the matching current capture; retain older outcomes separately if useful.

## Acceptance / reproduction

Hold writer A finalization, start B and publish B progress, then release A success/failure. B must remain visibly recording with B's filename.

## Comparison and limitations

Competitor event ordering unverified; this is an internal ownership race.

Source-confirmed missing identity fence. Scheduling-sensitive visual behavior requires an injected delayed-finalization test; no phone reproduction.

