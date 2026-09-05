# Settings simulator

`cutout-dev simulator aero-settings` runs the deterministic NOSFET Aero
settings simulator without Bluetooth hardware:

```text
nix develop -c cargo run -p cutout-dev -- simulator aero-settings
```

The simulator is Rust-owned and wraps the production
`StationarySettingsWriteSession<NosfetAeroModel, false>`. It therefore uses
the same protocol identity, stationary/500 mm/s safety gate, encoders, write
channel, write mode, and bounded payloads as the live session. Its typed
readback changes only after a transport write is emitted.

The command is software evidence only. Its payloads and readback prove that
the library path is deterministic and write-safe; they do not prove a wheel's
physical effect, firmware acknowledgement, or rollback behavior. Device-proof
work must record those observations separately for the protocol-confirmed
model and firmware, then compare the captured write and readback with this
simulator transcript.
