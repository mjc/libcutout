# SET-016: Lowest-cell detail labels full spread as distance below average

- Severity: P2
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open the lowest cell’s detail. Text states X mV below pack average, but X is the maximum-minus-minimum pack spread.

## Expected behavior

Compute average-minus-selected-cell for this wording, or describe the existing number as total spread.

## Evidence

CutoutMobile.swift:5035-5043 passes cellDelta to bms.detail.lowest_below_average. Sources/CutoutMobile/Localizable.xcstrings:162 says “below pack avg”. crates/cutout-mobile-ffi/src/lib.rs:11823-11834 defines cell delta as max minus min.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Reuse averageGroupVoltage and selected group voltage for the difference, or change the label to the correct baseline.

## Acceptance and reproduction checks

For 3900,4000,4100 mV cells, lowest is 100 mV below average and total spread is 200 mV. Check both lowest and other-cell detail text.

## Limits

Page aggregation errors are separate findings; this baseline mismatch remains with a single correct complete snapshot.

