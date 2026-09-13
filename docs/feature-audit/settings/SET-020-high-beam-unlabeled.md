# SET-020: Tune does not identify the Aero high-beam buttons

- Severity: P2
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open Aero Tune. Two buttons say Turn On and Turn Off in Lights, followed by a separate Headlight toggle. The buttons actually control high beam but have no visible High beam label.

## Expected behavior

Label each physical lighting function so a rider can distinguish headlight from high beam.

## Evidence

CutoutRouteViews.swift:222-245 displays generic On/Off buttons plus Headlight toggle. CutoutAppModel.swift:2259-2262 routes those buttons to setAeroHighBeam for Aero. Lines 470-474 compute headlightControlTitle as High beam but repository search finds no view consuming it.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Render the existing model-provided control title beside the two buttons and use it in accessibility labels.

## Acceptance and reproduction checks

With an Aero selected, both sighted and VoiceOver users should be able to identify High beam On/Off separately from Headlight.

## Limits

This is about naming two implemented commands; it does not conflate Aero built-in lighting with the separate MELK accessory.

