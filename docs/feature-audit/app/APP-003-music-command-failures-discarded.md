# APP-003: Music buttons discard failed and refused command outcomes

- Severity: P2
- Evidence status: source-confirmed defect
- Status: OPEN
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and actual behavior

Tap Play, Pause, Next or Open provider when the provider rejects the action or its operation fails. The control has no command-result feedback, inviting repeated taps.

## Expected behavior

Tell the rider whether an explicit action failed, was unavailable or is still pending, while preserving truthful now-playing state.

## Evidence

`swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:16-19` explicitly discards `handleMusicCommand` results. `AppSetupView.swift:56-59` does the same for provider handoff. `CutoutAppModel.swift:724-759` returns unavailable/refused/failed and only refreshes after accepted, without publishing a command error. `MusicIntegration.swift:1531-1577` contains real failure returns.

## Minimal remedy direction

Publish a small command-result/pending state consumed by both live and setup controls; keep provider lifecycle and command failure distinct.

## Acceptance checks

Fake-provider accepted, refused, unavailable and failed outcomes should produce appropriate visible feedback. A rejected skip must not appear to advance the track.

## Comparison and limits

This follows the local music-control contract. No competitor-specific music failure UI was verified.
