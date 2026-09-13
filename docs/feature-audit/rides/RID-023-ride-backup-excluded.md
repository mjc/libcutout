# RID-023: Ride database is excluded from backup without a usable ride backup/export flow

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Rely on normal phone backup to preserve saved rides when replacing/resetting a phone. The app explicitly excludes its ride database, while Share Ride emits summary text rather than restorable ride data.

## Evidence

RustPersistenceStore.swift:29-34 sets databaseURL.isExcludedFromBackup=true after opening ride.sqlite. RideMapHistoryDetailView.swift:278-284 shares only summary text. No ride backup/import UI is present in the current app.

- `swift/CutoutMobile/Sources/CutoutMobile/RustPersistenceStore.swift:29`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapHistoryDetailView.swift:278`

## Expected behavior and smallest remedy

Define a visible data-retention policy and provide a restorable export or an intentional backup option. Explain any privacy-driven exclusion before users depend on ride history.

## Acceptance / reproduction

Use an isolated device/container backup-and-restore test to verify which ride files survive; confirm a rider can preserve complete selected rides through a supported workflow.

## Comparison and limitations

Competitor cloud/backup behavior is not assumed. This concerns continuity of the app's own locally saved data.

The exclusion flag is source-confirmed. Actual system backup behavior, SQLite sidecars and a replacement-phone restore were not tested; do not claim every backup necessarily omits every byte.

