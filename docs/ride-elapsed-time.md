# Elapsed ride time

Elapsed time is the measured span of one ride, including explicit pauses and
same-process interruptions. It is separate from active recording duration and
Moving time. Active recording duration keeps its existing lifecycle and average
speed uses. Moving time keeps its existing speed-evidence coverage policy.

The Rust elapsed continuation publishes `Unavailable`, `Complete(duration)`, or
`Partial(duration)`. Partial duration is a measured lower bound. A complete zero
is valid at Start; absent timing evidence is unavailable. Swift formats the Rust
result and does not subtract dates or run a second elapsed clock.

## Clock evidence

Within a process, accepted monotonic receipts advance a scalar duration and
anchor. Pausing does not close elapsed time. An interruption freezes the visible
prefix; resuming in that process includes the intervening monotonic interval.
Stop freezes the endpoint. Save and later reads do not add time. Older receipts
or projections cannot rewind the committed prefix or count an interval twice.

The existing mobile logical-clock offset only clamps a new raw timestamp to an
old watermark. It does not prove that two process epochs are comparable. The
existing wall-clock update time supports recovery deadlines but is not a
trusted elapsed-time bridge: it can include unrelated receipt time, and wall
clocks can jump. Elapsed time therefore does not infer an exact restart gap from
either field. Restoration retains the committed duration as partial, clears its
old anchor, and begins a new measured interval on Resume. An unknown gap is never
represented as a measured zero. A reliable durable clock-epoch bridge would be
required before restart gaps could produce complete elapsed coverage.

Expired interruption recovery uses the elapsed prefix captured at the last
recorded point, matching the D04 recording endpoint. Later app reopening does
not extend that saved ride. A point older than the committed timing cursor
cannot synthesize an earlier prefix by rewinding the continuation.

## Persistence and publication

Elapsed continuation and recorded-point prefix belong to the existing SQLite
ride transactions and versioned timing checkpoint. No timer operation, extra
telemetry write, or Swift-owned store is needed. An unsuccessful point or
lifecycle transaction cannot publish elapsed time separately. Read-only live
and durable summaries project the same committed continuation. Older queued
write outcomes cannot replace a newer timing revision.

An FFI read with an explicit logical as-of timestamp projects live and durable
summaries to that same instant. History list/find operations without such a
clock return the committed prefix, rather than reading a wall clock or inventing
a current monotonic epoch. Terminal rows stay frozen under either read API.

Historical and imported rows without elapsed timing evidence remain unavailable.
Migration does not backfill them from point span, active recording duration, or
wall-clock dates. Physical phone and Android verification are outside LIBCU-943;
checks use the ARM64 host and Simulator.
