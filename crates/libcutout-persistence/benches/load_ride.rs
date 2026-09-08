//! Ride-detail instruction counts through the same worker APIs used by mobile history.

#![allow(
    unused_qualifications,
    reason = "Gungraun emits qualified calls in its harness"
)]

use std::hint::black_box;

#[cfg(target_os = "linux")]
use gungraun::client_requests::callgrind;
use gungraun::{
    Callgrind, EntryPoint, LibraryBenchmarkConfig, OutputFormat, library_benchmark,
    library_benchmark_group,
};

#[path = "support/load_ride.rs"]
mod load_ride_support;
use load_ride_support::Fixture;

fn measure(fixture: Fixture) {
    // Instrumentation is process-wide; the default function toggle would count only
    // the caller thread and omit the SQLite worker. Setup and assertions stay outside.
    #[cfg(target_os = "linux")]
    callgrind::start_instrumentation();
    let actual = black_box(fixture.load());
    #[cfg(target_os = "linux")]
    callgrind::stop_instrumentation();
    fixture.assert_matches(&actual);
    fixture.shutdown();
}

fn setup() -> Fixture {
    Fixture::setup()
}

#[library_benchmark(setup = setup)]
fn bench_load_ride(fixture: Fixture) {
    measure(fixture);
}

library_benchmark_group!(name = rides; benchmarks = bench_load_ride);

fn main() {
    // Exercise the identical workload without a runner on hosts without Valgrind.
    if std::env::args().any(|argument| argument == "--smoke") {
        measure(Fixture::setup());
        return;
    }
    run_harness();
}

fn run_harness() {
    gungraun::main!(
        config = LibraryBenchmarkConfig::default()
            .pass_through_env("CUTOUT_BENCH_DATABASE")
            .output_format(OutputFormat::default().show_intermediate(true))
            .tool(Callgrind::with_args(["--instr-atstart=no", "--collect-atstart=yes"])
                .entry_point(EntryPoint::None));
        library_benchmark_groups = rides
    );
    // The macro defines an entry point; call it after handling our native smoke mode.
    main();
}
