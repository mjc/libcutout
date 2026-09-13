# SET-014: BMS pack spread and lowest cell come from one page

- Severity: P1
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Receive multiple BMS pages with disjoint cell groups. The map preserves those groups where indices are unique, but pack spread and lowest-cell identity are overwritten by the final sorted page.

## Expected behavior

Pack summaries must be computed from the same complete set of observed cells as the map, with partial coverage identified.

## Evidence

crates/cutout-mobile-ffi/src/lib.rs:11570-11574 calculates delta/lowest index per page; 11823-11846 computes page min/max. CutoutMobile.swift:4921-4923 takes update.cellDelta and update.lowestGroupIndex rather than recomputing after merge; CutoutSessionCore.swift:1435-1439 folds in page-key sort order.

The BmsSnapshot initializer at swift/CutoutMobile/Sources/CutoutMobile/CutoutMobile.swift:4690-4693 simply assigns supplied summary values; no hidden recomputation corrects the page-local aggregate.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Recompute aggregate extrema from merged, valid cells in the shared owner after preserving page identities; retain per-bank summaries when appropriate.

## Acceptance and reproduction checks

Use page A cells 3900/4000 mV and page B 4100/4150 mV. Aggregate spread must be 250 mV and lowest from A, independent of arrival order.

## Limits

This is distinct from Veteran index collisions and also affects uniquely indexed Begode pages. No battery condition on the user’s actual wheel is inferred.

