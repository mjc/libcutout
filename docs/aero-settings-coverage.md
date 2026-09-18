# Aero settings coverage

The completeness target is the Aero settings available in DarknessBot and EUC
World. Tests of Cutout's implemented command variants are not a completeness
test of those apps. EUC World 2.66.1 has a versioned code/resource inventory,
and DarknessBot 6.1.0 has a static Flutter/AOT menu and adapter inventory in
the linked reverse-engineering record. Its recovered `VeteranAdapter` write
methods now corroborate the frame shapes for the existing Rust controls; the
remaining model-gated branches are still intentionally outside the generic
write API.

## Implementation coverage, not acceptance

Status reviewed 2026-09-18: this catalog describes software paths and source
evidence, not a completed settings implementation or a merge gate for PR #106.
The [settings design review](settings-design-review.md) is the proposed repair
baseline across all protocols. Veteran is the protocol, NOSFET its dialect,
and Aero the model used by the NF2557 fixture.

| Setting | Rust / CLI / mobile / Tune | Live confirmation |
| --- | --- | --- |
| Headlight | Implemented as the one canonical Aero lighting control with direct Off/On values | The Aero profile emits the modern `SetLightON`/`SetLightOFF` command used by DarknessBot and EUC World; the wheel does not expose a decoded light-state readback |
| Trip reset | Implemented, with reset lifecycle feedback | Transport submission is not confirmation |
| Legacy hard / medium / soft | Implemented as the semantic riding preset | Generic family pedal readback does not establish Aero numeric MD state |
| Speed limit / TLT stop speed (V) | Implemented by the semantic `SettingId::TiltbackSpeed` control | EUC World `vn_speed_limit` and the official `StopSpeedSettingActivity` share LdAp position 12; typed page-8 byte 52 readback; new writes still need device proof |
| ALM speed alarm | Implemented | Typed telemetry field; matching readback still required |
| Stop power / PWT PWM tilt-back threshold (U) | Implemented as a semantic 30–100% duty control, with an explicit `Disabled` value | EUC World `vn_safety_margin_limit` and the official `StopPowerSettingActivity` share LdAp position 13; page-8 byte 53 decodes both duty and wire-200 Off; capture reports 21% margin (wire value 79); new writes still need device proof |
| ANG vertical angle | Implemented with the documented −8.0°…8.0° bound | No decoded setting readback |
| MD numeric pedal hardness | Implemented as a distinct 0–100 percent command | Page-8 readback decoded; capture reports 50%; new writes still need device proof |
| High-speed mode (N) | Implemented as a typed toggle | Page-8 byte 61 readback decoded; new writes still need device proof |
| Low-battery mode (P) | Implemented as a typed toggle | Page-8 byte 60 readback decoded; new writes still need device proof |
| Transportation mode (W/TRM) | Implemented as a typed toggle | Page-8 byte 57 readback decoded; new writes still need device proof |
| ANG TLT gyro calibration | Implemented as a stationary source-backed toggle; page-8 byte 56 reports idle/waiting/complete | Official NOSFET app frame and state mapping are decoded; the simulator and Tune surface now stop a completed calibration with the same command; physical calibration effect still needs device proof |
| Display backlight (J) | Implemented as a typed 0–100% control | Page-8 byte 55 reaches mobile state; a live mirrored NF2557 test requesting 1% left the wheel at 0% and the app reported `Not confirmed`; the target was restored to 0% |
| Beeper volume (F) | Implemented as a typed 0–100% control | Page-8 byte 63 reaches mobile state; new writes still need device proof |
| Dynamic assist (L) | Implemented as a typed 0–100% control | Page-8 byte 66 reaches mobile state; new writes still need device proof |
| Pedal-dip compensation (Q) | Implemented as a typed 0–100% control | Page-8 byte 68 reaches mobile state; new writes still need device proof |
| Voltage correction (X) | Implemented as a typed signed −15…15 control in tenths of a percent (−1.5…1.5%) | Page-8 byte 59 reaches mobile state; new writes still need device proof |
| Lateral cutoff (O) | Implemented as a typed 35–75° control | Selector-2 byte 47 reaches mobile state; new writes still need device proof |
| Modern binary riding mode (T) | Implemented as a distinct hard/medium/soft command | EUC World source-backed LkAp frames; new writes still need device proof |
| Brake overpressure alarm | Implemented as a typed 90–125% command | Official NOSFET source-backed LdAp frame and page-8 byte 65; new writes still need device proof |

The production Tune catalog exposes all 18 source-backed editable settings
without a validation-mode switch: four lights/display controls, five limits
and alarms, five ride-feel controls, and four wheel modes. Source/hardware
evidence remains distinct from ordinary command availability;
the production descriptor does not claim a physical test. Rust owns the model,
value-bound, fresh-speed, command sequencing, and lifecycle policy. Mobile
setting requests now retain identity through native queueing and distinguish
acceptance from host submission. Lower-level bindings, delayed multi-step
actions, and rechecking authorization at delayed native handoff still need
their broader contract work.
The low-level NOSFET/Falcon encoders now reject inexact speed values instead of
truncating them. Swift submits typed values and
renders state; it must not supply a second policy. Simulator state is synthetic
evidence and must not become a live readback claim.

## Physical evidence and open acceptance

### September 18 Tune failure report

The phone capture covering the user's 15:15 screenshot contains successful
writes followed by matching CRC-validated page-8 readback: display brightness
0→30%, beeper volume 0→5%, and tilt-back speed 56→55 km/h (34.2 mph).
Those observations are stronger evidence than the generic error retained in
the screenshot. The same capture also contains wheel speed above the existing
500 mm/s settings limit around the screenshot time. It does not record the
timestamp and reason of each refused button press, so it cannot establish the
exact cause of every displayed error.

The regression `nf2557_reported_tune_values_send_when_stopped_and_refuse_while_moving`
replays complete stopped and moving packets from that capture. It checks the
four reported values (55 km/h tilt-back, 80% PWM duty, 45° lateral limit, and
56 km/h speed alarm), their command destinations and payload values, refusal
without any write while moving, and a successful retry after stopping.

The repaired mobile path retains the typed protocol refusal instead of
replacing it with the generic send error. Rejected drafts remain drafts; edits
and matching readback clear stale local feedback. Transport receipts carry a
request identity through BLE queueing and chunk completion. Queue overflow
rejects the incoming write rather than silently evicting earlier work, and
confirmation timing begins after host submission. Host submission alone is
not proof of delivery or a physical effect. Delayed multi-step actions still
need their own end-to-end operation correlation; this settings repair does
not close that acceptance item.

The user's NF2557 reports are evidence of individual interactions, not a
controlled acceptance run with recorded request, host receipt, readback and
restoration. Preserve positive reports without using them to close other paths:

| Controls | Reported result | Remaining acceptance |
| --- | --- | --- |
| Beeper volume, tilt-back speed, PWM tilt-back, pedal hardness, dynamic assist, pedal dip, voltage correction | User reported working | Individually record exact targets, units, effect, available readback and restoration under the repaired lifecycle |
| Display brightness | User previously reported working; a live mirrored NF2557 test requested 1%, displayed `Wheel 0%`, and ended `Not confirmed`; the requested target was restored to 0% | Determine whether the request was submitted, why no matching readback arrived, and why the operation did not produce a clean terminal result |
| Headlight | The binary `LkAp` frame was transmitted but had no physical effect on NF2557 | Use the modern literal `SetLightON`/`SetLightOFF` commands and re-verify the physical lamp effect; transport submission alone is insufficient |
| Horn, reset trip, pedal angle | User reported no effect | Establish correct applicability/encoding and required physical effect; transport submission alone is insufficient |
| Lateral tilt and speed alarm | Repeated unconfirmed/timeout results; speed alarm also had a reported crash | Resolve encoding, display-unit conversion, observation acquisition and terminal outcomes; reproduce the crash path offline |
| Brake alarm | Initially reported working without updating the wheel percentage; later 120 reported sent without confirmation | Establish desired effect and fresh reported percentage separately |
| High-speed and low-battery modes | Beeped but were not confirmed | Sound is not state confirmation; verify mode observation and effect |
| Calibration and transport mode | No acceptance established; user declined calibration | Remain open; no automatic action sweep |

The user distinguished working beeper volume from nonworking “wheel volume.”
That requested effect remains unresolved; it does not establish a second wire
setting. Trace the intended sound behavior against the source-backed key-tone
mapping below before adding another control or declaring sound parity complete.

The reported speed-alarm value “34.8” had no specified unit. Canonical 348
deci-km/h is inexact for a whole-km/h write, but displayed 34.8 mph is about
56 km/h. The actual display-to-request path must be reproduced before assigning
cause. Library bounds and confirmation flags are declarations, not negotiated
wheel capabilities. The recent early `current=nil` inventory preceded settings
telemetry and does not negate the captured settings pages below.

The subsequent audible incident ended after a power cycle. Its offending
command and complete restoration remain unknown. Further physical work requires
the offline harness repairs and individually reviewed cases in the design review;
descriptor minima, guessed values and broad audible opt-ins are not safe cases.

## Remaining settings and unresolved parity

| Manufacturer setting or function | Current software status | Remaining evidence or rationale |
| --- | --- | --- |
| ANG % acceleration assist | Not a separate EUC World wheel write; the source-backed `vn_dynamic_assist` control is implemented above | Do not infer a second command from the manual label |
| ANG TLT gyro re-centering | No corresponding `vn_*` wheel setter in EUC World 2.66.1; the manufacturer source-backed command is implemented above | Physical calibration effect still needs device proof |
| ALM torque alarm | EUC World exposes current/torque alarms as app alarm preferences, not the Veteran wheel-settings path | No Aero wheel command established |
| BRT automatic headlight | EUC World’s automatic-headlight controls are Inmotion (`in_*`) settings, not Veteran/NOSFET | No Aero wheel command established |
| MxV charging ceiling | Raw page-8 readback is exposed as a diagnostic; no write command is available | The NF2557 fixture reports page-8 byte 64 as raw `46`. The official generic UI's `145.0 + raw / 10` V display conflicts with Aero's 126 V pack, so the mobile surface keeps this explicitly raw and read-only until an Aero-specific conversion and safe range are established |
| CAL gyro calibration | Covered by the source-backed ANG TLT gyro command above; it remains a lifecycle rather than a scalar setting | Keep requested, reported, and physically confirmed states separate |
| Running lights / stealth | No corresponding Veteran/NOSFET entry exists in the inspected EUC World menu | Exact Aero commands remain unknown |
| App-only persistence, scaling, logs, and firmware entries | `vn_headlight_persistent`, `vn_safety_margin_scaling`, `vn_download_event_log`, and firmware preferences do not mutate the wheel setting page | Kept outside the settings-write surface |
| PWT disabled setting | EUC World maps its −1 Off choice to wire 200; Rust and the mobile Tune surface preserve this as an explicit `Disabled` write | Source-backed encoding is not a new physical acceptance result |

EUC World's `beeper_volume` and the official app's `key tone` control share the
same page-8 byte 63 and `LdAp` position; they are one wire setting, not two
independent controls.

The PWM safety beep is not the PWT tilt-back threshold. The manual describes a
fixed PWM warning that cannot be disabled; a phone-generated alarm in another
app would be a different feature. Inventory the app behavior before adding a
wheel-write toggle for it.

FreeWheel labels page-8 byte 68 `acceleration_limit`, whereas EUC World labels
the same field `pedal_dip_compensation`. Cutout treats this as one wire setting,
not two Aero commands; a distinct acceleration-limit control requires a
model-specific capture.

The max-charge field is intentionally different from the other source-backed
settings: it is present in the captured page-8 payload and its raw transport
frame is source-backed, but the official NOSFET/EUC World generic conversion
produces a voltage that cannot be valid for the identified 126 V Aero pack.
Keep the field in engineering diagnostics, outside the production Tune screen,
until an Aero-specific source or controlled capture establishes its meaning and
safe range; do not label it as volts or manufacture a charging-limit control.

Unknown payloads remain out of the write surface. An Unsupported row does not
mean the setting is implemented, and a simulator cannot supply missing wire
evidence. The inspected EUC World Veteran numeric controls are represented
by the implemented source-backed controls above. Modern binary riding-mode T
remains distinct from the legacy pedal presets. DarknessBot's decoded
`VeteranAdapter` paths corroborate the implemented controls; its model-gated
legacy light and mileage-reset branches remain open until their selection rules
and device effects are independently established.

## Sources and limits

- The ignored `.protocol-references/freewheel` source snapshot provides an
  additional cross-platform app inventory. `core/.../domain/settings/WheelSettingsConfig.kt`
  enumerates the Veteran menu (headlight, screen backlight, pedal mode and
  hardness, alarm/stop speed, pedal tilt, PWM limit, dynamic assist,
  acceleration limit, units, transport/high-speed/low-voltage modes, key tone,
  voltage correction, charge ceiling, brake-pressure alarm, lateral cutoff,
  lock, calibration, power-off and trip reset). Its
  `core/.../protocol/VeteranDecoder.kt` `buildCommandWithVer` implementation
  records the corresponding LkAp/LdAp positions and firmware gates. This
  corroborates the source-backed settings implemented here while preserving
  FreeWheel's model gates: dynamic assist and acceleration limit are removed
  for NOSFET models, and brake-pressure alarm uses a different field for
  non-NOSFET Veteran models. Lock, power-off and password operations remain
  outside this benign settings surface because they are dangerous controls.

- [NOSFET support and official manuals](https://www.nosfet.com/support), with
  [AERO manual 4.0 mirror](https://device.report/m/15c21f5faf7c4b946395bd8596be32a778817c729ce71cdcc1f10e7f53ae463f.pdf):
  manufacturer menu names, numeric MD, distinct lighting modes, and display
  ranges. These document device features, not their BLE encodings.
- [EUC Planet Veteran protocol](https://github.com/eried/eucplanet/blob/fac53425ff418821f8a8792eae072e063e7d23e9/docs/protocols/veteran.md)
  and [command builders](https://github.com/eried/eucplanet/blob/fac53425ff418821f8a8792eae072e063e7d23e9/app/src/main/java/com/eried/eucplanet/ble/VeteranCommands.kt):
  MIT source. Numeric MD uses a 15-byte `LdAp` frame with payload prefix
  `01 02 80 80 80`, a percent byte, and big-endian CRC32. The builder accepts
  0–100; 30–100 in the prose is an observed subset. The reference capture is
  from a Lynx S and is not Aero hardware evidence. Other vendor settings are
  explicitly unmapped by this source.
- [EUC World Aero support announcement](https://euc.world/blog/euc-world-2-54-0-has-been-released)
  and [DarknessBot listing](https://apps.apple.com/us/app/darknessbot/id1108403878):
  app support claims do not enumerate every Aero command. DarknessBot's
  static 6.1.0 artifact and decoded AOT frame inventory are recorded in the
  linked RE inventory; static construction evidence remains distinct from
  physical write/readback proof.
- The ignored `.protocol-references/nosfet-official` decompilation is the
  authoritative Aero cross-check for lighting: its `BtManager` sends the
  high-beam `LkAp` frame alone. The older `LdAp` companion capture is not sent
  for this Aero control.

External app/reference source stays in the ignored `.protocol-references`
directory. No APK, decompiled source, or vendor binary belongs in the branch.
