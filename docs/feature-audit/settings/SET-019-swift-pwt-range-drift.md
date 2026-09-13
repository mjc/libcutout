# SET-019: Swift accepts PWT values Rust rejects

- Severity: P2
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

A Swift caller constructs AeroPwmPercent(74) successfully, but submitting it crosses into Rust’s 0–70 domain and is rejected. A Swift form test explicitly expects 74 to be valid.

## Expected behavior

The typed public wrapper and tests should enforce the same margin domain as Rust.

## Evidence

CutoutMobile.swift:2294-2309 accepts percent <=100. crates/cutout-core/src/lib.rs:593-604 accepts only <=70; cutout-mobile-ffi/src/lib.rs:14607-14609 validates the core bound. CutoutAppRouteTests.swift:14-26 constructs/expects 74; the current visible stepper correctly uses 0...70 at CutoutRouteViews.swift:694.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Align the Swift constructor and its tests with the authoritative Rust bound; keep validation in Rust.

## Acceptance and reproduction checks

Constructor boundaries: 0 and 70 succeed, 71 and 100 fail. All Swift-produced values must pass Rust domain validation.

## Limits

The current Tune stepper cannot produce 74. This is a public API/test inconsistency, not a claim that the UI sends out-of-range values or crashes.

