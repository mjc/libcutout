# RID-009: GPS-only rides show no live speed despite valid phone GPS speed

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Start a GPS-only ride and move with valid phone speed samples. Distance and route update, but the speed tile uses the disconnected vehicle readout and remains unavailable.

## Evidence

RideMapRouteView.swift:107 passes model.speed. CutoutAppModel.swift:431-433 returns displayState.speed only. RideMapSummaryView.swift:44-49 converts missing speed to unavailable. The model separately receives phoneLocationReadback at 692-694 but it is not consulted here.

- `swift/CutoutMobile/Apps/CutoutApp/RideMapRouteView.swift:107`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:431`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:692`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapSummaryView.swift:44`

## Expected behavior and smallest remedy

Select a fresh, explicitly labeled GPS speed for GPS-only rides. Preserve the distinction from wheel-reported speed when associated.

## Acceptance / reproduction

Inject valid GPS speed without BLE telemetry and verify the speed tile updates; stale/invalid GPS remains unavailable rather than zero.

## Comparison and limitations

WheelLog/EUC World source attribution distinctions are summarized in ../comparison-baseline.md; no exact fallback policy is assumed.

Source-confirmed data-flow omission. No physical movement or speed accuracy testing was performed.

