# libcutout agent guide

- Reuse the shared Swift FFI ensure boundary; do not add consumer-specific generation or linker workarounds.
- Run project commands from the repository root through Devenv. For Swift package tests use `devenv tasks run test:swift-package`; for an iOS Simulator app build use `devenv tasks run build:ios-app`; for the full non-device gate use `devenv tasks run project:quality-gate`. These tasks regenerate stale Rust FFI and build in the same invocation through `cargo cutout swift -- ...` or `cargo cutout xcodebuild -- ...`. Do not substitute bare `swift`, `xcodebuild`, or `devenv test` for these checks.
- Direct Xcode Product > Build validates an already-prepared FFI package; it cannot generate a missing package before SwiftPM resolution. Use `devenv tasks run build:swift-ffi-package` only to prepare SourceKit/Xcode project opening, not as a required separate step before the supported build tasks.
- Keep generated Swift FFI output ignored and never commit generated bindings, headers, module maps, fingerprints, or static libraries.
- Treat `crates/cutout-mobile-ffi/src/**/*.rs` as handwritten UniFFI boundary source. The only valid generated Swift package is the generation selected by `target/swift-ffi/Package.swift`; never inspect or edit the legacy `target/swift-ffi/CutoutMobileFFI` or `crates/cutout-mobile-ffi/CutoutMobileFFI` paths, and remove those exact directories if they remain in a checkout.
- Read [the music scope contract](docs/music-integration.md) before changing music, listening history, or ride-replay code.
