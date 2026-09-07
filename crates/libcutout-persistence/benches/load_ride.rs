//! Ride-detail instruction counts through the same worker APIs used by mobile history.

#![allow(
    unused_qualifications,
    reason = "Gungraun emits qualified calls in its harness"
)]

use std::hint::black_box;

use cutout_ride_maps::{
    Coordinate, LocationAdmission, LocationSample, LocationSource, MAX_ROUTE_DISPLAY_POINTS,
    RideEvent, RouteDisplayBudget, RoutePrivacyPolicy,
};
#[cfg(target_os = "linux")]
use gungraun::client_requests::callgrind;
use gungraun::{
    Callgrind, EntryPoint, LibraryBenchmarkConfig, OutputFormat, library_benchmark,
    library_benchmark_group,
};
use libcutout_persistence::{RideDatabase, RideId, RideRecord, RideSource, RoutePointProjection};
use rusqlite::{Connection, OpenFlags};

const MILE_MILLIMETRES: u64 = 1_609_344;
const GENERATED_POINTS: u32 = 4_697;

fn generated_ride(database: &RideDatabase) -> RideId {
    let wall_clock_ms = 1_700_000_000_000;
    let ride = database
        .create_ride_with_monotonic_start(RideSource::Live, wall_clock_ms, Some(0))
        .expect("create generated ride");
    database.transition_at(ride, RideEvent::Start, 0).unwrap();
    for index in 0..GENERATED_POINTS {
        // A gently winding, roughly 13-mile route at one sample per second.
        // Generated coordinates deliberately contain no personal ride data.
        let coordinate = Coordinate::from_degrees(
            f64::from(index) * 0.00004,
            (f64::from(index) / 100.0).sin() * 0.0002,
        )
        .unwrap();
        let elapsed_ms = u64::from(index) * 1_000;
        let sample = LocationSample::new(
            coordinate,
            elapsed_ms,
            wall_clock_ms + elapsed_ms,
            Some(1_000),
            LocationSource::Live,
        );
        assert_eq!(
            database.append_location(ride, sample).unwrap(),
            LocationAdmission::Accepted
        );
    }
    let end_ms = u64::from(GENERATED_POINTS - 1) * 1_000;
    database
        .transition_at(ride, RideEvent::Stop, end_ms)
        .unwrap();
    database
        .transition_at(ride, RideEvent::Save, end_ms)
        .unwrap();
    assert_eq!(
        database.summary(ride).unwrap().point_count().as_u64(),
        u64::from(GENERATED_POINTS)
    );
    ride
}

fn load_ride(database: &RideDatabase, ride: RideId) -> (RideRecord, RoutePointProjection) {
    let record = database.find_ride(ride).unwrap().expect("visible ride");
    let projection = database
        .project_route_points(
            ride,
            None,
            RouteDisplayBudget::new(MAX_ROUTE_DISPLAY_POINTS).unwrap(),
            RoutePrivacyPolicy::Precise,
        )
        .unwrap();
    (record, projection)
}

struct Fixture {
    database: RideDatabase,
    ride: RideId,
    expected: (RideRecord, RoutePointProjection),
    _directory: tempfile::TempDir,
}

fn setup() -> Fixture {
    let directory = tempfile::tempdir().expect("private benchmark directory");
    let path = directory.path().join("ride.sqlite");
    let (database, ride, workload) =
        if let Some(source_path) = std::env::var_os("CUTOUT_BENCH_DATABASE") {
            // Use SQLite backup, not a file copy: committed WAL data belongs in the snapshot.
            // Only the temporary copy is opened by the recovery-capable production service.
            let source = Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect("open source database read-only");
            source
                .backup("main", &path, None)
                .expect("snapshot database");
            let snapshot =
                Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
            let ride: String = snapshot
                .query_row(
                    "SELECT id FROM rides
                     WHERE source = 'pevcap_import' AND state = 'imported'
                       AND distance_mm >= ?1 AND point_count >= 2
                     ORDER BY point_count DESC, id LIMIT 1",
                    [MILE_MILLIMETRES],
                    |row| row.get(0),
                )
                .expect("snapshot must contain an imported ride at least one mile long");
            let database = RideDatabase::open(&path).expect("open snapshot worker");
            (
                database,
                RideId::from_uuid(ride.parse().unwrap()),
                "imported",
            )
        } else {
            let database = RideDatabase::open(&path).expect("open generated database");
            let ride = generated_ride(&database);
            (database, ride, "generated")
        };

    // Validate before measuring and afterward; never silently measure an empty/short ride.
    let expected = load_ride(&database, ride);
    let summary = expected.0.summary();
    assert!(summary.distance_millimetres() >= MILE_MILLIMETRES);
    assert_eq!(
        expected.1.source_point_count(),
        summary.point_count().as_u64()
    );
    assert!(!expected.1.points().is_empty());
    assert!(expected.1.points().len() <= MAX_ROUTE_DISPLAY_POINTS);
    let source_points = usize::try_from(summary.point_count().as_u64()).unwrap();
    if source_points <= MAX_ROUTE_DISPLAY_POINTS {
        assert_eq!(expected.1.points().len(), source_points);
    }
    assert!(expected.1.camera_region().is_some());
    eprintln!(
        "{workload}: {} mm, {} source points, {} displayed points, {} segments",
        summary.distance_millimetres(),
        summary.point_count().as_u64(),
        expected.1.points().len(),
        expected.1.source_segment_count(),
    );

    Fixture {
        database,
        ride,
        expected,
        _directory: directory,
    }
}

fn measure(fixture: Fixture) {
    // Instrumentation is process-wide; the default function toggle would count only
    // the caller thread and omit the SQLite worker. Setup and assertions stay outside.
    #[cfg(target_os = "linux")]
    callgrind::start_instrumentation();
    let actual = black_box(load_ride(
        black_box(&fixture.database),
        black_box(fixture.ride),
    ));
    #[cfg(target_os = "linux")]
    callgrind::stop_instrumentation();
    assert_eq!(actual, fixture.expected);
    fixture
        .database
        .shutdown()
        .expect("stop benchmark worker before cleanup");
}

#[library_benchmark(setup = setup)]
fn bench_load_ride(fixture: Fixture) {
    measure(fixture);
}

library_benchmark_group!(name = rides; benchmarks = bench_load_ride);

fn main() {
    // Exercise the identical workload without a runner on hosts without Valgrind.
    if std::env::args().any(|argument| argument == "--smoke") {
        measure(setup());
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
