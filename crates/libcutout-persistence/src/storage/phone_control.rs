use super::{Reply, StorageError};
use cutout_core::{
    MonotonicTimestamp, PhoneControlMode, PhoneDriveAction, PhoneDriveContactId,
    PhoneDriveEvidence, PhoneDriveLimits,
};
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

pub(super) const EVIDENCE_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS phone_drive_evidence (
        device_identity TEXT NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
        sequence INTEGER NOT NULL CHECK (sequence > 0),
        phase TEXT NOT NULL CHECK (phase IN ('intent', 'admitted', 'native_receipt', 'observed_effect')),
        action TEXT NOT NULL CHECK (action IN ('throttle', 'brake')),
        contact_id INTEGER NOT NULL CHECK (contact_id > 0),
        observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
        PRIMARY KEY (device_identity, sequence)
    );";

pub(super) fn create_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(SCHEMA_V55)?;
    connection.execute_batch(BUDGET_MIGRATION)?;
    connection.execute_batch(DEVICE_SCHEMA)?;
    ensure_evidence_schema(connection)
}

pub(super) fn ensure_evidence_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(EVIDENCE_SCHEMA)?;
    Ok(())
}

pub(super) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    verify_device_schema(connection)
}

pub(super) fn verify_evidence_schema(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(phone_drive_evidence)")?;
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
        ("sequence".to_owned(), "INTEGER".to_owned(), 1, 2),
        ("phase".to_owned(), "TEXT".to_owned(), 1, 0),
        ("action".to_owned(), "TEXT".to_owned(), 1, 0),
        ("contact_id".to_owned(), "INTEGER".to_owned(), 1, 0),
        ("observed_at_ms".to_owned(), "INTEGER".to_owned(), 1, 0),
    ];
    if columns != expected {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
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
    RecordEvidence {
        device_identity: String,
        evidence: PhoneDriveEvidence,
        reply: Reply<()>,
    },
    LoadEvidence {
        device_identity: String,
        reply: Reply<Vec<PhoneDriveEvidence>>,
    },
}

pub(super) fn dispatch(connection: &mut Connection, command: Command) {
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
                .map_err(StorageError::from)
                .and_then(|affected_rows| {
                    if affected_rows == 1 {
                        Ok(())
                    } else {
                        Err(StorageError::InvalidDatabaseIdentity)
                    }
                });
            let _ = reply.send(result);
        }
        Command::RecordEvidence {
            device_identity,
            evidence,
            reply,
        } => {
            let _ = reply.send(record_evidence(connection, &device_identity, evidence));
        }
        Command::LoadEvidence {
            device_identity,
            reply,
        } => {
            let _ = reply.send(load_evidence(connection, &device_identity));
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

fn record_evidence(
    connection: &mut Connection,
    device_identity: &str,
    evidence: PhoneDriveEvidence,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    let sequence = next_sequence(&transaction, device_identity)?;
    let affected = transaction
        .execute(
            "INSERT INTO phone_drive_evidence
                (device_identity, sequence, phase, action, contact_id, observed_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                device_identity,
                sequence,
                phase_label(evidence),
                encode_action(evidence.action()),
                stored_contact(evidence.contact_id())?,
                stored_observed_at(evidence.observed_at())?,
            ],
        )
        .map_err(StorageError::from)?;
    if affected != 1 {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    transaction.commit()?;
    Ok(())
}

fn load_evidence(
    connection: &Connection,
    device_identity: &str,
) -> Result<Vec<PhoneDriveEvidence>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT phase, action, contact_id, observed_at_ms
           FROM phone_drive_evidence
          WHERE device_identity = ?1
          ORDER BY sequence ASC",
    )?;
    let rows = statement
        .query_map([device_identity], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(phase, action, contact_id, observed_at_ms)| {
            decode_evidence(phase, action, contact_id, observed_at_ms)
        })
        .collect()
}

fn next_sequence(connection: &Connection, device_identity: &str) -> Result<i64, StorageError> {
    let current: i64 = connection.query_row(
        "SELECT COALESCE(MAX(sequence), 0)
           FROM phone_drive_evidence
          WHERE device_identity = ?1",
        [device_identity],
        |row| row.get(0),
    )?;
    current
        .checked_add(1)
        .filter(|sequence| *sequence > 0)
        .ok_or_else(|| StorageError::InvalidStoredValue {
            field: "phone drive evidence sequence",
            value: current.to_string(),
        })
}

fn phase_label(evidence: PhoneDriveEvidence) -> &'static str {
    match evidence {
        PhoneDriveEvidence::Intent(_) => "intent",
        PhoneDriveEvidence::Admitted(_) => "admitted",
        PhoneDriveEvidence::NativeReceipt(_) => "native_receipt",
        PhoneDriveEvidence::ObservedEffect(_) => "observed_effect",
    }
}

fn encode_action(action: PhoneDriveAction) -> &'static str {
    match action {
        PhoneDriveAction::Brake => "brake",
        PhoneDriveAction::Throttle => "throttle",
    }
}

fn stored_contact(contact_id: PhoneDriveContactId) -> Result<i64, StorageError> {
    i64::try_from(contact_id.get()).map_err(|_| StorageError::InvalidStoredValue {
        field: "phone drive evidence contact",
        value: contact_id.get().to_string(),
    })
}

fn stored_observed_at(observed_at: MonotonicTimestamp) -> Result<i64, StorageError> {
    i64::try_from(observed_at.get()).map_err(|_| StorageError::InvalidStoredValue {
        field: "phone drive evidence time",
        value: observed_at.get().to_string(),
    })
}

fn decode_evidence(
    phase: String,
    action: String,
    contact_id: i64,
    observed_at_ms: i64,
) -> Result<PhoneDriveEvidence, StorageError> {
    let action = decode_action(action)?;
    let contact_id = contact_from_row(contact_id)?;
    let observed_at = timestamp_from_row(observed_at_ms)?;
    match phase.as_str() {
        "intent" => Ok(PhoneDriveEvidence::intent(action, contact_id, observed_at)),
        "admitted" => Ok(PhoneDriveEvidence::admitted(
            action,
            contact_id,
            observed_at,
        )),
        "native_receipt" => Ok(PhoneDriveEvidence::native_receipt(
            action,
            contact_id,
            observed_at,
        )),
        "observed_effect" => Ok(PhoneDriveEvidence::observed_effect(
            action,
            contact_id,
            observed_at,
        )),
        _ => Err(StorageError::InvalidStoredValue {
            field: "phone drive evidence phase",
            value: phase,
        }),
    }
}

fn decode_action(value: String) -> Result<PhoneDriveAction, StorageError> {
    match value.as_str() {
        "brake" => Ok(PhoneDriveAction::Brake),
        "throttle" => Ok(PhoneDriveAction::Throttle),
        _ => Err(StorageError::InvalidStoredValue {
            field: "phone drive evidence action",
            value,
        }),
    }
}

fn contact_from_row(value: i64) -> Result<PhoneDriveContactId, StorageError> {
    let raw = u64::try_from(value).map_err(|_| StorageError::InvalidStoredValue {
        field: "phone drive evidence contact",
        value: value.to_string(),
    })?;
    PhoneDriveContactId::new(raw).ok_or_else(|| StorageError::InvalidStoredValue {
        field: "phone drive evidence contact",
        value: value.to_string(),
    })
}

fn timestamp_from_row(value: i64) -> Result<MonotonicTimestamp, StorageError> {
    let raw = u64::try_from(value).map_err(|_| StorageError::InvalidStoredValue {
        field: "phone drive evidence time",
        value: value.to_string(),
    })?;
    Ok(MonotonicTimestamp::new(raw))
}
