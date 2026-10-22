use super::{Reply, RideDatabase, StorageError};
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

pub(super) fn create_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(SCHEMA_V55)?;
    connection.execute_batch(BUDGET_MIGRATION)?;
    Ok(())
}

pub(super) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
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

pub(super) enum Command {
    LoadLimits {
        reply: Reply<PhoneDriveLimits>,
    },
    SaveLimits {
        limits: PhoneDriveLimits,
        reply: Reply<()>,
    },
    Load {
        reply: Reply<PhoneControlMode>,
    },
    Save {
        mode: PhoneControlMode,
        reply: Reply<()>,
    },
}

impl RideDatabase {
    /// Loads independent current budgets without restoring activation.
    ///
    /// # Errors
    /// Returns storage or corrupt-value failures.
    pub fn phone_drive_limits(&self) -> Result<PhoneDriveLimits, StorageError> {
        self.request(|reply| super::Command::PhoneControl(Command::LoadLimits { reply }))
    }

    /// Saves checked current budgets atomically through the database worker.
    ///
    /// # Errors
    /// Returns the actual storage failure.
    pub fn save_phone_drive_limits(&self, limits: PhoneDriveLimits) -> Result<(), StorageError> {
        self.request(|reply| super::Command::PhoneControl(Command::SaveLimits { limits, reply }))
    }

    /// Loads the saved phone input layout.
    ///
    /// # Errors
    /// Returns [`StorageError`] if the preference cannot be read or is corrupt.
    pub fn phone_control_mode(&self) -> Result<PhoneControlMode, StorageError> {
        self.request(|reply| super::Command::PhoneControl(Command::Load { reply }))
    }

    /// Saves the phone input layout.
    ///
    /// # Errors
    /// Returns [`StorageError`] if the preference cannot be saved.
    pub fn save_phone_control_mode(&self, mode: PhoneControlMode) -> Result<(), StorageError> {
        self.request(|reply| super::Command::PhoneControl(Command::Save { mode, reply }))
    }
}

pub(super) fn dispatch(connection: &Connection, command: Command) {
    match command {
        Command::LoadLimits { reply } => {
            let _ = reply.send(load_limits(connection));
        }
        Command::SaveLimits { limits, reply } => {
            let result = connection.execute(
                "UPDATE phone_control_preferences SET throttle_milliamps = ?1, brake_milliamps = ?2 WHERE id = 1",
                rusqlite::params![limits.throttle_milliamps(), limits.brake_milliamps()],
            ).map_err(StorageError::from).and_then(|updated| {
                if updated == 1 { Ok(()) } else { Err(StorageError::InvalidDatabaseIdentity) }
            });
            let _ = reply.send(result);
        }
        Command::Load { reply } => {
            let _ = reply.send(load(connection));
        }
        Command::Save { mode, reply } => {
            let _ = reply.send(save(connection, mode));
        }
    }
}

fn load_limits(connection: &Connection) -> Result<PhoneDriveLimits, StorageError> {
    let (throttle, brake): (i64, i64) = connection.query_row(
        "SELECT throttle_milliamps, brake_milliamps FROM phone_control_preferences WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let invalid = || StorageError::InvalidStoredValue {
        field: "phone drive current budgets",
        value: format!("{throttle},{brake}"),
    };
    PhoneDriveLimits::new(
        u32::try_from(throttle).map_err(|_| invalid())?,
        u32::try_from(brake).map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())
}

fn load(connection: &Connection) -> Result<PhoneControlMode, StorageError> {
    let value: String = connection.query_row(
        "SELECT mode FROM phone_control_preferences WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
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

fn save(connection: &Connection, mode: PhoneControlMode) -> Result<(), StorageError> {
    let value = match mode {
        PhoneControlMode::Disabled => "disabled",
        PhoneControlMode::Buttons => "buttons",
        PhoneControlMode::Area => "area",
    };
    let updated = connection.execute(
        "UPDATE phone_control_preferences SET mode = ?1 WHERE id = 1",
        [value],
    )?;
    if updated != 1 {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}
