# RID-024: A transient database-open failure disables recording until the process restarts

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Launch while protected storage or another transient database-open condition is unavailable, then resolve the condition. The static shared database retains nil; the map core is permanently constructed as storageUnavailable for that process.

## Evidence

RustPersistenceStore.swift:9-49 initializes static shared once and returns nil on any open failure. CutoutSessionCore.swift:580-583 converts nil into MobileRideMapState(storageUnavailable:). RideMapState.swift:703-710 stores nil core/database and a fixed initialization error. The map warning has no re-open action.

- `swift/CutoutMobile/Sources/CutoutMobile/RustPersistenceStore.swift:9`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:580`
- `swift/CutoutMobile/Sources/CutoutMobile/RideMapState.swift:703`

## Expected behavior and smallest remedy

Allow an explicit or lifecycle-triggered retry of the existing persistence bootstrap, replacing unavailable state only after recovery succeeds and fencing old requests.

## Acceptance / reproduction

Inject one open failure followed by success, then retry without killing the app. Recording/history recover while unrelated state remains intact; a persistent failure stays visible.

## Comparison and limitations

Competitor storage recovery behavior unverified.

Source-confirmed lack of retry. Protected-data failure is an example trigger and needs device validation; do not delete or reset the database as a recovery shortcut.

