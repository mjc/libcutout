# Settings simulator

`cutout-dev simulator aero-settings` runs the deterministic NOSFET Aero
settings simulator without Bluetooth hardware:

```text
nix develop -c cargo run -p cutout-dev -- simulator aero-settings
```

The simulator is Rust-owned and wraps the production
`StationarySettingsWriteSession<NosfetAeroModel, false>`. It therefore uses
the same protocol identity, stationary/500 mm/s safety gate, encoders, write
channel, write mode, bounded payloads, and production-shaped settings-readback
output conversion as the live session. Its simulated readback event is emitted
only after the complete transport write sequence is emitted.

The simulator's private typed snapshot records every encoded setting, including
PWT, ANG, high beam, headlight, and trip-reset count. The production-shaped
settings event intentionally contains only fields the live Veteran readback
currently exposes (TLT, ALM, and pedal mode, plus the existing garage
projection). The private snapshot is therefore encoder/state evidence, not a
claim that the live protocol can confirm those other settings.

The command is software evidence only. Its payloads and readback prove that
the library path is deterministic and write-safe; they do not prove a wheel's
physical effect, firmware acknowledgement, or rollback behavior. Device-proof
work must record those observations separately for the protocol-confirmed
model and firmware, then compare the captured write and readback with this
simulator transcript.

The same simulator is available through the Rust-owned mobile FFI as
`AeroSettingsSimulator`. It accepts the existing typed mobile command DTOs and
returns the existing mobile session output DTOs, including the same limited
production-shaped settings-readback events. Its typed simulator readback
record remains available for Rust-owned encoder/state assertions.
`AeroSettingsSimulatorTests` exercises that facade from Swift without Bluetooth
hardware:

```text
nix develop -c swift test --package-path swift/CutoutMobile --filter AeroSettingsSimulatorTests
```
