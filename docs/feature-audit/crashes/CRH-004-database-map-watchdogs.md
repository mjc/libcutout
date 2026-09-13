# CRH-004: Database integrity checking blocks app launch and has caused watchdog kills

- Severity: P1
- Evidence status: phone-confirmed historical watchdogs; matching current synchronous path
- Status: OPEN — current phone acceptance missing
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and observed behavior

Launch Cutout with the existing ride database. Five phone watchdog reports show the main thread reading SQLite pages through configure_connection -> service.acquire -> RideDatabase.open -> RustPersistenceStore.shared. The phone's current database is 1,139,482,624 bytes (about 1.06 GiB).

Reports: `CutoutApp-2026-09-08-220851.ips`, `CutoutApp-2026-09-08-224141.ips`, `CutoutApp-2026-09-09-153812.ips`, `CutoutApp-2026-09-09-180242.ips`, `CutoutApp-2026-09-12-143915.ips`. See [hashes and binary UUIDs](../phone-evidence.md).

## Expected behavior

Database validation and recovery must preserve data integrity while allowing a responsive initial screen, including with real accumulated ride history.

## Current source evidence

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:580-583` obtains RustPersistenceStore.shared synchronously in the default initializer.
- `swift/CutoutMobile/Sources/CutoutMobile/RustPersistenceStore.swift:9-29` initializes the shared database by calling openRideDatabase inline.
- `crates/libcutout-persistence/src/storage/service.rs:55-63` opens and configures the connection before spawning the database worker.
- `crates/libcutout-persistence/src/storage.rs:3621-3643` performs PRAGMA quick_check(1), migration, repair and recovery in that initialization path.

The production startup still has the blocking shape shown by the historical stacks. Detached history queries do not remove this constructor work. A quick_check result limit of one does not mean it reads one data page.

## Minimal remedy direction

Move database bootstrap into an explicit asynchronous initialization boundary with visible loading/unavailable state. Preserve integrity checking, migration ordering and one shared database owner; do not simply delete validation to make launch appear fast.

## Acceptance checks

Cold-launch against a realistically sized isolated database and the user's phone data without deleting it. Capture time to first usable screen and main-thread responsiveness while validation runs. Check valid, corrupt, temporarily inaccessible and migrating databases, plus retry behavior. Record exact installed binary UUID and absence of launch watchdogs.

## Comparison and limits

A usable launch is a baseline expectation independent of competitor design. These are historical crashes with a surviving code path, not a new reproduction of a current build. Do not close based on report age or background workers that begin only after initialization. [CRH-010](CRH-010-active-route-launch-projection.md) covers the separate route restoration work after database opening; [RID-024](../rides/RID-024-storage-failure-no-retry.md) covers recovery after open failure.
