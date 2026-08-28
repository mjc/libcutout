use std::{
    fs,
    sync::{Mutex, PoisonError},
    thread,
    time::Duration,
};

use cutout_mobile_ffi::{
    MobileRideIdDto, MobileRideLifecycleStateDto, MobileRideMapCore, open_ride_database,
};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn mobile_clients_open_the_rust_owned_database() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-mobile-ffi-ride-database-{}-{}.sqlite",
        std::process::id(),
        thread::current().name().unwrap_or("test")
    ));
    let handle = open_ride_database(path.to_string_lossy().into_owned())
        .expect("Rust should own the mobile database service");
    let capabilities = handle
        .capabilities()
        .expect("SQLite capabilities should be observable through FFI");
    assert!(capabilities.major >= 3);
    handle.shutdown().expect("database worker should shut down");
    let _ = fs::remove_file(path);
}

#[test]
fn mobile_clients_bootstrap_recovered_rides_and_page_history() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-mobile-ffi-recovery-{}-{}.sqlite",
        std::process::id(),
        thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_file(&path);
    let path_string = path.to_string_lossy().into_owned();
    let handle = open_ride_database(path_string.clone()).expect("database opens");
    let state = MobileRideMapCore::with_database(handle.clone());
    let ride_id = MobileRideIdDto {
        value: state
            .start_gps_only(100, None)
            .expect("ride starts")
            .ride_id,
    };
    state
        .ingest_location(101, 102, 39.7392, -104.9903, 1.0)
        .expect("location is admitted");
    for _ in 0..100 {
        if !state.poll_location_writes().is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        state
            .points_after(None, 1)
            .expect("route page is returned")
            .points
            .len(),
        1
    );
    handle.shutdown().expect("database worker shuts down");

    let reopened = open_ride_database(path_string).expect("database reopens");
    assert_eq!(
        reopened.bootstrap_snapshot().recovered_rides,
        vec![ride_id.clone()]
    );
    let rides = reopened.list_rides(None, 1).expect("ride page is returned");
    assert_eq!(rides.rides.len(), 1);
    assert_eq!(rides.rides[0].id, ride_id.clone());
    assert_eq!(
        rides.rides[0].state,
        MobileRideLifecycleStateDto::Interrupted
    );
    assert!(rides.next_cursor.is_none());
    let route = reopened
        .route_points(ride_id, None, 1)
        .expect("route page is returned");
    assert_eq!(route.points.len(), 1);
    assert_eq!(route.points[0].sequence, 0);
    assert!(route.next_cursor.is_none());

    reopened.shutdown().expect("database worker shuts down");
    let _ = fs::remove_file(path);
}
