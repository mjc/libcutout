# Load ride microbenchmarks

The Divan benchmark measures wall-clock ride loads per second. The Gungraun
benchmark measures deterministic instruction counts. Both exercise `find_ride`
(metadata, summary, duration, segment count) plus `project_route_points` through
the public SQLite worker. Projection uses the mobile history detail limit of
16,384 points, precise coordinates, and no viewport. They do not measure
Swift/FFI conversion, rendering, or phone latency.

## Divan throughput

Use `--items-count=1` so Divan reports whole ride loads per second. Fixture
setup, snapshotting, the initial correctness load, and shutdown are outside the
timed loop. Divan measures the same warmed SQLite worker request that mobile
history uses.

```sh
nix develop -c cargo bench -p libcutout-persistence --bench load_ride_ips -- --items-count=1
```

## Gungraun instruction counts

Gungraun measures instruction counts for `find_ride` (metadata,
summary, duration, segment count) plus `project_route_points` (route, camera,
endpoints, segment metadata) through the public SQLite worker. Projection uses
the mobile history detail limit of 16,384 points, precise coordinates, and no
viewport. It does not measure Swift/FFI conversion, rendering, or phone latency.

The default fixture is a deterministic, winding, approximately 13-mile ride with
4,697 one-second samples. Every sample must be accepted. Checks require at least
one mile (1,609,344 mm), matching source counts, nonempty geometry within budget,
all points when within budget, a camera region, and identical results before and
after measurement.

## Run

Use a Linux host supported by Valgrind, provided in the Linux development shell.
The library and runner versions must match; 0.18.2 preserves this workspace's
Rust 1.85 minimum. Install the runner locally under ignored build output:

```sh
nix develop -c cargo install gungraun-runner --version 0.18.2 --locked --root target/gungraun-runner
export GUNGRAUN_RUNNER="$PWD/target/gungraun-runner/bin/gungraun-runner"
nix develop -c cargo bench -p libcutout-persistence --bench load_ride -- --save-baseline=generated_before --nocapture
nix develop -c cargo bench -p libcutout-persistence --bench load_ride -- --baseline=generated_before --nocapture
```

On Apple Silicon macOS, compile and exercise the identical setup, load, and
correctness checks without Valgrind or its runner (this produces no counts):

```sh
nix develop -c cargo bench -p libcutout-persistence --bench load_ride -- --smoke
```

## Real imported history

Set `CUTOUT_BENCH_DATABASE` to a Cutout SQLite database. Setup opens it read-only
and uses SQLite backup to create a private temporary snapshot, including committed
WAL data. Only the snapshot is opened by the recovery-capable production worker.
It selects the imported ride with the most points among rides at least one mile
long; no qualifying ride is a hard failure, never a synthetic fallback. Private
databases and GPS fixtures must not be committed.

```sh
CUTOUT_BENCH_DATABASE=/path/to/ride.sqlite nix develop -c cargo bench \
  -p libcutout-persistence --bench load_ride_ips -- --items-count=1
CUTOUT_BENCH_DATABASE=/path/to/ride.sqlite nix develop -c cargo bench \
  -p libcutout-persistence --bench load_ride -- --save-baseline=imported_before --nocapture
CUTOUT_BENCH_DATABASE=/path/to/ride.sqlite nix develop -c cargo bench \
  -p libcutout-persistence --bench load_ride -- --baseline=imported_before --nocapture
```

Use the same environment variable with `--smoke` for a local real-data check.
Keep generated and imported baselines separate. Compare the same immutable
database contents, selected ride, target, toolchain, features, optimized bench
profile, and Valgrind version. Output reports distance, source/display point
counts, and segments; preserve it with `rustc -Vv` and `valgrind --version`.

## Measurement boundary

Snapshotting, migrations, fixture creation, worker startup, one initial load,
correctness checks, result destruction, and shutdown are outside instrumentation.
Worker dispatch, SQL, decoding, projection, and result allocation are inside.
Callgrind starts with instrumentation disabled and collection enabled, without
Gungraun's default function toggle. Explicit process-wide start/stop requests
include the existing SQLite worker thread; otherwise its counts would be zero.
Per-thread output is enabled so this can be checked in the profiling result.

SQLite's cache is warmed by the initial load, but starting instrumentation resets
Callgrind's simulated caches. Cache estimates are therefore not warm-cache wall
time. Results and baselines live in `target/gungraun`; repeat the same command to
check instruction-count stability before attributing differences to code changes.

See [Gungraun's multithreading guide](https://gungraun.github.io/gungraun/latest/html/benchmarks/library_benchmarks/threads_and_subprocesses.html)
and [Callgrind's instrumentation controls](https://valgrind.org/docs/manual/cl-manual.html#cl-manual.clientrequests).
