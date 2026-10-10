# Locked ride resource audit — 2026-10-09

Tracker: [LIBCU-891](https://lific.mjc.lol/LIBCU/issues/LIBCU-891). Checkout: `/Users/mjc/projects/libcutout`, shared working tree on mika-m1, native ARM64 Rust 1.99 through the matching Devenv environment. Android and phone access are excluded. The final supported Swift and ARM64 Simulator gates run after Rust inputs are frozen.

## Evidence and attribution

The user reports app shutdown while riding with Cutout in the foreground before locking the phone. Code review identifies avoidable work and unsafe backlog boundaries. This report does not attribute the latest shutdown to any one path without a corresponding termination report from that installed binary.

Historical main-app evidence is recorded in [LIBCU-711](https://lific.mjc.lol/LIBCU/issues/LIBCU-711): September 12 CPU_FATAL, 48 CPU seconds over about 58 seconds, 82% average. The October 8 Live Activity extension report shows 88% CPU with no action taken. These are different processes and older binaries. This change has no new device termination report or sustained locked-ride measurement.

## Historical LIBCU-891 integration status

This section records the completed LIBCU-891 snapshot before the subsequent LIBCU-892/893 repairs. Current combined-checkout receipts supersede these counts in [LIBCU-DOC-15](https://lific.mjc.lol/LIBCU/pages/157). At that snapshot, the final Rust workspace gate passed: 2,281 tests, two configured skips, all doctests, strict Clippy, Rust formatting and repository lint policy. The supported ARM64 Swift package gate passed 1,039 tests across 615 mobile, five validator and 419 App tests, with one configured mobile skip and zero failures. The final supported ARM64 Simulator build passed. Independent semantic reviews are complete and all findings are resolved. All software acceptance for LIBCU-891 is complete. This is source and build validation; no device was installed or inspected.

## Native presentation — scope 1

Rust connection generations fence phase/settings publication. Repeated `.live` telemetry does not republish identical phase or rebuild unchanged settings. A real phase change, settings readback or new generation still publishes. Recording, alarms and protocol effects continue for every applicable notification.

The display bridge holds one replaceable snapshot before dispatch to the main queue. One native timer delivers the latest snapshot at the foreground cadence; an already queued timer event must recheck its current deadline. Background delivery is limited to half the Rust stale window for ActivityKit. The app caches this platform snapshot outside Observation and leaves visible device state unchanged while inactive. Foreground activation publishes current retained state. Route snapshots and non-error decisions use scene-gated latest-value mailboxes. Durability errors use a separate always-live mailbox with their original Rust ride/generation context; a subsequent successful decision cannot erase the pending failure. Error admission reads the authoritative runtime snapshot, including an authoritative absent ride after discard. Acquisition notifications remain independent and live while locked. Diagnostic messages remain in the bounded log but no longer enqueue a main callback when there is no observer.

Settings and BMS also retain one pending presentation before Main dispatch. Required phase callbacks acquire a native delivery hold atomically with replacing the settings payload, retain no settings DTO themselves, and release the hold through `defer`, including rejected generations. Settings delivery follows those phase callbacks. Foreground entry leaves both mailboxes paused until the BLE owner submits current state, then resumes them under the latest scene flag. This avoids publishing an old retained value immediately before the fresh catch-up. This hold is native dispatch ownership; Rust still decides semantic phases, settings and generations.

Coalescing can replace an Accepted route decision with a later Ignored decision. `LiveRideModel` therefore invalidates projected geometry when the Rust ride identity or durable point count changes, including foreground duration catch-up. Unchanged geometry keeps the existing explicit-clock duration behavior. Scene activation never fabricates a new GPS receipt time. Its native Core handoff is asynchronous and fenced by the latest scene flag, so a stalled BLE queue does not synchronously block scene entry.

## ActivityKit admission and lifecycle — scope 3

Rust owns a constant-space work queue: one in-flight Apple operation, one latest replaceable presentation and a fixed terminal obligation. Superseding presentation cannot erase stop/end cleanup. Native lifecycle reductions and required recording flush/checkpoint execute independently of Apple manager completion. A permanently suspended manager must not create an unbounded waiter array or prevent Rust disconnect, background flush or stop.

Request creation and coordinator arrival can occur in different orders. Replaceable telemetry does not authorize discarding a background checkpoint or explicit stop for the same ride. Rust uses separate constant-space admission fences for those obligations. A late background event behind a newer foreground event performs the checkpoint without reverting presence. Terminal admission checks both platform-start intent and the actual Rust-issued UUID boundary, so a delayed old stop cannot end a replacement ride on the same wheel.

While startup is pending, replacing the payload for the same wheel preserves the original start-intent boundary and disconnect fence. Realizing that intent with a UUID from a later payload still fulfills the original intent. Only a distinct platform intent or a genuinely new session UUID advances the boundary. A terminal request clears pending intent. The regression covers a pending wheel behind stalled orphan cleanup, same-wheel payload replacement, delayed end/disconnect and eventual UUID creation.

Rust preserves the session UUID while Apple start is pending, accepts the matching acknowledgement through reconnect/stale transitions, and fences startup recovery against every live phase. A new telemetry receipt may renew an equal-content Apple presentation after half the stale window; wall time, foreground entry and repeated old telemetry cannot renew freshness. Background/end lifecycle retains stale presentation and terminal identity.

Background entry before the first start or recovery records presence even while the lifecycle is Idle and flushes the supplied capture without requiring a fabricated session UUID. Start and recovery preserve that ambient presence. Idle recovery retains the persisted marker; explicit stop establishes an Ended tombstone, clears the marker and invalidates older queued startup work. Orphan Apple cleanup acquires terminal work ownership even before a Rust identity exists. These decisions are typed Rust admission outcomes, with Swift routing the native flush and Apple effect selected by Rust.

Transport loss has its own source-clock fence. A newer display request carrying an older telemetry receipt cannot erase a real disconnect. Conversely, an already-admitted newer actual telemetry receipt fences an older delayed transport-loss event. Reconnecting, Ending and Ended never acquire fresh telemetry merely through scene projection.

Native presentation carries an immutable telemetry expiry across optional Apple work. The enqueue monotonic anchor is captured before the App's task hop; actor-entry delay and a stalled earlier Apple operation both consume the original freshness window. At dispatch, the existing Rust telemetry/freshness inputs classify the payload. Apple start/adoption/update receive the same absolute expiry, including after sibling-activity cleanup. Scene projection can use a newer already-admitted telemetry receipt, but cannot turn a reconnect or terminal transition into fresh telemetry.

At the original LIBCU-891 baseline, identical session markers still entered the SQLite worker and synchronously awaited its reply. The SQL equality predicate avoided disk mutation, not worker admission; a prolonged storage stall could block the coordinator before later lifecycle messages entered. The subsequent LIBCU-617/893 marker repair replaces that admission with Rust-owned desired/in-flight/durable/latest-pending intent and one asynchronous settlement owner. Identical durable intent produces no worker command, and queued intent does not claim durability. Current marker ordering, failure and restart receipts are recorded in [LIBCU-DOC-15](https://lific.mjc.lol/LIBCU/pages/157). Broader synchronous Core pairing/manual/reconnect entry remains LIBCU-617.

Before Rust opens the database, native code repairs lock-safe protection on existing main, WAL and SHM files; newly created sidecars are checked after open. Failure prevents opening against an existing unprotected sidecar. Tests instrument metadata ordering and retained bytes using a controlled opener. They establish native ordering, not an iPhone lock test.

## Demand-driven work — scope 4

The ride-map recorder no longer polls completion every 100 ms. Rust admission receipts wait for their actual worker result; errors and acknowledgements remain explicit. BLE scheduling uses one timer at Rust's next protocol, authorization or readable-setting confirmation deadline. Native subscription/backpressure suppresses protocol polling but must not suppress command expiry. A passive connected decoder has no idle wakeup; VESC polling remains 100 ms when required by its protocol. Settings presentation stops while inactive and catches up when activated. Capture progress and ride duration display timers stop while inactive; durability and acquisition continue.

## Bounded recording ingress — scope 5

Rust reserves weighted capacity before native dispatch across GPS, connection and BMS producers. The shared budget is 64 observations. A larger single callback remains whole, owns the budget and excludes other producers until released. Capacity survives restoration. FIFO ownership releases after success, failure or cancellation. Source/receipt clocks, ride association and generation are captured before capacity waits; no material callback is silently discarded. A native admission lock preserves reservation order through dispatch. Empty BMS batches do not queue work.

The actual storage-stall fixture holds an external SQLite `BEGIN IMMEDIATE`, queues a write-only worker command, then retains GPS behind that worker. It fills 64 slots, proves the 65th producer waits, releases storage, and verifies all 65 accepted points durably in order. This distinguishes bounded native ownership from a synthetic queue limit. A direct route read-to-write upgrade under an external writer lock can return typed SQLite BUSY immediately; this change does not invent an implicit retry policy.

The initial LIBCU-891 baseline delivered Core Location on Main and left a concrete capacity-stall hazard. The subsequent [LIBCU-892](https://lific.mjc.lol/LIBCU/issues/LIBCU-892) repair creates the manager on a dedicated active-run-loop owner and keeps lossless admission there. Native tests now hold the actual SQLite worker, fill all 64 recording slots, block the 65th callback on the owner and prove Main progress; releasing the worker retains all 65 exact points. Permission/demand/thread ownership and signal-only teardown tests also pass. The final combined completion gates and source findings are recorded in [LIBCU-DOC-15](https://lific.mjc.lol/LIBCU/pages/157). [LIBCU-617](https://lific.mjc.lol/LIBCU/issues/LIBCU-617) retains the broader nonblocking callback contract, including synchronous BLE-queue entry. Physical locked-ride CPU/memory/termination acceptance remains [LIBCU-711](https://lific.mjc.lol/LIBCU/issues/LIBCU-711).

## Capture completion and explicit export — scope 2

The native disconnect path calls `finishCaptureAfterLinkDown` before reconnect. Its recorder calls Rust `finishWriterAndPublishCapture`. Previously that operation completed the SQLite capture, streamed the entire JSONL export, and re-imported the file for saved-history publication. The database already held each admitted event and structured observations. None of that export/re-import was necessary to preserve the recording.

Rust now selects database-only completion in this native terminal operation. The closed metadata and completion state commit durably, and the terminal result has `DatabaseFinished` plus `NotAttempted` export. Export failure is separate from primary-storage failure. A private-field `SavedDatabaseCapture` receipt carries the original writer artifact identity, database capture identity and exact final header. Foreign DTOs and parsed database identifiers cannot construct completion authority.

Recording provenance is retained on the existing `live_capture_sessions` row in nullable `recording_context_json`; no new table or content-addressed duplicate is required. Schema 39 adds the nullable column without scanning event payloads, copying capture data or inventing provenance for old sessions. Identical publication retries retain the original identity/provenance; conflicting identity or origin is rejected. Invalid publication time leaves the completed database capture visible and retryable.

`list_live_capture_history` queries bounded inactive session rows, including interrupted sessions and provenance-publication failures. Stable descending pagination uses recording start and capture identity. History does not decode event payloads. `BluetoothCapturesView` now reads this Rust history, including after relaunch, with explicit pagination. Its detail screen requests export and then exposes the synced file through `ShareLink`. The native model handles display state and off-main calls; Rust owns history, integrity and export policy. A refresh during a pending query retains one refresh obligation. Repeated requests coalesce; success, failure and pagination completion all trigger a fresh first-page query so an older result cannot permanently hide the newly completed recording.

Explicit export reads bounded pages of exact retained JSONL header/event bytes and computes SHA-256 in the same streaming pass. It never overwrites an existing output. Failure removes a newly created partial output and leaves the database recording intact. Interrupted and incomplete sessions can export their retained evidence; export does not promote their integrity to complete. Existing explicit writer/file export APIs retain their behavior. Existing opted-in music metadata remains subject to its recording policy; no audio is introduced.

Files:

- `crates/libcutout-persistence/src/capture_writer.rs`: database-only finish, typed receipt, streamed export.
- `crates/libcutout-persistence/src/storage/live_capture_history.rs`: provenance and paginated session history.
- `crates/libcutout-persistence/src/storage/{live_capture,migrations,worker}.rs`: session update, schema 39 and worker commands.
- `crates/cutout-mobile-ffi/src/{lib,capture_history}.rs`: native terminal policy and history/export DTOs.
- `swift/CutoutMobile/Apps/CutoutApp/{CaptureFeatureModel,BluetoothCapturesView}.swift`: thin history/export presentation.

## Durable transaction reduction — scope 6

The old terminal barrier committed the closed header, then committed the finished state in a second FULL/WAL transaction. The external terminal receipt arrived only after both. The writer now sends one command that updates header, stored-byte accounting, state, finish time, integrity and drop count atomically in one transaction. Per-event transactions, FULL synchronization, raw events, structured observations and acknowledgement boundaries are preserved. A failed terminal-state update rolls back label closure as well as completion state.

Provenance publication remains a separate small idempotent row write. Its failure must not turn a durable capture into a failed recording. Folding provenance into terminal completion would require preserving that separate failure outcome and the consumed-writer authority boundary.

Two apparent write targets require care:

- Repeating 100 unchanged header flushes already produced zero WAL bytes before the optimization. SQLite avoids dirty-page writes for that unchanged row; do not claim a fixed disk-write amplifier here.
- A newer source-clock observation at the same coordinates is a durable clock/duration checkpoint. Existing `location_observation_checkpoint_preserves_point_and_advances_exact_clock_pair` and backwards-wall-clock fixtures require the exact clock pair while retaining one route point. Suppressing or delaying that checkpoint changes acknowledged recovery state. No clock-only debounce or weaker durability was introduced.

## Reproducible measurements

Fixture: `capture_writer::database_completion_tests::database_only_finalization_avoids_export_bytes_with_identical_deferred_contents`, 1,024 identical admitted observations per policy, queue flushed every 32 observations. Both exports are independently hashed and compared byte-for-byte. Only terminal completion is timed with `Instant`; input generation and ingest occur before that timer. Deferred export runs afterward for exact-content verification.

Run the already-built native test executable, avoiding compilation inside the measurement:

```sh
/usr/bin/time -l target/debug/deps/libcutout_persistence-06e423215767451f \
  --exact capture_writer::database_completion_tests::database_only_finalization_avoids_export_bytes_with_identical_deferred_contents \
  --nocapture
/usr/bin/time -l target/debug/deps/libcutout_persistence-06e423215767451f \
  --exact capture_writer::database_completion_tests::atomic_terminal_update_writes_one_wal_frame_instead_of_two \
  --nocapture
```

The executable fingerprint changes after rebuilding; select the current executable containing these exact test names. Three repetitions were run for each fixture. Retained extracted samples: `target/locked-ride-audit/capture-measurements-2026-10-09.txt`; original full command receipt: session 15596. The target artifact is local and ignored; measured values and method are retained here.

| Repetition | Explicit completion | Database-only completion | Explicit finalization JSONL writes | Database-only finalization JSONL writes |
| --- | ---: | ---: | ---: | ---: |
| 1 | 550,853 µs | 221 µs | 515,241 bytes | 0 bytes |
| 2 | 258,759 µs | 211 µs | 515,241 bytes | 0 bytes |
| 3 | 222,794 µs | 222 µs | 515,241 bytes | 0 bytes |
| Median | 258,759 µs | 221 µs | 515,241 bytes | 0 bytes |

All six verified exports had SHA-256 `064fceae1795648baeba2f3ce17f75ec4987ce7ae9b5ec3dd492b199aff69589` and exactly 515,241 bytes. This is a small native debug fixture on a busy desktop, not an iPhone throughput or energy benchmark.

The atomic terminal fixture reproduces the previous header-then-finish command sequence and compares the new combined update. All three repetitions retained the same closed header, finished state, exact finish time and complete integrity. With 4,096-byte SQLite pages, prior finalization produced 8,272 WAL bytes / 2 frames; the new update produced 4,152 WAL bytes / 1 frame. This measures OS-visible WAL bytes and transaction frames, not physical flash writes.

`/usr/bin/time -l` full-process measurements include ingest, both policies and later deferred-export validation. Repetitions used 0.80/0.21, 0.71/0.17 and 0.72/0.16 seconds user/system CPU; peak RSS was 12,009,472, 11,747,328 and 11,616,256 bytes. These combined values cannot establish a per-policy CPU percentage reduction. APFS block input/output counters were zero despite actual file writes and are not evidence of zero physical I/O.

## Characterization and validation

Commands ran directly from the matching Devenv root.

| Proof | Result |
| --- | --- |
| Scope 2 behavioral RED | Current completion created a compulsory 32,900-byte file instead of retaining the database alone. Nextest `cfd80326-ad7b-429f-85b6-d1e6233333c4`. |
| Scope 6 behavioral RED | Denying the terminal state update left closed labels durably written in an active session. Nextest `cd1b89df-2f00-4faa-9aa8-b458e9546125`. |
| Focused completion/migration GREEN | 12/12 passed, `054c3f88-8b3b-46cc-98f1-83aca9683043`. |
| Capture FFI + entire persistence suite | 337/337 passed, `38725144-8242-4c7d-b1d9-5211c32588dd`, before the final typed-row/style refactor. The central final gate must include the final tree. |
| Built-binary measurement fixtures | Three exact-data/finalization repetitions and three atomic-WAL repetitions passed; session 15596. |
| Native integration behavioral RED | `target/test-logs/swift-package-20261009T162624-45764.log`: inactive Core/App publication, native burst delivery/cancellation, idle transport wakeups, blocked ActivityKit lifecycle effects, equal-content renewal and database protection ordering failed against the prior native implementation. |
| Final integration behavioral RED | `target/test-logs/swift-package-20261009T164550-58174.log`: blocked history refresh after success/failure/pagination, hidden GPS catch-up, coalesced route geometry and immutable ActivityKit age regressions failed. Compiler-only preparation failures are not behavior receipts. |
| Intermediate native verification | `target/test-logs/swift-package-20261009T165006-60447.log`: 599 mobile, 5 validator and 419 app tests executed; one configured mobile skip. The app and validator suites passed; the connection-admission receipt fixture timed out. This is not the final GREEN. Further review also found time omitted before actor admission. |
| Lifecycle ordering behavioral RED | `target/test-logs/swift-package-20261009T165557-64143.log`: pre-actor elapsed time, newer telemetry retained through scene replacement, delayed background flush and delayed explicit stop failed against the prior admission/age behavior. The recorder fixture still failed after explicit owner lifetime, ruling out that initial hypothesis; its shared debug database retained another vehicle and Rust correctly rejected the identity mismatch. |
| Queue and startup behavioral RED | `target/test-logs/swift-package-20261009T170645-70714.log`: 100 distinct BMS inputs produced 100 presentation callbacks; 50 subscribing/live cycles produced 100 settings callbacks. Startup background entry lost capture flush and presence before start/recovery; newer UI requests could discard an actual disconnect despite carrying older telemetry. A private recorder fixture database violated the existing single-path persistence invariant; the final fixture uses the persisted vehicle candidate from the shared database and captures every receipt. |
| Pending-start intent RED/GREEN | `target/locked-ride/activity-intent-rust-red.log`: same-wheel replacement incorrectly advanced the session boundary and rejected the valid earlier terminal/disconnect. `target/locked-ride/activity-intent-rust-green.log`: 31/31 focused lifecycle/work-queue tests pass after separating pending platform intent from replaceable presentation revision. |
| Native boundary verification | `target/test-logs/swift-package-20261009T172436-81864.log`: 615 mobile tests passed with one configured skip; five validator tests passed. Two older App reconnect fixtures failed three assertions. Independent review confirmed fixture receipt omissions: the transient fixture supplied `TelemetrySnapshot.at` but omitted `RideDisplayState.lastUpdate`, so the actual receipt and spy loss clock were both zero; the recovery fixture supplied no new display receipt at all. Corrected fixtures use coherent spy clocks and actual receipt times, explicitly retain Reconnecting through phase-only input, then require Active with the original UUID and one Apple start after new telemetry. This run is not full GREEN. |
| Recovered-fixture handoff RED | `target/test-logs/swift-package-20261009T173111-84752.log`: the only remaining failure was the new restored-ride assertion reading Active before asynchronous transport loss settled. Independent review predicted this race. The initial identity waiter now requires settled Reconnecting with the same UUID, marker and one Apple start before testing phase-only input. No production condition or final Active assertion was weakened. |
| Final ARM64 Swift GREEN | `target/test-logs/swift-package-20261009T173310-86073.log`: 615 mobile, five validator and 419 App tests executed; one configured mobile skip, zero failures. All scoped native regression tests and corrected reconnect fixtures pass. The unused recorder Task-result warning was fixed by explicitly discarding the existing task handle; existing unrelated warnings are outside this change. |

Final focused command:

```sh
cargo nextest run --locked -p libcutout-persistence -p cutout-mobile-ffi \
  -E 'package(libcutout-persistence) | test(capture)' \
  --no-fail-fast --status-level fail --final-status-level fail
```

Fixtures cover exact multi-page data/digest, closed labels, publication retries/conflicts, publication failure without history loss, active-source export rejection, stable pagination, interrupted/incomplete integrity, output collision preservation, corrupted-payload failure without partial files, no-event-scan schema migration, terminal rollback and old/new WAL frames. Prior payload-encoding migration tests compare the original session fields explicitly, because schema 39 adds a nullable metadata column.

Queue/resource characterization is deterministic rather than a phone energy claim. The Rust ActivityKit fixture submits 2,000 presentation observations behind one stalled operation and retains only the latest pending request; a fixed terminal obligation runs before that presentation. The storage fixture saturates the shared 64-observation budget and proves the next producer waits without losing its eventual point. The native passive-transport regression observed two idle reads in 250 ms before the change and requires zero afterward. Presentation burst tests retain one payload before crossing the main queue. These bounds constrain pending ownership; they do not measure physical-device RSS or CPU consumption.

## Historical LIBCU-891 integration acceptance

- [x] All six scopes documented with strict RED/GREEN receipts and measured/deterministic resource evidence.
- [x] Independent semantic/queue/deadline review complete with findings resolved. Final API and pending-intent findings were corrected and reviewed again before current-tree gates.
- [x] Current-tree native ARM64 full workspace tests and doctests: 2,281 passed, two configured skips; nextest `788c244d-919b-4b71-87b0-aa9ad5b03f11`, `target/test-logs/rust-workspace-20261009T172159-78089.log`.
- [x] Current-tree strict Clippy, repository lint and Rust formatting gates: `cargo fmt --all -- --check`; `devenv tasks run project:lint`; `devenv tasks run project:test`; `devenv tasks run test:rust-lint-policy`. Logs: `target/locked-ride/activity-project-lint-final.log` and `target/locked-ride/{fmt,rust,lint-policy}-final.log`. The prior `clippy-final.log` records the resolved RED, not final success. Four narrow owned-record UniFFI boundary allowances address `needless_pass_by_value`; internal policy is unchanged.
- [x] Supported Swift package gate through the shared Rust FFI ensure boundary: `devenv tasks run test:swift-package`, 1,039 tests, one configured skip, zero failures. Full log `target/test-logs/swift-package-20261009T173310-86073.log`.
- [x] Final supported ARM64 Simulator app build: `devenv tasks run build:ios-app`, exit 0, retained command session 12597 and task receipt `target/locked-ride/ios-completion.log`. Devenv returned `{}` on success; the outer receipt does not contain detailed Xcode compiler output. The executable was independently inspected below. History/export model, persistence and exact-data integration tests pass; no interactive UI acceptance was performed in this scope.
- [x] Final `git diff --check` and scoped Swift formatting pass. Accumulated changes from the same conversation remain intact; no commit, push or deployment was made. The previous classification as unrelated work was incorrect. LIBCU-893 inventories and verifies the combined checkout.
- [x] Final report saved as [LIBCU-DOC-14](https://lific.mjc.lol/LIBCU/pages/156); LIBCU-891 marked done with [verification comment 10542](https://lific.mjc.lol/LIBCU/issues/LIBCU-891#comment-10542) and status read back. Physical and nonblocking-storage follow-ups remain separate.

Built executable: `/Users/mjc/Library/Developer/Xcode/DerivedData/CutoutApp-brykajlutxolrgcuklngkuihcdzg/Build/Products/Debug-iphonesimulator/CutoutApp.app/CutoutApp`, modified October 9 at 17:36:35 MDT. DerivedData `info.plist` identifies this checkout's `swift/CutoutMobile/CutoutApp.xcodeproj`. `file` and `lipo -archs` both report ARM64 only; `xcrun vtool -show-build` reports `IOSSIMULATOR`, minimum OS/SDK 27.0. Executable SHA-256: `772708b9f3bca3ae1f27dae88dd10f0e12ffc18325b2583084159899f49f241a`.

No physical locked-ride acceptance was run in this scope. Keep that separate from source invariants and native/simulator validation. Exact-device acceptance and any installed-binary termination attribution remain under the relevant physical validation ticket.
