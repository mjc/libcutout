# APP-004: Spotify transport commands have no timeout or cancellation completion

- Severity: P2
- Evidence status: hypothesis needing device validation
- Status: OPEN
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and actual behavior

Send a Spotify transport command and lose its App Remote connection before the callback returns. The awaiting UI task may never finish if the SDK does not deliver the callback.

## Expected behavior

Every user command should reach a terminal result within a bounded interval, including connection loss, without leaving repeated pending tasks.

## Evidence

`swift/CutoutMobile/Sources/CutoutMobile/MusicProviderAdapters.swift:608-633` awaits `withCheckedContinuation`, resumed only by the SDK callback; there is no timeout/cancellation path here. The UI starts a new Task for every tap at `CutoutRouteViews.swift:16-19`. Unlike artwork requests, transport requests have no visible bounded-completion policy.

## Minimal remedy direction

Use the SDK’s documented completion guarantee if sufficient; otherwise add one shared exactly-once completion/cancellation boundary and expose pending status. Verify SDK semantics before changing continuations.

## Acceptance checks

Fake missing, delayed and duplicate callback cases; disconnect during play/skip on phone. Verify exactly one terminal outcome and no accumulated unfinished tasks.

## Comparison and limits

An omitted local timeout is confirmed; actual callback loss and task leakage are not reproduced. This is not a confirmed crash.
