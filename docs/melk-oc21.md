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


Power, solid RGB, and brightness were physically confirmed before this extension.
On 2026-09-06 the user reported that the first ten pattern entries and the four
offered shortcuts (IDs 1, 16, 22, 75) visibly worked. The exact ID boundaries of
the ten entries were not recorded. Native speed 0 was the fastest observed, but
still substantially slower than the official app. Music could not be tested;
music and schedules remain physically unverified. Tests prove frame encoding and
software behavior, not those remaining hardware capabilities.
The upstream catalog contains generic MELK and model-specific mappings; exposing
pattern IDs 0–220 does not prove that this firmware implements every visual mode.

| Control | Parameters | Nine-byte frame (hex) |
| --- | --- | --- |
| Pattern | ID 0–220 | `7e 05 03 ID 06 ff ff 00 ef` |
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

The UI uses wire IDs 1–212 from the [MELK OA21 reference catalog](https://gist.github.com/clienthax/5b3cc5fa68f7c4c943f2252eaa21d804).
These are reference names, not a claim that OC21 renders every mode identically.
The catalog contains duplicate names at different wire IDs; those IDs are
preserved, not collapsed by parsing names as unique dictionary keys. The UI gives
ID 0 and 213–220 generic STRIPX reference labels marked “(reference)” so every
wire ID has a human-readable name; these entries remain unverified and disabled
until exact OC21 capture.

The production picker groups the reference IDs into Basic, Curtain, Trans, Water,
Flow, Tail, Run, Run Back, and Unmapped. The grouping covers each ID 0–220 exactly
once, while the names remain reference data pending exact OC21 matching. The
user's official-app screenshot corroborates Basic/Curtain/Trans/Water and the
Magic Forward, Magic Back, and 7-Color Energy names, but it is disconnected and
contains no wire IDs. An official-app list from this controller is needed to
finish name matching.

Moving the speed slider right sends a lower native byte: 100% sends 0 and 0%
sends 255. It spans the same full byte range as the reference. Changing speed
sends only the speed frame, without reselecting the pattern or changing power.
Complete effect restoration selects the pattern before applying speed. This
removes a possible animation reset; it does not yet establish speed parity with
the official app.

## Production behavior

- Lighting is accessible from Ride or directly from the device picker. Its
  independent central remains separate from EUC/VESC telemetry.
- MELK readiness is gated on both the FFF4 notification subscription and the
  documented two-frame initialization handshake. Reconnect and failure paths
  cancel any delayed second frame before clearing the session.
- Color drag retains the existing 30 Hz preview path; queued superseded solid-color frames are coalesced so the newest drag value is preserved under BLE backpressure. Complete playback changes
  are validated in Rust and admitted together to a bounded Bluetooth write queue.
  CoreBluetooth backpressure pauses draining; disconnect discards pending writes.
- Effects and controller-microphone music modes are saved with named presets.
  Schema version 2 reads version 1 records as solid RGB.
- Optional reconnect restore uses the last confirmed settings for the same
  accessory identity. Requested state is not a claim of physical confirmation.
- Manual clock scheduling is exposed as two controller-local on/off slots with
  weekday repetition and local-hour/minute controls. Saving sends a clock sync
  followed by the selected slot; opening the editor never writes. The controller
  has no timer readback, so the editor retains drafts rather than claiming the
  current device schedule.
- Music uses the accessory microphone, with no phone recording or audio permission.
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
