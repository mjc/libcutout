# RID-019: Map mixes locale-derived distance and history speed with mph live speed

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Use a metric locale and compare a live ride with its saved detail. Live speed stays mph while distance follows locale and history average speed switches to km/h.

## Evidence

RideUnits.swift:4 and 16-21 hardcode mph conversion. RideMapSummaryView.swift:90-93 formats distance using locale road usage. RideMapHistoryDetailView.swift:104-120 independently selects km/h for metric locales.

- `swift/CutoutMobile/Sources/CutoutMobile/RideUnits.swift:4`
- `swift/CutoutMobile/Sources/CutoutMobile/RideUnits.swift:16`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapSummaryView.swift:90`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapHistoryDetailView.swift:104`

## Expected behavior and smallest remedy

Use one user-visible unit preference across dashboard, live Map, history, share text and accessibility. Coordinate with the global units finding.

## Acceptance / reproduction

Under US and metric locales, verify a known speed/distance reads consistently on every surface and after relaunch. Unit switches must not alter stored physical values.

## Comparison and limitations

WheelLog's official strings include mph and temperature unit settings; see ../comparison-baseline.md.

Source-confirmed inconsistent format policies. Exact road-distance formatting under each locale needs runtime assertions.

