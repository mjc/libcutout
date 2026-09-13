# SET-010: Tune uses km/h while Ride and readback use mph

- Severity: P2
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open Aero tilt-back/alarm or Begode max-speed editing after inspecting Ride. The editors display and step km/h while Ride and generic speed readback use mph. The app currently has no phone speed-unit preference; mph is fixed.

## Expected behavior

Keep editor and readback units consistent, or explicitly explain that these are wheel-native settings and show the mph equivalent.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:687-690,724-727,1269-1274 hard-code km/h and integer steps. Lines 1373-1380 use SpeedReadout for readback. swift/CutoutMobile/Sources/CutoutMobile/RideUnits.swift:4 fixes the phone speed unit to mph. Aero wheel display units are a separate control at CutoutRouteViews.swift:1083-1105; changing the wheel setting does not change the phone units or these editors.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Use the shared speed-unit presentation boundary while retaining exact integer km/h protocol values.

## Acceptance and reproduction checks

With the current fixed-mph phone presentation, inspect and edit 32 km/h: expose the approximately 19.9 mph equivalent and a clear quantization rule; round-trip without unintentionally changing the wheel value. Verify the separate wheel-display-units setting does not masquerade as a phone preference. If a phone unit preference is added in future, test both selections across Ride, readback and editing then.

## Limits

This is a presentation mismatch, not a proven wire-unit conversion defect or a broken existing phone preference.
