# APP-005: Live Activity failures have no ordinary visible explanation

- Severity: P2
- Evidence status: UX inconsistency
- Status: OPEN
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and actual behavior

Live Activity authorization or creation fails. A rider without VoiceOver can see no explanation for the missing lock-screen display.

## Expected behavior

Expose Live Activity availability/failure visibly and accessibly with an actionable explanation, without disrupting the ride.

## Evidence

`swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:159` stores liveActivityError; several lifecycle completions assign it. The only consuming UI path found is `ContentView.swift:91-96`, which posts AccessibilityNotification.Announcement. `ConnectionPresentation.swift:95-104` maps error descriptions, but there is no visible banner or setup status bound to that property.

## Minimal remedy direction

Reuse the stored typed error for a visible status and recovery action where supported; keep accessibility announcements.

## Acceptance checks

Inject authorizationDenied, requestFailed and activityUnavailable. Verify a visible explanation with VoiceOver off, and a matching accessible announcement when enabled.

## Comparison and limits

Source-confirmed feedback gap; actual authorization denial was not triggered on the phone during this audit.
