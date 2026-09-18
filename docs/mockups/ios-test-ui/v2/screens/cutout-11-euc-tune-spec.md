# Aero settings — production UI

The SVG and PNG show four scroll positions of the same Settings screen, covering
all 18 supported editable Aero settings. They replace the earlier annotated
validation-dashboard comp. Values are design samples, not hardware observations.

This remains the approved clean final-screen reference. The
[settings design review](../../../../settings-design-review.md) and
[LIBCU-DOC-8](https://lific.mjc.lol/LIBCU/pages/30) change the correctness
requirements, not this visual direction. Recent implementation changes are not
renewed visual approval, and catalog coverage is not physical acceptance.

## Screen contract

- Grouped labels, values, and native controls in the existing dark CutOut theme.
- No validation mode, needs-validation label, confirmation disclaimer, Details
  disclosure, provenance, raw diagnostic group, instructional subtitle, or
  permanent Requested text.
- Headlight and mode booleans use direct Off/On commands. Before any request,
  unknown current state selects neither button. A locally accepted request may
  highlight the chosen button as pending, never as an observed or confirmed
  value. Host submission and completion come from Rust; failure must not leave a
  false success indication. Preserve any known reported state separately.
  No menu or Apply for booleans.
- Numeric rows use the descriptor's bounded slider and precise stepper. The
  displayed value comes from the local draft or reported current value; unknown
  is a dash. Apply appears only after editing. An edit survives telemetry updates
  and failed submission; accepted edits remain visible while awaiting readback,
  rather than snapping back. A new connection clears the old draft.
- Choice rows use a native picker. Only an explicit selection creates a draft.
- Only actionable errors, refusals, pending activity, and procedure progress add
  feedback. No invented current value or success state.
- Trip reset keeps its confirmation. Calibration keeps its start/stop lifecycle.
- Swift renders the Rust catalog. Ordinary availability requires no UI-side
  validation authorization. Identity, range, stationary/fresh telemetry, charging,
  arming, and procedure checks remain in Rust. These are requirements; the
  checkpoint's delayed-write and outcome gaps still need repair.
- Semantic text styles and adaptive layout support Dynamic Type. Interactive
  targets are at least 44 points. Light appearance uses the shared theme.

## Complete visible settings inventory

| Group | Controls |
| --- | --- |
| Lights & display | Headlight; display brightness; display units; beeper volume |
| Limits & alarms | Tilt-back speed; PWM duty tilt-back (including Off); lateral tilt limit; speed alarm threshold; brake alarm |
| Ride feel | Pedal hardness; dynamic assist; pedal-dip compensation; pedal angle; riding mode |
| Wheel modes | Voltage correction; high-speed mode; low-battery mode; transport mode |
| Actions | Horn; reset trip; calibrate pedals |

The unresolved charging-ceiling conversion is not a usable setting. Raw charge
bytes, charging status, and shutdown countdown do not belong in Tune. Unknown
manufacturer commands are not fabricated to fill a menu. See
[the coverage audit](../../../../aero-settings-coverage.md) for remaining protocol gaps.

## Behavioral acceptance against this comp

Exercise unknown, locally pending, host-submitted, observed, failed and reconnect
snapshots without adding permanent status prose or engineering rows. Defaults
and design sample values must not become device readings. An unknown readback
does not hide an otherwise supported writable control.

Verify speed values, bounds and increments through the actual display-unit
round trip. The reported “34.8” has no recorded unit; a canonical 348 rejection
does not explain 34.8 mph (approximately 56 km/h). Rust must reject inexact
canonical values, and the UI must not silently truncate a displayed request.
Voltage correction −15…15 at precision 1 displays as −1.5…1.5%, not ±15%.

Confirm that new telemetry never overwrites a draft, reconnect clears old
operation identity/drafts, and late callbacks cannot show success on a new
connection. A matched readback establishes observed desired state; a host
write-without-response receipt alone does not. UI snapshot/unit tests, matching
the comp, and physical device effects are separate acceptance checks. Keep
unresolved controls open in LIBCU-390; the visual design does not certify them.

## Design references

- [DarknessBot official screenshots](https://play.google.com/store/apps/details?id=com.darknessproduction.darknessbot&hl=en_US): compact groups and inline controls.
- [EUC World quick controls](https://euc.world/app?lang=en) and [bounded calibration settings](https://euc.world/blog/how-to-calibrate-speed-and-distance-readings-of-your-wheel-using-euc-world?lang=en).
- [EUC World alarm simplification](https://euc.world/blog/euc-world-2-56-0-has-been-released).
- [Onewheel ride-feel sliders](https://onewheel.com/pages/faq-2020).

Tracker: LIBCU-664, LIBCU-772, LIBCU-815. Design samples are not simulator
screenshots or proof of physical wheel behavior.
