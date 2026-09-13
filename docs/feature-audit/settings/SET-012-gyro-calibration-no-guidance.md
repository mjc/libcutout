# SET-012: Gyro calibration lacks a usable physical procedure

- Severity: P1
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

In Aero validation mode, tap Calibrate gyro. The screen supplies only a button and Ready/In progress/Complete text, with Stop calibration shown after Complete.

## Expected behavior

Explain the verified orientation/support/setup procedure and the difference between device state complete and exiting calibration, before the rider starts it.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:989-1006 chooses Start/Stop and disables waiting; Localizable.xcstrings:821-839 labels state Complete and action Stop. The only section footer at lines 1010-1011 is the general parked/500 mm/s note. docs/aero-settings-coverage.md:28 records source-backed lifecycle but missing physical-effect proof.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Establish the manufacturer-specific workflow, then supply concise step-by-step guidance and states using the existing lifecycle.

## Acceptance and reproduction checks

On a secured stationary device, validate start, waiting, completion and exit with manufacturer instructions. Verify cancellation/failure feedback and no duplicate command while waiting.

## Limits

Do not infer a calibration procedure from the code or perform this test on a moving/unsupported wheel. No physical calibration was performed in this audit.

