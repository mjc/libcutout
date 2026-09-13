# RID-005: Disconnect ends the ride activity but silently leaves map recording active

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Tap Disconnect and return to the device picker. The Live Activity ends and vehicle session clears, while the GPS map ride stays active and continues accumulating route/time.

## Evidence

CutoutAppModel.disconnectTransport at 2727-2740 ends Live Activity and calls disconnectAndScan, without stopping/pausing the map. Core disconnect cleanup at CutoutSessionCore.swift:1295-1337 similarly resets BLE/capture but not the ride-map reducer. GPS ingestion at 3208-3242 remains live.

- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:2727`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1295`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:3208`

## Expected behavior and smallest remedy

Choose and communicate one policy: finish/pause the ride on explicit disconnect, or clearly retain an independent GPS recording with an accessible stop action in the picker.

## Acceptance / reproduction

Start a connected ride, explicitly disconnect, then inspect map state, GPS callbacks and Live Activity. UI must explain whether recording continues; automatic brief link loss must be tested separately.

## Comparison and limitations

WheelLog distinguishes logging from connection and offers automatic logging preferences; see ../comparison-baseline.md. GPS continuation itself is not necessarily wrong.

Source confirms divergent lifecycles. User expectation and desired default need a product decision; no background battery measurement performed.

