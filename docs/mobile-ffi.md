# Mobile FFI Boundary

`cutout-mobile-ffi` is the UniFFI boundary between the Rust protocol engine and
the Swift and Kotlin clients. Rust owns the transport-independent DTOs and
concrete protocol sessions; platform code owns Bluetooth and UI concerns.

## Shared application services

`cutout-application` provides portable application operations for direct Rust
clients and the native binding adapter. It has no dependency on UniFFI or a UI
toolkit. Its charging services own nearby discovery, synchronization admission
and retry decisions, and revision-aware map reconciliation. The corresponding
`cutout-mobile-ffi` exports convert inputs and results and delegate to those
services; their existing Swift and Kotlin API remains the binding contract.

Discovery retains the caller's filter value without interpreting it. Reconciliation
uses the caller's row through `ChargingObservation`, preserving its payload while
applying shared revision, visibility, and withdrawal rules. This lets Rust callers
use ordinary application/domain values while bindings retain their native records.

Other application workflows are still hosted in the mobile binding crate and
are being extracted separately. Native adapters continue to own platform handles
and effects. Link the application library through the final process composition
so consumers share the same session and database owners.

## Native shell ownership

Swift supplies Apple API observations and executes native effects. Portable
decisions belong in Rust, including validation, admission, recording policy,
durability, loss accounting, clock normalization, receipt ownership, and
recovery. Native code must not infer a lifecycle transition from a Boolean
setter result, writer instrumentation, or a presentation error string.

Recording startup uses a Rust-prepared request before reading the native
monotonic clock. Rust validates the wall-clock observation, assembles required
metadata, reserves annotation capacity, constructs the output filename, and
admits the existing writer once. Swift supplies the filesystem directory,
platform identity, clocks, and optional provider observation; it installs the
admitted native references and publishes presentation state.

Flush and completion return typed Rust receipts. Rust distinguishes a rejected
flush barrier from a failed durability barrier and classifies terminal SQLite
and file results independently of optional history publication. The session
owner consumes each terminal receipt once, including a retired writer's
receipt after a replacement starts. Swift maps authorized publication results
to UI events and native file URLs.

Keep this boundary when changing the app: a policy fix must be testable without
an Apple runtime. Swift tests verify observation delivery, native effect
ownership, and presentation; Rust tests verify the portable invariants.

The charging editor uses Rust's power draft for suggested/manual provenance and
explicit inclusion. Adding a suggested supply confirms that value; a country
change that replaces it requires confirmation again. Manual observations survive
country and venue changes. Swift retains incomplete field text and translates
native edits into one draft transition. An incomplete edit marks the row manual
and revokes inclusion before validation; reading a draft never confirms it.
Inclusion is represented by the optional confirmed supply, without a separate
confirmation flag in the mobile snapshot.

Charging preflight and persistence use the same Rust constructor pipeline.
Preflight returns a normalized input or a typed field issue. Swift adds typed
feedback for incomplete native text and unconfirmed supplies, and renders the
issues beside the affected fields. Persistence validates again when saving.

Charging map reconciliation belongs to Rust: revision preference, delayed readback
admission, withdrawal suppression and restoration are one projection policy.
Swift supplies observations and publishes the resulting page and selection while
retaining native task cancellation and map presentation. Cached observations
carry checked revisions; raw revisions are validated at the boundary.

The charging sync policy owns registration admission, bounded attempts and rounds,
retry delays, progress accumulation and terminal outcomes. Swift executes Keychain
operations and native tasks, and maps typed outcomes to localized presentation.
Committed upload/download progress remains available when work is blocked.
Pass disposition and retry actions share one terminal result vocabulary. Native
progress reads the returned run state directly. Keychain claim rereads remain
native record extraction, and a lost claim cannot initiate registration.

Nearby discovery uses explicit state transitions for empty results, location
availability, cache updates and user cancellation. Swift executes one-shot search
and camera decisions without deriving another discovery state from UI flags.
Rust emits a validated camera target once; Swift owns its pending presentation
and consumption. One native sheet route selects details, issues, or the existing
editor model. The map Add action enters placement; confirming the pin opens the
compact form. Dismissing the editor cancels that draft.
Charging location selection validates freshness and chooses fixes in Rust; Swift
uses acceptance predicates and retains the original accepted native location
and its platform metadata.

## Shared settings ownership

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
request-specific failure; these remain open defects. Settings writes now carry
the Rust request operation identity through the core and mobile FFI into the
Swift action, correlated CoreBluetooth planned writes (including chunks), and
the native receipt boundary. A settings write cancelled before native
submission is now reported distinctly from host rejection; broader operation
classes still need the same cancellation treatment. This identity path is not
a device acknowledgment.

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
Successful publication atomically selects the replacement and removes obsolete
managed generations. The selected package stays immutable; historical package
payloads are not retained.

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
source hash and selected generation are unchanged. Cargo Swift compiles and
bundles the ARM64 archives. Before sealing the package, the shared prepare step
runs `cutout-uniffi-bindgen` from the same immutable workspace snapshot against
the packaged macOS archive, then installs its Swift wrapper and matching headers
and module maps in every slice. This uses the workspace's locked UniFFI version
for both Swift and Kotlin; Cargo Swift 0.11.1 embeds an older generator whose
callback pointers fail Swift 6 concurrency checking. No consumer edits or
separate preparation command are needed. Cargo owns incremental
compilation and configuration freshness; the project has no separate Cargo
environment resolver or source-fingerprint early reuse shortcut.

The generator keeps private source snapshots in the repository's sibling
`.cutout-ffi-sources/<repository-path-hash>/<source-hash>` directory. This preserves
Cargo's ancestor/global configuration lookup without loading the project's
configuration twice. Both `.cargo/config` and `.cargo/config.toml`, when present,
are copied so Cargo applies its normal precedence. Repeated prepares of the
current snapshot preserve its compilation paths and mtimes so Cargo can reuse
its cache. Switching source identity removes the previous snapshot after the new
copy is ready. Returning to an earlier identity therefore copies fresh inputs
rather than resurrecting old mtimes against artifacts built from later sources.
Relative files and directories named by those Cargo
configuration files are copied into the snapshot at their original paths. A
damaged private snapshot (including a Cargo-rewritten lockfile)
is moved aside and rebuilt from current inputs. A corrupt published generation is
moved aside until its verified replacement is selected. Successful publication
removes the damaged package payload as well as obsolete managed generations;
unchanged publication also removes older packages left by previous prepares.
A lock serializes publication and holds the selector stable throughout supported
builds. Their native processes retain that lock, so a replacement cannot remove
the generation while they use it. Pruning recognizes generated receipts and
leaves unrelated paths and symlink targets untouched.
The separate source-snapshot cache retains only the current snapshot. A healthy
capture or repair removes obsolete source copies and quarantined snapshots while
that lock is held. A failed capture leaves the last valid snapshot available.
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

Direct Xcode builds and SourceKit indexing do not hold the publication lock for
their full lifetime. Finish those reads before preparing a replacement, then
resolve the current package selection before building or indexing again. The
supported Cargo build commands coordinate preparation and native readers under
one lock; the sandboxed checker alone cannot provide that coordination.

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

### Kotlin session usage

`tests/mobile-ffi/shared/EucReplaySmoke.kt` is an executable usage example for
the current generic `CutoutSessionStateHandle`. It runs in both the host smoke
and Android instrumentation. Rust selects the decoder from observed evidence;
Kotlin uses `step.session.identity.vehicleKind` for the product-category screen
and the independently reported `model` for its label. A model name or a generic
VESC transport must not select the physical vehicle category in Kotlin.

Use one ordered background owner for the handle and native connection events.
The example uses a single-thread executor and returns an owned snapshot after
closing the handle. Android's future BluetoothGatt adapter should enqueue
callbacks onto that owner, copying callback buffers before enqueueing, and
publish owned snapshots to its UI dispatcher. Rust's internal locks protect
individual calls; they do not order independently launched Kotlin coroutines.

Retain the token returned by `beginConnectionAttempt` with that native connection.
Pass the captured token through identification, notifications, timers and
transport receipts. Reconnecting to the same peripheral creates a new token.
Late callbacks must retain their original token; replacing it with the latest
token would admit old bytes into the new session. During shutdown, stop accepting
callbacks, invalidate the Rust attempt before native cancellation, reject queued
stale work, then close the handle on its owner.

`ingestDeviceSession` returns this input's ordered `result.outputs`, current
telemetry, diagnostics and `captureNotificationEvidence` together. Consume each
output once; this path has no additional output-drain call. A missing step
means the attempt was not admitted. Keep typed `result.error` and parser ingest
outcomes distinct. The transport adapter executes subscribe/write/disconnect
requests and reports actual completion to the existing Rust receipt APIs.
Capture hooks preserve the original timestamp/channel/bytes and the Rust
admission evidence; Kotlin must not infer stationary state or deduplication.

The replay uses the existing public hardware-tested
`crates/cutout-cli/fixtures/pevcap/aero-veteran-live.jsonl`. The runner converts
its link/notification records into ignored Kotlin fixture source, preserving
timestamps and chunk boundaries without another runtime JSON dependency or
copied wire data. Recorded outbound writes are excluded. Identification and
decoding use separate replay passes; the capture's model label is not an input
to detection. Tests check fragmented parsing, semantic outputs, retention of all
fragmented capture bytes, owned snapshot lifetime and stale-token refusal. The receipt
includes the source capture's hash. This exercises Kotlin consumption of the
existing session API. The production Android adapter below supplies scanning,
permissions, GATT queues and transport receipt delivery for LIBCU-34.

### Android app and physical tests

The production Gradle project is `android/`, application ID `lol.cutout`, for
Android 13/API33 or newer (physical ARM64, headless x86_64 emulator). It reuses the original test project's SDK,
generator, native staging and Gradle cache. Its native launcher provides Devices,
Ride and descriptor-driven Configuration. `PevSession` consumes the shared Rust
owner; `AndroidBleClient` supplies permission-aware scanning, GATT inventory,
subscriptions, serialized writes, MTU limits and attempt-scoped callbacks.
The main Ride view is fixed; device and configuration lists can scroll.

The existing shared assertions now belong to the production app's instrumentation
target rather than its main sources. Only generated FFI and production adapters
ship in the app. Settings labels reuse the checked-in shared string catalogs;
Android presentation strings live in `android/src/main/res/values/strings.xml`.

The host JVM and Android instrumentation use the same assertions in
`tests/mobile-ffi/shared/MobileFfiSmoke.kt`. The Android wrapper supplies an
isolated app-private temporary directory. It exercises generated Kotlin/JNA
calls into the actual selected-ABI Rust library: session identity, settings snapshots,
owned frame buffers, camera command outcomes, typed refusals, SQLite writes and
reopen, and explicit handle shutdown. Inputs include synthetic cases and the
public EUC capture replay; no wheel or camera connection is made.

```console
devenv tasks run test:mobile-ffi-runner
devenv tasks run test:kotlin-bindings-smoke
devenv tasks run test:android-ui
devenv tasks run build:android-tests
CUTOUT_ANDROID_DEVICE=192.168.1.115:5555 devenv tasks run test:android-device
CUTOUT_ANDROID_DEVICE=192.168.1.115:5555 CUTOUT_ANDROID_SERIAL=ZY22K6J64X devenv tasks run run:android-app
```

`test:android-ui` runs the production native views through Robolectric 4.16
with API 35 and Native Graphics on ARM64 macOS. It reuses the host Rust library,
the production Gradle project and the picker, settings and Ride instrumentation
assertions. It checks recovery actions, stable view identity, attempt-bound
settings drafts, Rust-selected operating states and relayout without new
telemetry. It also renders light and dark PNGs of the actual picker, EUC Ride and
settings views. These checks require no phone, emulator or Simulator. They do
not establish physical Bluetooth behavior or on-device rendering acceptance.
The first run downloads the pinned test dependencies and Android runtime jars;
later runs reuse the existing project cache.

The address above is an example. Use `devenv shell -- adb devices -l` to select
the current USB serial or wireless endpoint explicitly. With multiple transports,
the runner refuses to guess. It checks the physical phone's serial, model, API
and ABI before building, before installing, and before testing. It will not fall
back to another device if the selected phone is unauthorized, offline or absent.
Set `CUTOUT_ANDROID_SERIAL` to the known hardware serial to also reject an IP
address that has been reassigned to another phone before the run begins.
For a previously authorized TCP connection, reconnect with
`devenv shell -- adb connect ADDRESS:5555`. Rebooting the phone can require
re-enabling TCP debugging over USB.

The production app and instrumentation packages are `lol.cutout` and
`lol.cutout.test`. Installation updates only those packages, preserves
their app data. Each storage test removes its own temporary storage after closing
native handles. Uninstall the instrumentation package explicitly with ADB when
no longer needed. The earlier test-only
package is not used or modified by the production runner.

The native GATT queue is bounded and checks the original attempt before every
submission. Subscription failure, rejected operations, overflow or missing
callbacks retire that GATT connection; an untagged late callback cannot advance
another request after timeout. A native API submission is distinct from device
readback. Queued cancellation, partial ATT submission and full submission retain
different receipts. ATT chunks preserve the original payload and respect the
negotiated limit, capped at 512 bytes. The Rust-owned next-wakeup deadline drives
session polling; there is no Kotlin polling interval or model-specific protocol
loop. Permission revocation and explicit disconnect retire the native connection
and invalidate its Rust attempt. Configuration submissions retain the attempt
that opened the editor.

The first Android app slice does not claim reliable locked/background riding
(LIBCU-35) or live recording/export (LIBCU-36). Production-adapter replay and
runtime smoke are not live PEV acceptance: record the exact wheel/profile/firmware
when hardware becomes available. Android framework contracts are documented in
the [GATT API](https://developer.android.com/reference/android/bluetooth/BluetoothGatt)
and [permission guide](https://developer.android.com/develop/connectivity/bluetooth/bt-permissions).

Devenv provisions API 35, Build Tools 35.0.0 and the existing NDK, accepting the
SDK licenses for this composition. Its `CUTOUT_ANDROID_API`,
`CUTOUT_ANDROID_BUILD_TOOLS` and `CUTOUT_JNA_VERSION` feed the Gradle build;
`devenv.lock` owns the JDK, Gradle, Kotlin host compiler and SDK package pin.
Android Gradle and Kotlin plugins are pinned in the Android build file. The
JNA Android AAR supplies its native dispatcher. Default builds package only
`arm64-v8a`; explicit emulator builds package only `x86_64`. Linux x86_64 devenv
also provisions the API 35 Google APIs emulator image and emulator binary.

The runner serializes its own invocations, uses one Cargo compiler job and at
most two Gradle workers, runs Kotlin compilation within the capped Gradle JVM,
and disables persistent Gradle daemons. Rust uses the existing `target`;
Android intermediates and native-library symlinks live in `target/android-tests`.
Generated bindings remain under ignored `target/uniffi-smoke`. Gradle reuses its
shared dependency cache and keeps project metadata in
`target/android-tests/gradle-cache`; local UI tests use one
768 MiB worker. Do not run this concurrently with another
Cutout native build or Simulator window.

Each run retains logs and `receipt.json` in
`target/test-logs/mobile-ffi-MODE-TIMESTAMP`. Device runs also produce JUnit XML,
raw instrumentation output and at most 200 logcat lines per reported test PID.
Receipts distinguish build, install and test outcomes and include the source
revision/diff, tool versions, device identity, APK hashes and packaged native
library hashes. An ADB exit code of zero alone does not pass instrumentation:
all advertised tests must finish with explicit outcomes.

The Android FFI CI workflow runs host smoke, local native-view tests and ARM64
APK builds serially on an ARM64 Mac runner. Physical-device tests remain opt-in. Android cross-builds
remain outside `project:quality-gate` under LIBCU-860. Remove obsolete run logs
or `target/android-tests` only when no runner owns its lock; retain the relevant
receipts and reuse the shared Cargo/Gradle caches rather than creating new ones.

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

## Locked-ride native tests

Run the native location-owner, Main-actor SQL backpressure/lifetime, and marker
receipt tests on an ARM64 iOS Simulator with the supported shared FFI wrapper:

```console
devenv tasks run test:ios-locked-ride
```

`CUTOUT_IOS_TEST_DESTINATION` selects an explicit simulator; the default is the
latest iPhone 18 Pro. The task runs the existing `PhoneLocationAdapterTests`,
`LockedRideMainThreadTests`, and `RideSessionMarkerNonblockingTests` classes in
`CutoutAppIOSUnitTests`. The Xcode target references their Swift package source
files directly. The shared SQLite fixture stays in `LockedRideMainThreadTests`;
there is no copied fixture or platform-specific test implementation. The hosted
test scheme disables application startup and runs without parallel tests, so
the process-owned database has one test owner at a time. ARM64 is explicit.

These tests exercise the iOS Core Location branch and the Swift async admission
boundary under real SQLite contention. Simulator results do not prove a locked
physical phone's file-protection behavior or that iOS will keep a riding process
alive; those require separate device evidence.

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
The signed-device deployment build waits for its native child so verification
and installation can follow. That child inherits the same FFI lock, keeping its
generation protected if the coordinator exits. Offline subprocess tests exercise
the waited helper's lock inheritance, normal return, and failed-start cleanup.

Existing configuration is recorded here for code review, not as a run recipe:

| Switch | Current behavior / limitation |
| --- | --- |
| `CUTOUT_AERO_SETTINGS_TEST` | Value `1` is rejected before Bluetooth starts |
| `CUTOUT_AERO_INCLUDE_HEADLIGHT`, `CUTOUT_AERO_INCLUDE_AUDIBLE`, `CUTOUT_AERO_INCLUDE_ALARM_MODES`, `CUTOUT_AERO_INCLUDE_TRIP_RESET` | Value `1` is rejected, even without settings mode |
| `CUTOUT_AERO_ALLOW_UNRESTORABLE_WRITES` | Value `1` is rejected; unknown baselines never authorize generated probes |
| `CUTOUT_AERO_TARGET`, `CUTOUT_AERO_VALIDATION_TIMEOUT` | Select target and connection timeout (default 45 seconds, finite, greater than 0 and at most 600); neither establishes settings-page coverage or a per-operation deadline |

Disabling the settings suite does not make connection-only discovery proven
harmless. The Rust detector now avoids starting Begode N/V/M probes after
Veteran, VESC or conflict evidence resolves the protocol, and retires the
corresponding pending Rust probe state. A recorded NF2557 connection emitted
those probes and flushed queued bytes after Veteran identity resolved; typed
native cancellation for discovery probes is still not implemented. FFE0/FFE1 is
shared, so probe eligibility while multiple protocols remain plausible still
needs a safe plan.
“Read-only” describes intended semantic operations, not an absence of transport
writes.

The removed descriptor inventory ran before settings telemetry and exited once
ride telemetry was live. Its `current=nil` values do not show lack of device
readback. Bounds and typed completion strategies came from library declarations, not
wheel negotiation. The Rust readback boundary now carries the selected protocol
on normalized observations, but that tag is not independent packet provenance
and does not turn a matching observation into an acknowledgment. The replacement
observation mode must run for a bounded relevant page cycle and report actual
fields/pages received separately.

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
## R3 Pro camera validation on iPhone

The R3 Pro creates the local network that owns `192.168.1.254`. The Mac does
not need to join that network: build and install `CutoutApp` on the iPhone
first (USB is sufficient), then join the camera Wi-Fi from the iPhone and
open the Camera route. Grant local-network access when iOS asks. The route
will remain read-only until the phone reports a usable Wi-Fi path, then it can
load the R3 Pro firmware/configuration/media evidence and start or save the
RTSP preview.

For the hardware pass, retain the firmware/configuration response log, one
successful RTSP preview file (or the app's saved preview), and one downloaded
media file with its reported size. Keep those artifacts separate from the
offline simulator/UI evidence: this is the proof that the phone reached the
camera network, not proof that the Mac was connected to it.

The Mac command builds the iPhone app for Apple Silicon Mac and opens it. The
Mac build, device deployment, and ad-hoc archive check the built app's name,
Bluetooth usage description, device family, and supported orientations before
continuing. These checks live in `cutout-dev` and are covered by its Rust tests.
To check an existing Debug or Release bundle directly, use
`devenv shell -- cargo cutout ios verify-app /path/to/CutoutApp.app`.

The phone deployment task builds, verifies, and installs on a connected device
using the `ios-device-deploy` SecretSpec scope. A normal deployment does not
launch the app, so install and launch remain separate checks; installation can
complete while the phone is locked. Configure the task's
`CUTOUT_IOS_DEVELOPMENT_TEAM` and `CUTOUT_SPOTIFY_CLIENT_ID` in the OS keychain
with `devenv shell -- secretspec set --profile development NAME --provider keyring`
(`NAME` is one variable at a time); certificates and
provisioning profiles remain managed by Xcode. `CUTOUT_IOS_APP_BUNDLE_ID` is
forwarded when set. To explicitly launch after installation, use
`secretspec run --scope ios-device-deploy --profile development -- cargo cutout ios deploy --launch`;
an accepted launch command does not establish visible UI while the phone is
locked. For custom launch arguments, pass them
after `--launch --`; for example, append `-- --launch-smoke` to
`cargo cutout ios deploy --launch`. The existing
`cargo cutout ios deploy -- --launch-smoke` form also explicitly requests
launch. The project does not commit personal signing values.
The physical iOS UI test runner resolves the same scope before building and
checks that the app embeds the configured Spotify client ID.

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

### Headless Linux Android acceptance

On an x86_64 Linux host with readable/writable `/dev/kvm`, run the production
app and JNI instrumentation in a project-owned API 35 Google APIs emulator:

```console
devenv tasks run test:mobile-ffi-runner
devenv tasks run test:android-emulator
```

The Linux devenv provisions the emulator and system image. The runner builds
`x86_64-linux-android`, packages only the matching `x86_64` JNI libraries, and
creates its AVD under ignored `target/android-tests/avd`. It uses KVM, two guest
cores, 2 GiB RAM, software rendering, no window/audio/cameras and no snapshots.
It does not use the desktop display or gaming session. It owns the emulator
process handle, validates its explicit adb serial, AVD name, API and ABI before
installation, and stops that process in a `finally` block, including on failure.
The physical-device modes retain their ARM64/non-emulator guards.

The default emulator selection runs `MobileFfiTest` and `ProductionAppTest`:
real Kotlin/JNA/Rust calls, public capture replay, stale callbacks, SQLite
reopen, production launch/navigation and Ride rendering. It then launches the
production activity and retains its rendered screenshot. This establishes
emulator/JNI acceptance, not physical Bluetooth, camera or whole-app parity.
`test:android-ui` remains the separate ARM64 macOS Robolectric entrypoint.

For an already-built artifact, use the shared runner directly within devenv:

```console
devenv shell -- python3 scripts/test-mobile-ffi.py build --abi x86_64
devenv shell -- python3 scripts/test-mobile-ffi.py emulator --artifacts target/test-logs/mobile-ffi-build-TIMESTAMP/receipt.json
```

Reuse checks the source-content identity, requested ABI and both APK hashes.
The receipt paths are relative to the checkout so verified artifacts can be
transferred together with their receipt to matching source without rebuilding.
Physical `device`/`app` modes also accept `--artifacts`, but require ARM64 proof.
The default `build` mode still produces the physical ARM64 APKs. An emulator
build does not replace or validate those artifacts. A later build can overwrite
the APK output paths; changed artifacts fail receipt validation.

Unset physical target selectors when using emulator mode. `--test-class` can
select a specific instrumentation class or comma-separated list; the receipt
records the exact selection. Each run retains source identity, ABI, APK/library
hashes, emulator image/command/target, strict instrumentation results, raw logs,
JUnit and a screenshot when capture is available. A crash, incomplete run,
skipped result or zero-test run fails. Inspect the retained log before retrying.

The emulator command follows the pinned tool help and the Android
[command-line](https://developer.android.com/studio/run/emulator-commandline)
and [acceleration](https://developer.android.com/studio/run/emulator-acceleration)
contracts. Newer tooling may expose other launch interfaces; the runner uses
the executable declared by this checkout.
