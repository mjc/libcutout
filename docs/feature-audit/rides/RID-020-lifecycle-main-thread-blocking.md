# RID-020: Ride lifecycle buttons synchronously wait for the database worker

- Severity: P2
- Evidence status: hypothesis needing device validation
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Tap Start/Pause/Resume/Stop/Save while a large history projection or slow storage operation is occupying the shared database worker. The MainActor action synchronously waits on the ride queue and Rust persistence response, which can stall interaction.

## Evidence

CutoutAppModel.swift:1321-1337 invokes core lifecycle methods synchronously. CutoutSessionCore.swift:3245-3250 uses rideMapQueue.sync; lib.rs:9534-9547 transition_state calls the synchronous RideDatabaseHandle transition. History projection paths already use detached work and cancellation.

- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:1321`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:3245`
- `crates/cutout-mobile-ffi/src/lib.rs:9534`

## Expected behavior and smallest remedy

Run lifecycle commands off MainActor through the existing serialized boundary, with an explicit pending state and one result application. Keep command ordering and rider intent intact.

## Acceptance / reproduction

Inject a delayed worker response while issuing Pause or Stop; UI remains responsive, disables duplicate submissions, and reflects success/failure exactly once.

## Comparison and limitations

Competitor concurrency details unverified.

The blocking call chain is source-confirmed; user-visible latency and watchdog termination are unmeasured. This is not a reproduced crash.

