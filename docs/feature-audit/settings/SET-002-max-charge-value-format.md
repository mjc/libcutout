# SET-002: Max-charge editor hides its numeric value

- Severity: P2
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open Max charge (raw), change its stepper, or inspect current readback. Every numeric value renders literally as “(value)”.

## Expected behavior

The chosen raw number and the observed raw number must be visible even while their physical meaning remains unverified.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:938-949 selects `.raw`; lines 1108-1151 render editor/readback through `unit.text`; `.raw` returns the literal string `"(value)"`. swift/CutoutMobile/Tests/CutoutAppTests/CutoutAppRouteTests.swift:8-12 tests the other units but omits raw.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Return the actual number from the existing raw formatter.

## Acceptance and reproduction checks

Verify 0, 46 and 70 produce distinct visible and accessible values; the current row must show 46 when its readback is 46.

## Limits

This is a deterministic formatting defect; it does not establish the physical charge ceiling.

