# Protocol Source Audit

Date: 2026-06-21

This note records public protocol sources checked before replacing provisional
Aero/Begode request bytes. It is intentionally provenance-focused; implementation
must still be test-driven and verified against live Bluetooth captures before a
device is considered fully specced.

## Sources Checked

- `eried/eucplanet`
  - Clone used for audit: `.protocol-references/eucplanet`.
  - License: MIT.
  - Relevant files:
    - `docs/protocols/veteran.md`
    - `docs/protocols/begode.md`
    - `app/src/main/java/com/eried/eucplanet/ble/VeteranModel.kt`
    - `app/src/main/java/com/eried/eucplanet/ble/VeteranCommands.kt`
    - `app/src/main/java/com/eried/eucplanet/ble/BegodeCommands.kt`
  - Status: best source for clean-room implementation planning because the
    docs explicitly describe themselves as original prose and MIT-licensed.

- `Wheellog/Wheellog.Android`
  - Clone used for audit: `.protocol-references/Wheellog.Android`.
  - License: GPL-3.0.
  - Relevant files:
    - `app/src/main/java/com/cooper/wheellog/utils/VeteranAdapter.java`
    - `app/src/main/java/com/cooper/wheellog/utils/GotwayAdapter.java`
    - `app/src/test/java/com/cooper/wheellog/utils/VeteranAdapterTest.kt`
    - `app/src/test/java/com/cooper/wheellog/utils/GotwayAdapterTest.kt`
  - Status: useful corroborating evidence and fixture ideas, but do not copy
    implementation code into this repo.

- `Pickelhaupt/EUC-Dash-ESP32`
  - Clone used for audit: `.protocol-references/EUC-Dash-ESP32`.
  - License: GPL-3.0.
  - Relevant files:
    - `src/hardware/Gotway.cpp`
    - `src/hardware/blectl.h`
  - Status: older background only. It explicitly derives Gotway decoding from
    WheelLog, so it adds little for a clean-room Rust implementation.

## Findings

- NOSFET Aero should be treated as a Veteran/LeaperKim protocol-family wheel,
  not as a distinct first protocol. EUC Planet documents that NOSFET Apex,
  Aero, and Aeon use the Veteran wire format, and maps Aero to model id `43`.
- The live NF2557 capture already matches this: notifications start with
  `dc5a5c`, the documented Veteran/LeaperKim magic.
- Aero uses the generic HM-10-style GATT profile already observed locally:
  service `0000ffe0-0000-1000-8000-00805f9b34fb` and characteristic
  `0000ffe1-0000-1000-8000-00805f9b34fb` for both notifications and writes.
- Brand cannot be inferred from GATT alone. The initial notification bytes
  should classify protocol family: `dc5a5c` for Veteran/LeaperKim/NOSFET,
  `55aa` for Begode/Gotway.
- Veteran/Aero telemetry frames are length-prefixed after the `dc5a5c` magic.
  A reassembled frame is `len + 4` bytes. Short frames have no CRC; longer
  smart-BMS frames include a big-endian CRC32 trailer.
- NOSFET Aero is documented as a 30-cell / 134 V Patton-family model with
  model id `43`. Its battery range is documented as raw centivolts
  `9918..12337` for 0..100%.
- Veteran/NOSFET smart-BMS long frames use the byte at absolute offset `46` as
  the BMS page selector. Selectors `0` and `4` are pack metadata; selectors
  `1`/`5` and `2`/`6` are 15-value cell-voltage pages starting at absolute
  offset `53`; selectors `3`/`7` carry six signed temperature values starting
  at absolute offset `47`; selector `8` is currently reserved/newer and must
  stay raw until live behavior proves a stable meaning.
- For metadata pages `0` and `4`, the documented BMS pack-current fields live
  at absolute offsets `69` and `71` and are signed big-endian centiamps. For
  Aero's 30-cell layout, page `3`/`7` should not expose cells beyond index 29;
  those pages are temperature/status evidence until typed core support exists.
- Battery chemistry should be represented separately from Veteran series-cell
  count. NOSFET Aero is confirmed as a Samsung 50S 30s2p pack. Public retailer
  specs also identify Samsung 50S packs for Veteran Patton/Patton S, Lynx/Lynx S,
  Sherman L, Oryx, NOSFET Apex, and NOSFET Aeon. The protocol model should
  therefore keep `cell_count` as electrical series count while using the shared
  Samsung 50S single-cell curve for those models. Older Sherman/Abrams/Sherman S
  should stay on documented linear ranges until cell-profile evidence is added.
  NOSFET Aeon is a smaller 151 V-class Samsung 50S pack at about 1300 Wh, so it
  uses the same 36-series voltage curve as Apex/Lynx but 2p pack capacity.
- Begode/Falcon remains a separate family: same GATT UUIDs, but `55aa` identifies
  the fixed 24-byte Begode/Gotway frame family. A valid complete frame has the
  `55aa` header, tag byte at offset `18`, sub-index at offset `19`, and
  `5a5a5a5a` terminator at offsets `20..23`. Do not overfit classification to
  sample payload bytes after the `55aa` magic.
- Begode/Gotway tag `0x00` is Live A telemetry: raw voltage at offset `2`,
  speed at `4`, trip-distance candidates at `6`/`8`, phase current at `10`,
  IMU temperature at `12`, and raw hardware PWM at `14`. For Falcon, user
  confirmation puts the current target hardware at 84 V. Public retailer specs
  for 100.8 V / 900 Wh Falcons are variant evidence to track separately; do not
  make that class the default Falcon profile without matching Bluetooth or
  app-visible evidence for the specific device under test.
- Begode/Gotway tag `0x04` is Live B telemetry/settings: total distance,
  settings bits, power-off timer, tiltback/max speed, LED mode, alert flags,
  and light mode. Tag `0x07` is extra telemetry for true battery current, motor
  temperature, and true PWM. Smart-BMS tags `0x01`/`0x02`/`0x03` are separate
  parser work because `0x01` can provide authoritative pack voltage and BMS
  current while `0x02`/`0x03` carry cell pages.
- Begode/Gotway ASCII banners share the same FFE1 notify pipe as binary frames
  and must be detected before binary reassembly. Source behavior shows `V`
  query replies with firmware prefixes `GW`, `JN`, `CF`, or `BF`; `N` query
  replies with `NAME=...` or `NAME:...`; and `MPU...` banners are IMU evidence
  for temperature conversion. `CF` and `BF` firmware imply the frame `0x00`
  hardware-PWM field is authoritative; stock `GW` and `JN` do not.

## Aero settings terminology

The [NOSFET support page](https://www.nosfet.com/support) links the AERO
manual, whose setting names are the protocol-facing labels used here:

- `TLT`: tilt-back speed.
- `PWT`: PWM tilt-back alarm percentage.
- `ALM`: alarm speed.
- `ANG`: vertical angle adjustment.

The manual also lists `ANG %` (acceleration assist) and `ANG TLT` (gyro
re-centering duration). Those are separate settings and must not be inferred
from the `ANG` write. The Rust `AeroAngleAdjustment` type therefore describes
`ANG` as a vertical-angle value; it does not claim to decode pedal mode.

On the live NF2557/Aero capture (firmware 43.2.54), the repeated settings
readback currently proves raw fields `0x0018=550`, `0x001a=540`, and
`0x001e=1920`. The first two are the observed alarm and tilt-back values;
`0x001e` remains raw until a controlled capture proves its meaning. A
reversible `TLT` write has completed a 54 -> 53 -> 54 readback round trip;
an `ALM` write that did not produce a matching readback is reported as failed
closed.

## Backlog Implications

- The next Aero implementation slice should parse captured notifications before
  inventing read request bytes. The device is already streaming telemetry
  unsolicited once subscribed.
- Add a protocol classifier/reassembler for Veteran/Aero and Begode magic bytes.
- Add an Aero telemetry fixture from the existing live capture and prove the
  parser decodes at least family, frame length, model id, voltage, firmware
  version, and basic telemetry fields.
- Keep any setting without a typed readback and controlled device evidence
  explicitly provisional; do not report a write as successful merely because
  the BLE transport accepted the frame.
- GPL repositories can be used as evidence and for behavioral cross-checks, but
  Rust code should be written from MIT docs, live captures, and our own tests.
