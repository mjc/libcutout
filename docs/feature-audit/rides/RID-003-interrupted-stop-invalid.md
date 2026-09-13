# RID-003: Recovered interrupted ride offers a Stop button that cannot work

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Reopen after an interrupted ride and tap Stop. UI groups interrupted with paused, but Rust rejects Interrupted + Stop, leaving a generic command error.

## Evidence

RideMapControlsView.swift:20-25 selects resumable controls for interrupted, and 42-51 includes stopButton. lifecycle.rs:53-65 permits Stop only from Active or Paused; Interrupted can instead Save or Discard. The existing presentation test only checks the control group.

- `swift/CutoutMobile/Apps/CutoutApp/RideMapControlsView.swift:20`
- `crates/cutout-ride-maps/src/lifecycle.rs:53`
- `swift/CutoutMobile/Tests/CutoutAppTests/RideMapPresentationTests.swift:9`

## Expected behavior and smallest remedy

Expose Save/Discard for an interrupted ride alongside Resume, or define and implement an intentional Interrupted-to-Stopped transition.

## Acceptance / reproduction

Drive every visible control from every lifecycle state through the real Rust reducer. An interrupted ride must be finishable without first pretending to resume.

## Comparison and limitations

Competitor recovery controls unverified; this is an internal UI/reducer mismatch.

Source-confirmed rejection, not a reproduced phone crash. Existing test: RideMapPresentationTests.swift:9.

