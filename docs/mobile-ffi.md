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
devenv tasks run build:swift-ffi-package
```

Cargo Swift 0.11 cannot spell the Xcode 27 platform enum in its generated
manifest, so the generated binary package uses compatible iOS 18 and macOS 15
floors. The app package and Xcode targets still require iOS 27 and macOS 27.
The Swift FFI generator records a fingerprint of the Rust source inputs beside
the package. All repository Swift/Xcode entry points run the same idempotent
ensure operation before building and regenerate only when the package is
missing, incomplete, stale, or has a wrong-architecture slice.

The generated package is never committed. Do not copy its sources or archives
into the app package.

Before an Xcode, device, or app workflow, verify the selected host Xcode and
its iOS SDK with:

```console
devenv tasks run check:xcode-ios
```

## Checks

Use Devenv's built-in treefmt integration for Rust and Nix:

```console
devenv shell -- treefmt      # format the working tree
devenv shell -- treefmt --ci # format and fail if files change
```

The treefmt Git hook uses the same configuration. The quality gate runs
`treefmt --ci` after its other checks finish, so formatting cannot edit their
inputs while they run. This command can rewrite files; use a disposable
checkout in CI. Shell entry does not automatically run the treefmt task.

The `cutout-dev` Rust tests check that generated FFI inputs are present,
nonempty, and include a package manifest. They run with the workspace tests;
there is no separate shell validation layer.

Run the Swift package tests, including the mobile-boundary protocol and
Bluetooth transport assertions:

```console
devenv tasks run test:swift-package
```

Build the real iOS UI-test graph without running UI automation:

```console
devenv tasks run build:ios-ui-tests
```

Every UI-test invocation performs Xcode's normal incremental
`build-for-testing` before it runs. There is deliberately no
`test-without-building` shortcut that can reuse an app from an older source
revision.

The Kotlin smoke remains a generation test because Kotlin does not consume the
Swift XCFramework:

```console
devenv tasks run test:kotlin-bindings-smoke
```

The task uses the JNA jar pinned by the devenv environment.

These checks exercise typed Aero, Falcon, and VESC sessions, captured
notification bytes, telemetry snapshots, parser diagnostics, command refusal,
and PEVCAP behavior through the generated boundary.

The canonical non-device quality gate combines formatting, `-D warnings`
Clippy, nextest (including FFI validation), doctests, and dependency policy:

```console
devenv tasks run project:quality-gate
```

`devenv test` remains the lightweight devenv lifecycle/environment check; it
does not run this project gate automatically so entering a shell stays cheap.

## SourceKit and app commands

The shared ensure operation prepares the ignored package before activating
SourceKit or building Swift. It is safe to call repeatedly; an unchanged
fingerprint is a no-op.

Prepare the generated dependency for SourceKit or Xcode with:

```console
devenv tasks run build:swift-ffi-package
```

Then open the project normally, or from the command line:

```console
open swift/CutoutMobile/CutoutApp.xcodeproj
```

Opening the project directly in Xcode before this bootstrap cannot resolve a
missing local package.

Useful app commands are:

```console
devenv tasks run run:ios-app-on-mac
devenv tasks run deploy:ios-device
devenv tasks run validate:aero-live-connection
devenv shell -- cutout-melk-live
```

The Mac command builds the iPhone app for Apple Silicon Mac and opens it. The
Mac build, device deployment, and ad-hoc archive check the built app's name,
Bluetooth usage description, device family, and supported orientations before
continuing. These checks live in `cutout-dev` and are covered by its Rust tests.
To check an existing Debug or Release bundle directly, use
`devenv shell -- cargo cutout ios verify-app /path/to/CutoutApp.app`.

The phone task builds, installs, and launches on a connected unlocked device using
the `ios-device-deploy` SecretSpec scope. Configure its
`CUTOUT_IOS_DEVELOPMENT_TEAM` and `CUTOUT_SPOTIFY_CLIENT_ID` in the OS keychain
with `devenv shell -- secretspec set --profile development NAME --provider keyring`
(`NAME` is one variable at a time); certificates and
provisioning profiles remain managed by Xcode. `CUTOUT_IOS_APP_BUNDLE_ID` is
forwarded when set. For custom launch arguments, use `secretspec run --scope
ios-device-deploy --profile development -- cargo cutout ios deploy -- --launch-smoke`. The project
does not commit personal signing values.

The live connection tasks run the opt-in macOS CoreBluetooth validators. Set
`CUTOUT_AERO_VALIDATION_TIMEOUT` for the Aero timeout. Set
`CUTOUT_MELK_VALIDATION_TIMEOUT` or `CUTOUT_MELK_PLATFORM_IDENTIFIER` for the
MELK validator timeout or remembered peripheral identity.

Pull the newest capture files from a connected iPhone with:

```console
devenv shell -- cargo cutout ios captures
```

This shares device discovery with deployment. `CUTOUT_IOS_DEVICE_UDID` selects
a device, `CUTOUT_IOS_APP_BUNDLE_ID` selects the app, and
`CUTOUT_IOS_CAPTURE_DESTINATION` overrides `target/ios-captures`.
`CUTOUT_IOS_CAPTURE_LIMIT` is a positive file count, defaulting to five.

The deployment and ad-hoc export tasks select the `development` profile and
provide an audit reason automatically:

```console
devenv tasks run deploy:ios-device
devenv tasks run export:ios-ad-hoc-ipa
```

The profile also accepts optional App Store Connect values for ad-hoc export:
`CUTOUT_APPSTORE_AUTH_KEY_PATH`, `CUTOUT_APPSTORE_AUTH_KEY_ID`, and
`CUTOUT_APPSTORE_AUTH_KEY_ISSUER_ID`. Store private key material with the same
profile and keyring options. For `CUTOUT_APPSTORE_AUTH_KEY_PATH`, store the
private-key contents (the PEM text), not the pathname of an existing `.p8`
file; `as_path = true` materializes those contents as a temporary path only
while the export command runs. SecretSpec does not commit the material.

`devenv tasks run export:ios-ad-hoc-ipa` archives and exports a
release-testing IPA with the `ios-ad-hoc-export` scope. It uses Xcode's current
export method and the signing environment documented in the shared Swift
tooling.

Xcode beta may still emit App Intents metadata warnings even though the app has
no App Intents dependency. Those warnings come from Xcode's build pipeline and
must not be hidden by filtering stderr.
