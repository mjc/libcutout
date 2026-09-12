# Aero settings coverage

The completeness target is the Aero settings available in DarknessBot and EUC
World. Tests of Cutout's implemented command variants are not a completeness
test of those apps. EUC World 2.66.1 has a versioned code/resource inventory,
and DarknessBot 6.1.0 has a static Flutter/AOT menu and adapter inventory in
the linked reverse-engineering record. Its recovered `VeteranAdapter` write
methods now corroborate the frame shapes for the existing Rust controls; the
remaining model-gated branches are still intentionally outside the generic
write API.

## Implemented commands

| Setting | Rust / CLI / mobile / Tune | Live confirmation |
| --- | --- | --- |
| Manual headlight | Implemented | Physical light behavior was observed; no decoded light-state readback |
| High beam | Implemented with the official single `LkAp` frame | Physical behavior was observed; no decoded light-state readback |
| Trip reset | Implemented, with reset lifecycle feedback | Transport submission is not confirmation |
| Legacy hard / medium / soft | Implemented | Generic family pedal readback does not establish Aero numeric MD state |
| Speed limit / TLT stop speed (V) | Implemented by the typed `SetAeroTiltbackSpeed` control | EUC World `vn_speed_limit` and the official `StopSpeedSettingActivity` share LdAp position 12; typed page-8 byte 52 readback; new writes still need device proof |
| ALM speed alarm | Implemented | Typed telemetry field; matching readback still required |
| Stop power / PWT PWM tilt-back threshold (U) | Implemented as a 0–70% margin control, with an explicit Disable PWT action | EUC World `vn_safety_margin_limit` and the official `StopPowerSettingActivity` share LdAp position 13; page-8 byte 53 decodes both margin and wire-200 Off; capture reports 21% margin (wire value 79); new writes still need device proof |
| ANG vertical angle | Implemented with the documented −8.0°…8.0° bound | No decoded setting readback |
| MD numeric pedal hardness | Implemented as a distinct 0–100 percent command | Page-8 readback decoded; capture reports 50%; new writes still need device proof |
| High-speed mode (N) | Implemented as a typed toggle | Page-8 byte 61 readback decoded; new writes still need device proof |
| Low-battery mode (P) | Implemented as a typed toggle | Page-8 byte 60 readback decoded; new writes still need device proof |
| Transportation mode (W/TRM) | Implemented as a typed toggle | Page-8 byte 57 readback decoded; new writes still need device proof |
| ANG TLT gyro calibration | Implemented as a stationary source-backed toggle; page-8 byte 56 reports idle/waiting/complete | Official NOSFET app frame and state mapping are decoded; the simulator and Tune surface now stop a completed calibration with the same command; physical calibration effect still needs device proof |
| Display backlight (J) | Implemented as a typed 0–100% control | Page-8 byte 55 reaches mobile state; new writes still need device proof |
| Beeper volume (F) | Implemented as a typed 0–100% control | Page-8 byte 63 reaches mobile state; new writes still need device proof |
| Dynamic assist (L) | Implemented as a typed 0–100% control | Page-8 byte 66 reaches mobile state; new writes still need device proof |
| Pedal-dip compensation (Q) | Implemented as a typed 0–100% control | Page-8 byte 68 reaches mobile state; new writes still need device proof |
| Voltage correction (X) | Implemented as a typed signed −15…15 control | Page-8 byte 59 reaches mobile state; new writes still need device proof |
| Lateral cutoff (O) | Implemented as a typed 35–75° control | Selector-2 byte 47 reaches mobile state; new writes still need device proof |
| Modern binary riding mode (T) | Implemented as a distinct hard/medium/soft command | EUC World source-backed LkAp frames; new writes still need device proof |
| Brake overpressure alarm | Implemented as a typed 90–125% command | Official NOSFET source-backed LdAp frame and page-8 byte 65; new writes still need device proof |

Release capability evidence remains separate from explicit validation-mode
permission. Validation mode exposes source-backed unverified writes for phone
testing and continues to label them unverified. Rust applies the model,
value-bound, fresh-speed, command sequencing, and lifecycle rules. Swift submits
typed values and renders state. Simulator state is synthetic evidence and must
not become a live readback claim.

## Remaining settings and unresolved parity

| Manufacturer setting or function | Current software status | Remaining evidence or rationale |
| --- | --- | --- |
| ANG % acceleration assist | Not a separate EUC World wheel write; the source-backed `vn_dynamic_assist` control is implemented above | Do not infer a second command from the manual label |
| ANG TLT gyro re-centering | No corresponding `vn_*` wheel setter in EUC World 2.66.1; the manufacturer source-backed command is implemented above | Physical calibration effect still needs device proof |
| ALM torque alarm | EUC World exposes current/torque alarms as app alarm preferences, not the Veteran wheel-settings path | No Aero wheel command established |
| BRT automatic headlight | EUC World’s automatic-headlight controls are Inmotion (`in_*`) settings, not Veteran/NOSFET | No Aero wheel command established |
| MxV charging ceiling | Raw page-8 readback and raw `LdAp` write are available; Tune exposes the validated `0..=70` raw control | The NF2557 fixture reports page-8 byte 64 as raw `46`. The official generic UI's `145.0 + raw / 10` V display conflicts with Aero's 126 V pack, so the mobile surface keeps this explicitly raw/unverified until an Aero-specific conversion and safe range are established |
| CAL gyro calibration | Covered by the source-backed ANG TLT gyro command above; it remains a lifecycle rather than a scalar setting | Keep requested, reported, and physically confirmed states separate |
| Running lights / stealth | No corresponding Veteran/NOSFET entry exists in the inspected EUC World menu | Exact Aero commands remain unknown |
| App-only persistence, scaling, logs, and firmware entries | `vn_headlight_persistent`, `vn_safety_margin_scaling`, `vn_download_event_log`, and firmware preferences do not mutate the wheel setting page | Kept outside the settings-write surface |
| PWT disabled setting | EUC World maps its −1 Off choice to wire 200; Rust and the mobile Tune surface preserve this as an explicit `Off` write |

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
Keep the mobile control raw and explicitly unverified until an Aero-specific
source or controlled capture establishes its meaning and safe range; do not
label it as volts.

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
