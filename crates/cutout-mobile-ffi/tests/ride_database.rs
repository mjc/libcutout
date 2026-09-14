use std::{
    fs,
    sync::{Mutex, PoisonError},
};

use cutout_mobile_ffi::{
    MobileRideEventDto, MobileRideLifecycleStateDto, MobileRideLocationDto, MobileRideSourceDto,
    MobileStoredBmsVoltageSampleDto, Voltage, open_ride_database,
};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn mobile_clients_open_the_rust_owned_database() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-mobile-ffi-ride-database-{}-{}.sqlite",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
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
fn mobile_clients_store_device_scoped_bms_samples_without_a_ride() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-mobile-ffi-bms-database-{}-{}.sqlite",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_file(&path);
    let handle = open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
    handle
        .record_bms_voltage_samples(
            "wheel-a".to_owned(),
            vec![MobileStoredBmsVoltageSampleDto {
                session_identifier: "test-session".to_owned(),
                event_sequence: 1,
                monotonic_milliseconds: 1_000,
                wall_clock_milliseconds: 2_000,
                observation_index: 45,
                pack_index: Some(1),
                pack_observation_index: Some(15),
                voltage: Voltage { value: 4_193 },
            }],
        )
        .expect("BMS sample is stored");
    handle.shutdown().expect("database worker shuts down");

    let connection = rusqlite::Connection::open(&path).expect("database reopens directly");
    let stored: (String, u16, Option<u16>, Option<u16>, i32) = connection
        .query_row(
            "SELECT device_identity, observation_index, pack_index,
                    pack_observation_index, millivolts
             FROM bms_voltage_samples",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("stored BMS sample is queryable");
    assert_eq!(stored, ("wheel-a".to_owned(), 45, Some(1), Some(15), 4_193));
    drop(connection);
    let _ = fs::remove_file(path);
}

#[test]
fn mobile_clients_bootstrap_recovered_rides_and_page_history() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-mobile-ffi-recovery-{}-{}.sqlite",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_file(&path);
    let path_string = path.to_string_lossy().into_owned();
    let handle = open_ride_database(path_string.clone()).expect("database opens");
    let ride_id = handle
        .create_ride(MobileRideSourceDto::Live, 100)
        .expect("ride is created");
    handle
        .transition(ride_id.clone(), MobileRideEventDto::Start, 0)
        .expect("ride starts");
    handle
        .append_location(
            ride_id.clone(),
            MobileRideLocationDto {
                latitude_degrees: 39.7392,
                longitude_degrees: -104.9903,
                monotonic_milliseconds: 1,
                wall_clock_unix_milliseconds: 102,
                horizontal_accuracy_millimetres: Some(1_000),
                source: MobileRideSourceDto::Live,
            },
        )
        .expect("location is stored");
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
