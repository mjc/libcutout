# RID-012: Listening history stops observing songs when the ride is backgrounded

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Enable listening history, begin recording and lock the phone while songs change. appDidEnterBackground stops the monitoring task and Apple Music observer, so the app cannot observe the intervening transitions through this path.

## Evidence

CutoutAppModel.swift:2658-2663 calls stopMusicMonitoring on every background transition. That method at 1091-1101 cancels the task and provider observers. Foreground handling at 2685-2688 restarts observation, but does not reconstruct intervening song changes.

- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:2658`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:1091`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:2685`
- `docs/music-integration.md:1`

## Expected behavior and smallest remedy

State the background capture limitation clearly and represent an observation gap. Investigate only provider-supported background observation; do not record audio or synchronize playback.

## Acceptance / reproduction

Background across at least two song changes, foreground, save and inspect history. It must either retain supported observations or explicitly mark the unknown interval, without inventing songs.

## Comparison and limitations

Competitor music integration unverified. docs/music-integration.md defines metadata-only history and prohibits soundtrack/audio capture.

Source confirms observation suspension. OS/provider limits may make continuous history unavailable; actual provider behavior needs device validation.

