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

## Generator Commands

The repository now has a `cutout-mobile-ffi` crate with `cdylib`/`staticlib`
output and a workspace-local `cutout-uniffi-bindgen` runner. The exported
UniFFI surface intentionally uses mobile-local DTOs so `cutout-core` and
`cutout-protocols` do not depend on UniFFI.

Generate bindings from the checked-in FFI surface with:

```console
cargo build -p cutout-mobile-ffi
cargo run -p cutout-uniffi-bindgen -- generate \
  --library target/debug/libcutout_mobile_ffi.dylib \
  --language swift \
  --no-format \
  --out-dir target/uniffi/swift
cargo run -p cutout-uniffi-bindgen -- generate \
  --library target/debug/libcutout_mobile_ffi.dylib \
  --language kotlin \
  --no-format \
  --out-dir target/uniffi/kotlin
```

On Linux the library extension is `.so`; on Windows it is `.dll`.

## Smoke Checks

The repository smoke script generates Swift and Kotlin bindings from the
checked-in FFI surface, compiles tiny generated-language clients, and runs them:

```console
./scripts/smoke-mobile-bindings.sh
```

The smoke clients prove the same behavior in Swift and Kotlin:

- construct Aero and Falcon read-only sessions;
- feed `LinkUp` and command DTO inputs;
- drain owned output DTOs;
- query current telemetry snapshot and parser diagnostics;
- observe a typed unsupported-command error from `ingest_checked`;
- observe a typed unsupported Falcon profile construction error.

These checks should compile generated Swift and Kotlin code rather than scanning
Rust source text.
