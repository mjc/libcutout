# CRH-010: Restoring an active ride synchronously projects its route during launch

- Severity: P1
- Evidence status: phone-confirmed historical watchdog; matching current synchronous path
- Status: OPEN — current phone acceptance missing
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and observed behavior

Launch with a recoverable ride. `CutoutApp-2026-09-12-151519.ips` records a scene-create watchdog with the triggered main thread inside RideDatabaseHandle.project_route_points -> MobileRideMapCoreInner.new -> MobileRideMapCore.with_database -> MobileRideMapState.init(database:) -> CutoutSessionCore.init -> CutoutAppModel allocation. See [phone evidence](../phone-evidence.md).

## Expected behavior

Offer a responsive recovery screen before expensive route reconstruction. Recovering a summary such as background-gap count should not require blocking initial scene creation on a full route projection.

## Current source evidence

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:580-583` constructs MobileRideMapState synchronously.
- `swift/CutoutMobile/Sources/CutoutMobile/RideMapState.swift:695-703` constructs the Rust core using the database.
- `crates/cutout-mobile-ffi/src/lib.rs:9658-9676` calls restore_active_ride within the constructor.
- `crates/cutout-mobile-ffi/src/lib.rs:9724-9742` projects the recoverable route with output budget 1 merely to read background_gap_count, then restores route samples. A small output budget is not proof that input work is bounded.

This exact startup shape remains despite separate asynchronous live/history projection tasks.

## Minimal remedy direction

Separate lightweight recovery metadata from full route restoration, and make necessary database/projection work asynchronous relative to scene creation. Preserve interrupted-ride identity, gap semantics and clock handling.

## Acceptance checks

Restore long interrupted rides with many segments and location points. Verify first screen appears promptly, commands wait on truthful recovery state, recovered summaries remain correct and no watchdog occurs. Retain phone runtime evidence for the installed artifact; test cancellation and backgrounding during restore.

## Comparison and limits

This is an observed watchdog path, not a proof that every route size triggers it. It is separate from [CRH-004](CRH-004-database-map-watchdogs.md), which blocks before the worker starts. [RID-020](../rides/RID-020-lifecycle-main-thread-blocking.md) covers later manual lifecycle waits. Historical age does not close either issue.
