# RID-002: High-accuracy background GPS keeps running without a ride

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Open the app, grant location access, then remain in the picker or stop/save a ride. Best-for-navigation location updates are started unconditionally and no stopUpdatingLocation path exists.

## Evidence

CutoutSessionCore.swift:569-577 enables navigation accuracy and background updates; start():651-661 creates the location manager before any ride. Authorization callback:3192-3200 starts updates for both authorized states. Stop/save methods:3279-3290 only transition storage.

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:569`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:651`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:3192`
- `swift/CutoutMobile/Apps/CutoutApp/Info.plist:50`

## Expected behavior and smallest remedy

Tie high-accuracy updates to explicit recording/capture demand. A separate foreground location preview can use a bounded request or lower-power mode.

## Acceptance / reproduction

Instrument location-manager start/stop calls: idle picker and saved/discarded rides release high-accuracy demand; an explicitly active background ride retains it.

## Comparison and limitations

Competitor idle power behavior was not independently verified; expectation follows the app's own permission disclosure.

Actual battery impact needs device measurement. Info.plist:50-53 describes background recording while an active ride continues.

