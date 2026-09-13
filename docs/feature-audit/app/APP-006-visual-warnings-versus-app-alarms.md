# APP-006: Riding warnings do not provide a configurable phone alarm system

- Severity: P2
- Evidence status: UX inconsistency / capability gap
- Status: OPEN
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Trigger and actual behavior

A rider familiar with phone PWM/current/temperature alarms sees warning cards and wheel-alarm controls, but cannot configure equivalent phone-generated alarms in this app.

## Expected behavior

Clearly distinguish a visual advisory, a wheel firmware alarm and a phone-generated alarm. Do not imply a warning card provides background audible protection.

## Evidence

`swift/CutoutMobile/Apps/CutoutApp/EucRideViews.swift:80-120` presents warning cards and posts accessibility announcements on severity changes. `VescRideViews.swift:107-130` similarly announces warning/stop changes. `AppSetupView.swift:19-37` offers Music and Forget device only. Scoped search of app and shared Swift sources found wheel-alarm settings but no ordinary phone alarm audio/haptic delivery/configuration path.

## Minimal remedy direction

Clarify current capability in the UI and decide whether a phone alarm feature belongs in scope. If implemented, define source, units, thresholds, per-device persistence, delivery and stale-data behavior before adding controls.

## Acceptance checks

With VoiceOver off and screen locked, document what each warning actually delivers. Check that wheel settings and phone alarm settings cannot be confused. Never use hazardous riding to provoke an alarm; use deterministic inputs.

## Comparison and limits

[Comparison baseline](../comparison-baseline.md) documents familiar apps’ alarms. Missing parity is a product decision, not proof that wheel firmware alarms fail or that accessibility announcements are regular audible alarms.
