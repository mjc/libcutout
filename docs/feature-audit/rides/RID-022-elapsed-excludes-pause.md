# RID-022: The elapsed timer actually reports recording time excluding pauses

- Severity: P3
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Pause a ride for several minutes and resume. The tile labeled elapsed excludes that pause, so it differs from wall-clock time since Start without explaining the distinction.

## Evidence

Localizable.xcstrings:1719 labels the metric elapsed. RideMapSummaryView.swift:95-98 formats Rust durationMilliseconds. recording.rs:832-856 subtracts paused_duration_milliseconds; the same elapsed label is reused in history detail.

- `swift/CutoutMobile/Apps/CutoutApp/Localizable.xcstrings:1719`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapSummaryView.swift:95`
- `crates/cutout-ride-maps/src/recording.rs:832`

## Expected behavior and smallest remedy

Label the value Recording time or Active time, or separately expose total elapsed and paused/moving durations with explicit definitions.

## Acceptance / reproduction

Record 60 seconds, pause 120, record 60 more. The display must make clear whether it reports 120 or 240 seconds and use the same definition in history/share.

## Comparison and limitations

Competitor definitions vary and were not independently verified; no auto-pause or moving-time semantics should be implied.

Pause exclusion is intentional and has Rust tests. The finding concerns naming, not arithmetic corruption.

