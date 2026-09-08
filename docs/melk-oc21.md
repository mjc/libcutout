# MELK-OC21 lighting

The MELK-OC21 is the user's RGB controller installed on their Aero. It is a
separate BLE peripheral from the wheel's telemetry connection. The historical
official app name, LotusLamp X, is provenance, not a discovery or protocol key.

## Protocol source and evidence

Commands are based on [dave-code-ruiz/elkbledom](https://github.com/dave-code-ruiz/elkbledom/tree/f41b8a27838f5b76f2b8b2c628b61cd700fe5fc3),
specifically `models.json`, `definitions.json`, `model.py`, and `elkbledom.py`.
The user's nRF Connect inventory establishes FFF0 service, FFF3 read/write
without response, and FFF4 notify. The advertisement does not include FFF0:
scan without a service filter, then check identity and discovered GATT roles.
The same upstream integration documents a MELK initialization handshake: send
`7e 07 83`, wait one second, then send `7e 04 04`. Production performs this
one-time sequence after FFF3 discovery and before admitting user commands; it is
initialization, not command confirmation.
At the pinned upstream commit, `models.json` has no `MELK-OC21` entry. Its closest `MELK-OA21` entry selects the `EFFECTS_STRIPX` catalog (228 symbolic entries), while the separate OA21 reference list used by the UI supplies verified wire names only for IDs 1–212. The shared effect frame shape is useful protocol evidence, but it is not enough to assign the remaining names or claim those modes on OC21.
The same source also has a generic `MELK` profile with a smaller, different
effect-name mapping and the controller-microphone frames. We retain those
frames as a future capability path, but do not transplant its names into the
OC21 catalog while the model-specific mapping is absent.
An independent [LotusLamp X reverse-engineering project](https://l0lsec.github.io/LotusLampX/)
corroborates the FFF0/FFF3/FFF4 transport and documents scenes, microphone
reactivity, and timers for its tested MELK-OA10/SYMPHONY profile. Its basic
frame templates are not identical to the OC21 captures, so this is family-level
corroboration only, not OC21 capability proof.
The [Lotus Lamp X manual for a MELK-OC21 product](https://manuals.plus/yisibo/yisibo-rbr-024w-rainbow-table-lamp-user-manual)
also shows the app's color, mode, music-rhythm, scene, and timing surfaces for
that model family. It confirms product-level feature intent, but it does not
publish OC21 command bytes or replace a capture-backed write check.


Power, solid RGB, and brightness were physically confirmed before this extension.
On 2026-09-06 the user reported that the first ten pattern entries and the four
offered shortcuts (IDs 1, 16, 22, 75) visibly worked. The exact ID boundaries of
the ten entries were not recorded. Native speed 0 was the fastest observed, but
still substantially slower than the official app. Music could not be tested;
music and schedules remain physically unverified. Tests prove frame encoding and
software behavior, not those remaining hardware capabilities.
The upstream catalog contains generic MELK and model-specific mappings; exposing
pattern IDs 0–227 does not prove that this firmware implements every visual mode.

| Control | Parameters | Nine-byte frame (hex) |
| --- | --- | --- |
| Pattern | ID 0–227 | `7e 05 03 ID 06 ff ff 00 ef` |
| Speed | Native 0–255 | `7e 04 02 SPEED ff ff ff 00 ef` |
| Microphone effect | Index 0–7 | `7e 05 03 (80+INDEX) 04 ff ff 00 ef` |
| Microphone sensitivity | 0–100 | `7e 04 06 GAIN ff ff ff 00 ef` |
| Microphone enabled | 0 or 1 | `7e 04 07 ENABLED ff ff ff 00 ef` |
| Timer | On slot 0, off slot 1 | `7e 00 82 HH MM 00 SLOT FLAGS ef` |
| Clock | ISO weekday Monday=1 | `7e 00 83 HH MM SS WEEKDAY 00 ef` |

Timer flags contain Monday through Sunday in bits 0–6 and enabled in bit 7.
No repeating days means a one-shot timer. Saving sends current local clock time
before the selected slot. Merely opening the editor never writes a timer. There
is no implemented timer readback: the editor presents drafts, not controller state.

## Pattern names and speed

The UI displays wire IDs 1–212 from the [MELK OA21 reference catalog](https://gist.github.com/clienthax/5b3cc5fa68f7c4c943f2252eaa21d804), but the current MELK-OC21 profile enables only capture-backed IDs 1, 16, 22, and 75.
These are reference names, not a claim that OC21 renders every mode identically.
The catalog contains duplicate names at different wire IDs; those IDs are
preserved, not collapsed by parsing names as unique dictionary keys. The UI gives
ID 0 and 213–227 generic STRIPX reference labels marked “(reference)” so every
wire ID has a human-readable name; these entries remain unverified and disabled
until exact OC21 capture.
The official-app screenshot labels its first Basic card “Auto Play”; the UI
preserves that as an ID 0 reference label until an OC21 capture ties the name to
a wire ID.
The mobile FFI restore and preset boundary permits only capture-backed effect IDs 1, 16, 22, and 75; all other reference IDs, controller-microphone playback, and schedules fail closed as unavailable.

The production picker groups the reference IDs into Basic, Curtain, Trans, Water,
Flow, Tail, Run, Run Back, and Unmapped. The grouping covers each ID 0–227 exactly
once, while the names remain reference data pending exact OC21 matching. The
user's official-app screenshot corroborates Basic/Curtain/Trans/Water and the
Magic Forward, Magic Back, and 7-Color Energy names, but it is disconnected and
contains no wire IDs. An official-app list from this controller is needed to
finish name matching.
The effect cards include deterministic LED-dot previews derived from each
pattern's reference group and ID. These thumbnails are illustrative UI
affordances, not captures of OC21 output; only the documented wire names and
hardware observations establish controller behavior.
The eight controller-microphone choices use the pinned catalog's names: Music Flow Flash, Music Flash, Music Rainbow, Music Snake, Music Rainbow 2, Music Pulse, Music Flow, and Music Pulse 2. They are displayed with a “(reference)” suffix because the OC21 has not yet been physically checked for each mode; the protocol's microphone indices remain 0–7.


Moving the speed slider right sends a lower native byte: 100% sends 0 and 0%
sends 255. It spans the same full byte range as the reference. Changing speed
sends only the speed frame, without reselecting the pattern or changing power.
Complete effect restoration selects the pattern before applying speed. This
removes a possible animation reset; it does not yet establish speed parity with
the official app.

## Production behavior

- Lighting is accessible from Ride or directly from the device picker. Its
  independent central remains separate from EUC/VESC telemetry.
  Schedules are presented as their own Lighting page alongside Color, Effects, and Music.
- MELK readiness is gated on both the FFF4 notification subscription and the
  documented two-frame initialization handshake. Reconnect and failure paths
  cancel any delayed second frame before clearing the session.
- When CoreBluetooth already has a connected MELK peripheral, the session
  recovers it by the FFF0 service and the same MELK name gate before starting a
  broad advertisement scan. This covers restored/system-managed links without
  treating an arbitrary FFF0 device as the controller.
- Color drag retains the existing 30 Hz preview path; queued superseded solid-color frames are coalesced so the newest drag value is preserved under BLE backpressure. Complete playback changes
  are validated in Rust and admitted together to a bounded Bluetooth write queue.
  CoreBluetooth backpressure pauses draining; disconnect discards pending writes.
- App-local scenes support the complete save, replace-from-current-state, and delete lifecycle through the versioned Rust-backed record. Effects are saved with named presets only when their IDs are capture-backed (currently 1, 16, 22, and 75). Controller-native named scenes are not claimed; controller-microphone music and schedules remain visible as future design surfaces but are disabled until physical verification.
  Schema version 2 reads version 1 records as solid RGB.
- Optional reconnect restore uses the last confirmed settings for the same
  accessory identity. Requested state is not a claim of physical confirmation.
- Manual clock scheduling is retained as a future controller-local design surface; this MELK-OC21 profile does not send schedule writes until physical verification. The editor shows the protocol shape without claiming device state.
- The Mac-only MelkLightingLiveValidator executable uses CoreBluetooth to discover MELK-OC21, verify FFF0/FFF3/FFF4, and exercise capture-backed commands without sharing the ride telemetry connection. Run it with `nix develop -c swift run --package-path swift/CutoutMobile MelkLightingLiveValidator [timeout-seconds] [platform-UUID]`; the optional UUID retries a previously observed CoreBluetooth identity and is parsed fail-closed.
  A timeout or failed remembered-identity check exits nonzero.
- When scheduling is enabled in a future profile, saving will send a clock sync followed by the selected slot; opening the editor never writes. The controller has no timer readback, so future profiles must retain drafts rather than claim device state.
- A future capture-backed profile may use the accessory microphone for music modes; this MELK-OC21 profile keeps music unavailable, with no phone recording or audio permission.
- Unknown zone, pixel-count, calibration, and status-query commands are not sent.
  The reference does not establish these capabilities for this exact controller.

## Physical checks still required for the extension

Try several pattern IDs and both ends of speed, all eight microphone effects and
sensitivity endpoints, and transitions back to solid color. Save an effect and
a music preset, reconnect, and check the opted-in restore and final power state.
Test on/off timers shortly ahead of local time, disable both slots afterward, and
check repeat-day behavior separately. Confirm lighting interactions do not
interrupt the wheel telemetry connection. Automated payload tests do not replace
these observations.

On 2026-09-07, the Mac validator confirmed that CoreBluetooth was powered on
and scanning, but observed neither a connected MELK link nor a `MELK-OC21`
advertisement during 20-second, 30-second, 45-second, 60-second, 90-second,
120-second, and 180-second runs. A ten-minute validator run also ended with
`validation=timeout` and `state=disconnected`. An unrestricted 20-second
CoreBluetooth scan also found no MELK advertisement. The upstream LotusLampX
`bleak` CoreBluetooth backend independently found 22 nearby devices in a
ten-second scan and also found no MELK device. This is a discovery-state
observation, not evidence that the controller is unsupported.

The validator advertisement trace also observed unrelated nearby names (including Govee, GAFVent, `uac088`, and iPhone) and repeated RSSI updates during that run. This confirms the Mac callback path is active while the exact MELK controller remains absent from the radio environment.
