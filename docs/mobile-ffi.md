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
- feed owned notification bytes from a captured Aero frame through Rust;
- export a small PEVCAP JSONL capture with preserved provenance annotations,
  advertised services, GATT fingerprints, and resolved identity metadata;
- drain owned output DTOs;
- query current telemetry snapshot and parser diagnostics;
- observe a typed unsupported-command error from `ingest_checked`;
- observe a typed unsupported Falcon profile construction error.

These checks should compile generated Swift and Kotlin code rather than scanning
Rust source text.

## Swift Package Smoke

The repository also has a SwiftPM packaging smoke for the first ergonomic
Aero/Falcon library surface:

```console
./scripts/smoke-swift-package.sh
```

The script builds `cutout-mobile-ffi`, generates Swift UniFFI bindings into a
temporary package, assembles the generated `cutout_mobile_ffiFFI` C module in
SwiftPM's expected layout, and runs a Swift executable that imports
`CutoutMobile`.

The checked-in package source under `swift/CutoutMobile` intentionally contains
only hand-written facade code and package metadata. Generated UniFFI files stay
under `target/` so app-facing Swift remains reviewable while the FFI bindings
continue to come from the Rust crate.

## Swift SourceKit Workspace

The checked-in Swift package depends on generated UniFFI bindings, so tools that
expect a complete SwiftPM workspace should use a generated workspace under
`target/`:

```console
./scripts/prepare-swift-sourcekit-workspace.sh
```

The script builds `cutout-mobile-ffi`, generates Swift UniFFI bindings, lays out
the `cutout_mobile_ffiFFI` system-library target, and verifies the staged package
with `swift package describe`. On Darwin it clears `SDKROOT` and `DEVELOPER_DIR`
for the Swift command so Xcode's Swift toolchain does not accidentally pair with
a Nix Apple SDK.

By default the workspace is written to:

```text
target/swift-sourcekit/CutoutMobile
```

Set `CUTOUT_SWIFT_SOURCEKIT_PACKAGE_DIR` to choose a different generated
workspace path. Do not check in the generated bindings, header, or module map.
