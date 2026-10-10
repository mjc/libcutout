# SoundCloud feasibility

LIBCU-894 requires Rust-owned integration policy and thin iOS effects. Kotlin
work is blocked by the user. Playback controls must not depend on network
access. This record separates control of the installed SoundCloud app from a
possible native player owned by CutOut.

## Public iOS integration

Apple's [system music player](https://developer.apple.com/documentation/mediaplayer/mpmusicplayercontroller/systemmusicplayer)
controls the Music app. It does not supply SoundCloud state.
[Remote command events](https://developer.apple.com/documentation/mediaplayer/remote-command-center-events)
let an app receive accessory/system commands for its own player. Registering a
handler is not a way to send play, pause, or skip to another app.

The SoundCloud [API guide](https://developers.soundcloud.com/docs/api/guide)
describes playback in an embedded widget or a custom player using stream URLs.
The [Widget API](https://developers.soundcloud.com/docs/api/html5-widget)
controls an embedded widget; it is not a remote for the installed SoundCloud app.
Neither provides the local app-control interface used by Spotify App Remote.
This is the conclusion from the published interfaces inspected, not proof that
SoundCloud has no private or partner-specific interface.

The [public API specification](https://developers.soundcloud.com/docs/api/explorer/api.json)
was downloaded on 2026-10-10. Its SHA-256 was
`196cb3a31cd01a804c46a0ece0c276a4145cbbfb651fb2b33a51c84afff6ea65`.
It contains no remote now-playing, play, pause, or skip endpoint.
`GET /me/recently-played/tracks` returns at most 25 tracks in reverse chronological
order, omits duplicates, and returns track objects without listening timestamps.
That list cannot establish the live player or correlate listening to a ride clock.

An explicit app-opening effect is the current candidate for the public iOS
integration. It must not claim playback started, authorize an account implicitly,
poll a Web API, or record a synthetic song. The local `soundcloud://` app-opening
URL still needs physical-device validation. If iOS cannot open it, report failure
rather than claim SoundCloud connected or launch a network fallback.

## Native-client spike

A native client would own playback instead of controlling the SoundCloud app.
Its play, pause, and skip operations could act on its own local player, but that
does not establish that new audio is available offline or that downloads are
permitted. The current [music scope contract](music-integration.md) excludes
provider streaming and audio capture. A native-client spike needs a separate
scope decision before adding streaming behavior.

The API guide documents PKCE authorization and requires a confidential client
secret for token exchange and renewal. A mobile binary must not contain that
secret. The spike must establish developer access and a supported token-exchange
arrangement before promising sign-in. It must also verify provider requirements
for attribution, permitted playback, caching, local metadata, and ride history.

## Device evidence

On 2026-10-10, Xcode 27.0 detected the paired physical iPhone 15 Pro Max
(`00008130-000C60E021F2001C`). A read-only
`devicectl device info apps --include-all-apps --search SoundCloud` query reported
`com.soundcloud.TouchApp`, version 8.81.0, build 1261949.

The signed app and UI-test runner built on that device target. The physical
test attempt on 2026-10-10 failed before product assertions: Xcode could not
establish communication with `CutoutAppUITests-Runner`. The result is retained
at `target/xcode-ui-tests/TestResults/run.7Rwv8o/Result.xcresult`, with the build
log at `target/test-logs/soundcloud-physical-ui.log`. A subsequent device query
reported a local-network connection and `passcodeRequired: true`.

The second physical attempt reached product assertions but stopped at bootstrap
with `UnsupportedSchemaVersion`, before music settings or handoff. Its result is
`target/xcode-ui-tests/TestResults/run.MZ1h0u/Result.xcresult`, with log
`target/test-logs/soundcloud-physical-ui-retry.log`. No phone database was reset,
deleted, or copied.

The user then restricted acceptance to Simulator and prohibited further phone
interaction during software verification. Simulator acceptance covers selection,
unavailable capabilities, missing-app failure, foreground return, and cold
selection restoration. The user subsequently authorized phone deployment after
splitting the changes into signed commits and pushing the branch. Physical
handoff, offline dispatch, and concurrent BLE/ride behavior remain unverified
at publication. Android acceptance is deferred while Kotlin work is blocked.

## Software verification

The implementation adds no HTTP, OAuth, streaming, or provider polling path.
Rust admits only `OpenProvider`, projects an unavailable snapshot with no item
or playback position, and rejects SoundCloud listening events and capture
identifiers. Swift executes the admitted `soundcloud://` effect through UIKit
and the shared transport completion/cancellation executor. A successful callback
means iOS accepted the handoff, never that audio is playing.

The branch was rebased onto local main at
`19dc43ce2d01cae6aab417a40d1a4a7f921ddabd` on 2026-10-10; `origin/main`
matched that commit. Three Swift autostash conflicts were resolved while keeping
main's music observation admission, Stop, and history privacy behavior.

The rebased Rust workspace passed 2,338 nextest tests and its doctests, including
839 tests in the affected packages. Two subprocess fixtures were intentionally
skipped by nextest; their parent WAL recovery regression and dashboard input gate
exercise them. The run used `NEXTEST_TEST_THREADS=2`. Repository Clippy passed
with warnings denied. The supported Swift package task passed 636 library,
5 validator, and 460 app tests. The supported Simulator app build passed.
The iOS music task passed 17 tests, including the new SoundCloud restoration
regression; the capture runtime task passed 7 tests. Both ran without skips.

The dashboard input gate, Rust lint policy, dependency checks, shell regressions,
FFI freshness and production fixtures, and camera probe self-tests passed.
The production fixture restored its temporary Core source change; the final
Core diff was empty. The final formatting check examined 382 files and changed
none. All non-Kotlin quality-gate checks passed; Kotlin remains blocked.

Simulator UI acceptance passed one test without skips on iPhone 18 Pro,
iOS 27.0, build 24A434, simulator `24E3DCB9-342A-4B71-AF18-A8FE18A01DEA`.
It verified SoundCloud selection, unavailable controls/history, the real
missing-app failure alert, selection after foreground return, and cold launch
restoration. The first run assumed the Music sheet stayed open after returning
from Home; the captured hierarchy showed Choose device. The corrected test
reopens Music when needed and verifies the saved selection. Production code was
unchanged by that test correction.

The successful result is
`target/xcode-ui-tests/TestResults/run.unUoPk/Result.xcresult`; its readback is
`target/test-logs/soundcloud-simulator-summary.json`. Rust and Swift logs are
`rust-workspace-20261010T125012-94355.log` and
`swift-package-20261010T125421-99026.log` under `target/test-logs`. Repository
checks use the `soundcloud-rebased-*.log` files in that directory.

An earlier default-concurrency run before the rebase failed an existing BLE
exact-millisecond assertion. The focused test and bounded workspace run passed;
[LIBCU-897](https://lific.mjc.lol/LIBCU/issues/LIBCU-897) tracks deterministic
timing assertions. These software results do not establish successful handoff
to an installed SoundCloud app or physical offline acceptance.

## Follow-up tickets

[LIBCU-895](https://lific.mjc.lol/LIBCU/issues/LIBCU-895) tracks a native-client spike.
Candidate libraries include [emilsharkov/soundcloud-rs](https://github.com/emilsharkov/soundcloud-rs),
[maxjoehnk/soundcloud-rs](https://github.com/maxjoehnk/soundcloud-rs),
[Riva](https://github.com/resonix-dev/riva), and
[rsoundcloud](https://docs.rs/rsoundcloud/0.2.6/rsoundcloud/).
These are API or media-fetching clients, not proof of a local remote-control
interface for the installed SoundCloud app. None is a dependency of this handoff.

[LIBCU-896](https://lific.mjc.lol/LIBCU/issues/LIBCU-896) tracks a dated review of
Apple Music, Spotify, and SoundCloud requirements for metadata saved with rides,
including retention, artwork, attribution, export, consent, and deletion. Local
storage alone does not establish provider compliance.
