# SET-004: Unknown wheel settings look like concrete editable values

- Severity: P2
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open Aero Tune before the first settings page arrives. Numeric editors show defaults or lower bounds, toggles show Off, and riding mode shows Medium. Send is enabled while the connection is live.

## Expected behavior

Unknown current values should remain visibly unknown; draft proposals should be labeled and require a deliberate choice before writing.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:839-843 seeds tilt-back 20, PWT 60, hardness 60, alarm 20 and angle 0. Lines 1018-1039 select Medium for absent riding mode; 1062-1080 select false for absent toggle values; 1108-1135 use the range lower bound and immediately send it. Aero numeric controls at 866-966 include volume 0 and lateral cutoff 35.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Represent absent readback separately from a user-created draft. Disable Send until a draft is explicit, and label draft versus current value.

## Acceptance and reproduction checks

Delay page-8 readback: verify no current value is fabricated and merely opening Tune cannot supply an unintended default write. Then edit one field and send it.

## Limits

Some numeric controls do show a separate unavailable-current row; this reduces ambiguity but does not distinguish the default editor proposal or make its selection deliberate.

