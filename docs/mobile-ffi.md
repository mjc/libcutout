# Mobile FFI Boundary

`cutout-mobile-ffi` is the UniFFI boundary between the Rust protocol engine and
the Swift and Kotlin clients. Rust owns the transport-independent DTOs and
concrete protocol sessions; platform code owns Bluetooth and UI concerns.

The Swift app consumes a checked-in Cargo Swift package:

```text
crates/cutout-mobile-ffi/CutoutMobileFFI
├── Package.swift
├── Sources/CutoutMobileFFI/cutout_mobile_ffi.swift
└── cutout_mobile_ffiFFI.xcframework
```

The XCFramework contains static iOS device, iOS simulator, and macOS slices.
`swift/CutoutMobile/Package.swift` depends on that package by local path, so
SwiftPM, Xcode, SourceKit, tests, and app builds all use the same artifacts.
Normal Swift work does not build Rust, generate bindings, set dynamic-library
paths, or pass custom linker flags.

## Regenerating the Swift package

Regenerate the package only after changing the Rust FFI surface or Rust code
that must ship in the app:

```console
nix develop -c sh -c '
  cd crates/cutout-mobile-ffi
  exec cargo swift package \
    --platforms ios@18 macos@15 \
    --release \
    --name CutoutMobileFFI \
    --lib-type static \
    --skip-toolchains-check \
    --accept-all \
    --swift-tools-version 6.0 \
    --silent
'
```

Cargo Swift 0.11 cannot spell the Xcode 27 platform enum in its generated
manifest, so the generated binary package uses compatible iOS 18 and macOS 15
floors. The app package and Xcode targets still require iOS 27 and macOS 27.

Commit the regenerated `CutoutMobileFFI` directory with the Rust change. Do not
copy its sources or archives into the app package.

## Checks

Run the Swift package tests and executable smoke directly:

```console
nix develop -c swift test --package-path swift/CutoutMobile
nix develop -c ./scripts/smoke-swift-package.sh
```

Build the real iOS UI-test graph without running UI automation:

```console
nix develop -c ./scripts/run-ios-ui-tests.sh --build-only
```

The Kotlin smoke remains a generation test because Kotlin does not consume the
Swift XCFramework:

```console
nix develop -c ./scripts/smoke-kotlin-bindings.sh
```

These checks exercise typed Aero, Falcon, and VESC sessions, captured
notification bytes, telemetry snapshots, parser diagnostics, command refusal,
and PEVCAP behavior through the generated boundary.

## SourceKit and app commands

No preparation step is required. Activate Serena at the repository root and
diagnose files under `swift/CutoutMobile`; the checked-in dependency makes the
package complete in a clean clone.

Useful app commands are:

```console
nix develop -c ./scripts/smoke-ios-app-metadata.sh
nix develop -c ./scripts/run-ios-app-on-mac.sh
CUTOUT_IOS_DEVELOPMENT_TEAM=YOURTEAM nix develop -c ./scripts/run-ios-app-on-phone.sh
```

The Mac command builds the iPhone app for Apple Silicon Mac and opens it. The
phone command builds, installs, and launches on a connected unlocked device.
Signing stays local through `CUTOUT_IOS_DEVELOPMENT_TEAM` and optional
`CUTOUT_IOS_APP_BUNDLE_ID`; the project does not commit a personal team.

`scripts/export-ios-ad-hoc.sh` archives and exports a release-testing IPA. It
uses Xcode's current export method and the signing environment documented in
the script.

Xcode beta may still emit App Intents metadata warnings even though the app has
no App Intents dependency. Those warnings come from Xcode's build pipeline and
must not be hidden by filtering stderr.
