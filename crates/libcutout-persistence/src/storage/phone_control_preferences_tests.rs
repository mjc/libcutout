use super::{RideDatabase, StorageError, test_guard};
use crate::PhoneDriveSettingsRecord;
use cutout_core::{PhoneControlMode, PhoneDriveLimits};
use rusqlite::{Connection, OptionalExtension};

#[test]
fn phone_drive_preferences_are_per_device_disabled_by_default_and_durable() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let default = PhoneDriveSettingsRecord::default();
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        default
    );
    assert_eq!(
        database.phone_drive_preferences("board-b").unwrap(),
        default
    );

    let board_a = PhoneDriveSettingsRecord::new(
        PhoneControlMode::Buttons,
        PhoneDriveLimits::new(7_125, 4_500).unwrap(),
    );
    let board_b = PhoneDriveSettingsRecord::new(
        PhoneControlMode::Area,
        PhoneDriveLimits::new(9_000, 8_000).unwrap(),
    );
    database
        .save_phone_drive_preferences("board-a", board_a)
        .unwrap();
    database
        .save_phone_drive_preferences("board-b", board_b)
        .unwrap();
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        board_a
    );
    assert_eq!(
        database.phone_drive_preferences("board-b").unwrap(),
        board_b
    );
    assert_eq!(
        database.phone_drive_preferences("board-c").unwrap(),
        default
    );
    database.shutdown().unwrap();

    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        board_a
    );
    assert_eq!(
        database.phone_drive_preferences("board-b").unwrap(),
        board_b
    );
    assert_eq!(
        database.phone_drive_preferences("board-c").unwrap(),
        default
    );
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_preferences_save_is_atomic_and_rejects_invalid_identity() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let previous = PhoneDriveSettingsRecord::new(
        PhoneControlMode::Buttons,
        PhoneDriveLimits::new(7_000, 8_000).unwrap(),
    );
    database
        .save_phone_drive_preferences("board-a", previous)
        .unwrap();
    assert!(matches!(
        database.phone_drive_preferences("  "),
        Err(StorageError::InvalidStoredValue {
            field: "device identity",
            ..
        })
    ));

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_phone_drive_settings BEFORE UPDATE ON phone_drive_preferences
             BEGIN SELECT RAISE(ABORT, 'injected phone-drive settings failure'); END;",
        )
        .unwrap();
    let replacement = PhoneDriveSettingsRecord::new(
        PhoneControlMode::Area,
        PhoneDriveLimits::new(11_000, 12_000).unwrap(),
    );
    assert!(
        database
            .save_phone_drive_preferences("board-a", replacement)
            .is_err()
    );
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        previous
    );
    assert_eq!(
        database.phone_drive_preferences("board-b").unwrap(),
        PhoneDriveSettingsRecord::default()
    );
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_preferences_reject_ignored_insert_and_update() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let previous = PhoneDriveSettingsRecord::new(
        PhoneControlMode::Buttons,
        PhoneDriveLimits::new(7_000, 8_000).unwrap(),
    );
    database
        .save_phone_drive_preferences("board-existing", previous)
        .unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER ignore_phone_drive_insert BEFORE INSERT ON phone_drive_preferences
             WHEN NEW.device_identity = 'board-new'
             BEGIN SELECT RAISE(IGNORE); END;
             CREATE TRIGGER ignore_phone_drive_update BEFORE UPDATE ON phone_drive_preferences
             BEGIN SELECT RAISE(IGNORE); END;",
        )
        .unwrap();

    let inserted = database.save_phone_drive_preferences(
        "board-new",
        PhoneDriveSettingsRecord::new(
            PhoneControlMode::Area,
            PhoneDriveLimits::new(11_000, 12_000).unwrap(),
        ),
    );
    let updated = database.save_phone_drive_preferences(
        "board-existing",
        PhoneDriveSettingsRecord::new(
            PhoneControlMode::Area,
            PhoneDriveLimits::new(13_000, 14_000).unwrap(),
        ),
    );
    database.shutdown().unwrap();
    drop(connection);

    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.phone_drive_preferences("board-new").unwrap(),
        PhoneDriveSettingsRecord::default(),
        "ignored insert must not create settings"
    );
    assert_eq!(
        database.phone_drive_preferences("board-existing").unwrap(),
        previous,
        "ignored update must preserve the last complete settings"
    );
    database.shutdown().unwrap();

    assert!(
        matches!(inserted, Err(StorageError::InvalidDatabaseIdentity)),
        "zero affected rows from an ignored insert must not report success"
    );
    assert!(
        matches!(updated, Err(StorageError::InvalidDatabaseIdentity)),
        "zero affected rows from an ignored update must not report success"
    );
}

#[test]
fn phone_drive_preferences_schema_preserves_legacy_singleton_without_using_it() {
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
                "SELECT id, mode, throttle_milliamps, brake_milliamps
                   FROM phone_control_preferences",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                }
            )
            .unwrap(),
        (1, "disabled".to_owned(), 3_000, 3_000)
    );
    assert!(
        connection
            .execute("UPDATE phone_control_preferences SET mode = 'armed'", [])
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO phone_drive_preferences
                (device_identity, mode, throttle_milliamps, brake_milliamps)
             VALUES ('board-a', 'buttons', 0, 3000)",
                [],
            )
            .is_err()
    );
}

#[test]
fn schema_v58_adds_per_device_phone_settings_without_assigning_legacy_singleton() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-control-v58.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    database
        .remember_selected_device("board-a", Some("Board A"), 1)
        .unwrap();
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP TABLE IF EXISTS phone_drive_preferences;
             UPDATE phone_control_preferences
                SET mode = 'area', throttle_milliamps = 7125, brake_milliamps = 4500
              WHERE id = 1;
             PRAGMA user_version = 58;",
        )
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        PhoneDriveSettingsRecord::default()
    );
    database.shutdown().unwrap();
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT device_identity FROM phone_drive_preferences",
                [],
                |row| row.get::<_, String>(0)
            )
            .optional()
            .unwrap(),
        None,
        "legacy global settings are not copied to the selected device"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT mode, throttle_milliamps, brake_milliamps
                   FROM phone_control_preferences WHERE id = 1",
                [],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?
                )),
            )
            .unwrap(),
        ("area".to_owned(), 7125, 4500),
        "legacy singleton values remain explicitly preserved"
    );
}
