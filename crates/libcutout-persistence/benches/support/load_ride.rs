use cutout_ride_maps::{
    Coordinate, LocationAdmission, LocationSample, LocationSource, MAX_ROUTE_DISPLAY_POINTS,
    RideEvent, RouteDisplayBudget, RoutePrivacyPolicy,
};
use libcutout_persistence::{RideDatabase, RideId, RideRecord, RideSource, RoutePointProjection};
use rusqlite::{Connection, OpenFlags};

const MILE_MILLIMETRES: u64 = 1_609_344;
const GENERATED_POINTS: u32 = 4_697;

type LoadResult = (RideRecord, RoutePointProjection);

pub(super) struct Fixture {
    database: RideDatabase,
    ride: RideId,
    expected: LoadResult,
    _directory: tempfile::TempDir,
}

impl Fixture {
    pub(super) fn setup() -> Self {
        let directory = tempfile::tempdir().expect("private benchmark directory");
        let path = directory.path().join("ride.sqlite");
        let (database, ride, workload) = if let Some(source_path) =
            std::env::var_os("CUTOUT_BENCH_DATABASE")
        {
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

        Self {
            database,
            ride,
            expected,
            _directory: directory,
        }
    }

    pub(super) fn load(&self) -> LoadResult {
        load_ride(&self.database, self.ride)
    }

    pub(super) fn assert_matches(&self, actual: &LoadResult) {
        assert_eq!(actual, &self.expected);
    }

    pub(super) fn shutdown(self) {
        self.database
            .shutdown()
            .expect("stop benchmark worker before cleanup");
    }
}

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

fn load_ride(database: &RideDatabase, ride: RideId) -> LoadResult {
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
