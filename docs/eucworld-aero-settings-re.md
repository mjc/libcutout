# EUC World settings reverse-engineering inventory

## Artifact and scope

Inspected on 2026-09-06: `net.lastowski.eucworld` version **2.66.1**, code
`2020660001`, confirmed from its decoded Android manifest. The APK was retrieved
from [APKPure's EUC World distribution](https://apkpure.net/euc-world/net.lastowski.eucworld)
for static inspection, not installed or executed.

SHA-256: `9fe7c80e682d1ef992c00afcf43d4df3209cd763f5c427cba3d005293c06b220`.
The archive contains a `BNDLTOOL` signer; publisher-certificate continuity with
Google Play has not been established. This is a version-pinned reverse-engineering
reference, not a verified publisher distribution or device proof.

All APKs and decompiled resources remain in ignored `.protocol-references`.
This document records protocol facts and references, not copied app source.

DarknessBot 6.1.0 was also inspected statically from its split XAPK. The base
APK identifies a Flutter application and the arm64 split contains an AOT
`libapp.so`; JADX therefore cannot recover its Dart implementation. The AOT
image was decoded with [Blutter](https://github.com/worawit/blutter), retaining
the generated assembly outside the repository while keeping the input artifact
under the ignored reverse-engineering workspace. The
input `libapp.so` is pinned by SHA-256
`ebfaecb6eb3cb02dbeb3c39abf93e978fbdb690044ed086829fec71583d3ed96`.

The recovered `VeteranAdapter` methods now provide a frame-level parity trace
for the settings already implemented in Rust:

| DarknessBot method | Decoded frame shape | Existing Cutout command |
| --- | --- | --- |
| `changeGyroLevel` | `LkAp`, declared length 16, value at absolute byte 11 | `SetAeroAngleAdjustment` |
| `changeMaxRollAngle` | `LkAp`, declared length 22, value at absolute byte 17 | `SetAeroLateralTiltLimit` |
| `changeSafeMode` | `LdAp`, declared length 25, value at absolute byte 20 | `SetAeroLowBatteryMode` |
| `changeMaxSpeed` | `LdAp`, declared length 17, value at absolute byte 12 | `SetAeroTiltbackSpeed` |
| `changeVolume` | `LdAp`, declared length 28, value at absolute byte 23 | `SetAeroBeeperVolume` |
| `changeTorchMode` | Modern branch uses the ASCII `SetLightON`/`SetLightOFF` commands; an older branch constructs model-gated binary frames | `SetLights` for the modern branch |
| `changeRidingLevel` | Selects an advanced `LdAp` frame (declared length 24) or a legacy binary frame based on the adapter's riding-mode flag | `SetAeroRidingMode` covers the source-backed modern command; legacy selection still needs a model/firmware gate |
| `changeLimitSpeed` | Stores the app's limit-speed preference and delegates to `changeMaxSpeed` only when the adapter's limit-mode flag is enabled; it is not a second wheel setting | `SetAeroTiltbackSpeed`; no separate limit-mode wheel write is inferred |
| `changeLimitMode` | Updates the app/device preference record and conditionally invokes `changeMaxSpeed`; no independent command builder is present | App policy, not a separate benign wheel command |
| `resetSingleMileage` | Protocol-v2 branch constructs a multi-write reset sequence; the recovered AOT does not expose a stable setting key or generic model gate | `ResetTripMeter` exists, but the multi-step legacy sequence remains outside the generic API |

The byte shapes, positions, and CRCs are covered by the existing Rust golden
frame tests. The older `changeTorchMode` branch is recorded as evidence but is
not emitted by the generic encoder: model/firmware selection is not an input to
that API, and sending an un-gated legacy companion frame can produce the extra
acknowledgement beep observed on Aero. This is static command-construction
evidence, not proof of a physical write or readback result.

## Reproducible code trail

Local reference roots: `eucworld-2.66.1`, `eucworld-resources`,
`eucworld-fallback`, and `eucworld-fallback-ui` under `.protocol-references`.
JADX 1.5.6 recovered the command builders normally; its fallback instruction
output was needed for two larger methods. Do not treat failed Java decompilation
as evidence that those paths do not exist.

- `res/xml/preferences_wheel_veteran.xml`: 24 menu keys, units, bounds and off labels.
- `res/values/strings.xml`, `arrays.xml`: display labels and enum values.
- `sources/mi/p.java`, method `l`: capability/readback-dependent UI enablement.
- `sources/ki/q.java`, fallback method `a`: preference key to protocol setter.
- `sources/uh/g0.java`: setters `E` through `X`; helper `y` initializes payload
  bytes to `0x80`; helper `H` adds header, total length and big-endian CRC32.
- `sources/uh/g0.java`, fallback method `h`: protocol model detection and page-8
  readback dispatch. Property names survive in the static `z` table.

## Wheel-write inventory

`pos` below is the absolute, zero-based value-byte position. `Ld` means `LdAp`
with bytes 5–6 = `01 02`; intervening bytes are `80`. Total frame size is
`pos + 5`, including the trailing four-byte CRC32. `Lk` setters use `LkAp` and
byte 5 = `01`; the serializer can select `LdAp` for newer firmware, so these
are not unconditional family-wide encodings. Preserve protocol/firmware gates.

| EUC World key (`vn_` prefix omitted) | Setter | Values and frame | Readback / Cutout gap |
| --- | --- | --- | --- |
| `alarm_speed` | E | 10–200 km/h, Lk pos 12 | Header 24–25 deci-km/h; typed Cutout setting uses 10–200 |
| `speed_limit` | V | 10–200 km/h, Ld pos 12 | Header 26–27 and page-8 byte 52; typed Cutout setting uses 10–200 |
| `safety_margin_limit` | U | UI 0–70% → wire `100 − UI`, Ld pos 13; UI −1 = Off → wire 200 | Page-8 byte 53; Cutout exposes margin and explicit Off writes |
| `display_mode` | K | 0/1, Ld pos 18 | Page-8 byte 58; typed wheel-units readback and write implemented |
| `display_backlight` | J | 0–100%, Ld pos 15 | Page-8 byte 55; typed mobile readback implemented |
| `beeper_volume` | F | 0–100%, Ld pos 23 | Page-8 byte 63; typed mobile readback implemented; not the fixed PWM alarm |
| `dynamic_assist` | L | 0–100%, Ld pos 26 | Page-8 byte 66; typed mobile readback implemented; generic On/Off assist is not this control |
| `pedal_dip_compensation` | Q | 0–100%, Ld pos 28 | Page-8 byte 68; typed mobile readback implemented |
| `pedals_sensitivity` | R | 0–100%, Ld pos 10 | Page-8 byte 50; Cutout MD write and readback implemented |
| `pedals_tilt` | S | Signed −80…80 tenths of a degree, pos 11 | Cutout ANG write uses the documented −80…80 range; live setting readback unresolved |
| `lateral_tilt_limit` | O | 35–75 degrees, Lk pos 17 | Selector-2 byte 47 readback is decoded; typed mobile readback implemented |
| `high_speed_mode` | N | 0/1, Ld pos 21 | Page-8 byte 61; typed encoder/readback/UI implemented on the settings branch; physical write proof pending |
| `low_battery_mode` | P | 0/1, Ld pos 20 | Page-8 byte 60; typed encoder/readback/UI implemented on the settings branch; physical write proof pending |
| `transportation_mode` | W | 0/1, Ld pos 17 | Page-8 byte 57; typed encoder/readback/UI implemented on the settings branch; physical write proof pending |
| `charging_voltage_limit` | G | Raw page-8 byte 64; official generic UI uses decivolts minus 1450 at Ld pos 24 | Raw readback, write, and a `0..=70` raw Tune control are implemented for protocol validation, but no voltage conversion or safe Aero range is claimed: the generic `145 + raw / 10` display would report 149.6 V for a 126 V Aero pack |
| `voltage_correction` | X | Signed −15…15, displayed in tenths of a percent, Ld pos 19 | Page-8 byte 59 signed; typed mobile readback implemented; not a volts offset |
| `headlight_mode` | M | Off/On; binary pos 8 or legacy ASCII | Page-8 byte 47; existing writes; readback/beam applicability needs separate proof |
| `riding_mode` | T | UI hard/medium/soft → binary 3/2/1 at pos 7, or legacy ASCII | Distinct from MD; Cutout exposes both the modern binary command and legacy presets |

Remaining menu keys are accounted for separately, not invented as new wheel writes:

| Key | Classification / remaining trace |
| --- | --- |
| `vn_headlight_persistent` | App persistence preference; not another physical lighting mode |
| `reset_user_distance` | Reset action through the app confirmation handler; Cutout trip reset exists, binary/legacy parity still needs tracing |
| `vn_download_event_log` | Log retrieval, not a settings mutation |
| `vn_safety_margin_scaling` | App-side correction setting; do not confuse with the PWT wheel command |
| `vn_firmware_list` | Firmware listing/update entry point; not authorized by ordinary settings writes |
| `vn_firmware_high_transfer_rate` | Firmware-transfer preference; separate from this settings slice |

## Applicability and readback findings

The menu is shared across Veteran/NOSFET devices. Most controls are enabled only
when binary settings are available **and their corresponding decoded value is
present**. A menu entry alone does not prove an Aero supports that command.
Page-8 byte `0x80` means unavailable. The existing NF2557 capture contains
**100 at byte 66 and 0 at byte 68**, not unavailable sentinels. EUC World's mapping
therefore supplies a concrete lead for numeric assist/dip controls on this Aero;
it still does not establish the result of sending a new value.

The 2026-06-22 NF2557 page-8 capture reports MD 50% and raw PWT 79%. EUC World's
PWT control displays unused PWM margin, so this raw threshold corresponds to a
21% margin. Its legal input is 0..70%, encoded as `100 - margin`; Off is a
distinct state encoded as 200. Cutout now decodes
those into the existing Rust-owned setting trackers; the mobile integration test
replays captured bytes and checks current, pending and matching-readback states.
CRC bytes, invalid percentages, other pages and other protocol models cannot
populate these fields. Mapping provenance remains SourceVerified, and write
capabilities remain Unverified until physical write behavior is established.

The same capture reports page-8 byte 64 as raw `46`. Although the generic
NOSFET/EUC World setting path labels that byte as a 145–151.6 V charging limit,
that conversion is incompatible with the identified 126 V Aero pack. Cutout
keeps the source-backed transport as an explicitly raw, unverified read/write;
the mobile surface must not present it as volts until an Aero-specific
conversion and safe range are physically established.

FreeWheel's Veteran decoder calls page-8 byte 68 `acceleration_limit`, while
EUC World 2.66.1 exposes the same wire position as
`vn_pedal_dip_compensation` (setter `Q`, `LdAp` position 28). This is a label
and product-policy difference, not evidence for two independent Aero commands;
Cutout keeps one typed `PedalDipCompensation` field until a model-specific
capture proves a distinct acceleration-limit command.

Cross-check: NOSFET's official Android 1.1.3 download independently uses the same
MD/PWT offsets and excludes the CRC. This is corroboration, not the parity target.

The Rust command/encoder surface now includes backlight brightness and beeper
volume (0..100%), wheel units, lateral tilt limit (35..75 degrees), voltage
correction (-1.5..+1.5%), numeric dynamic assist/pedal-dip compensation
(0..100%), the N/P/W safety-mode toggles, the modern binary riding mode, and the
NOSFET brake-overpressure alarm (90..125%). Speed controls use 10..200 km/h.
Golden frame tests cover the numeric additions and safety modes, including CRC
and the 33-byte pedal-dip command. These are source-backed software
capabilities awaiting device effect/readback proof, not hardware-validated
writes. The official NOSFET control menu, the EUC World Veteran inventory, and
the decoded DarknessBot `VeteranAdapter` frame paths above are covered. The
remaining DarknessBot-only actions (including model-gated legacy light and riding-level frames
and its multi-step mileage reset path) stay out of the generic write surface
until their protocol gate and device behavior are independently established.
`changeLimitSpeed` and `changeLimitMode` do not add new wheel commands: the
recovered code only persists policy and, in one branch, delegates to the
already traced maximum-speed setter.
Unknown manufacturer-menu items stay open.
