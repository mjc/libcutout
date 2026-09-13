# APP-002: Limp-home range is advertised but never populated by live telemetry

- Severity: P2
- Evidence status: UX inconsistency
- Status: OPEN
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and actual behavior

Open the live EUC dashboard and look for Limp home distance. It remains unavailable even though its display supports a typed distance.

## Expected behavior

Range availability and estimation limits should be truthful, particularly when the label suggests a way home on low charge.

## Evidence

`crates/cutout-mobile-ffi/src/lib.rs:15356-15394` maps every live core telemetry snapshot with `limp_home_range: None` at 15387. `swift/CutoutMobile/Apps/CutoutApp/PevDashboardPresentation.swift:35-38` still adds the tile; `CutoutMobile.swift:5738-5748` can only render an explicitly supplied value. `CutoutSessionCoreTests.swift:3334-3346` tests supplied fixture values, not a live range producer.

## Minimal remedy direction

Remove or explicitly mark the unsupported tile until a defined range estimator exists. Do not fabricate a remaining-distance estimate from voltage alone.

## Acceptance checks

Trace an actual protocol snapshot through the production FFI conversion and dashboard; verify unavailable versus measured/estimated states, units and explanatory text.

## Comparison and limits

This finding is about the permanent UI promise. It does not assert a universal safe range or require the competitor’s algorithm.
