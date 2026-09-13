# CRH-002: Spotify authorization has crashed on an SDK callback queue

- Severity: P1
- Evidence status: Phone-confirmed historical crash; current mitigation unverified on phone
- Status: OPEN — no current phone acceptance test
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Observed behavior

Three September 9 crashes at 16:04:00, 16:04:23 and 16:04:40 fail an executor assertion in `@objc SpotifyProviderAdapter.sessionManager(manager:didInitiate:)`, reached from a URL-session callback queue.

## Expected behavior

Authorize or renew Spotify without terminating the app or disrupting the ride.

## Evidence and current source

`swift/CutoutMobile/Sources/CutoutMobile/MusicProviderAdapters.swift:67-92` now defines a SessionManagerBridge with nonisolated delegate entries; `:465-469` enqueues onto MainActor. This differs from the historical symbol and is a plausible mitigation, not proof that authorization is now reliable.

Reports (hashes and binary UUIDs in [phone evidence](../phone-evidence.md)):

- `CutoutApp-2026-09-09-160400.ips`
- `CutoutApp-2026-09-09-160423.ips`
- `CutoutApp-2026-09-09-160440.ips`

## Minimal remedy direction

Retain and verify the SDK-to-actor handoff for success, renewal and failure. Check callback generation ownership and repeated authorization, including after backgrounding.

## Acceptance checks

Run explicit Spotify authorization, cancel, retry and renewal on the physical phone using the current signed artifact. Capture new logs and binary UUID; test callback delivery from a background queue.

## Comparison and limits

Keeping the app responsive is a baseline expectation, independent of competitor design. No crash-free retest was performed. Do not close because the delegate implementation changed or because the reports are older.
