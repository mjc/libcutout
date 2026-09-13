# SET-003: Max-charge write offers a number with unresolved physical meaning

- Severity: P1
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

In Aero validation mode, open Max charge (raw) and send a value from 0 through 70. The normal Tune form offers a charge-related setting without a meaningful charge voltage or pack-specific consequence.

## Expected behavior

A charge-ceiling control needs an established Aero-specific interpretation, range and effect, or should be a clearly explained protocol diagnostic outside ordinary tuning.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:938-949 exposes the raw editor and send action; Localizable.xcstrings:794-797 only names “Max charge (raw)”. docs/aero-settings-coverage.md:53 and 73-78 explicitly record that generic conversion conflicts with the Aero 126 V pack and safe interpretation remains unresolved. crates/cutout-mobile-ffi/src/lib.rs:2697-2698 marks the setting unverified; validation mode enables unverified controls.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Keep raw inspection available, but defer ordinary charge-ceiling writes until the model-specific meaning is established. If validation writes remain, place the precise uncertainty beside this control.

## Acceptance and reproduction checks

Verify release mode gating; validation UI must explain that raw 46 is not 46 V and no accepted maximum-charge voltage is known. Establish units/range against manufacturer evidence before promoting support.

## Limits

This is not a claim that a particular raw value damages a battery. Existing validation mode limits exposure, and wire framing evidence is distinct from physical effect.

