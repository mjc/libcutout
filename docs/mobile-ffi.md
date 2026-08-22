# Mobile FFI Boundary

`cutout-mobile-ffi` is the UniFFI boundary between the Rust protocol engine and
the Swift and Kotlin clients. Rust owns the transport-independent DTOs and
concrete protocol sessions; platform code owns Bluetooth and UI concerns.

The Swift app consumes a generated Cargo Swift package in ignored build state:

```text
target/swift-ffi/CutoutMobileFFI
├── Package.swift
├── Sources/CutoutMobileFFI/cutout_mobile_ffi.swift
└── cutout_mobile_ffiFFI.xcframework
```

The XCFramework contains static iOS device, iOS simulator, and macOS slices.
`swift/CutoutMobile/Package.swift` depends on that package by local path, so
SwiftPM, Xcode, SourceKit, tests, and app builds all use the same artifacts.
Repository Swift/Xcode scripts ensure that the package exists, contains the
required architectures, and was generated from the current Rust inputs before
building. Normal Swift-only work does not set dynamic-library paths or pass
custom linker flags.

## Regenerating the Swift package

Regenerate the package after changing Rust code that ships in the app (the
output remains ignored):

```console
nix develop -c cargo cutout swift-ffi
```

Cargo Swift 0.11 cannot spell the Xcode 27 platform enum in its generated
manifest, so the generated binary package uses compatible iOS 18 and macOS 15
floors. The app package and Xcode targets still require iOS 27 and macOS 27.
The regeneration script records a fingerprint of the Rust source inputs beside
the package. All repository Swift/Xcode entry points run the same idempotent
ensure operation before building and regenerate only when the package is
missing, incomplete, stale, or has a wrong-architecture slice.

The generated package is never committed. Do not copy its sources or archives
into the app package.

## Checks

Run the Swift package tests and executable smoke directly:

```console
nix develop -c ./scripts/test-swift-package.sh
nix develop -c ./scripts/smoke-swift-package.sh
```

Build the real iOS UI-test graph without running UI automation:

```console
nix develop -c ./scripts/run-ios-ui-tests.sh --build-only
```

Every UI-test invocation performs Xcode's normal incremental
`build-for-testing` before it runs. There is deliberately no
`test-without-building` shortcut that can reuse an app from an older source
revision.

The Kotlin smoke remains a generation test because Kotlin does not consume the
Swift XCFramework:

```console
nix develop -c ./scripts/smoke-kotlin-bindings.sh
```

These checks exercise typed Aero, Falcon, and VESC sessions, captured
notification bytes, telemetry snapshots, parser diagnostics, command refusal,
and PEVCAP behavior through the generated boundary.

## SourceKit and app commands

The shared ensure operation prepares the ignored package before activating
SourceKit or building Swift. It is safe to call repeatedly; an unchanged
fingerprint is a no-op.

Prepare the real Swift package for SourceKit or MCPLS indexing with:

```console
nix develop -c ./scripts/prepare-swift-sourcekit-workspace.sh
```

On a fresh checkout, bootstrap the generated local dependency before Xcode
tries to resolve packages. The supported command ensures the dependency and
then opens the project:

```console
nix develop -c ./scripts/open-cutout-xcode.sh
```

Opening the project directly in Xcode before this bootstrap cannot resolve a
missing local package.

Useful app commands are:

```console
nix develop -c ./scripts/smoke-ios-app-metadata.sh
nix develop -c ./scripts/run-ios-app-on-mac.sh
CUTOUT_IOS_DEVELOPMENT_TEAM=YOURTEAM nix develop -c cargo cutout ios deploy
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
