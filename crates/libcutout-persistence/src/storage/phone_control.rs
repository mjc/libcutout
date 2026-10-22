use super::{Reply, RideDatabase, StorageError};
use cutout_core::PhoneControlMode;
use rusqlite::Connection;

pub(super) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS phone_control_preferences (
        id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
        mode TEXT NOT NULL CHECK (mode IN ('disabled', 'buttons', 'area'))
    );
    INSERT INTO phone_control_preferences (id, mode) VALUES (1, 'disabled')
        ON CONFLICT (id) DO NOTHING;";

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
    ];
    if columns != expected {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

pub(super) enum Command {
    Load {
        reply: Reply<PhoneControlMode>,
    },
    Save {
        mode: PhoneControlMode,
        reply: Reply<()>,
    },
}

impl RideDatabase {
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
        Command::Load { reply } => {
            let _ = reply.send(load(connection));
        }
        Command::Save { mode, reply } => {
            let _ = reply.send(save(connection, mode));
        }
    }
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
