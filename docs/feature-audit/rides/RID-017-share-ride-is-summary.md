# RID-017: Share Ride sends only summary text, with no route or telemetry artifact

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Open a saved route and choose Share. Recipients get distance, duration and a point count, not the displayed map, a route file, or ride data they can inspect/import.

## Evidence

RideMapHistoryDetailSections.swift:331 passes a String to ShareLink. RideMapHistoryDetailView.swift:278-284 constructs only title plus formatted distance/duration/point count. No file exporter or route payload participates.

- `swift/CutoutMobile/Apps/CutoutApp/RideMapHistoryDetailSections.swift:331`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapHistoryDetailView.swift:278`

## Expected behavior and smallest remedy

Label the existing action Share summary. If route export is in scope, add one explicit interoperable export action with metadata/privacy rules and an honest format label.

## Acceptance / reproduction

Inspect the share-sheet item: summary action is clearly described; an eventual route export opens in an independent reader and does not silently include music or precise location beyond the selected policy.

## Comparison and limitations

WheelLog's primary README advertises recording and export; see ../comparison-baseline.md. This does not mandate its exact formats.

Source-confirmed payload; no live share was sent. Standard route export is a product gap, not evidence that the text payload is corrupted.

