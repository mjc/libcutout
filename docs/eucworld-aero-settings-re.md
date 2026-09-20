# EUC World settings reverse-engineering inventory

## Artifact and scope

Re-inspected on 2026-09-19: `net.lastowski.eucworld` version **2.66.1**, code
`2020660001`, confirmed from its decoded Android manifest. The XAPK was retrieved
from [APKPure's EUC World distribution](https://apkpure.net/euc-world/net.lastowski.eucworld)
for static inspection, not installed or executed.

SHA-256: `024f1f88ed0fd447aadec61b34508b70fdb2843ed3b8d3bb49e27c22ae0ab46a`.
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
| `changeGyroLevel` | `LkAp`, declared length 16, value at absolute byte 11 | `SettingId::PedalAngle` |
| `changeMaxRollAngle` | `LkAp`, declared length 22, value at absolute byte 17 | `SettingId::LateralTiltLimit` |
| `changeSafeMode` | `LdAp`, declared length 25, value at absolute byte 20 | `SettingId::LowBatteryMode` |
| `changeMaxSpeed` | `LdAp`, declared length 17, value at absolute byte 12 | `SettingId::TiltbackSpeed` |
| `changeVolume` | `LdAp`, declared length 28, value at absolute byte 23 | `SettingId::BeeperVolumePercent` |
| `changeTorchMode` | Modern branch uses the ASCII `SetLightON`/`SetLightOFF` commands; an older branch constructs model-gated binary frames | `SetLights` for the modern branch |
| `changeRidingLevel` | Selects an advanced `LdAp` frame (declared length 24) or a legacy binary frame based on the adapter's riding-mode flag | Family-level evidence only; Aero exposes numeric `PedalHardness`, not an additional `RidingPreset` |
| `changeLimitSpeed` | Stores the app's limit-speed preference and delegates to `changeMaxSpeed` only when the adapter's limit-mode flag is enabled; it is not a second wheel setting | `SettingId::TiltbackSpeed`; no separate limit-mode wheel write is inferred |
| `changeLimitMode` | Updates the app/device preference record and conditionally invokes `changeMaxSpeed`; no independent command builder is present | App policy, not a separate benign wheel command |
| `resetSingleMileage` | Protocol-v2 branch constructs a multi-write reset sequence; the recovered AOT does not expose a stable setting key or generic model gate | The established Aero-compatible reset command is the literal `CLEARMETER`; the un-gated multi-write legacy path remains outside the generic API |

The byte shapes, positions, and CRCs are covered by the existing Rust golden
frame tests. The older `changeTorchMode` branch is recorded as evidence but is
not emitted by the generic encoder: model/firmware selection is not an input to
that API, and sending an un-gated legacy companion frame can produce the extra
acknowledgement beep observed on Aero. This is static command-construction
evidence, not proof of a physical write or readback result.

The later audible live-test incident has no established offending command;
the earlier companion-frame observation does not identify its cause. The sound
ceased after a power cycle, which does not prove restoration of every setting.

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

## Aero settings missing from the current Tune catalogue

The Aero manual supplies two device controls that must stay distinct from
similarly named controls already in the generic catalog:

| Aero menu item or incomplete control | Current evidence | Required status |
| --- | --- | --- |
| `BRT` — display brightness | This is the source-backed `vn_display_backlight` setting, setter `J`, with a 0–100% domain and page-8 byte 55 readback. | Implemented and physically confirmed on NF2557; 0% turns the display off. It is not a hardware auto-headlight setting. |
| Headlight brightness cycle | The physical wheel has three levels; neither inspected app establishes its Aero Bluetooth command. A constrained live candidate `AA 55 14 02 16` was captured as an outbound FFE1 write after a verified NF2557/Aero connection, but produced no decoded acknowledgement and no distinguishable physical brightness effect. | Missing, deferred. The candidate result is inconclusive, not a valid command binding; do not conflate it with display BRT or direct lamp power. |
| `MxV` — maximum charge voltage | The manual identifies it as an Aero setting. EUC World's Veteran path explicitly maps `vn_charging_voltage_limit` to setter `G`, a Ld value field, and page-8 byte 64. Its generic UI accepts 147.0–151.6 V (default 151.2), writes requested decivolts minus 1450, and reads `145.0 + raw / 10`. | Real setting with a known generic transport frame, but its Aero unit conversion and safe range are unresolved: NF2557 raw 46 would display as 149.6 V for a 126 V pack. Keep it unavailable; do not render or write it until model-specific semantics are proven. |
| `ALM` — torque alarm | The manual identifies a distinct on/off torque alarm. No matching Veteran preference key or setter exists in EUC World 2.66.1. | Real manual-defined Aero setting with no EUC World Veteran mapping. Do not alias it to speed or brake-overpressure alarm. |
| `ANG TLT` — angle-tilt re-centre | The manual describes a percentage/time tuning control, distinct from vertical pedal angle and calibration. No matching Veteran preference key or setter exists in EUC World 2.66.1. | Real manual-defined Aero setting with no EUC World Veteran mapping. Do not present the gyro-calibration action as this setting. |
| `ANG` — vertical angle | Single `LkAp` write matches NOSFET/EUC World. Selector-0/4 signed bytes 67–68 report hundredths of a degree; the earlier claim of no source-backed readback was wrong. | Decode that observation and use exact matching completion. Writes remain 0.1° steps within −8.0°…8.0°; preserve 0.01° observations. Historical zero is not proof of the latest physical write. |
| `MD` / ride mode | NOSFET explicitly hides Hard/Medium/Soft when numeric hardness is available. NF2557 reports numeric hardness. | Expose only numeric `PedalHardness` for Aero. The separate family preset was a catalog error, not another Aero control awaiting confirmation. |
| `ALM` — speed alarm | Source-backed field and readback mapping exist. The Aero descriptor and session now reject 56 km/h before transport, without replacing an existing pending request; 55 km/h is the model maximum. | Valid 54–55 km/h device application/readback remains unproven. This is a live acceptance gap, not a reason to widen the model bound or relabel a submitted command as confirmed. |
| Brake-overpressure alarm | Source-backed field and readback mapping exist, but the live 120% request remained pending while the wheel showed 121%. | Fix the command/confirmation path; do not treat a submitted request as a changed wheel threshold. |
| Low-battery mode | Source-backed boolean field exists, but no live readback/physical effect was established. | Incomplete acceptance. Keep it distinct from high-speed mode. |
| Beeper volume | Numeric field readback works, but the intended audible wheel-volume behavior remains unresolved. | Determine whether the observed issue is a protocol effect, firmware behavior, or a distinct control before adding another volume setting. |
| Trip reset | `CLEARMETER` is the current Aero command candidate, but Tune does not expose a completed trip-value observation. | Incomplete completion evidence. Verify against a readable trip meter; the generic action remains conditional on that capability. |
| Horn | A momentary command exists, with no in-app completion or independently recorded physical effect in this pass. | Incomplete acceptance; do not add a persistent state for a momentary action. |
| Auto shutdown / charge mode | Decoded as passive Veteran fields but not listed as interactive Aero menu controls in the manual. | Diagnostics only until an Aero-specific writable command is established. |
| High-speed mode, transportation mode, calibration | Shared Veteran fields/actions are decodable, but the NF2557 Aero product scope excludes them from current Tune work. | Keep out of the Aero UI and live test matrix; retain wire research separately. |

`AL` light sensitivity was investigated as a possible Veteran-family hint, but is
not an Aero menu label or observed NF2557 setting. It is deliberately excluded
from this inventory: the published manual's duplicate `BRT` automatic-headlight
line and the official app's absence of an ambient-light command do not make a
valid Aero protocol binding.

## Wheel-write inventory

`pos` below is the absolute, zero-based value-byte position. `Ld` means `LdAp`
with bytes 5–6 = `01 02`; intervening bytes are `80`. Total frame size is
`pos + 5`, including the trailing four-byte CRC32. `Lk` setters use `LkAp` and
byte 5 = `01`. Aero lateral cutoff sends one 22-byte `LkAp` frame with its value
at position 17; neither inspected Android setter sends a companion frame.
EUC World's shared serializer can substitute `LdAp` under a model/firmware gate;
that alternative is not a second write and the Aero branch does not enable it.
Preserve protocol/firmware gates; see the primary-artifact comparison below.
That does **not** authorize replacing a field's header and discriminator while
keeping its offset: `LdAp` position 12 is tilt-back, not speed alarm, and
`LdAp` position 17 is transport mode, not lateral tilt.

Cutout's shared [protocol/dialect wire contract](control-wire-contract.md)
checks the declared Veteran wire destinations and its NOSFET dialect at compile time.
Duplicate bank/offset destinations fail compilation, including inherited fields;
the checked schema also supplies the emitted header, discriminator, and offset.
The speed-alarm/lateral-tilt regression checks both bank separation and the
resulting frames, and the session test checks the bytes scheduled for transport.
These checks prevent the discovered aliasing bug; they do not establish that
the remaining NF2557 physical-control failures have been resolved.
Nor do they establish complete typed semantic bindings, exact input conversion,
host submission or operation completion; those gaps are tracked in the
[settings design review](settings-design-review.md) and
[LIBCU-DOC-8](https://lific.mjc.lol/LIBCU/pages/30).

| EUC World key (`vn_` prefix omitted) | Setter | Values and frame | Readback / Cutout gap |
| --- | --- | --- | --- |
| `alarm_speed` | E | 10–200 km/h, Lk pos 12 | Header 24–25 deci-km/h; typed Cutout setting uses 10–200 |
| `speed_limit` | V | 10–200 km/h, Ld pos 12 | Header 26–27 and page-8 byte 52; typed Cutout setting uses 10–200 |
| `safety_margin_limit` | U | UI 0–70% → wire `100 − UI`, Ld pos 13; UI −1 = Off → wire 200 | Page-8 byte 53; Cutout exposes semantic duty and explicit `Disabled` writes |
| `display_mode` | K | 0/1, Ld pos 18 | Page-8 byte 58; typed wheel-units readback and write implemented |
| `display_backlight` / `BRT` | J | 0–100%, Ld pos 15 | Page-8 byte 55; typed mobile readback implemented and physically confirmed on NF2557; 0% is display off |
| `beeper_volume` | F | 0–100%, Ld pos 23 | Page-8 byte 63; typed mobile readback implemented; not the fixed PWM alarm |
| `dynamic_assist` | L | 0–100%, Ld pos 26 | Page-8 byte 66; typed mobile readback implemented; generic On/Off assist is not this control |
| `pedal_dip_compensation` | Q | 0–100%, Ld pos 28 | Page-8 byte 68; typed mobile readback implemented |
| `pedals_sensitivity` | R | 0–100%, Ld pos 10 | Page-8 byte 50; Cutout MD write and readback implemented |
| `pedals_tilt` | S | Signed −80…80 tenths of a degree, `LkAp` pos 11 for Aero | Selector-0/4 signed i16 at bytes 67–68, in hundredths. Cutout canonical values preserve hundredths and reject writes not divisible by ten. |
| `lateral_tilt_limit` | O | 35–75 degrees, one 22-byte `LkAp` frame at pos 17, byte 5 `01`, bytes 6–16 `80` | Selector-2 byte 47 readback is decoded; typed mobile readback implemented; the unsupported companion was removed, but NF2557 physical write confirmation remains open |
| `high_speed_mode` | N | 0/1, Ld pos 21 | Page-8 byte 61; typed encoder/readback/UI implemented on the settings branch; physical write proof pending |
| `low_battery_mode` | P | 0/1, Ld pos 20 | Shared page-8 byte 60 and wire command retained. Aero manual specifies Always ON, so the Aero semantic catalog rejects this toggle; a beep is not evidence of applicability. |
| `transportation_mode` | W | 0/1, Ld pos 17 | Page-8 byte 57; typed encoder/readback/UI implemented on the settings branch; physical write proof pending |
| `charging_voltage_limit` / `MxV` | G | EUC World accepts 147.0–151.6 V in 0.1 V units (default 151.2); its wire value is requested decivolts minus 1450 at Ld pos 24 | Page-8 byte 64 is read as `145.0 + raw / 10`. The generic Veteran mapping is **not Aero-safe**: NF2557 raw 46 would display as 149.6 V despite its 126 V pack. Keep it out of Tune and do not write it until a model-specific conversion is established. |
| `voltage_correction` | X | Signed −15…15, displayed in tenths of a percent, Ld pos 19 | Page-8 byte 59 signed; typed mobile readback implemented; not a volts offset |
| `headlight_mode` | M | Off/On; modern `SetLightON`/`SetLightOFF` literals, with an older model-gated binary branch | Cutout's canonical Aero Headlight power control follows the modern literal commands in both detected Veteran command modes. EUC World's inspected path does not establish the AERO's separate physical brightness-cycle command, so no intensity setting is inferred |
| `riding_mode` | T | UI hard/medium/soft → binary 3/2/1 at pos 7, or legacy ASCII | Alternative to numeric MD where supported, not an independent Aero control; excluded from Aero's semantic catalog |

The speed bounds above are in km/h; Cutout's canonical request quantities use
deci-km/h. A whole-km/h write must reject inexact canonical inputs rather than
truncate them. The live report establishes that the user's “34.8” was mph,
approximately 56 km/h and above Aero's 55 km/h limit. A negative test for
canonical 348 alone does not cover that displayed-value path. The later 54 km/h
probe was valid but remained pending; see LIBCU-DOC-9. Voltage correction's
signed −15…15 domain is in tenths of
a percent, meaning −1.5…1.5%, not ±15%. These source-derived domains are library
declarations, not negotiated device bounds or factory defaults.

Remaining menu keys are accounted for separately, not invented as new wheel writes:

| Key | Classification / remaining trace |
| --- | --- |
| `vn_headlight_persistent` | App persistence preference; not another physical lighting mode |
| `reset_user_distance` | Reset action through the app confirmation handler; the generic trip-reset action is attempted for any live device profile that exposes a trip meter, with the current Aero encoding as the established `CLEARMETER` command |
| `vn_download_event_log` | Log retrieval, not a settings mutation |
| `vn_safety_margin_scaling` | App-side correction setting; do not confuse with the PWT wheel command |
| `vn_firmware_list` | Firmware listing/update entry point; not authorized by ordinary settings writes |
| `vn_firmware_high_transfer_rate` | Firmware-transfer preference; separate from this settings slice |

## Applicability and readback findings

### Lateral correction and send-lifecycle comparison (2026-09-20)

Primary decoded roots for this pass are
`/private/tmp/eucworld-re-6y3lc2/decoded` (EUC World 2.66.1) and
`/private/tmp/nosfet-re-qOyQn0/decoded` (NOSFET 1.1.3), with versions in each
`apktool.yml`, lines 9–11. Line numbers below are physical file lines, not the
embedded smali `.line` directives. `E` denotes the first root's `smali` directory;
`N` denotes the second root's `smali/com/laoniao/leaperkim` directory.

The paired representation originated in commit
`3721a62faa08a61a5122bdf6cae15d6a2ad2d967` (`fix: send paired lateral tilt command`).
That commit introduced the companion golden bytes and the paired-source claim
together, without adding a capture; its coverage note explicitly left paired
physical confirmation open. Inspection of that diff, subsequent history and
checked-in artifacts found no independent successful paired capture superseding
the reference apps. The earlier single-write attempt remaining at 40 degrees
does not prove that a companion is required.

- `E/uh/g0.smali:9898–9970`, setter `O`, creates one payload and calls `H` once
  at line 9963. `N/setting/SetFallProtectionAngleActivity.smali:59–164` creates
  one `LkAp` frame and calls `sendBytesData` once at line 162. Both put degrees
  at byte 17, declare length 22 and use `80` padding after byte 5 `01`.
  Cutout now emits that single frame, not `LkAp` plus `LdAp/01 00`.
- `E/uh/g0.smali:8877–9085`, helper `H`, selects `LdAp` when `K0 > 0`, adds
  total length and appends big-endian CRC32. Its CRC loop is at lines 1054–1125.
  The Aero model branch, lines 12325–12597, does not set `K0`; assignments occur
  in other model/firmware branches at lines 12886, 13150, 13438 and 13730.
  For lateral `O`, header substitution alone retains byte 6 `80`; the explicit
  byte 6 `00` alternative belongs to pedal-angle setter `S`, lines 10197–10287.
- Pedal angle is one 16-byte frame: `E/uh/g0.smali:10197–10287` and
  `N/setting/SetAngelActivity.smali:78–159`. Display brightness is one 20-byte
  `LdAp/01 02` frame: `E/uh/g0.smali:9390–9464` and
  `N/setting/control/ScreenBacklightSettingActivity.smali:111–206`. These match
  Cutout's headers, offsets and CRC. NOSFET's CRC append and Java CRC32
  implementation are `N/utils/Util.smali:86–118` and `292–388`.

Admission checks must be distinguished from command pacing and device acceptance:

| Stage | EUC World | NOSFET | Cutout comparison |
| --- | --- | --- | --- |
| Admission | Angle/brightness require binary-settings flag `o0` and valid bounds; UI additionally requires a decoded value (`E/mi/p.smali:394–478`, `926–1011`). | `N/utils/BtManager.smali:5088–5112` returns before CRC/enqueue if `receivingData` is false or the service is absent. The flag is refreshed by inbound packet processing at line 901 and expires after 1500 ms (`538–556`; `N/utils/BtManager$1.smali:35–53`). | Verified connection, model/value validation and fresh stationary speed evidence are semantic admission checks, not transport delays. |
| Buffering | Veteran sets bypass flag `p=false` (`E/uh/g0.smali:676`). `E/uh/h0.smali:2089–2358` holds one active frame and rejects an overlapping write. | `N/service/BluetoothLeService.smali:1314–1343` appends to a byte FIFO and immediately calls `writeData`; `N/utils/DataQueue.smali:52–145` does not retain command boundaries. | Native operation queue waits for CoreBluetooth capacity; queued status must remain distinct from native submission. |
| Before native API | `E/si/a.smali:2567–2641` looks up FFE0/FFE1 and submits the chunk. | `N/service/BluetoothLeService.smali:1392–1434` checks GATT, `gatt.connect()` and characteristic, dequeues at most 20 bytes, then calls `writeCharacteristic`. It ignores the returned boolean and does not recheck inbound freshness at dequeue. | Validate queued work when it reaches the native API; an earlier admission or outbound log is not submission evidence. |
| Chunks and timing | `E/uh/h0.smali:2373–2535` caps chunks at 20 bytes; success callbacks advance them (`1593–1767`). Its 2000 ms timer is a callback timeout, not inter-command pacing. | Success callbacks drain the next chunk (`N/service/BluetoothLeService$1.smali:116–139`), but each enqueue also starts a drain without an explicit busy guard. | ANG and BRT fit one reference chunk; lateral requires 20+2 bytes. Cutout uses the host write limit. No reference establishes an angle/BRT-specific delay. |
| Failure/retry | Timeout clears the active write and reports an error (`E/f2/d0.smali:57–85`). Settings use null firmware context; `E/yh/c.smali:312–452` does not retry that context. | Failure callbacks log without requeueing the removed bytes (`N/service/BluetoothLeService$1.smali:132–139`). | Neither reference supplies an automatic angle/BRT retry policy or turns a native callback into matching setting readback. |

The parent investigation reports that `jq` inspection of the latest brightness
capture's `LinkUp` event gives `link_max_write_len = 20`, and that Cutout's planner
uses that value. For this current NF2557 connection, the active chunk limit
therefore matches both Android apps: angle is one 16-byte write, brightness one
20-byte write, and lateral one 22-byte protocol frame split into 20+2-byte writes.
This is parent-provided capture evidence, not an independently replayed capture
in this pass; chunk-size parity does not establish write-mode or device-acceptance
parity.

For short writes EUC passes mode 0, retaining the characteristic write type;
NOSFET likewise does not call `setWriteType` in this path. This is not evidence
of mandatory with-response writes: Android initializes the type from the
no-response property ([AOSP Android 12 initializer, lines 252–266](https://github.com/aosp-mirror/platform_frameworks_base/blob/android-12.0.0_r1/core/java/android/bluetooth/BluetoothGattCharacteristic.java#L252)).
EUC explicitly selects type 2 for nonfinal chunks longer than 20 bytes
(`E/uh/h0.smali:2413–2435`); a later mode 0 does not reset it. Cutout explicitly
uses without-response writes. EUC's FFE1 call also falls through to a conditional
FFE5/FFE9 lookup (`E/si/a.smali:2641–2835`); its 20 ms sleep belongs to that
secondary-service loop, not a universal Veteran delay.

Both references perform separate startup clock synchronization
(`E/uh/g0.smali:17618–17645`, builder `9088–9288`;
`N/utils/BtManager.smali:4430–4457`, builder `N/utils/Util.smali:805–946`). Neither
setting waits for a clock-sync acknowledgement. This does not establish an
unlock prerequisite. DarknessBot remains frame-level corroboration from the
earlier inventory above; its generated AOT transport assembly was not located in
this pass. No transport-parity or physical-success claim is inferred from it.

### Corrected Aero angle and riding-mode evidence

Rechecked the existing EUC World 2.66.1 and official NOSFET Android 1.1.3
artifacts after the `702a83423` phone retest. The durable source trails are:

- NOSFET `ControlActivity.initControlData`, smali lines 95–138: a non-`0x80`
  `getPedalHardness()` displays numeric hardness and sets `layoutSetRideMode`
  to GONE. The fallback does the opposite. Do not make temporary missing
  settings pages switch an already identified Aero into the legacy preset UI.
- NOSFET `SetAngelActivity.sendCmd`, lines 51–157: one `LkAp` frame, total
  length 16, byte 5 `01`, bytes 6–10 `80`, signed angle tenths at byte 11;
  `BtManager.sendBytesData` appends CRC32. This matches the existing encoder.
- EUC World `uh/g0.S`, lines 10197–10287: its alternate `LdAp`/`01 00` angle
  form is gated by `K0 > 0`. The Aero detection branch does not set that gate;
  those assignments belong to other models/firmware. No Aero companion is inferred.
- EUC World `uh/g0.h`, selector dispatch 17777–17787 and decode 16991–17212:
  selectors 0 and 4 share the angle field; at least 69 pre-CRC bytes are
  required; signed big-endian i16 at 67–68 is divided by 100.
- NOSFET `BtManager`, lines 3608–3647, independently reads the same `lrAngle`;
  `SetAngelActivity`, lines 243–270, initializes its setting from that value.
- The existing `nf2557-2026-06-21-powered-on-long.hex` fixture contains 23
  CRC-valid selector-0/4 angle observations, all zero. These are historical
  baseline readings, not a before/after write acceptance test.

The library now uses matching readback for ANG and preserves hundredths in its
semantic value domain. Requests remain exact tenth-degree steps; neither
rounding a nearby observation nor publishing the requested value as observed
can complete a request. The CLI's existing integer-tenths input is converted
exactly into that canonical domain. Low-battery mode is also excluded for Aero:
the manual specifies it as Always ON. Shared command availability is not model
applicability.

The menu is shared across Veteran/NOSFET devices. Most controls are enabled only
when binary settings are available **and their corresponding decoded value is
present**. A menu entry alone does not prove an Aero supports that command.
Page-8 byte `0x80` means unavailable. The existing NF2557 capture contains
**100 at byte 66 and 0 at byte 68**, not unavailable sentinels. EUC World's mapping
therefore supplies a concrete lead for numeric assist/dip controls on this Aero;
it still does not establish the result of sending a new value.

The recent Mac inventory sampled descriptors before settings telemetry and
stopped at ride readiness. Its empty current values cannot supersede this
capture evidence or establish unavailable readback. Observation acquisition must
cover the relevant page cycle and identify which fields were actually received.
Do not remove an observation mapping merely because a new write was unconfirmed.

The 2026-06-22 NF2557 page-8 capture reports MD 50% and raw PWT 79%. EUC World's
PWT control displays unused PWM margin, so this raw threshold corresponds to a
21% margin. Its legal input is 0..70%, encoded as `100 - margin`; Off is a
distinct state encoded as 200. Cutout now decodes
those into the existing Rust-owned setting trackers; the mobile integration test
replays captured bytes and checks current, pending and matching-readback states.
CRC bytes, invalid percentages, other pages and other protocol models cannot
populate these fields. Mapping provenance remains SourceVerified, and write
capabilities remain Unverified until physical write behavior is established.

The same capture reports page-8 byte 64 as raw `46`. EUC World's generic
Veteran path labels that as a 145.0–151.6 V charging limit and explicitly
encodes a requested decivolt value minus 1450. That conversion is incompatible
with the identified 126 V Aero pack. The transport evidence is useful, but it
is not permission to present or write MxV for Aero: the production mobile
surface omits it until an Aero-specific conversion and safe range are
physically established.

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
NOSFET brake-overpressure alarm (90..125%). EUC World's shared Veteran menu
uses 10..200 km/h speed domains; that is wire-format evidence, not a safe
Aero-domain claim. Aero's product profile must impose its own 55 km/h ceiling.
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
The current Aero trip reset uses the separately established `CLEARMETER` literal;
other devices should add their own checked dialect encoding when their trip-meter
capability is exposed. The recovered multi-write path is not substituted without
a model/firmware gate.
Unknown manufacturer-menu items stay open.
