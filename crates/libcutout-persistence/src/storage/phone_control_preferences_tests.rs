use super::{RideDatabase, StorageError, test_guard};
use cutout_core::PhoneControlMode;
use rusqlite::Connection;

#[test]
fn phone_drive_limits_save_rejects_an_ignored_update() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER ignore_budget BEFORE UPDATE ON phone_control_preferences BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(
        database
            .save_phone_drive_limits(cutout_core::PhoneDriveLimits::new(7_000, 8_000).unwrap())
            .is_err()
    );
    assert_eq!(
        database.phone_drive_limits().unwrap(),
        cutout_core::PhoneDriveLimits::default()
    );
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_limits_round_trip_and_failed_write_keeps_both_budgets() {
    use cutout_core::PhoneDriveLimits;
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.phone_drive_limits().unwrap(),
        PhoneDriveLimits::default()
    );
    let limits = PhoneDriveLimits::new(7_125, 9_500).unwrap();
    database.save_phone_drive_limits(limits).unwrap();
    assert_eq!(database.phone_drive_limits().unwrap(), limits);
    database.shutdown().unwrap();
    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(database.phone_drive_limits().unwrap(), limits);
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_phone_budget BEFORE UPDATE ON phone_control_preferences BEGIN SELECT RAISE(ABORT, 'injected budget failure'); END;").unwrap();
    assert!(
        database
            .save_phone_drive_limits(PhoneDriveLimits::default())
            .is_err()
    );
    assert_eq!(database.phone_drive_limits().unwrap(), limits);
    database.shutdown().unwrap();
}

#[test]
fn phone_control_mode_defaults_disabled_and_survives_reopen() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.phone_control_mode().unwrap(),
        PhoneControlMode::Disabled
    );
    for mode in [
        PhoneControlMode::Buttons,
        PhoneControlMode::Area,
        PhoneControlMode::Disabled,
    ] {
        database.save_phone_control_mode(mode).unwrap();
        assert_eq!(database.phone_control_mode().unwrap(), mode);
    }
    database
        .save_phone_control_mode(PhoneControlMode::Area)
        .unwrap();
    database.shutdown().unwrap();
    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.phone_control_mode().unwrap(),
        PhoneControlMode::Area
    );
    database.shutdown().unwrap();
}

#[test]
fn phone_control_mode_has_only_one_constrained_persistent_value() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    database.shutdown().unwrap();
    let connection = Connection::open(&path).unwrap();
    let columns = connection
        .prepare("PRAGMA table_info(phone_control_preferences)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        columns,
        ["id", "mode", "throttle_milliamps", "brake_milliamps"]
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT id, mode FROM phone_control_preferences",
                [],
                |row| { Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)) }
            )
            .unwrap(),
        (1, "disabled".to_owned())
    );
    assert!(
        connection
            .execute("UPDATE phone_control_preferences SET mode = 'armed'", [])
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO phone_control_preferences (id, mode) VALUES (2, 'buttons')",
                [],
            )
            .is_err()
    );
    assert!(
        connection
            .execute("UPDATE phone_control_preferences SET mode = NULL", [])
            .is_err()
    );
}

#[test]
fn phone_control_mode_upgrade_defaults_disabled_and_preserves_other_preferences() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    database
        .remember_selected_device("controller-a", Some("Board"), 1)
        .unwrap();
    database.save_ride_autostart_enabled(false).unwrap();
    database.shutdown().unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE phone_control_preferences; PRAGMA user_version = 54;")
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.phone_control_mode().unwrap(),
        PhoneControlMode::Disabled
    );
    assert_eq!(
        database.selected_device().unwrap().as_deref(),
        Some("controller-a")
    );
    assert_eq!(
        database.device_name("controller-a").unwrap().as_deref(),
        Some("Board")
    );
    assert!(!database.ride_autostart_enabled().unwrap());
    database.shutdown().unwrap();
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        crate::storage::CURRENT_SCHEMA_VERSION
    );
}

#[test]
fn phone_control_mode_rejects_corrupt_text_instead_of_defaulting() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE phone_control_preferences SET mode = 'armed' WHERE id = 1;",
        )
        .unwrap();
    assert!(matches!(
        database.phone_control_mode(),
        Err(StorageError::InvalidStoredValue {
            field: "phone control mode",
            value,
        }) if value == "armed"
    ));
    database.shutdown().unwrap();
}

#[test]
fn phone_control_mode_save_failure_retains_previous_mode() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    database
        .save_phone_control_mode(PhoneControlMode::Buttons)
        .unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_phone_control_mode BEFORE UPDATE ON phone_control_preferences
             BEGIN SELECT RAISE(ABORT, 'injected mode save failure'); END;",
        )
        .unwrap();
    assert!(
        database
            .save_phone_control_mode(PhoneControlMode::Area)
            .is_err()
    );
    assert_eq!(
        database.phone_control_mode().unwrap(),
        PhoneControlMode::Buttons
    );
    database.shutdown().unwrap();
    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.phone_control_mode().unwrap(),
        PhoneControlMode::Buttons
    );
    connection
        .execute_batch("DROP TRIGGER reject_phone_control_mode;")
        .unwrap();
    database
        .save_phone_control_mode(PhoneControlMode::Area)
        .unwrap();
    database.shutdown().unwrap();
}
