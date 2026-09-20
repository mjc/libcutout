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

Status reviewed 2026-09-20: this catalog describes software paths and source
evidence, not a completed settings implementation or a merge gate for PR #106.
The [settings design review](settings-design-review.md) is the proposed repair
baseline across all protocols. Veteran is the protocol, NOSFET its dialect,
and Aero the model used by the NF2557 fixture.

## Protocol detection and capture boundary

Advertisement data chooses the device the user selected; it does not choose a
wire protocol. Detection accumulates fragmented notifications and keeps the
attempt pending while the evidence is incomplete. Once the Rust detector has a
supported protocol/model candidate, the mobile session must admit that exact
candidate before sending any family-specific fallback probe. This prevents a
known Veteran/NOSFET Aero from being treated as a generic Begode candidate just
because both families share a GATT transport.

Record-only capture remains an explicit path for an unresolved device and a
terminal fallback for a first-use attempt that never produces usable evidence.
An incomplete fragment or a temporary missing response is not, by itself, a
reason to navigate to capture, and a normal reconnect must not silently turn a
known device into capture mode.

| Setting | Rust / CLI / mobile / Tune | Live confirmation |
| --- | --- | --- |
| Headlight power | Implemented as the one canonical Aero lighting power control with direct Off/On values | A live NF2557 test physically confirmed both `SetLightON` and `SetLightOFF`; the wheel does not expose a decoded light-state readback |
| Headlight intensity modes | Deferred; not part of the current headlight power contract | The AERO has multiple physical headlight modes, but no brightness-cycle wire command has been established from EUC World, DarknessBot, or the NF2557 capture. A constrained live `AA 55 14 02 16` candidate was captured as an outbound FFE1 write with no decoded acknowledgement and no distinguishable physical brightness effect; it remains unverified and is not product protocol support. |
| Trip reset | Implemented, with reset lifecycle feedback | Transport submission is not confirmation |
| Legacy hard / medium / soft | Shared encoder retained, excluded from Aero | NOSFET's menu hides the preset picker when numeric hardness is available. NF2557 has numeric MD; these are alternatives, not independent controls. |
| Speed limit / TLT stop speed (V) | Implemented by the semantic `SettingId::TiltbackSpeed` control | EUC World `vn_speed_limit` and the official `StopSpeedSettingActivity` share LdAp position 12; typed page-8 byte 52 readback; new writes still need device proof |
| ALM speed alarm | Implemented with an Aero-specific 55 km/h ceiling enforced before transport | Typed telemetry field and matching readback are present; a valid 54–55 km/h device application/readback still requires live proof |
| Stop power / PWT PWM tilt-back threshold (U) | Implemented as a semantic 30–100% duty control, with an explicit `Disabled` value | EUC World `vn_safety_margin_limit` and the official `StopPowerSettingActivity` share LdAp position 13; page-8 byte 53 decodes both duty and wire-200 Off; capture reports 21% margin (wire value 79); new writes still need device proof |
| ANG vertical angle | −8.0°…8.0°, written in 0.1° steps; observed values preserve 0.01° precision | Live build `2341c112f` decoded −1.00°. One request for −0.90° was followed by 145 fresh observations still at −1.00°. Readback works; changed-value application remains unresolved. |
| MD numeric pedal hardness | Implemented as a distinct 0–100 percent command | Page-8 readback decoded; capture reports 50%; new writes still need device proof |
| High-speed mode (N) | Shared Veteran encoder/readback exists, but it is excluded from the Aero product surface | EUC World's generic Veteran menu does not establish that NF2557 offers this as a real user setting; do not expose it for Aero |
| Low-battery mode (P) | Shared Veteran encoder/readback exists; excluded from the Aero product surface | The NOSFET Aero manual specifies Always ON. A configuration beep does not establish a writable Aero mode. |
| Transportation mode (W/TRM) | Shared Veteran encoder/readback exists, but it is excluded from the Aero product surface | It remains out of the current UI and live-test scope |
| Display backlight (J) | Implemented as a typed 0–100% control; 0% means display off | Previously confirmed working by the user. The latest `2341c112f` request for 31% was followed by 21 fresh observations still at 30%; current delivery/application needs investigation. |
| Beeper volume (F) | Implemented as a typed 0–100% control | Page-8 byte 63 reaches mobile state; new writes still need device proof |
| Dynamic assist (L) | Implemented as a typed 0–100% control | Page-8 byte 66 reaches mobile state; new writes still need device proof |
| Pedal-dip compensation (Q) | Implemented as a typed 0–100% control | Page-8 byte 68 reaches mobile state; new writes still need device proof |
| Voltage correction (X) | Implemented as a typed signed −15…15 control in tenths of a percent (−1.5…1.5%) | Page-8 byte 59 reaches mobile state; new writes still need device proof |
| Lateral cutoff (O) | Typed 35–75° control; EUC World setter O and NOSFET's official activity send one 22-byte `LkAp` frame | Selector-2 byte 47 reaches mobile state. The previously added `LdAp` companion was not supported by either inspected source; its removal requires a fresh changed-value/restoration test. |
| Modern binary riding mode (T) | Shared source-backed command, not an Aero product control | The generic family menu does not override the official numeric-hardness/preset exclusion |
| Brake overpressure alarm | Implemented as a typed 90–125% command | Official NOSFET source-backed LdAp frame and page-8 byte 65; new writes still need device proof |

The production Tune catalog exposes 14 source-backed editable settings without
a validation-mode switch: four lights/display controls, five limits and alarms,
four ride-feel controls, and voltage correction. Source/hardware
evidence remains distinct from ordinary command availability;
the production descriptor does not claim a physical test. Rust owns the model,
value-bound, fresh-speed, command sequencing, and lifecycle policy. Mobile
setting requests now retain identity through native queueing and distinguish
acceptance from host submission. The native handoff repair rechecks the original
Rust authorization immediately before writing, and preserves request identity
through delayed setting stages. Delayed multi-step actions remain separate open
contract work; this is not acceptance of all actions or physical settings.
The low-level NOSFET/Falcon encoders now reject inexact speed values instead of
truncating them. Swift submits typed values and
renders state; it must not supply a second policy. Simulator state is synthetic
evidence and must not become a live readback claim.

## Physical evidence and open acceptance

### September 20 native handoff investigation

Build `2341c112f` was tested through CutOut's existing phone connection using
iPhone Mirroring, not a competing Mac Bluetooth connection. Pedal angle
−1.00° → −0.90° and brightness 30% → 31% each produced exactly one intended
outbound entry, but fresh CRC-valid observations remained at the original
values for approximately 124 and 78 seconds respectively. Returning each
editor to its current value cleared the draft; it did not send a restoration
command. The [full live report](https://lific.mjc.lol/LIBCU/pages/31) preserves
the measurements and distinguishes observed state from physical effect.

Those old outbound capture entries were recorded **before** native queue
submission. They do not prove that `CBPeripheral.writeValue` was called. The
new capture metadata records a native write ID with typed queued, submitted,
rejected or cancelled receipts. Submitted is recorded only after the native
write call; it is still not wheel acknowledgement. Legacy captures without
receipt metadata retain unknown submission evidence.

The repair also prevents an expired, replaced or revoked setting from leaving
the native queue later, and preserves request identity for later protocol
stages. Neither defect has yet been proved to explain the two physical failures.
A new approved deployment and controlled receipt/readback test must distinguish
host-side loss from a device-side rejection before changing the reference-backed
wire encoding. No retry loop or speculative alternate header is added.

The reference comparison separately found that the lateral-tilt encoder's
extra `LdAp/01 00` frame was not source-backed: EUC World and NOSFET each emit
one `LkAp` frame for Aero. An earlier failed single-frame readback did not
justify adding the second frame. The encoder and its parity tests now follow
the reference's single-frame command; this does not establish physical success.

### User retest of build `702a83423`

The user accepted Tune layout/scrolling and display-brightness edit/Cancel/Apply.
Pedal angle accepts a draft but remains Requested; Hard/Medium/Soft does the
same, with no independently established physical effect. Low-battery mode beeps,
briefly shows Pending, then clears it. These are user observations, not a captured
request/readback matrix.

The [NOSFET Aero manual, page 6](https://device.report/m/15c21f5faf7c4b946395bd8596be32a778817c729ce71cdcc1f10e7f53ae463f_optim.pdf#page=7)
specifies low-battery mode as Always ON. The Aero profile therefore excludes
that toggle and rejects either target even with validation authorization;
shared wire decoding is retained without promoting it to an Aero capability.
The same manual identifies MD as 0–100% pedal hardness. NOSFET Android 1.1.3
explicitly hides Hard/Medium/Soft when numeric hardness is available, so the
Aero catalog now exposes only numeric hardness. The shared legacy commands
remain available to protocol research, not as Aero controls.

The source investigation also found the missing ANG observation at selector
0/4, signed bytes 67–68, in hundredths of a degree. The existing single `LkAp`
angle command matches NOSFET/EUC World; no header change or companion is needed.
The library now preserves hundredths, accepts only exact tenth-degree writes,
and uses matching fresh readback for completion. A nearby reading cannot be
rounded into confirmation. Historical zero readings establish decoding, not
the effect of the user's latest write. The September 20 retest above establishes
live readback but not successful changed-value application.

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
four supported values (55 km/h tilt-back, 80% PWM duty, 45° lateral limit, and
55 km/h speed alarm), their command destinations and payload values, refusal
without any write while moving, and a successful retry after stopping. A
separate session test rejects 56 km/h before it can replace an existing pending
request or reach the wire.

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
| Display brightness | User confirmed it works as advertised; 0% turns the display off | Keep the 0% off semantics in the generic setting contract and verify other requested levels during the broader acceptance pass |
| Headlight power | The old binary `LkAp` frame was transmitted but had no physical effect on NF2557; the corrected literal commands turned the lamp on and off in a live test | Keep the exact command bytes and physical result as the acceptance evidence; no decoded light-state readback is expected |
| Headlight intensity modes | Deferred because the reference app path does not currently establish a brightness-cycle command | Revisit only if EUC World or another source provides a verified command; do not treat `SetLightON` as a brightness selector |
| Horn, reset trip, pedal angle | Horn and pedal angle remain unconfirmed; reset had no effect | A new app trip attempts the semantic trip-reset action automatically for any live device that exposes a trip meter; the current Aero encoding is the established `CLEARMETER` literal, while horn and pedal angle still need protocol/device evidence |
| Lateral tilt and speed alarm | An earlier lateral request left the observation at 40°; a later 41° → 42° test did change readback | Recheck source-matching single-frame lateral submission, selector-2 readback, and restoration; the valid below-limit speed-alarm request remains separately open |
| Brake alarm | Initially reported working without updating the wheel percentage; later 120 reported sent without confirmation | Establish desired effect and fresh reported percentage separately |
| High-speed and low-battery modes | Historical shared-catalog attempts beeped without an established change | Excluded from the current Aero catalog; neither belongs in the current live checklist |
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
| ANG TLT angle-tilt re-centering | The Aero manual identifies it, but EUC World 2.66.1 has no matching Veteran `vn_*` key or setter | It is neither pedal angle nor gyro calibration. A NOSFET-app trace or captured Aero button write is required before exposing it |
| ALM torque alarm | The Aero manual identifies it, but EUC World 2.66.1 has no matching Veteran `vn_*` key or setter | No Aero wheel command is established; do not reuse another app's alarm preference |
| MxV charging ceiling | EUC World maps `vn_charging_voltage_limit` to setter `G`, an Ld value field, and page-8 byte 64 | The generic 147.0–151.6 V UI domain and `145.0 + raw / 10` readback cannot describe a 126 V Aero. Keep MxV unavailable rather than presenting raw bytes or an invalid voltage until Aero-specific semantics are established |
| CAL calibration | The Aero manual lists calibration separately from angle-tilt re-centering | Both are outside the current Aero UI/live-test scope; neither may be aliased to the other without source-backed command evidence |
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
- The ignored `.protocol-references/nosfet-official` decompilation was the
  initial static cross-check for a legacy high-beam `LkAp` frame. The live
  NF2557 result disproved that frame for the modern Aero: it was transmitted
  but had no physical effect. The confirmed modern path is the literal
  `SetLightON`/`SetLightOFF` pair; the separate physical brightness-cycle
  command remains unidentified.

External app/reference source stays in the ignored `.protocol-references`
directory. No APK, decompiled source, or vendor binary belongs in the branch.
