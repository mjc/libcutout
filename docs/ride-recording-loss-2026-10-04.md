# October 4 ride recording loss

Implementation and verification are tracked in
[LIBCU-867](https://lific.mjc.lol/LIBCU/issues/LIBCU-867). The durable incident report
is [LIBCU-DOC-11](https://lific.mjc.lol/LIBCU/pages/125).

## Observed failure

The NOSFET Aero rider reported approximately 75 miles ridden on October 4, 2026,
starting around 13:00 America/Denver. The phone's canonical ride stopped accepting
GPS at 13:08:46.000. Native diagnostic captures retained later location and wheel
observations, so the history screen and the underlying capture data disagreed.

The original app database was copied over USB without changing its contents.
The copy is 8,286,318,592 bytes. Table integrity checks for `rides`, `ride_points`,
and `ride_segments` returned `ok`. Source file metadata did not change during the
copy, and the source directory had no SQLite WAL or SHM files.

The affected live ride is `eef5e04c-5d58-4802-a346-39a57bbe7078`. It contains
6,668 canonical points and 515,513 mm of distance, approximately 0.32 miles.
Its stored lifecycle includes an explicit pause at monotonic time 197,911,738 ms.
The last canonical GPS observation coincides with that pause. Later wheel
telemetry continued to update the ride's telemetry timestamp.

The observed pause is consistent with `prepare_disconnect`, which previously
persisted a ride Pause transition before disconnecting Bluetooth. Reconnection
respected that explicit pause and did not resume GPS recording. The database
proves the pause and the resulting cutoff; it does not independently identify
which UI action invoked the transition.

### Capture boundary evidence

All 11 gaps in the recovered October 4 route cross different GPS-bearing capture
sessions. The last source location precedes capture completion, and the next
source location belongs to the following GPS-bearing capture. In the largest
moving gap, the last location is at 18:34:40.000 and its capture finishes at
18:34:40.317. The next location is at 18:58:19.000, with its capture starting at
18:58:19.994. These source timestamps can precede capture start because Core
Location delivers cached observations with their original timestamps.

Twelve short capture sessions also occur inside the gaps. They contain only a
link-down event, with no location observations. The evidence therefore establishes
capture-correlated GPS coverage, rather than proving acquisition was stopped for
every instant inside each gap.

The lifecycle policy explains the exposure: an Active canonical ride independently
requests GPS, while a Paused ride requests GPS only when a diagnostic capture is
active. The old disconnect path converted the former into the latter. It stopped
canonical admission and made subsequent acquisition dependent on capture lifetime.
The stored pause, the cutoff, and the capture boundaries fit that mechanism. There
is no independent action log identifying the exact tap that persisted the pause.

The 20,670 recovered raw samples were 7–9,641 ms older than their callback receipt;
none was future-dated. This calculation uses receipt monotonic time minus the
calibrated source offset relative to the same capture start. The maximum callback
contains four samples. All 26 source captures report zero dropped messages.
There is no evidence of a phone clock error or of oversized callbacks causing
the October 4 loss.

The read-only reproduction is
`target/diagnostics/ride-loss-20261004/correlate-capture-gaps.py`; its output is
`gap-capture-correlation.json`. The original database and observation provenance
remain unchanged.

## Recording changes

`MobileRideMapCore::prepare_disconnect` now checkpoints pending location writes
and preserves the ride's lifecycle, ID, and recording generation. It still rejects
commands for a replaced recording and propagates checkpoint failures before the
transport disconnect proceeds. Pause and Stop remain explicit ride actions.

The native location callback path now runs through
`ingest_location_callback_with_outcomes` on the existing serial background
recording executor. It processes callbacks in groups of 32 and checkpoints each
group before admitting the next one. This avoids truncating callbacks at the
256-sample nonblocking batch limit or rejecting their tail at the 64-write pending
limit. The original bounded nonblocking APIs retain their existing contract.
Receipt clock anchors and the recording token are preserved across the groups.

Native callback writes now also wait for worker queue capacity. Reserving half the
pending-outcome budget for a group does not reserve capacity in the shared database
queue: other producers can occupy it between checkpoints and submissions. The old
nonblocking enqueue could return QueueFull and abort the unprocessed callback tail.
`queue_location_waiting_for_capacity` waits on the bounded worker channel and
returns a pending durability ticket. Only the serialized background callback path
uses this submission policy; existing nonblocking APIs continue to return QueueFull.
Worker and storage failures still propagate. This closes an additional data-loss
path found during the incident audit; it is not established as the October 4 cause.

`RideDatabase::wait_for_ride_checkpoint` uses the worker's blocking enqueue path.
A checkpoint waits for queue capacity instead of failing with QueueFull. Callers
must execute it off the main actor.

The iOS location manager sets `pausesLocationUpdatesAutomatically = false`.
Rust recording demand controls acquisition. Automatic platform pausing could
otherwise suspend delivery during stops independently of the ride lifecycle.
The data contains GPS gaps, but does not prove automatic platform pausing caused
any particular gap.

## Recovery

There are 20,670 raw location observations from 26 native captures for this
vehicle between 13:08:46.622 and 21:09:07 MDT. A derived PEVCAP artifact orders
these observations by their source wall-clock time. Its synthetic replay clock
uses that observation time; exact original receipt times, source offsets, capture
IDs, sequences, and raw payloads are retained separately in a provenance file.
Coordinates, source timestamps, and accuracy values are unchanged.

The production Rust PEVCAP importer admitted 20,647 points into a separate local
database, yielding 95,161,008 mm, **59.1303 GPS miles**, in 12 segments. Its
30-second gap policy produced 11 background gaps. The derived database passes
SQLite integrity checking. The GPX export retains a separate track segment for
every canonical segment. Missing intervals are not bridged.

The wheel's final stored trip counter is 120,904,000 mm, **75.1263 miles**.
The captured lifetime odometer increased by 110,265,000 mm, **68.5155 miles**.
The first captured trip counter already included 10,639,000 mm, about 6.61 miles.
The trip counter supports the rider's reported distance; it does not establish
GPS geometry for missing sections. The largest moving gap is 18:34:40–18:58:19,
whose endpoints are approximately 5.41 miles apart in a straight line.

Recovery artifacts are private local files under
`target/diagnostics/ride-loss-20261004/` and are not checked in:

- `ride.sqlite`: unchanged full source copy.
- `recovery-observations.jsonl`: original observation payloads and provenance.
- `recovered-2026-10-04.pevcap.jsonl`: derived replay input.
- `recovered-source-time.sqlite`: imported canonical recovered ride.
- `recovered-2026-10-04.gpx`: observed route sections with gaps preserved.
- `recovery-summary.json`: exact counts, distance, endpoints, and gap intervals.

The recovered ride was added to the phone database through the existing importer
on October 5. The original ride and capture records remain present. The phone
database was not replaced with the derived recovery database.

## Regression coverage

- `recording_survives_capture_gaps_and_transport_disconnects_until_explicit_pause`
  exercises three disconnect/capture-close cycles, persists GPS while both are
  inactive, and verifies unchanged ride identity and recording token. An explicit
  user pause stops canonical admission; diagnostic acquisition during that pause
  does not resume the ride, and stale capture completion cannot retire the current
  capture's demand. Resume adds a segment to the same ride. Save and database reopen
  verify one history ride containing all five admitted points and both endpoints.
- `background_location_submission_waits_for_capacity_without_losing_the_point`
  gates the worker and fills its queue with competing commands. Nonblocking
  submission returns QueueFull; background submission waits, then persists the
  point once capacity is released. The test failed with QueueFull before the change
  and passed after switching to blocking enqueue.

- `mobile_ride_map_core_keeps_recording_across_explicit_disconnect_and_preserves_state_on_failure`
  verifies unchanged recording identity and generation, accepts a subsequent GPS
  point, and preserves state when storage fails. It failed against the previous
  Pause transition before the fix.
- `testDisconnectKeepsGpsRecordingActive` exercises the Swift app disconnect
  path against real Rust recording state and verifies the same active token.
- `location_callback_persists_bursts_larger_than_the_write_queue_and_batch_limit`
  verifies complete durable storage for callbacks of 65, 257, and 4,501 points.
  Its 4,823-point route also exceeds the 4,096-point projection budget and checks
  canonical endpoint and count preservation. Before the fix, the first burst
  persisted only 64 of 65 points.
- `testNativeLocationCallbackPersistsEveryPointAcrossRustWriteGroups` verifies
  all 322 Swift callback points become canonical stored points.
- `blocking_ride_checkpoint_waits_for_capacity_in_a_full_worker_queue` fills a
  gated worker queue, requests a checkpoint, releases capacity, and verifies the
  checkpoint completes. It previously failed with QueueFull.
- `testRideLocationManagerKeepsAcquiringAcrossStops` checks the actual iOS
  location manager configuration. It explicitly skips on macOS, where platform
  defaults differ. The host run is not iOS runtime evidence.

The October 5 Rust checks, including the additional capacity and capture-lifetime
regressions, passed strict workspace Clippy, 2,105 nextest tests with one skip,
and all workspace doctests. The matching Swift package host run executed 931 tests:
930 passed and the iOS-only location-manager policy check was skipped on macOS.
A prolonged locked-screen ride with wheel disconnect/reconnect remains a physical
acceptance check; the host tests do not establish that result.

The earlier signed iPhone build passed embedded configuration and code signature
checks. CoreDevice confirmed installation of `lol.cutout.app` and successful
launch on the physical iPhone. The disconnect, complete-callback, and location
manager fixes are in that installed build. The additional queue-capacity hardening
from October 5 has not been deployed. The user stopped further computer use and
phone inspection; this phase uses the existing local data copy and host checks.
Recovery import verification is recorded separately below.

## Empty Last 30 Days history

The history view did not start a query when it first appeared. Its initial state
was an empty array with Last 30 Days selected. Changing the date filter triggered
`RideHistoryModel.reload`, which concealed the missing appearance query.
`RideMapRouteView.historyContent` now starts `history.reload()` in its view task.
The existing rolling 30-day cutoff and Rust SQL filtering are unchanged.

`testOpeningHistoryLoadsDefaultRecentRidesWithoutChangingFilter` mounts the real
SwiftUI map view in a host window, switches from Live to History, and waits for a
stored ride while retaining Last 30 Days. It timed out before the appearance hook
was added and passed afterward. It does not manually reload or change filters.

## Phone restoration command

The debug app supports an explicit `--import-ride-capture` maintenance command.
It accepts a filename in Documents, the ride's original creation timestamp, and
the reviewed SHA-256 digest. It verifies the digest before confirming the import
through the existing process-owned Rust database service, off the main actor.
The importer deduplicates by digest. No database replacement or direct SQL edits
are involved. A receipt in Documents records the imported ride's stored point
count and distance, the prior history IDs, and the current Last 30 Days query IDs.

Recovery tests verify that a digest mismatch does not add a ride, existing ride
records are preserved, repeated confirmation returns the same ride, and an old
ride retains its source date and stays outside Last 30 Days. Tests reuse the
process-owned database rather than attempting to open a second database path.

The final package run after these changes executed 931 tests: 930 passed and the
iOS-only location-manager check was skipped on macOS. The signed update and
reviewed recovery artifact are installed on the phone. The first restoration
launch was rejected by iOS because the device was locked; no import ran in that
attempt. After connecting through iPhone Mirroring on October 5, the launch and
import completed and the on-device receipt passed verification.

## Full history audit

The unchanged database contains 16 live rides created September 17–October 4:
15 saved rides and one interrupted ride. Each summary point count agrees with
the actual stored point rows. All 16 creation dates fall within Last 30 Days at
the October 5 audit. The empty history view therefore did not reflect an empty
database.

The phone's Documents directory was copied separately without modifying its
contents. It contains 351 Bluetooth capture files totaling 2,283,725,725 bytes.
All capture files parsed successfully. Older captures attach cached phone
locations to transport records; the native independent-location tables only
begin October 2. Checking only those tables would miss September evidence.
Repeated attached locations were deduplicated by vehicle, source timestamp, and
coordinates before comparison with canonical stored points.

Two additional omissions have recoverable source locations:

| Date | Stored history failure | Raw observations used | Imported points | Additional GPS miles | Segments / gaps |
| --- | --- | ---: | ---: | ---: | ---: |
| October 2 | Existing ride stops at 19:12:54; locations continue until 20:36:21.785 | 1,090 | 1,086 | 2.6923 | 3 / 2 |
| October 3 | No canonical ride; locations exist from 13:27:27.505 to 19:26:11.001 | 5,417 | 5,390 | 3.8509 | 6 / 5 |

October 2 recovery starts strictly after the original ride's last GPS timestamp.
It adds a separate recovered tail and retains the original 1.2173-mile ride.
October 3 recovery adds the missing day's observed sections. Both use the same
source-time replay conversion and production Rust importer as October 4. They
were imported into a separate local database, which passes SQLite integrity
checking, and exported to GPX with gaps retained as separate track segments.
Neither repair infers travel between observations or overwrites an existing ride.

The reviewed artifacts and original dates are:

- October 2: `recovered-2026-10-02.pevcap.jsonl`, created at `1790989975393` ms,
  SHA-256 `d83f28ce05a990f7a81ad8044452650ca469b38d8e8da379deb277893ad1bbe3`.
- October 3: `recovered-2026-10-03.pevcap.jsonl`, created at `1791055647505` ms,
  SHA-256 `4ccdcf4165900f2a9f9a600b2d8105c659c26fddbb4302758e0ede6383aaaa6f`.

All three recovery inputs were transferred into the phone's Documents directory
and imported successfully on October 5. The acceptance verifier requires preservation
of all 16 original history IDs and every prior recent-history ID, the exact
reviewed digest, source date, point count, distance, and presence of each imported
ride in Last 30 Days. The on-device receipt is copied after each import because
the next maintenance command replaces the receipt file. Each receipt passed:

| Source date | Restored phone ride ID | Points | GPS miles | Last 30 Days count after import |
| --- | --- | ---: | ---: | ---: |
| October 4 | `d664e425-2679-47e2-9290-b7492c79f520` | 20,647 | 59.1303 | 17 |
| October 3 | `3f0cc3c7-efd6-474c-a896-abb727c28e7e` | 5,390 | 3.8509 | 18 |
| October 2 | `b9a86a34-6614-4af1-8121-f2a2c7b0fd03` | 1,086 | 2.6923 | 19 |

iPhone Mirroring showed the October 4 detail route with 59 miles and the populated
Last 30 Days history list, including October 3 at 3.9 miles and the October 2
recovered tail at 2.7 miles. No date-filter change was needed to populate the list.
The original October 4 entry and October 2 entry remain separately visible.
The 16 original history entries span September 17–October 4. They are not fragments
of the October 4 ride. The affected October 4 original and its recovered continuation
remain separate entries; no cross-day grouping or consolidation was applied. The
current work addresses recording invariants and the October 4 failure mechanism.

September capture comparison found 46,877 of 46,890 unique raw GPS observations
already stored with identical timestamps and coordinates. Eleven more have a
canonical observation within two seconds; two isolated observations do not.
There are no additional captured tails beyond the saved September endpoints and
no run of ten missing observations within 30-second spacing. The comparison
found no additional recoverable September route sections. It does not establish
that acquisition ran for every part of those rides: an interval absent from both
canonical storage and raw captures cannot be reconstructed.

An auxiliary odometer replay succeeded for October captures but rejected
September's PEVCAP 1.2 headers. Those failures are recorded as an audit limitation;
no September odometer completeness claim is made. The new phone Documents copy
also contains isolated late October 4 observations after the reviewed recovery
endpoint. They do not establish another route section and are not added to the
reviewed October 4 artifact.

Private audit files are under `target/diagnostics/ride-loss-20261004/history-audit/`:
`history-audit.json`, `capture-files-audit.json`,
`capture-file-location-provenance.jsonl`, `raw-location-provenance.jsonl`,
`interior-gps-audit.json`, `wheel-counter-audit.json`, `recovery-manifest.json`,
the two derived capture inputs, the local recovery database, and both GPX exports.
