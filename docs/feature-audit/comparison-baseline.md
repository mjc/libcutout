# Comparison baseline

Checked 2026-09-13. The names in the request are interpreted as DarknessBot, EUC World and WheelLog. These are comparison references, not a requirement to reproduce their layouts or every feature. No competitor app was operated on the phone during this audit. Published capability is distinct from verified runtime behavior on a particular wheel and firmware.

| Reference | Verified published behavior | Cutout expectation derived from it |
| --- | --- | --- |
| [DarknessBot developer App Store listing](https://apps.apple.com/no/app/darknessbot/id1108403878) | Lists PWM, configurable telemetry tiles, wheel settings, trip recording, charts, camera telemetry and Apple Watch support; availability depends on manufacturer support. | Explain unsupported controls and keep units and telemetry meaning explicit. Missing competitor-only features are coverage decisions, not automatically bugs. |
| [EUC World 2.56 release explanation](https://euc.world/blog/euc-world-2-56-0-has-been-released) | Describes safety-margin alarms, separate GPS/wheel metric provenance and configurable dashboard fields. | Label remaining margin differently from consumed duty; identify GPS versus wheel values and avoid permanently empty dashboard promises. |
| [EUC World current explanation](https://euc.world/blog/battery-current-vs-motor-phase-current) | Distinguishes measured or estimated battery current from motor phase current, and describes per-wheel current alarms. | Do not interchange current types, hide estimation, or imply visual warnings are audible app alarms. |
| [WheelLog maintained UI strings](https://raw.githubusercontent.com/Wheellog/Wheellog.Android/master/app/src/main/res/values/strings.xml) | Separate app and wheel settings, PWM/duty labels, MPH and Fahrenheit options, start/stop logging and explicit location/storage failure wording. | Offer coherent unit selection and understandable state/error feedback. Strings establish intended UI wording, not complete runtime semantics. |

## PWM versus headroom

For complementary values on the same normalized 0–100 basis, 80% used duty corresponds to 20% unused headroom. Those labels describe opposite directions. A wheel's PWT wire value, app-computed safety margin and measured motor duty are not interchangeable merely because each is a percentage. Confirm model/firmware semantics and preserve disabled sentinel values before changing any encoder. SET-001 traces Cutout's actual mismatch.

## Shared product rules proposed for this audit

- A displayed setting is either read from the wheel, a clearly marked draft, or unavailable.
- A command distinguishes queued/sent, device-confirmed, timed out and refused, with the reason visible.
- Stale telemetry must not look freshly measured.
- Recording, exporting and deleting describe the data actually affected.
- Opening another page, provider or display appearance must preserve the session and must not crash.
- Intentional departures from familiar EUC apps should explain their benefit in the interface.

These rules are audit recommendations. They are not claims that every competitor implements them correctly.
