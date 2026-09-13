# SET-021: Headlight status says sent before a request and ignores failure

- Severity: P2
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open Aero Tune before touching the manual Headlight toggle, or trigger a failed/refused request. Its accessibility hint still says the request was sent unconfirmed whenever the capability is supported.

## Expected behavior

Present unknown, submitted, refused and failed states based on actual command lifecycle.

## Evidence

CutoutAppModel.swift:508-512 returns settings.headlight.sent_unconfirmed solely from manualHeadlightControlAvailable. CutoutRouteViews.swift:242-244 installs that string as the toggle’s accessibility hint. CutoutAppModel.swift:2267-2270 submits via the shared setting path and preserves no manual-specific status text.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Build manual-headlight status from the existing headlight setting state and typed result, following the main high-beam status approach.

## Acceptance and reproduction checks

Inspect the initial hint, then inject accepted, failed and refused submissions. The hint must never claim a send before one occurred or after pre-transport refusal.

## Limits

Capability status elsewhere may show a failure. This record concerns the misleading feedback directly attached to the manual control.

