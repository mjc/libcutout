# Mobile FFI Boundary

Cutout currently has Rust-owned DTOs and concrete protocol wrapper types, but it
does not yet have a generated UniFFI boundary. The existing surface is ready for
that boundary:

- `cutout-core` owns transport-independent DTOs such as `SessionInputDto`,
  `SessionOutputDto`, `TelemetrySnapshotDto`, `ParserDiagnosticsDto`, and
  `ControlRefusalDto`.
- `cutout-protocols` owns concrete wrapper types that can name protocol models:
  `ConcreteAeroReadOnlySession` and `ConcreteFalconReadOnlySession`.
- Concrete wrappers avoid generics, trait objects, borrowed buffers, async
  streams, and platform transport types in their public mobile-facing methods.
- `ingest_checked` returns `ConcreteSessionStepResultDto`, preserving owned
  output DTOs and stable `ConcreteSessionErrorDto` values for unsupported
  commands.

## Generator Gap

Generated Swift and Kotlin smoke checks are blocked until the repository has an
actual UniFFI component crate or UDL/proc-macro export surface. The missing
pieces are:

- a mobile FFI crate with `cdylib`/`staticlib` output configured;
- UniFFI dependency and code generation wiring in Cargo/Nix;
- exported constructors for Aero and Falcon read-only sessions;
- exported DTO/result/error types for session inputs, outputs, snapshots, and
  diagnostics;
- local or CI commands that generate Swift and Kotlin bindings from the exact
  checked-in FFI surface.

## Smoke Matrix

Once the generator scaffold exists, generated-language smoke checks should prove
the same behavior in Swift and Kotlin:

- construct Aero and Falcon read-only sessions;
- feed `LinkUp` and command DTO inputs;
- drain owned output DTOs;
- query current telemetry snapshot and parser diagnostics;
- observe a typed unsupported-command error from `ingest_checked`;
- observe a typed unsupported Falcon profile construction error.

These checks should compile generated Swift and Kotlin code rather than scanning
Rust source text.
