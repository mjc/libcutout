# Settings simulator

`cutout-dev simulator aero-settings` runs the deterministic NOSFET Aero
settings simulator without Bluetooth hardware:

```text
devenv shell -- cargo run -p cutout-dev -- simulator aero-settings
```

The simulator is Rust-owned and wraps the production
`StationarySettingsWriteSession<NosfetAeroModel, false>`. It therefore uses
the same protocol identity, stationary/500 mm/s safety gate, encoders, write
channel, write mode, bounded payloads, and production-shaped settings-readback
output conversion as the live session. Its simulated readback event is emitted
only after the complete transport write sequence is emitted.

The simulator's private typed snapshot records every implemented setting, including
MD numeric pedal hardness, PWT, the additional numeric Aero controls, wheel
units, ANG, the canonical Headlight command, and trip-reset count. PWT is represented as an
explicit margin or disabled wire setting, and the mobile Tune control exposes both
forms. The production-shaped
settings event is a synthetic projection, not an inventory of everything the
live Veteran/NOSFET dialect can report. Captured settings pages include additional
fields documented in the [source inventory](eucworld-aero-settings-re.md).
Neither absence from the simulator's projection nor an early live snapshot
proves unsupported readback. The private snapshot is encoder/state evidence,
not a claim that a wheel confirmed those settings.

Gyro calibration is a source-backed lifecycle toggle; page-8 byte 56 is
decoded as 0 idle, 1 waiting, or 2 complete. The simulator follows the
official start/wait/complete/stop transition, but does not claim that its
synthetic state performs a physical calibration.

The command is software evidence only. It exercises deterministic encoding and
simulated transitions through shared production code. Agreement between its
encoder and synthetic readback is not independent protocol evidence, native
queue safety, host submission, firmware acknowledgment, physical effect or
restoration. In particular, it cannot close the transport and harness defects
in the [settings design review](settings-design-review.md).

Further offline coverage must use independently sourced golden frames and
captured page cycles, including fragmentation, unavailable fields and differing
write/observation domains. Fake-transport tests must cover operation identity,
backpressure, cancellation, stale guards and exactly-once case execution before
individual physical cases resume. Compare those cases against the simulator
transcript without treating simulated success as device acceptance.

The simulator is intentionally not exported through the mobile FFI. Mobile
callers use the generic device-session and semantic settings/action boundaries;
the deterministic simulator remains a Rust development tool and is covered by
the protocol-layer tests above.
