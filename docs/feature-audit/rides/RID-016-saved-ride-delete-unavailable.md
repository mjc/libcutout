# RID-016: Saved rides have no visible deletion action

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Save an accidental/private ride, then open its history detail. The available destructive action forgets music only; there is no way to remove the ride's location history from the app.

## Evidence

RideMapHistoryListView.swift:17-26 provides only selection. RideMapHistoryDetailSections.swift:259-346 offers music deletion, preview and Share. Rust lifecycle.rs:58-65 excludes Saved and Imported from Discard. Search of the app and ride database FFI found music deletion but no saved-ride deletion flow.

- `swift/CutoutMobile/Apps/CutoutApp/RideMapHistoryListView.swift:17`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapHistoryDetailSections.swift:259`
- `crates/cutout-ride-maps/src/lifecycle.rs:58`

## Expected behavior and smallest remedy

Add one explicit delete-ride action with confirmation and durable dependent-record cleanup; clearly distinguish separate capture/export files.

## Acceptance / reproduction

Save and import rides, delete one from history, relaunch, and verify it and its dependent local records are gone while other rides remain.

## Comparison and limitations

Competitor ride-management behavior unverified; this is a missing user control over persisted local route data.

Source scope covers the current iOS UI/FFI. Does not assert that uninstalling or external developer access cannot remove data.

