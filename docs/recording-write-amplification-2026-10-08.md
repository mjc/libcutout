# Recording write amplification: October 8, 2026

Tracker: [LIBCU-883](https://lific.mjc.lol/LIBCU/issues/LIBCU-883), supporting
[LIBCU-711](https://lific.mjc.lol/LIBCU/issues/LIBCU-711) and
[LIBCU-713](https://lific.mjc.lol/LIBCU/issues/LIBCU-713).

## Finding and change

The live SQLite capture path commits each event separately. A BLE event updates
capture counters and inserts the serialized payload, structured transport facts,
raw telemetry fields, and a semantic snapshot. Accepted GPS route points commit
in separate transactions. The current path does not rewrite the accumulated
capture on each event.

The database worker previously left SQLite's default DELETE rollback journal in
place. Repeated commits journal and overwrite frequently changed pages. The
worker now checks that WAL was enabled and explicitly selects `synchronous=FULL`
after schema verification. Each successful commit still syncs; this change adds
no buffering window and does not use WAL's weaker NORMAL commit policy. SQLite's
normal 1,000-page autocheckpoint remains enabled.

Telemetry-driven Live Activity reconciliation also repeatedly saved the same
ride-identity marker. The marker UPSERT now updates only when the bytes differ.
A real SQLite regression verifies no changes for identical bytes, an update for
new bytes, and propagation of SQL failures.

On iOS, the dedicated database directory is excluded from backup before opening
SQLite, covering WAL/SHM files created later. The existing database file exclusion
and `completeUntilFirstUserAuthentication` protection remain. Setup now also
reapplies that protection to pre-existing directories before opening SQLite,
since directory creation does not update an existing directory's attributes. Production backups
use `VACUUM INTO`; benchmark snapshots use SQLite backup. An external live database
copy must include committed WAL contents consistently or use a SQLite snapshot.
Copying only `ride.sqlite` while the app is running is insufficient.

## Host measurement

Host: ARM64 macOS, repository Devenv, Rust 1.99.0 (`b940084d7`), bundled SQLite
3.50.2, 4,096-byte pages. Debug executable, fresh temporary database per run.
Three interleaved before/after runs per scenario, 3,600 items each. Medians:

| Workload | Before process writes | After process writes | Reduction | Before elapsed | After elapsed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Route | 392,335,360 B | 143,392,768 B | 63.45% | 1.867 s | 0.684 s |
| Raw capture | 591,691,776 B | 199,639,040 B | 66.26% | 2.295 s | 0.800 s |
| Structured BLE capture | 2,321,154,048 B | 736,268,288 B | 68.28% | 6.494 s | 2.826 s |

Route samples move 400 E7 latitude units per second. Checks require every sample
to be admitted, the exact point and segment counts, and 4,448 rounded millimetres
per edge. Capture samples contain a 160-byte inbound payload, eight integer and
four float raw fields, and one small semantic snapshot. Raw capture stores the
same JSON payload without normalized BLE rows. Checks require exact row counts,
contiguous sequences, and matching payload-byte accounting. The initial BLE
fixture incorrectly requested 16 integer fields; the sparse decoder retains
eight. It was corrected to explicitly request and check eight before measurement.

Timestamps model 1 Hz GPS and 10 Hz BLE, but execution is accelerated. There is
no wall-time pacing or capture finalization in the measurement. The interval
starts after database/scenario setup and ends after workload correctness checks;
it includes worker requests, normal autocheckpoints, and verification reads,
and excludes compilation, shutdown, cleanup, and JSONL export.

The sampler reads Darwin `proc_pid_rusage` flavor 2 immediately before READY is
released and after DONE. `ri_diskio_byteswritten` measures process disk-I/O
accounting across the SQLite worker, not physical flash bytes. Final database
size and `CaptureWriterStatus.physical_bytes_written` are not substitutes: the
latter excludes SQLite and journal traffic. PRAGMAs printed by the harness's
read-only observer include connection-local values; the separate configuration
regression checks FULL on the actual configured connection.

Baseline executable SHA-256:
`11b2bd2169f946a9afed12d311afd19eb7714fa3b026bf7e15d7f4d1844aabcf`.
Measured after executable SHA-256:
`942a8ce07d3148462b7ca003861dc4e4d0b3a921a67bd457eb187a8cd3623c4b`.
A later lint-only change borrows the JSON boundary value instead of passing it
by value; it does not change the workload or production storage policy.
Raw runs and aggregate results are retained under `target/write-amplification/`.

Reproduce the current workload from the repository Devenv:

```sh
cargo build --locked -p libcutout-persistence --example write_amplification
python3 scripts/measure-ride-writes.py target/debug/examples/write_amplification route 3600
python3 scripts/measure-ride-writes.py target/debug/examples/write_amplification raw-capture 3600
python3 scripts/measure-ride-writes.py target/debug/examples/write_amplification ble-capture 3600
```

The sampler intentionally requires ARM64 macOS. The Rust harness uses the
production configuration and does not override durability or journal policy.

## Recovery and evidence limits

The WAL configuration regression was red with the old DELETE configuration.
The process-exit regression commits one event, opens a later uncommitted mutation,
and exits without dropping SQLite. Reopening retains exactly the committed event,
discards the mutation, passes integrity checking, and creates a `VACUUM INTO`
backup containing that event. This checks process termination recovery and backup
snapshot semantics; it is not a physical power-loss test.

The copied October 8 phone report records 17.18 GB of file-backed memory dirtied
in 3,132 seconds, average 5,486.04 KB/s, with **Action taken: none**. Its app UUID
is `173B57E5-2DB3-3842-BAED-357B77AA882A`; no matching symbols were retained.
The structured fixture's old configuration corresponds to approximately 6.45 MB
of process writes per modeled second at 10 Hz. That is consistent in scale with
the phone report, but the fixture is neither a replay of that ride nor a source
attribution for the unsymbolicated report.

WAL reduces amplification substantially; it does not eliminate capture write
volume. The corrected fixture still writes about 205 KB per BLE event. Canonical
observation storage and redundant managed capture representations remain tracked
by LIBCU-713 and its existing ingestion/migration work. No retained observations,
raw evidence, formats, or existing captures were deleted in this repair.

The user's locked-ride shutdown remains open until a matching current diagnostic
or physical locked-ride acceptance establishes the remaining behavior. The
nonfatal disk-write report alone cannot establish why iOS terminated an app.

## Validation

- Rust WAL configuration: observed red against DELETE, then green after the fix.
- Subprocess recovery/backup: two focused tests pass; the ignored helper is
  invoked only by its parent regression, where it exits without dropping SQLite.
- Marker idempotence: observed red (two SQLite changes instead of one), then green.
- Persistence package: 199 tests pass, one subprocess helper skipped.
- Full Rust workspace: 2,111 tests pass, two skipped; all workspace doctests pass.
- Strict Clippy: affected package and full workspace, all targets/features,
  `-D warnings`, pass.
- Rust format and diff whitespace checks pass.
- ARM64 Swift package: 975 tests, one skipped, zero failures. The Foundation
  directory-exclusion test passes. Log:
  `target/test-logs/swift-package-20261008T213355-77846.log`.
- Native ARM64 iOS filesystem and replacement-vector suite: all seven tests
  passed, zero failures; the test host exited with code 0. Xcode then stalled
  collecting asynchronous diagnostics for more than five minutes. The task was
  interrupted with exit 130 after preserving assertion and session logs; this
  is not a successful full-command exit. Logs:
  `target/write-amplification/ios-runtime-final-{assertions,session}.log`.
  Simulator does not expose `NSFileProtectionKey` on the database or its sidecars;
  the physical-only protection assertion remains unverified. Simulator checks
  still require directory backup exclusion and actual database/WAL/SHM creation.
  An obsolete optional media-list assertion was corrected to unwrap and require
  a present empty list. Failed protection readback attempts are retained in
  `target/write-amplification/ios-runtime-tests-output.log`.
- Signed ARM64 device build succeeded. The deployment task then failed with
  a CoreDevice connection error during installation. After refreshing device
  details, installing the same completed app with `devicectl` succeeded. Launch
  succeeded, and subsequent process readback confirms PID 47217 running from the
  installed bundle container `E58DB703-5AC1-46D1-B979-5031415A638C`. App readback
  confirms `lol.cutout.app`, version 1.0/build 1. Locked-ride acceptance remains
  separate. Evidence: `target/write-amplification/{install-retry,launch,
  deployed-app,deployed-process}.json`; signed build log: `deploy.log`.
- Device executable UUID: `13E22B61-9A90-369A-8A07-BDE52E454684`.
  SHA-256: `302503c045807f61e0370a3457c46798773fc591004f224aa4737aa72d2a0c45`.

Rust logs: `target/write-amplification/{wal-red,wal-green,package-tests,clippy}.log`,
`target/test-logs/libcutout-persistence-marker-{red,green}.log`, and
`target/test-logs/rust-workspace-20261008T213304-76111.log`.

## Further reduction: unused capture indexes (LIBCU-884)

The WAL/FULL repair above is installed on the phone. The follow-up is tracked
in [LIBCU-884](https://lific.mjc.lol/LIBCU/issues/LIBCU-884) and is also installed.
It removes five secondary indexes that have no current production read path:

- `live_capture_events_receipt_order`
- `live_capture_events_source_wall_clock`
- `live_capture_ble_characteristic`
- `live_capture_ble_raw_telemetry_field_id`
- `live_capture_ble_semantic_telemetry_time`

Production capture paging filters by capture identity and sequence, orders by
sequence, and joins location observations on the same composite key. Retained
primary keys cover that path and the foreign keys. No production query uses
`INDEXED BY`, a characteristic lookup, a field-ID lookup, or a semantic-time
lookup on these tables. Sem tracing and an independent agent review agree.

The field-ID-first index maintains one entry per raw field, spanning separate
field ranges for each notification. The other indexes each maintain one entry
per applicable event. Dropping them removes derived lookup structures, not
source rows or stored facts. It does not change raw bytes, decoded observations,
location precision, serialization, ordering, admission limits, or WAL/FULL commit
policy. The schema 36 migration drops indexes in one transaction and
does not rebuild data tables or run `VACUUM`. Freed pages can be reused; the main
database file need not shrink immediately.

Three interleaved runs per workload, 3,600 events per run, use the same host,
harness, fixture, and process counter as above. This follow-up compares the
WAL/FULL baseline against WAL/FULL without the five indexes:

| Workload | WAL/FULL before | Without unused indexes | Additional reduction |
| --- | ---: | ---: | ---: |
| Route | 143,392,768 B | 143,343,616 B | 0.03% (effectively unchanged) |
| Raw capture | 199,643,136 B | 157,638,656 B | 21.04% |
| Structured BLE capture | 736,268,288 B | 286,113,792 B | 61.14% |

The BLE fixture now accounts for about 79.5 KB of process writes per event,
compared with about 204.5 KB under WAL/FULL with the indexes. Against the earlier
DELETE baseline the cumulative reduction is 87.67%. That cumulative comparison
spans the two separately retained measurement rounds. These remain accelerated
ARM64 macOS process counters, not phone I/O or flash-write measurements. Elapsed
samples varied during concurrent focused test compilation; no latency improvement
is claimed from this round.

All before/after runs retain identical verified counts, sequences, and serialized
payload-byte accounting. Fresh database growth for structured BLE falls from
26,423,296 B to 22,831,104 B because the removed indexes no longer occupy pages.
Retained payload bytes remain 5,296,976 B, with 3,600 events, 3,600 BLE rows,
43,200 raw-field rows, and 3,600 semantic snapshots.

Baseline executable SHA-256:
`47151eea5c805185983ed2faab00c3e74af083cb92b7e452fb80c17fcc36d276`.
After executable SHA-256:
`0938d1e72dfa176791ebfcc8d83cedb6397ae9ca218425b569c9da9317e6263c`.
Raw runs, medians, and the sequential measurement driver are retained in
`target/write-amplification/index-pass/` (`results.json`, `run.py`, and
`{before,after}-{scenario}-3600-r{1,2,3}.log`). Pre-edit source snapshots and both
executables are retained there as well.

Focused tests were observed red against the indexed schema, then green (three
passed). They compare every ordered SQLite value across six capture tables,
including raw packet bytes, integer extrema, raw float bits, unknown semantic
JSON, location source bits, and session metadata. Both schema 34 and 35 migrate
and reopen without changing these values. PK/FK/CHECK/cascade behavior and
integrity checks pass. Query plans retain primary-key paging and location joins
without a temporary sort. An authorizer denial on the second index drop rolls
back the first drop, preserves all five indexes and every data row, retains
version 35, and allows a successful retry after the denial is removed.

An independent agent reviewed production queries, all migration entry paths,
and the final patch with no blockers. Validation for this follow-up:

- Persistence package, all features: 202 tests pass, one subprocess fixture skipped;
  both package doctests pass.
- Full native Rust workspace: 2,114 tests pass, two skipped; all doctests pass.
  Raw log: `target/test-logs/rust-workspace-20261008T220259-16903.log`.
- Full workspace strict all-target/all-feature Clippy passes. An initial lint run
  rejected four empty-vector assertions; these were changed to equality assertions
  with failure values, then all three focused migration tests and Clippy passed.
- Format and diff whitespace checks pass.
- Source is frozen. Signed ARM64 phone build and installation succeeded. The
  deployment task then failed only at launch because the phone had locked.
  App readback confirms the installed bundle container
  `31E37924-FED0-498B-8B80-834E8C6BE04D`, `lol.cutout.app`, version 1.0/build 1.
  After the user unlocked the phone, launch succeeded. Subsequent process
  readback confirms PID 47369 running from that installed bundle.
  This verifies install and launch, not a locked ride or a direct readback of
  the phone's database schema. New executable
  UUID: `8AE52D86-4C24-34E9-9745-8C79B7199EA8`.
  SHA-256: `c3fd603e8b7cc4d5465902cad38a8824792ab2dc4d8c228a6a46019c86fed181`.
  This follow-up does not change Swift source or the FFI API; the prior Swift and
  native filesystem checks above remain evidence for the prior repair, rather
  than fresh Swift-test results for this revision.

Logs are in `target/write-amplification/index-pass/`: `package-tests.log`,
`package-doctests.log`, `migration-{red,green,final}.log`,
`workspace-clippy-final.log`, `format.log`, and `deploy.log`.
No Android work has been performed.

## Third reduction: lossless event payload encoding (LIBCU-885)

The next reduction changes the physical representation of newly appended capture
event payloads. The original serialized bytes remain the public read and export
contract. JSON is not parsed and reserialized for this optimization. Transport
bytes, typed BLE fields, semantic snapshots, location values and raw float bits
remain in their existing tables. Admission limits and session `stored_bytes`
continue to count original bytes. Each event still commits with WAL/FULL.

Schema 37 adds `payload_encoding` (0 = original bytes, 1 = zlib) and
`payload_original_bytes` (null for original bytes, bounded original length for
zlib). Existing rows remain encoding 0; migration does not rewrite or recompress
them. New payloads use zlib level 1 only when the encoded payload is smaller.
The decoder checks metadata before allocating, allocates at most 65,536 output
bytes, verifies the zlib checksum, requires exact output length and complete
input consumption, and rejects trailing bytes or concatenated streams. Invalid
stored payloads return an error rather than silently producing partial output.

The codec is `miniz_oxide` 0.9.1 with its allocation feature and the single
mandatory dependency `adler2` 2.0.1. Both are pure Rust and forbid unsafe code;
neither uses native build scripts or platform APIs. Their MIT/Apache license
alternatives satisfy repository policy. Neither package declares an MSRV;
compilation with the repository's current stable Rust is the compatibility
evidence. No Android build is part of this work.

The migration adds columns without CHECK constraints, then installs metadata
validation triggers for inserts and payload updates. This avoids scanning the
existing event table during ADD COLUMN. SQLite scans all preexisting rows when
adding a CHECK constraint; column additions without that constraint modify only
schema text. See [SQLite ALTER TABLE](https://www.sqlite.org/lang_altertable.html).
The existing payload-size, primary-key and foreign-key constraints remain.

Before selecting compression, temporary production-path probes measured these
alternatives with identical observations:

| Alternative | BLE write reduction | Cost or limitation |
| --- | ---: | --- |
| 512-byte SQLite pages | 53.36% | New files only without a full rewrite; existing WAL databases cannot change page size directly; large-payload read latency unmeasured |
| Ordinary rowid event table | 9.47% | Requires copying existing events and rebuilding table constraints |
| 8,192-page WAL checkpoint threshold | 6.57% | Larger checkpoint/recovery bursts; noisy route elapsed median increased |
| Lossless zlib level 1 payloads | 17.63% | Codec CPU cost; applies to new events without copying old events |

The compression probe includes a 5-byte temporary envelope per event, final
FULL/TRUNCATE WAL checkpoint, and byte-exact readback of every sequence. Three
interleaved pairs per scenario measured BLE writes 291,790,848 to 240,336,896 B;
raw writes 163,479,552 to 110,440,448 B (32.44%). Original event bytes totaled
5,296,976 B and compressed bytes including envelopes totaled 1,415,907 B.
Debug codec call wall time was about 121 microseconds to encode and 61
microseconds to decode per event. These are exploratory results; production
schema columns and actual append/read integration require separate measurements.

Temporary artifacts are under `target/write-amplification/{page-pass,
checkpoint-pass,compression-pass}/`. Production before/after measurements use
the strengthened example harness, which includes a final FULL/TRUNCATE checkpoint
and verifies every public read payload byte against its original fixture. Setup,
capture finalization and shutdown remain outside the sampled interval. Host
process I/O accounting remains distinct from phone writes and physical flash.

The actual schema 37 implementation measured three interleaved before/after
pairs per scenario, reversing order on the middle repeat:

| Workload, 3,600 observations | Schema 36 median writes | Schema 37 median writes | Additional reduction |
| --- | ---: | ---: | ---: |
| Structured BLE | 286,461,952 B | 240,705,536 B | 15.97% |
| Raw capture | 158,150,656 B | 110,379,008 B | 30.21% |
| GPS route | 143,429,632 B | 143,466,496 B | Effectively unchanged (+0.026%) |

Every capture run verified all 3,600 original payloads through bounded public
read pages, as well as sequence bounds, logical accounting, projection counts,
and a completed checkpoint with zero WAL bytes remaining. Original payloads
total 5,296,976 B on both revisions; schema 37 stores 1,397,907 B (73.61% less).
Final BLE database size falls from 23,224,320 to 8,282,112 B (64.34% less);
raw database size falls from 17,141,760 to 2,199,552 B. Existing database files
do not shrink from this change because old rows remain untouched.

Debug workload-and-checkpoint elapsed medians were BLE 1.962 to 2.493 seconds,
raw 0.910 to 1.178 seconds, and route 0.738 to 0.710 seconds. Public readback
is included in the process-I/O sample but follows the separately reported
elapsed interval. Compilation overlapped some runs, so these are not isolated
latency benchmarks. No CPU or latency improvement is claimed.

Before executable SHA-256:
`30a3acd79daff255ff3bfa95d67be85c775b7c3602f21ff3a43d84d110cf6ee1`.
After executable SHA-256:
`81af249265bdc86e226bd3854885ee425167c43f76f32e57f359d130c50884ae`.
Production medians, individual samples and driver are in
`target/write-amplification/compression-pass/production-results.json`,
`production-{before,after}-{scenario}-r{1,2,3}.log`, and `production-run.py`.

Focused tests were observed red before codec/migration behavior existed, then
green. The initial added-CHECK migration also failed a no-scan regression before
being replaced with trigger validation. Final coverage includes:

- Exact original bytes for compressed, incompressible, tiny and legacy raw
  events; maximum 65,536-byte input, oversize rejection and mixed-row reopen.
- Unknown codecs, invalid metadata, checksum failures, every truncated prefix,
  trailing bytes, concatenated streams and decompression exceeding declared size.
- Original logical admission limits and unchanged counters/sequences on rejection.
- Public read errors on damaged stored content; no silent skipped observations.
- All 501 compressed location events exported byte-exactly across read pages.
- Corruption in event 32 (after the first 32-event export page) causes export
  failure while retaining 33 events, 33 location rows, logical counters and the
  finished session. Only the partial JSONL artifact is removed. The first page
  remains readable and reading the corrupt page returns an error.
- Exact original values across all six capture tables for populated schema
  versions 31–36, including payload bytes, unknown JSON and raw float/source bits.
  Metadata defaults remain raw through upgrade and reopen.
- Second-ALTER denial rolls back both column additions, schema/index changes and
  data. Migration passes while event-table and SQLite quick-check reads are denied.
- Identical INSERT and UPDATE metadata enforcement in fresh and upgraded files.

Independent codec, migration and export-path reviews found no blockers. The
corrupt-export regression was added after review identified that coverage gap.
Persistence all-feature nextest passes 215 tests, with one subprocess fixture
skipped. The mobile invalid-location/export regression also passes, preserving
the original nonfinite float and timestamp representation in exported JSONL.
Repository dependency policy passes; its existing advisory-ignore warnings were
retained. Only `miniz_oxide` and `adler2` were added by this follow-up; preexisting
dirty dependency changes remain intact.

Two enlarged test/harness functions initially failed strict Clippy's line limit.
Their assertions were extracted into helpers; no lint suppression was added.
Final format and whitespace checks pass. The final native workspace run passes
2,127 tests with two skips, and every workspace doctest passes. Full workspace
strict all-target/all-feature Clippy also passes on the frozen source. Raw test
log: `target/test-logs/rust-workspace-20261008T223347-75163.log`.
This follow-up changes neither Swift source nor the public FFI API. Existing
Swift/native checks for the earlier repair remain separate evidence; they were
not rerun as new Swift-test results for the codec change. Signed-device build,
installation and launch verification follow below when complete.

The signed ARM64 iOS build succeeded and the update is installed on the paired
iPhone 15 Pro Max (`00008130-000C60E021F2001C`). Installed app readback confirms
`lol.cutout.app`, version 1.0/build 1, in new bundle container
`2BF867D2-799E-4543-AED6-0B9C087DA8E9`.
The deployment task returned failure only when iOS denied launching the locked
phone; build and installation had completed. After the user unlocked the phone,
launch succeeded. Subsequent process readback confirms PID 47806 running from
the verified new bundle container. Executable UUID:
`633AC090-6B6D-3E1B-8FAB-FD5807FDAD3D` (ARM64).
SHA-256: `caab32d98f310b797d04d0c94739e1e131a6d8b664efd03d2c2993559356c453`.
No direct phone database-schema readback or locked-ride acceptance is claimed.
Deployment and app readback logs: `compression-pass/deploy.log`,
`deployed-app.json`, `deployed-app.log`, `launch.json`, `deployed-process.json`,
and `verified-identity.json` within the evidence directory above.

## Fourth reduction: material observation admission (LIBCU-886)

This changes admission for future automatic database captures. Every receipt
still reaches decoding, alarms, estimators and live freshness. A clock advancing
alone can avoid a new value record; any material field change remains immediately
durable under WAL/FULL. Existing rides and captures are not rewritten.

`CaptureRecordingPolicy::{EveryObservation, MaterialChanges}` is fixed before
writer start. Manual captures and file diagnostics retain every observation.
Automatic database captures use MaterialChanges, with the immutable header
annotation `capture_recording_policy=material_changes`. Its absence means
EveryObservation. Caller annotations cannot override policy. Header admission
reserves capacity for both the policy marker and active label closures; invalid
updates fail before enqueue and leave the writer usable.

Stationary suppression requires a current verified connected wheel, known
reported speed observed at this exact receipt, the shared signed 500 mm/s
movement threshold, one recognized complete telemetry frame, and decoder
buffers empty before and after this notification. Unknown, missing, stale,
inferred or estimated speed cannot establish stationary admission. Other
protocol adapters currently retain every observation. Fragments, concatenated
frames, writes, replies, link changes and music occurrences retain evidence and
break the comparison baseline. Bluetooth loss never pauses or ends the ride.

The material comparison includes exact original transport bytes, raw typed
values, provenance, presence, contract versions and unknown JSON fields. Only
the documented outer and semantic observation clocks are excluded. Unknown
JSON values use RawValue, preserving large integers and decimal lexemes without
f64 rounding. Duplicate root or known-clock wrapper keys reject suppression.
Malformed clocks retain the observation. Original retained payloads are stored
and exported byte-exactly. Omitted clock-only repeats are intentionally not a
lossless raw packet trace.

PreparedMaterialRecord and CommittedMaterialRecord separate comparison from
durability. SQLite success is the only promotion point. Failed appends cannot
advance the baseline or hide a retry. Swift forwards the same original receipt
timestamp used by Rust decoding; resampling later would invalidate fresh-speed
proof and silently disable suppression.

Route admission uses Record(AdmittedLocationSample),
ObserveClockOnly(ClockOnlyLocationObservation), and Rejected(LocationAdmission).
Capabilities capture immutable native coordinates, accuracy, source, telemetry
context, segment, start reason, nonzero recording identity and active interval.
They cannot be replayed, transferred across rides or reinterpreted at settlement.
Pending writes settle in FIFO order; an unavailable reply retains the entire
suffix. Settlement measures from the actual durable predecessor, so failure of
the first point after a gap cannot add distance across that gap. Pending points
disable duplicate suppression, preserving material A→B→A and identical retries.

Clock-only GPS still commits a single ride-row update for observed-through
continuity. Schema 38 adds paired nullable GPS monotonic and wall clocks without
scanning or rewriting old rows. A partial or malformed pair is an error.
RideLocationObservationCheckpoint validates the active lifecycle, exact expected
point sequence and native location, and monotonic order inside the transaction.
Its result is Applied, AlreadyObserved or PointChanged. Exact replay writes
nothing. Generic lifecycle/music timestamps cannot manufacture GPS coverage.
Restoration uses these dedicated clocks, falling back only to the retained point
for older rows. A checkpoint failure leaves the durable baseline retryable while
a valid new live GPS receipt still updates displayed speed; replay cannot extend
freshness or replace that speed.

Telemetry receipt metadata writes once initially and thereafter at explicit
checkpoint/lifecycle boundaries. Dirty state is derived from live receipt versus
persisted receipt, rather than a separate boolean. Lifecycle operations carry the
latest receipt into their existing queued transaction. Recovery uses the latest
durable epoch anchor without treating historical telemetry as live.

Review also found an existing atomicity defect: a constraint violation during
location insertion was converted to Duplicate, allowing partial transaction
effects to commit. Real SQL errors now propagate and roll back the full append;
true duplicates are admitted explicitly before insertion. A trigger-induced
failure demonstrated the defect before the repair.

Three interleaved paired ARM64 Mac measurements used the same native debug
binary, 3,600 stationary timestamp-only receipts, production queue/writer APIs,
WAL/FULL, the same 32-event flush cadence, capture finalization, JSONL export and
final FULL/TRUNCATE checkpoint. Each run verified exact retained database and
export bytes and checkpoint [0,0,0].

| Measurement | EveryObservation | MaterialChanges |
| --- | ---: | ---: |
| Process-accounted writes, each of three runs | 253,014,016 B | 106,496 B |
| Retained observations | 3,600 | 1 |
| Export bytes | 5,820,816 B | 2,105 B |
| Final database size | 8,966,144 B | 299,008 B |
| Median elapsed through final checkpoint | 2.904174 s | 0.186754 s |

The process-accounted write reduction is 99.9579% for this preclassified
stationary writer fixture. It is not a moving-ride estimate or phone flash
measurement. Setup and shutdown are excluded. Elapsed ends before exact readback;
the process-I/O sample includes readback. Stationary GPS checkpoint writes remain
and are not represented by this capture-only percentage.

Artifacts: `target/write-amplification/material-policy-{every,material}-{1,2,3}.log`,
`material-policy-benchmark-summary.json` and `material-policy-benchmark-hashes.txt`.
Measured executable SHA-256:
`9eccee5b9e87e7ab086e388d31a46b1f865ab94993e061f693ca9d8acdbca8d1`.

Behavioral red/green tests cover numeric precision, duplicate keys, annotation
capacity, forged stationary evidence, persistence failure/retry, rollback,
capability ownership/replay, queued immutable context, failed boundary settlement,
GPS continuity and crash recovery. Independent writer, JSON, migration, framing
and route reviews are clean. Selected writer/FFI tests passed 44/44; route/core/
recording/FFI tests passed 84/84; GPS/migration/atomicity tests passed 24/24.
Final workspace and Apple validation are recorded below when complete.
+Control measurements used an extended harness binary and one paired run each.
All 3,600 records in every control were read from the database and exported
byte-exactly, with identical original record payload totals under both policies.

| Control, 3,600 observations | EveryObservation writes | MaterialChanges writes | Retained in both |
| --- | ---: | ---: | ---: |
| Parked with changed raw/typed/semantic voltage | 252,002,304 B | 252,080,128 B | 3,600 |
| Moving at 2,000 mm/s, full observation admission | 252,960,768 B | 252,960,768 B | 3,600 |
| Repeated diagnostic observation, full admission | 253,014,016 B | 253,063,168 B | 3,600 |

The policy marker adds 43 bytes to each MaterialChanges export header; record
payloads are unchanged. These single pairs establish retention controls, not
latency or percentage claims. All checkpoints completed [0,0,0]. Artifacts:
`material-policy-{parked-changes,moving,diagnostic}-{every,material}.log`,
`material-policy-controls-summary.json` and
`material-policy-controls-hashes.txt`. Control executable SHA-256:
`a02cbb64166d522994a1f0438794b4466f3811f3eae98e09981cf3439008b582`.

Final native workspace validation passes 2,189 tests with two configured skips
and all workspace doctests. Raw log:
`target/test-logs/rust-workspace-20261009T010012-6409.log`.
Full strict workspace all-target/all-feature Clippy with warnings denied, Rust
formatting, whitespace checks, the repository rust-lint-policy task and changed
Swift formatting all pass. The supported ARM64 Swift package check passes:
977 tests executed, one configured skip, zero failures. Raw log:
`target/test-logs/swift-package-20261009T010732-14446.log`.
The paging fixture uses 4,097 distinct nativeE7 coordinates; separate Swift
boundary coverage verifies forty seconds of static observations preserve ride
duration and segment continuity without extra points. The original receipt and
Rust evidence forwarding regression also passes. The final ARM64 Simulator app
build passes through `devenv tasks run build:ios-app`, exit 0. Product readback
confirms a Mach-O ARM64 executable, UUID `F69DE559-25E6-323B-B3D7-4BD3FCA97124`,
SHA-256 `279d90988c0d1588400a1feb1c1eeae8e98adcacf18ac242f09659b205d75a0a`.
Selected FFI generation:
`09afa2817d7c6a73ed94ab5499ed8354e44c289415f399639d128314b4dad633`.
Build identity evidence:
`target/write-amplification/record-on-change/ios-build-identity.json`.
Signed phone deployment subsequently completed on October 9 through
`devenv tasks run deploy:ios-device`, exit 0. Device session establishment resolved
the disconnected tunnel reported at initial preflight. Installed app readback
confirms `lol.cutout.app` in bundle container
`69E20E4C-E31B-4F80-B158-4E2A019A4479`; process readback confirms Cutout PID 50240
and Live Activity extension PID 50236 from that same bundle. The deployed ARM64
build UUID is `7F48B437-2EB6-337E-9BBA-21ACCDC14B2B`; local product SHA-256:
`aa7e91ddf5a69561837fc15d6bb68d6dae168bdbb1ec778c1df5c50b7d2c8c8b`.
The selected FFI generation matches the tested source generation above.
Evidence under `target/write-amplification/record-on-change/`:
`deploy-final.log`, `device-session-details.json`, `deployed-app-final.json`,
`deployed-process-final.json` and `deployed-build-identity.json`.
Physical locked-ride/CPU-budget acceptance remains open in LIBCU-711; installation
and launch do not prove sustained ride behavior. No Android work was performed.

## Recording lifecycle invariants (LIBCU-888, LIBCU-889, LIBCU-890)

This follow-up changes failure and completion ownership, not the material
comparison or its write-reduction measurements. It follows the network deployment
of LIBCU-886 recorded above.

[LIBCU-888](https://lific.mjc.lol/LIBCU/issues/LIBCU-888) separates healthy operation,
known admission loss and fatal worker failure. Rejecting a flush control message
does not poison the worker or mark retained data incomplete. Rejecting an actual
observation reports AdmissionLost with the request's exact dropped count; later
admissions and flushes can still succeed. The first fatal cause is retained.
SQLite finalization returns its durable DatabaseFinished receipt with Incomplete
integrity and NotAttempted export when observations were lost. File-only captures
with known loss refuse a saved-artifact receipt. The public mobile callback's
oversized batch guard now records that loss through the same ingress accounting
without converting rejected samples. It previously silently returned Rejected,
allowing a misleading Complete receipt.

Rust classifies flush and health receipts through the session owner. A healthy
control rejection returns Rejected; a successful admitted barrier returns
Flushed even after known observation loss. An accepted barrier with a lost reply
or a storage error returns Failed and preserves its first fatal cause. Native
code forwards the typed outcome into the save token instead of inferring writer
failure from status counters or a presentation string. Progress retains the
dropped count, fatal flag and diagnostic message separately. The existing Bluetooth
queued-write receipt gate remains: if that specific queued capture receipt was
not admitted, the native write is rejected before submission. This is distinct
from stopping the recording; a later accepted receipt can proceed. Annotation
requests likewise do not claim success after queue rejection.

[LIBCU-889](https://lific.mjc.lol/LIBCU/issues/LIBCU-889) keeps terminal receipt
ownership in Rust as CurrentAttempt, HistoricalAttempt and Rejected. Replaced Finalizing
attempts remain owned until their terminal receipt is consumed. Historical
completion publishes once without changing the current recording. Unknown,
premature and duplicate receipts are rejected. The owner retains pending
identities only; it does not retain an unbounded history of completed generations.
The FFI accepts the complete terminal writer result and returns PublishArtifact,
PublishDatabase, PublishFailure or Rejected. Swift forwards the raw prior write-admission result too; it no longer decides
whether a SQLite or file receipt counts as successful. Nonterminal NotStarted/Finalizing
results cannot consume a pending terminal receipt. Optional history-index
publication failure retains a valid file; an incomplete SQLite capture retains
its explicit integrity and export result.

Capture startup is a Rust-prepared, one-shot request. Native inputs are one raw
wall-clock observation, platform identity and source, canonical advertised UUID
bytes, filesystem directory, UUID filename nonce, requested annotations, origin,
and music-history policy. Rust normalizes the date, builds the filename, formats
mandatory metadata, sanitizes annotations, reserves active label closures and
rejects truncation or conflicting reserved metadata. Preparation performs no
file/database mutation. Swift reads the monotonic clock only after preparation
is admitted, then installs the writer returned by Rust. Reusing a prepared
request fails even if its first actual writer start failed. Stale optional
now-playing context does not prevent a valid capture.

Resolved identity and its optional evidence/detail now use one Rust metadata
admission. Capacity rejection or queue loss leaves the prior identity and
annotations intact. This replaces the native adapter's three independent
setter/admission operations. Protocol-confirmed model names, protocol families
and verification labels are mapped in Rust. Date and annotation helper bodies
in Swift delegate to the same Rust normalization used by startup. Native responsibilities are
Apple observations, queue dispatch, native reference ownership, effects and UI
publication, as specified in [the FFI ownership contract](mobile-ffi.md).

[LIBCU-890](https://lific.mjc.lol/LIBCU/issues/LIBCU-890) makes accepted lifecycle
and restore work independent of public observer lifetime. The lifecycle settlement
worker starts before SQLite submission, owns the typed storage result and
updates the core before sending its terminal receipt. Public polling observes
and caches that receipt; dropping the handle cannot strand an already committed
transition. A lost accepted SQLite reply or a post-commit projection failure
clears only the matching pending command and blocks mutation until durable
restoration. It cannot leave an orphaned admission barrier. Restoration preserves live permission/provider evidence,
diagnostic capture demand, revision and command counters. It advances the
recording fence and rejects exhaustion instead of reusing an old token.

A GPS callback now reports an unrelated telemetry metadata write failure as a
StorageError outcome while continuing admission of valid material locations.
The dirty telemetry receipt remains retryable. GPS writes still settle in order
and require their own durable confirmation. Explicit checkpoint and lifecycle
operations retain their failure behavior.

Behavioral RED preceded production edits: four writer/core failures in
`target/capture-finalization/writer-red.log`, six map/mobile failures in
`target/test-logs/LIBCU-890-combined-red.log`, and three intended Swift cases in
`target/test-logs/swift-package-20261009T104445-15419.log`. The separate restore
generation-exhaustion RED is in
`target/test-logs/LIBCU-890-generation-exhausted-red.log`.
The thin-shell extension has additional behavioral RED evidence in
`target/test-logs/LIBCU-startup-boundary-red.log` (startup truncation and policy
override), `target/capture-shell/flush-red.log` (accepted flush reply loss and
storage failure), `target/capture-shell/terminal-receipt-red.log` (nonterminal
receipt consumption), and `target/capture-shell/identity-red.log` (partial native
metadata admission). The first identity fixture exceeded capacity during setup;
the corrected characterization uses one remaining slot and reaches the actual
partial-admission defect.

Before the thin-shell extension, full native validation passed 2,208 tests with
two configured skips and all workspace doctests; map/mobile selection passed
51/51. Those results are historical evidence for that revision. Current-source
native validation now passes 2,234 tests with two configured
skips, all workspace doctests, full all-target/all-feature locked Clippy, Rust
formatting and lint policy. Evidence is in
`target/test-logs/rust-workspace-20261009T114923-55625.log` and
`target/capture-shell/workspace-clippy.log`. The supported ARM64 Swift package
gate passes 991 tests with one skip and zero failures; its detailed log is
`target/test-logs/swift-package-20261009T115644-61938.log`. The initial attempt
compiled the production boundary but caught five missing clock arguments in new
fixtures; those fixture initializers were corrected before the passing gate.
The selected Rust FFI generation is
`6fe29fe232c7d9764518df36a74f88a1e3bb0d03ac8b65310af0e6a0dabc3dbe`.
The supported final Simulator app build passes. Its executable is ARM64, UUID
`AA5C827C-C3C0-3A62-9F63-D03B0BC77B4A`, SHA-256
`5eb9badac4ff4bd24a74c7600c27e526dbbcc1499db710356fc934e2040060b5`.
The product belongs to this checkout's Xcode project; its manifest lists
iPhoneSimulator. Product identity and frozen source hashes are recorded under
`target/capture-shell/`. This establishes compilation/linking, not a simulator
ride or physical-device acceptance.

The validated follow-up was deployed over the local network on 2026-10-09 through
`devenv tasks run deploy:ios-device` (exit 0). The signed device executable is ARM64,
UUID `9CD544A5-60A1-34C3-8960-127F6D40048E`, SHA-256
`5a1ae7c7aa81e464756cd2e581f4126db6dfaa3db3177f972b164e1bbac3e3d6`.
Fresh CoreDevice readback confirms `lol.cutout.app` installed in bundle container
`D09ACF91-56A3-4C22-AFD5-6AD1A2E6D715` and running from that bundle as PID 50705.
The deployment log, device connection details, installed-app readback, process
readback and local build identity are in `target/capture-shell/device-deploy/`.
Physical locked-ride acceptance remains LIBCU-711.
