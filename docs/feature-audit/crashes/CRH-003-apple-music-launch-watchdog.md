# CRH-003: App launch can hang while eagerly initializing Apple Music

- Severity: P1
- Evidence status: Phone-confirmed watchdog; matching eager initialization in source
- Status: OPEN — no current phone acceptance test
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Observed behavior

The September 12 22:17:39 scene-create watchdog exceeded its 10-second allowance. Its main-thread stack contains synchronous XPC, MPMusicPlayerController server validation/currentPlaybackRate, AppleMusicProviderAdapter.init and CutoutAppModel initialization.

## Expected behavior

Launching Cutout for telemetry or lighting must not wait on an optional music service.

## Evidence and current source

`swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:523-525` constructs both provider adapters during model initialization. `swift/CutoutMobile/Sources/CutoutMobile/MusicIntegration.swift:1454-1465` eagerly obtains system music player instances. `CutoutApp.swift:12` constructs the app model before the first usable screen.

Reports (hashes and binary UUIDs in [phone evidence](../phone-evidence.md)):

- `CutoutApp-2026-09-12-221739.ips`

## Minimal remedy direction

Defer optional provider initialization until requested/needed, keep a usable initial screen and handle provider unavailability without blocking scene creation. Respect required API isolation instead of moving all framework calls indiscriminately to a worker.

## Acceptance checks

Cold-launch with music monitoring off, Spotify selected, Apple Music unavailable and a slow music service; repeat under background/foreground and retain watchdog-free phone evidence.

## Comparison and limits

Keeping the app responsive is a baseline expectation, independent of competitor design. The report also records serious thermal state. That is relevant context, not proof that heat alone caused the stall or that another OS version is immune.
