# Ride recording, maps, history and capture audit coverage

Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`, reviewed 2026-09-13. This is a source audit with historical phone crash correlation, not a declaration that the phone app has passed these workflows.

## Findings

| ID | Priority | Evidence | Individual document |
|---|---|---|---|
| RID-001 | P2 | source-confirmed defect | [Map says recording is available after location permission is denied](RID-001-gps-permission-status.md) |
| RID-002 | P2 | source-confirmed defect | [High-accuracy background GPS keeps running without a ride](RID-002-gps-keeps-running.md) |
| RID-003 | P2 | source-confirmed defect | [Recovered interrupted ride offers a Stop button that cannot work](RID-003-interrupted-stop-invalid.md) |
| RID-004 | P2 | source-confirmed defect | [Reconnect starts a new ride before the stopped ride is saved or discarded](RID-004-reconnect-overrides-stop.md) |
| RID-005 | P2 | UX inconsistency | [Disconnect ends the ride activity but silently leaves map recording active](RID-005-disconnect-recording-continues.md) |
| RID-006 | P1 | source-confirmed defect | [Manual Resume after a phone reboot does not rebase the ride clock](RID-006-manual-recovery-clock.md) |
| RID-007 | P2 | UX inconsistency | [Follow mode changes zoom with route size and can cut off the route](RID-007-follow-camera-scale.md) |
| RID-008 | P2 | UX inconsistency | [Opening Map before recording cannot center on the phone](RID-008-empty-map-no-location.md) |
| RID-009 | P2 | source-confirmed defect | [GPS-only rides show no live speed despite valid phone GPS speed](RID-009-gps-only-speed-unavailable.md) |
| RID-010 | P2 | source-confirmed defect | [Fresh map telemetry status is reset to no telemetry on notifications](RID-010-telemetry-status-reset.md) |
| RID-011 | P2 | source-confirmed defect | [Automatically started rides ignore the saved listening-history preference](RID-011-automatic-ride-music-default.md) |
| RID-012 | P2 | UX inconsistency | [Listening history stops observing songs when the ride is backgrounded](RID-012-background-listening-history.md) |
| RID-013 | P1 | source-confirmed defect | [Two capture starts in one second can overwrite the earlier capture](RID-013-capture-filename-collision.md) |
| RID-014 | P2 | source-confirmed defect | [An old capture's late finish can replace the new recording status](RID-014-stale-capture-finish.md) |
| RID-015 | P2 | UX inconsistency | [Discard hides the ride but keeps its route and listening metadata in storage](RID-015-discard-retains-data.md) |
| RID-016 | P2 | UX inconsistency | [Saved rides have no visible deletion action](RID-016-saved-ride-delete-unavailable.md) |
| RID-017 | P2 | UX inconsistency | [Share Ride sends only summary text, with no route or telemetry artifact](RID-017-share-ride-is-summary.md) |
| RID-018 | P2 | UX inconsistency | [Saved diagnostic captures have no in-app open, share or delete path](RID-018-capture-file-unreachable.md) |
| RID-019 | P2 | source-confirmed defect | [Map mixes locale-derived distance and history speed with mph live speed](RID-019-map-units-disagree.md) |
| RID-020 | P2 | hypothesis needing device validation | [Ride lifecycle buttons synchronously wait for the database worker](RID-020-lifecycle-main-thread-blocking.md) |
| RID-021 | P2 | UX inconsistency | [A new ride can inherit a panned-away camera with Follow still disabled](RID-021-new-ride-keeps-old-camera.md) |
| RID-022 | P3 | UX inconsistency | [The elapsed timer actually reports recording time excluding pauses](RID-022-elapsed-excludes-pause.md) |
| RID-023 | P2 | UX inconsistency | [Ride database is excluded from backup without a usable ride backup/export flow](RID-023-ride-backup-excluded.md) |
| RID-024 | P2 | source-confirmed defect | [A transient database-open failure disables recording until the process restarts](RID-024-storage-failure-no-retry.md) |
| RID-025 | P1 | source-confirmed defect | [Switching vehicles can attribute the new wheel's telemetry to the old ride](RID-025-cross-vehicle-telemetry.md) |

## Feature-by-feature review

| Area | Source/behavior reviewed | Outcome and remaining evidence |
|---|---|---|
| Manual GPS start, pause, resume, stop, save, discard | Swift control selection -> app model -> serialized core -> Rust reducer/storage | Interrupted Stop mismatch, manual reboot clock, discard retention and elapsed naming documented. Basic reducer invariants pass existing tests. |
| Automatic recording | First supported vehicle notification -> ensure_recording_for_vehicle | Reconnect can replace a stopped pending decision. No user-facing automatic logging preference or motion-based auto-pause is wired in these paths; absence is a product-policy gap, not hundreds of separate bugs. |
| Explicit disconnect versus temporary BLE loss | Transport cleanup versus map lifecycle | Independent lifecycles are not made clear; switching vehicles can refresh the previous vehicle's telemetry. Same-vehicle brief reconnect must retain continuity. |
| Background and crash recovery | Background location manager, persisted timing, newest recoverable ride, synchronous constructor | Permission/power and manual clock findings documented. Historical launch watchdogs remain open; see root crash findings. Resume window is hardcoded to three hours for same-vehicle automatic interrupted recovery. |
| GPS admission and recording | Coordinate validation, accuracy, out-of-order samples, jump rejection, segment boundaries, pending writes | Existing tests confirm bounded live tail, rejection, separate gap/resume segments and no false bridge distance. Device GPX/route accuracy, tunnels and reduced accuracy still unverified. |
| Live camera | Initial viewport, follow/recenter, span ownership, route identity | No pre-ride phone recenter, route-scale follow and cross-ride camera persistence documented. Exact pan/programmatic callback ordering needs MapKit UI reproduction. |
| History maps | Pagination, search/date/vehicle filters, selected/detail cancellation, viewport budgets, canonical endpoints | Source has bounded/cancellable projections and generation guards; these are safeguards, not proof that historical phone crashes are fixed. No new certain pagination defect found. Large real databases remain a performance test priority. |
| Metrics | GPS versus wheel speed, map telemetry provenance, units, duration | Missing GPS-only speed, freshness reset, mismatched units and elapsed wording documented. Route distance is GPS-derived; no claim that wheel and GPS distance should be identical. |
| History management | Saved detail, ShareLink, music forget, lifecycle deletion | No saved/imported ride delete; Share only summary; capture files lack user retrieval; backup exclusion documented. Rename/tagging is a product gap, not assumed broken implemented behavior. |
| PEVCAP streaming capture | Swift start/finish -> Rust bounded writer, durability, asynchronous terminal events | Filename collision and stale terminal event attribution documented. Queue health and flush failure are already represented, including picker status after finish. These existing guards do not close crash reports. |
| PEVCAP import/export | Rust preflight/commit primitives and mobile app wiring search | Storage APIs and duplicate-aware import tests exist; no current phone import/export UI was found. No independent format corruption claim was manufactured. Full malformed/partial artifact and low-disk matrix remains to run. |
| Listening history | Scope contract, persisted preference, automatic/manual ride creation, provider suspension, deletion | Automatic start ignores default; background observations stop. Metadata-only contract preserved. Deleting DB metadata does not erase separately created captures, as contract explicitly explains. |
| Ride-data replay | Current history UI and docs/music-integration.md | Route/telemetry playback controls are not implemented in current iOS history UI. Contract explicitly calls replay future work; do not misrepresent missing playback as a reproduced crash or add music transport/audio behavior. |
| Concurrency | MainActor lifecycle calls, ride queue, Rust mutexes/worker, detached projection tasks | Lifecycle UI blocking hypothesis documented; historical launch watchdog source paths independently survive and are owned by root crash documents. |

## Historical phone crash correlation

The copied phone report `CutoutApp-2026-09-12-151519.ips` records a scene-create watchdog with its triggered main thread inside RideDatabaseHandle.project_route_points -> MobileRideMapCoreInner.new -> MobileRideMapCore.with_database -> MobileRideMapState.init(database:) -> CutoutSessionCore.init -> CutoutAppModel allocation. Current source still performs synchronous constructor recovery:

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:580`
- `swift/CutoutMobile/Sources/CutoutMobile/RideMapState.swift:695`
- `crates/cutout-mobile-ffi/src/lib.rs:9672`
- `crates/cutout-mobile-ffi/src/lib.rs:9731`

Five other inspected reports (Sep 8 22:08:51 / 22:41:41, Sep 9 15:38:12 / 18:02:42, Sep 12 14:39:15) show main-thread SQLite configure_connection during database bootstrap. Current `storage/service.rs:55-56` still configures the connection before starting its worker, and `storage.rs:3631-3634` still runs PRAGMA quick_check(1). Moving history projection tasks off-main does not remove either startup chain. Root owns CRH-004 and CRH-010 to avoid duplicate tickets. Historical evidence remains relevant and open until the same workflows pass on the target phone with its real data.

## Checks performed and limits

- Read repository AGENTS.md and docs/music-integration.md. Used Ponytail and SwiftUI review guidance; MCPLS was unavailable, so source tracing used rg and numbered source reads.
- Default Nix entry initially encountered the shared cache failure. Used the available same-project cached shell: `nix develop /nix/store/7qk8y1zrsrp9dnhv6kiailgjzkmigddj-nix-shell-env -c ...`.
- Ran `cargo test -p cutout-ride-maps --lib --offline`: **35 passed, 0 failed, 0 filtered out**. Existing tests check valid lifecycle transitions, monotonic/accuracy admission, gap segmentation and distance, pause-excluded duration, bounded projection, antimeridian camera bounds and canonical endpoints. They do not validate the listed Swift production integration gaps.
- Reviewed relevant existing Swift model/presentation tests and mobile-FFI recovery/capture tests in source; did not claim they ran.
- No fixes, new application tests, commits, pushes, provider commands, file sharing or destructive phone actions were performed.
- No new phone crash, reboot-resume, permission denial, battery use, file collision or database restore was reproduced. Crash correlation used actual copied historical logs; source-only reports say so individually.
- Competitor facts are limited to the root [comparison baseline](../comparison-baseline.md); undocumented behavior is explicitly unverified. Findings are split by distinct rider problem and remedy rather than a target count.

