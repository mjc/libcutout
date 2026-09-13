# SET-001: PWT shows headroom under a PWM threshold label

- Severity: P1
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Connect an Aero in settings validation mode and inspect or change PWT. A margin of 20 is displayed as “PWT (PWM tilt-back): 20%”; the encoder sends a duty threshold of 80. The control never says that its number is unused PWM.

## Expected behavior

The rider should be able to distinguish 20% remaining headroom from 80% PWM utilization. Use one named basis consistently for the editor and current readback.

## Evidence

crates/cutout-core/src/lib.rs:588-623 defines the value as unused PWM margin; crates/cutout-protocols/src/request_encoder.rs:198-219 encodes `100 - margin` and preserves Off as 200. swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:693-708 renders the margin directly. swift/CutoutMobile/Apps/CutoutApp/Localizable.xcstrings:860-867 labels it PWT/PWM with no margin explanation.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Rename to “Tilt-back headroom” or convert the displayed editor/readback to PWM utilization. Preserve the distinct Off setting and Rust wire conversion.

## Acceptance and reproduction checks

Use margin 20 / encoded threshold 80; verify both displayed editor and current readback identify the basis. Check margins 0 and 70 and Off. Existing encoder tests distinguish zero from Off at request_encoder.rs:835-856.

## Limits

Source does not support claiming the current encoder is reversed. The illustrative complement uses this control’s shared 0–100 basis, not a universal motor safety percentage.

