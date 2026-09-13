# APP-001: Sag-adjusted energy is always an empty safety bar

- Severity: P2
- Evidence status: UX inconsistency
- Status: OPEN
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and actual behavior

Open EUC Ride with working telemetry. Sag-adjusted energy occupies the safety section but can only show unavailable.

## Expected behavior

An unsupported feature should explain its availability or stay out of the primary riding surface. A safety bar should not suggest a measurement is about to arrive when no producer exists.

## Evidence

`swift/CutoutMobile/Apps/CutoutApp/PevDashboardPresentation.swift:3-11` constructs this bar with `.unavailable` and `progress: nil` unconditionally. `CutoutMobile.swift:5899` explicitly categorizes it as unavailable; this is not a transient sensor-reading failure.

## Minimal remedy direction

Hide the bar until supported or label the feature unavailable with its actual reason. Only implement an estimator when its inputs and meaning are defined.

## Acceptance checks

Feed every supported live model; confirm no permanent unexplained empty bar. If enabled later, test missing/stale inputs separately from true zero.

## Comparison and limits

[Comparison baseline](../comparison-baseline.md) supports understandable, identifiable metrics; it does not require Cutout to copy a competitor energy algorithm.
