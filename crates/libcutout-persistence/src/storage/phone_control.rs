use super::{Reply, StorageError};
use cutout_core::{PhoneControlMode, PhoneDriveLimits};
use rusqlite::Connection;

pub(super) const SCHEMA_V55: &str = "
    CREATE TABLE IF NOT EXISTS phone_control_preferences (
        id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
        mode TEXT NOT NULL CHECK (mode IN ('disabled', 'buttons', 'area'))
    );
    INSERT INTO phone_control_preferences (id, mode) VALUES (1, 'disabled')
        ON CONFLICT (id) DO NOTHING;";

pub(super) const BUDGET_MIGRATION: &str = "
    ALTER TABLE phone_control_preferences ADD COLUMN throttle_milliamps INTEGER NOT NULL DEFAULT 3000 CHECK (throttle_milliamps BETWEEN 1 AND 500000);
    ALTER TABLE phone_control_preferences ADD COLUMN brake_milliamps INTEGER NOT NULL DEFAULT 3000 CHECK (brake_milliamps BETWEEN 1 AND 500000);";

pub(super) const DEVICE_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS phone_drive_preferences (
        device_identity TEXT PRIMARY KEY NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
        mode TEXT NOT NULL CHECK (mode IN ('disabled', 'buttons', 'area')),
        throttle_milliamps INTEGER NOT NULL CHECK (throttle_milliamps BETWEEN 1 AND 500000),
        brake_milliamps INTEGER NOT NULL CHECK (brake_milliamps BETWEEN 1 AND 500000)
    );";

pub(super) fn create_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(SCHEMA_V55)?;
    connection.execute_batch(BUDGET_MIGRATION)?;
    connection.execute_batch(DEVICE_SCHEMA)?;
    Ok(())
}

pub(super) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    verify_device_schema(connection)
}

pub(super) fn verify_legacy_schema(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(phone_control_preferences)")?;
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let expected = [
        ("id".to_owned(), "INTEGER".to_owned(), 1, 1),
        ("mode".to_owned(), "TEXT".to_owned(), 1, 0),
        ("throttle_milliamps".to_owned(), "INTEGER".to_owned(), 1, 0),
        ("brake_milliamps".to_owned(), "INTEGER".to_owned(), 1, 0),
    ];
    if columns != expected {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

fn verify_device_schema(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(phone_drive_preferences)")?;
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let expected = [
        ("device_identity".to_owned(), "TEXT".to_owned(), 1, 1),
        ("mode".to_owned(), "TEXT".to_owned(), 1, 0),
        ("throttle_milliamps".to_owned(), "INTEGER".to_owned(), 1, 0),
        ("brake_milliamps".to_owned(), "INTEGER".to_owned(), 1, 0),
    ];
    if columns != expected {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

pub(super) enum Command {
    LoadForDevice {
        device_identity: String,
        reply: Reply<super::PhoneDriveSettingsRecord>,
    },
    SaveForDevice {
        device_identity: String,
        settings: super::PhoneDriveSettingsRecord,
        reply: Reply<()>,
    },
}

pub(super) fn dispatch(connection: &Connection, command: Command) {
    match command {
        Command::LoadForDevice {
            device_identity,
            reply,
        } => {
            let _ = reply.send(load_for_device(connection, &device_identity));
        }
        Command::SaveForDevice {
            device_identity,
            settings,
            reply,
        } => {
            let result = connection
                .execute(
                    "INSERT INTO phone_drive_preferences
                     (device_identity, mode, throttle_milliamps, brake_milliamps)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(device_identity) DO UPDATE SET
                     mode = excluded.mode,
                     throttle_milliamps = excluded.throttle_milliamps,
                     brake_milliamps = excluded.brake_milliamps",
                    rusqlite::params![
                        device_identity,
                        encode_mode(settings.mode()),
                        settings.limits().throttle_milliamps(),
                        settings.limits().brake_milliamps()
                    ],
                )
                .map(|_| ())
                .map_err(StorageError::from);
            let _ = reply.send(result);
        }
    }
}

fn load_for_device(
    connection: &Connection,
    device_identity: &str,
) -> Result<super::PhoneDriveSettingsRecord, StorageError> {
    use rusqlite::OptionalExtension;

    let stored = connection
        .query_row(
            "SELECT mode, throttle_milliamps, brake_milliamps
               FROM phone_drive_preferences WHERE device_identity = ?1",
            [device_identity],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((mode, throttle, brake)) = stored else {
        return Ok(super::PhoneDriveSettingsRecord::default());
    };
    let invalid = || StorageError::InvalidStoredValue {
        field: "phone drive current budgets",
        value: format!("{throttle},{brake}"),
    };
    let limits = PhoneDriveLimits::new(
        u32::try_from(throttle).map_err(|_| invalid())?,
        u32::try_from(brake).map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())?;
    Ok(super::PhoneDriveSettingsRecord::new(
        decode_mode(mode)?,
        limits,
    ))
}

fn encode_mode(mode: PhoneControlMode) -> &'static str {
    match mode {
        PhoneControlMode::Disabled => "disabled",
        PhoneControlMode::Buttons => "buttons",
        PhoneControlMode::Area => "area",
    }
}

fn decode_mode(value: String) -> Result<PhoneControlMode, StorageError> {
    match value.as_str() {
        "disabled" => Ok(PhoneControlMode::Disabled),
        "buttons" => Ok(PhoneControlMode::Buttons),
        "area" => Ok(PhoneControlMode::Area),
        _ => Err(StorageError::InvalidStoredValue {
            field: "phone control mode",
            value,
        }),
    }
}
