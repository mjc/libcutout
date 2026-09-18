# Mobile FFI Boundary

`cutout-mobile-ffi` is the UniFFI boundary between the Rust protocol engine and
the Swift and Kotlin clients. Rust owns the transport-independent DTOs and
concrete protocol sessions; platform code owns Bluetooth and UI concerns.

For settings, that ownership includes value domains, applicability, observations,
completion strategy, guards, serialization, deadlines and retry policy. The
[settings design review](settings-design-review.md) proposes the remaining
contract repairs; the current adapter/FFI boundary is not proof that native
submission and operation completion already obey that contract.

Each operation must retain its connection-attempt and operation IDs through
preparation, native queueing, actual host submission/failure and completion.
Native adapters report receipts to Rust rather than deciding success. A BLE
write-without-response host receipt is not a wheel acknowledgment. Queue
overflow must fail the affected operation explicitly, and delayed sends must
recheck their applicable guards. The checkpoint currently publishes accepted
plans before native execution and can drop the oldest queued write without a
request-specific failure; these are open defects, not supported semantics.

The Swift app consumes a generated package selector in ignored build state:

```text
target/swift-ffi
├── Package.swift
└── generations/<receipt>/CutoutMobileFFI
    ├── Sources/CutoutMobileFFI/cutout_mobile_ffi.swift
    └── cutout_mobile_ffiFFI.xcframework
```

The XCFramework contains static iOS device, iOS simulator, and macOS slices.
`swift/CutoutMobile/Package.swift` depends on `target/swift-ffi` by local path.
The selector manifest contains literal paths to the Sources and XCFramework
inside one immutable `generations/<receipt>/CutoutMobileFFI` directory. SwiftPM,
Xcode, SourceKit, tests, and app builds therefore resolve a specific generation.
Publishing a new selector leaves previously selected generations intact.

`crates/cutout-mobile-ffi/src/**/*.rs` is handwritten Rust source. It defines
the UniFFI records, enums, objects, exports, and adapter behavior consumed by
the generator; it is not generated output. The generated Swift package exists
only under the selected immutable generation shown above. In particular,
`target/swift-ffi/CutoutMobileFFI` and
`crates/cutout-mobile-ffi/CutoutMobileFFI` are unsupported legacy output paths,
not alternate inputs or package locations. Do not inspect or edit them. If
either path is present in a checkout, remove that exact legacy directory; the
supported selector is the only generated package that native builds consume.

Normal tasks use `cargo cutout swift -- <native swift args>` or
`cargo cutout xcodebuild -- <native xcodebuild args>`. The shared Rust pipeline
ensures current FFI inputs, then invokes `/usr/bin/xcrun swift` or
`/usr/bin/xcrun xcodebuild` in the same invocation. No rerun is needed after
generation. Devenv leaves native `swift` on PATH and selects Xcode beta with its
matching SDK. The build uses no watcher, mutable generation symlink, cache
deletion, custom exported symbols, dynamic-library paths, or per-consumer
linker flags.

## Regenerating the Swift package

Regenerate the package after changing Rust code that ships in the app (the
output remains ignored):

```console
devenv tasks run build:swift-ffi-package
```

The standalone ensure command is `devenv shell -- cargo cutout swift-ffi`.

Cargo Swift 0.11 cannot spell the Xcode 27 platform enum in its generated
manifest, so the generated binary package uses compatible iOS 18 and macOS 15
floors. The app package and Xcode targets still require iOS 27 and macOS 27.
Every supported prepare operation invokes `cargo swift`, including when the
source hash and selected generation are unchanged. Cargo owns incremental
compilation and configuration freshness; the project has no separate Cargo
environment resolver or source-fingerprint early reuse shortcut.

The generator keeps private source snapshots in the repository's sibling
`.cutout-ffi-sources/<repository-path-hash>/<source-hash>` directory. This preserves
Cargo's ancestor/global configuration lookup without loading the project's
configuration twice. Both `.cargo/config` and `.cargo/config.toml`, when present,
are copied so Cargo applies its normal precedence. It copies those sources once,
preserving compilation paths and mtimes across repeated prepares so Cargo can
reuse its cache. Relative files and directories named by those Cargo
configuration files are copied into the snapshot at their original paths. A
damaged private snapshot (including a Cargo-rewritten lockfile)
is moved aside and rebuilt from current inputs. A corrupt published generation is
likewise quarantined and replaced from a verified staged package; healthy pinned
generations are never modified. Recovery has a brief rename gap for readers of
the already-corrupt path. A lock
serializes generation and holds the selector stable throughout supported builds.
The snapshot cache retains the current snapshot and three recent snapshots, and
keeps four quarantine directories for diagnosis; older unused entries are
removed while that lock is held.
The noninteractive Cargo generator inherits a clone of that lock on stdin, so
an orphaned generator retains ownership if its coordinating process is killed.
The native output receipt identifies the immutable
generation; unchanged output avoids rewriting the selector, but never skips
the Cargo invocation.

Every build of the shared `CutoutMobile` target also runs the `VerifyRustArtifact`
build-tool plugin. Its sandboxed Rust checker validates the generation pinned
by the resolved package graph, comparing the source fingerprint and hashes of
the bindings, manifest, headers, module maps, and static libraries with its
receipt. Missing or changed inputs fail the build. The checker writes only its
plugin output directory. Generation refuses to publish if Rust inputs changed
while it was compiling.

For a direct native build, the checker also compares the pinned generation's
receipt with the selected generation's receipt. This checks against the
currently selected prepared output; it does not inspect the actual Rust
toolchain or resolve Cargo configuration. The selected generation is expected
to have been prepared with the current build configuration. Use the explicit
Cargo pipeline to establish that freshness.

A generated Swift source records the Rust source identity. The plugin emits a
second Swift source with the verified artifact identity, preserving it when
unchanged. These compilation inputs invalidate linking after an implementation-only
Rust change even when the UniFFI interface is identical. Generated receipts and
checker executables remain ignored alongside the generated package.

Use the normal command; no separate regeneration invocation is needed:

```console
devenv shell -- cargo cutout swift -- test --package-path swift/CutoutMobile
devenv shell -- cargo cutout xcodebuild -- -project swift/CutoutMobile/CutoutApp.xcodeproj -scheme CutoutApp -destination 'generic/platform=iOS Simulator' ARCHS=arm64 ONLY_ACTIVE_ARCH=YES build
```

Normal tasks ensure FFI and run the native incremental build. Direct native
Swift or Xcode builds check their pinned inputs through the plugin, but the
plugin cannot generate Rust artifacts before package graph resolution. A missing
selector therefore needs standalone preparation; stale inputs require the
explicit Cargo pipeline or standalone ensure before a new native build.
Commands that skip building also skip plugin verification and cannot establish
freshness. There is no hidden Swift executable interception or argument parser.

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

The `cutout-dev` Rust tests check generation, receipts, recovery, and lock
ownership. The shell tests cover app-product selection and UI-test timeout
cleanup without running on a device.

The native warm-cache regression changes only a Rust function's implementation
and verifies A then B at runtime through default SwiftPM and Xcode, without
cleaning either build directory:

```console
devenv shell -- bash tests/fixtures/ffi-freshness/run.sh --xcode
```

Run the Swift package tests, including the mobile-boundary protocol and
Bluetooth transport assertions:

```console
devenv tasks run test:swift-package
```

Build the real iOS UI-test graph without running UI automation:

```console
devenv tasks run build:ios-ui-tests
```

Every UI-test invocation uses the shared Rust pipeline followed by Xcode's
normal incremental `test` action; `--build-only` uses `build-for-testing`.
There is deliberately no
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
Clippy, nextest (including FFI validation), doctests, and dependency policy.
On Darwin it also runs the Swift package tests and builds the iOS UI-test
graph. Both Swift/Xcode tasks prepare stale Rust FFI in the same invocation:

```console
devenv tasks run project:quality-gate
```

`devenv test` remains the lightweight devenv lifecycle/environment check; it
does not run this project gate automatically so entering a shell stays cheap.

## SourceKit and app commands

The shared ensure operation prepares the ignored package before activating
SourceKit or building Swift. Each call invokes `cargo swift`; Cargo reuses
unchanged compilation inputs, and unchanged output leaves the selector intact.

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
devenv tasks run build:ios-app
devenv tasks run run:ios-app-on-mac
devenv tasks run deploy:ios-device
devenv shell -- cutout-melk-live
```

### Live Aero settings validation on macOS

`validate:aero-live-connection` is the existing macOS CoreBluetooth harness,
not a settings acceptance procedure. The unsafe mutating sweep has been removed:
`--settings` and all legacy mutation opt-ins now fail before constructing the
Bluetooth session. The remaining connection smoke check runs once, streams logs
as they arrive, uses a finite timeout, and explicitly reports
`settings_validation=not_run`. It does not inventory settings observations.

The terminal native-tool handoff replaces the build coordinator on Unix instead
of leaving a waiting parent. Offline subprocess tests cover retained PID/FFI
lock, INT/TERM exit, and lock cleanup after failed exec. This fixes that handoff,
not cancellation of every outer launcher/descendant or native queued write.

Existing configuration is recorded here for code review, not as a run recipe:

| Switch | Current behavior / limitation |
| --- | --- |
| `CUTOUT_AERO_SETTINGS_TEST` | Value `1` is rejected before Bluetooth starts |
| `CUTOUT_AERO_INCLUDE_HEADLIGHT`, `CUTOUT_AERO_INCLUDE_AUDIBLE`, `CUTOUT_AERO_INCLUDE_ALARM_MODES`, `CUTOUT_AERO_INCLUDE_TRIP_RESET` | Value `1` is rejected, even without settings mode |
| `CUTOUT_AERO_ALLOW_UNRESTORABLE_WRITES` | Value `1` is rejected; unknown baselines never authorize generated probes |
| `CUTOUT_AERO_TARGET`, `CUTOUT_AERO_VALIDATION_TIMEOUT` | Select target and connection timeout (default 45 seconds, finite, greater than 0 and at most 600); neither establishes settings-page coverage or a per-operation deadline |

Disabling the settings suite does not make connection-only discovery proven
harmless. A recorded NF2557 connection emitted Begode N/V/M probes and flushed
queued probe bytes after Veteran identity resolved. FFE0/FFE1 is shared; probe
eligibility and retirement of incompatible queued work need repair. “Read-only”
describes intended semantic operations, not an absence of transport writes.

The removed descriptor inventory ran before settings telemetry and exited once
ride telemetry was live. Its `current=nil` values do not show lack of device
readback. Bounds and confirmation flags came from library declarations, not
wheel negotiation. The replacement observation mode must run for a bounded
relevant page cycle and report actual fields/pages received separately.

The [design review](settings-design-review.md) specifies the replacement:
fake-transport tests for zero control writes in inventory mode, exactly-once
execution, genuine terminal waiting, honest incomplete verdicts and cancellation
of the actual child and queued work; then one reviewed physical case at a time.
Each case names exact identity, target, expected physical effect, expected
evidence and restoration policy. No descriptor-minimum probes, generated numeric
targets, boolean sweeps or live invalid-input tests. Intent and transport/readback
evidence must stream durably, and failed restoration remains an explicit recovery
obligation. The audible incident ceased after power cycling; neither its cause
nor complete restoration was established.

Tracker ownership: [LIBCU-836](https://lific.mjc.lol/LIBCU/issues/LIBCU-836)
for harness repair; LIBCU-477 for operation lifecycle; LIBCU-641 for backpressure;
LIBCU-476 for observations; LIBCU-505 for probe safety. The shared proposed
contract is [LIBCU-DOC-8](https://lific.mjc.lol/LIBCU/pages/30); physical
acceptance remains in LIBCU-390 after those prerequisites.

### App build and deployment behavior

The app build task targets an ARM64 iOS Simulator. The Mac command builds the
iPhone app for Apple Silicon Mac and opens it. Its
bundle helper accepts only the CutoutApp project and scheme on a macOS
destination; other builds must select their own product instead of reusing a
retained device app. The Mac build, device deployment, and ad-hoc archive check
the built app's name, Bluetooth usage description, device family, and supported
orientations before continuing. These checks live in `cutout-dev` and are covered
by its Rust tests.
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

Default exports always build and verify a current archive through the shared
Rust pipeline, even when `CUTOUT_IOS_AD_HOC_ARCHIVE_PATH` already exists.
Set `CUTOUT_IOS_AD_HOC_ARCHIVE` explicitly to export an existing archive without
rebuilding; a missing explicit archive is an error. Failed builds stop the
workflow without deleting existing app products or archives.

Xcode beta may still emit App Intents metadata warnings even though the app has
no App Intents dependency. Those warnings come from Xcode's build pipeline and
must not be hidden by filtering stderr.
