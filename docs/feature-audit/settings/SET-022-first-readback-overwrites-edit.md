# SET-022: Delayed first readback can overwrite an Aero edit

- Severity: P2
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open Aero Tune before readback, adjust an editor, then receive the first device value before pressing Send. The one-time seeding function replaces the user’s edit because changing the stepper does not mark the field seeded/dirty.

## Expected behavior

Seed from the device only while an editor has not been touched; preserve an explicit unsent edit and show the newly received current value separately.

## Evidence

CutoutRouteViews.swift:657-681 seeds the five state values and boolean flags. Lines 687-734 bind steppers directly to state without marking edits. Lines 744-779 call seedFromDeviceIfNeeded on readback and overwrite any unseeded value.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Track whether a user edited each field, or use a draft/current representation already needed for other controls.

## Acceptance and reproduction checks

Delay first readback, edit tilt-back from 20 to 25, then supply current 40. Draft remains 25, current becomes 40, and Send’s target is unambiguous.

## Limits

The actual race frequency depends on readback timing. The overwrite follows directly from the current state update path.

