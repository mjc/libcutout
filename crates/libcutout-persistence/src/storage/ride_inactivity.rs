use super::{RideId, StorageError, state_from_db};
use cutout_ride_maps::{
    RideAppPresence, RideAppPresenceOrder, RideInactivityEvidence, RideLifecycleState,
    WallClockUnixMilliseconds,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ride_inactivity_evidence (
    ride_id TEXT PRIMARY KEY NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
    presence_order INTEGER CHECK (presence_order IS NULL OR presence_order > 0),
    presence_observed_at_ms INTEGER CHECK (
        presence_observed_at_ms IS NULL OR presence_observed_at_ms >= 0
    ),
    app_inactive_since_ms INTEGER CHECK (
        app_inactive_since_ms IS NULL OR app_inactive_since_ms >= 0
    ),
    last_verified_device_seen_ms INTEGER CHECK (
        last_verified_device_seen_ms IS NULL OR last_verified_device_seen_ms >= 0
    ),
    CHECK (
        (presence_order IS NULL) = (presence_observed_at_ms IS NULL)
    ),
    CHECK (
        app_inactive_since_ms IS NULL OR (
            presence_order IS NOT NULL
            AND presence_observed_at_ms IS NOT NULL
        )
    )
);
";

pub(crate) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(ride_inactivity_evidence)")?;
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
    let expected_columns = vec![
        ("ride_id".to_owned(), "TEXT".to_owned(), 1, 1),
        ("presence_order".to_owned(), "INTEGER".to_owned(), 0, 0),
        (
            "presence_observed_at_ms".to_owned(),
            "INTEGER".to_owned(),
            0,
            0,
        ),
        (
            "app_inactive_since_ms".to_owned(),
            "INTEGER".to_owned(),
            0,
            0,
        ),
        (
            "last_verified_device_seen_ms".to_owned(),
            "INTEGER".to_owned(),
            0,
            0,
        ),
    ];
    if columns != expected_columns {
        return Err(StorageError::InvalidDatabaseIdentity);
    }

    let mut statement = connection.prepare("PRAGMA foreign_key_list(ride_inactivity_evidence)")?;
    let foreign_keys = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let expected_foreign_keys = vec![(
        "rides".to_owned(),
        "ride_id".to_owned(),
        "id".to_owned(),
        "CASCADE".to_owned(),
    )];
    if foreign_keys != expected_foreign_keys {
        return Err(StorageError::InvalidDatabaseIdentity);
    }

    let definition: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_schema
             WHERE type = 'table' AND name = 'ride_inactivity_evidence'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let Some(definition) = definition else {
        return Err(StorageError::InvalidDatabaseIdentity);
    };
    let normalized: String = definition
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    for constraint in [
        "check(presence_orderisnullorpresence_order>0)",
        "check(presence_observed_at_msisnullorpresence_observed_at_ms>=0)",
        "check(app_inactive_since_msisnullorapp_inactive_since_ms>=0)",
        "check(last_verified_device_seen_msisnullorlast_verified_device_seen_ms>=0)",
        "check((presence_orderisnull)=(presence_observed_at_msisnull))",
        "check(app_inactive_since_msisnullor(presence_orderisnotnullandpresence_observed_at_msisnotnull))",
    ] {
        if !normalized.contains(constraint) {
            return Err(StorageError::InvalidDatabaseIdentity);
        }
    }
    Ok(())
}

pub(crate) fn read(
    connection: &Connection,
    ride_id: RideId,
) -> Result<Option<RideInactivityEvidence>, StorageError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE id = ?1)",
        [ride_id.uuid().to_string()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::NotFound);
    }

    read_row(connection, ride_id)
}

pub(crate) fn observe_app_presence(
    connection: &mut Connection,
    ride_id: RideId,
    order: RideAppPresenceOrder,
    presence: RideAppPresence,
    observed_at: WallClockUnixMilliseconds,
) -> Result<RideInactivityEvidence, StorageError> {
    timestamp_to_sql(Some(observed_at), "app presence timestamp")?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure_recoverable_ride(&transaction, ride_id)?;
    let mut evidence = read_row(&transaction, ride_id)?.unwrap_or_default();
    if evidence.observe_app_presence(order, presence, observed_at) {
        write_row(&transaction, ride_id, evidence)?;
    }
    transaction.commit()?;
    Ok(evidence)
}

pub(crate) fn observe_verified_device_seen(
    connection: &mut Connection,
    ride_id: RideId,
    observed_at: WallClockUnixMilliseconds,
) -> Result<RideInactivityEvidence, StorageError> {
    timestamp_to_sql(Some(observed_at), "verified device timestamp")?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure_recoverable_ride(&transaction, ride_id)?;
    let mut evidence = read_row(&transaction, ride_id)?.unwrap_or_default();
    if evidence.observe_verified_device_seen(observed_at) {
        write_row(&transaction, ride_id, evidence)?;
    }
    transaction.commit()?;
    Ok(evidence)
}

fn ensure_recoverable_ride(connection: &Connection, ride_id: RideId) -> Result<(), StorageError> {
    let state = connection
        .query_row(
            "SELECT state FROM rides WHERE id = ?1",
            [ride_id.uuid().to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(StorageError::NotFound)?;
    let state = state_from_db(&state)?;
    match state {
        RideLifecycleState::Active
        | RideLifecycleState::Paused
        | RideLifecycleState::Interrupted => Ok(()),
        RideLifecycleState::Draft
        | RideLifecycleState::Stopped
        | RideLifecycleState::Discarded
        | RideLifecycleState::Saved
        | RideLifecycleState::Imported => Err(StorageError::InvalidRideState(state)),
    }
}

fn read_row(
    connection: &Connection,
    ride_id: RideId,
) -> Result<Option<RideInactivityEvidence>, StorageError> {
    let parts = connection
        .query_row(
            "SELECT presence_observed_at_ms, app_inactive_since_ms,
                    last_verified_device_seen_ms, presence_order
             FROM ride_inactivity_evidence
             WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?;
    parts
        .map(|(presence, inactive, device, order)| {
            RideInactivityEvidence::from_state_parts(
                order_from_sql(order)?,
                timestamp_from_sql(presence, "app presence timestamp")?,
                timestamp_from_sql(inactive, "app inactivity timestamp")?,
                timestamp_from_sql(device, "verified device timestamp")?,
            )
            .map_err(|_| invalid_timestamp_tuple(order, presence, inactive, device))
        })
        .transpose()
}

fn write_row(
    connection: &Connection,
    ride_id: RideId,
    evidence: RideInactivityEvidence,
) -> Result<(), StorageError> {
    let presence = timestamp_to_sql(evidence.presence_observed_at(), "app presence timestamp")?;
    let inactive = timestamp_to_sql(evidence.app_inactive_since(), "app inactivity timestamp")?;
    let order = order_to_sql(evidence.presence_order())?;
    let device = timestamp_to_sql(
        evidence.last_verified_device_seen(),
        "verified device timestamp",
    )?;
    let affected_rows = connection.execute(
        "INSERT INTO ride_inactivity_evidence (
            ride_id, presence_order, presence_observed_at_ms, app_inactive_since_ms,
            last_verified_device_seen_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(ride_id) DO UPDATE SET
            presence_order = excluded.presence_order,
            presence_observed_at_ms = excluded.presence_observed_at_ms,
            app_inactive_since_ms = excluded.app_inactive_since_ms,
            last_verified_device_seen_ms = excluded.last_verified_device_seen_ms",
        params![
            ride_id.uuid().to_string(),
            order,
            presence,
            inactive,
            device
        ],
    )?;
    if affected_rows != 1 {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

/// Persists a Core-owned snapshot alongside an already accepted material write.
///
/// Presence order and verified-device time are monotone, and a repeated order must retain the
/// exact same presence projection. No SQL write occurs when the snapshot is unchanged.
pub(crate) fn persist_snapshot(
    connection: &Connection,
    ride_id: RideId,
    evidence: RideInactivityEvidence,
) -> Result<(), StorageError> {
    ensure_recoverable_ride(connection, ride_id)?;
    let current = read_row(connection, ride_id)?.unwrap_or_default();
    let merged = current.merge(&evidence);
    if current == merged {
        return Ok(());
    }
    write_row(connection, ride_id, merged)
}

fn timestamp_from_sql(
    value: Option<i64>,
    field: &'static str,
) -> Result<Option<WallClockUnixMilliseconds>, StorageError> {
    value
        .map(|value| {
            u64::try_from(value)
                .map(WallClockUnixMilliseconds::new)
                .map_err(|_| StorageError::InvalidStoredValue {
                    field,
                    value: value.to_string(),
                })
        })
        .transpose()
}

fn timestamp_to_sql(
    value: Option<WallClockUnixMilliseconds>,
    field: &'static str,
) -> Result<Option<i64>, StorageError> {
    value
        .map(|value| {
            i64::try_from(value.as_u64()).map_err(|_| StorageError::InvalidStoredValue {
                field,
                value: value.as_u64().to_string(),
            })
        })
        .transpose()
}

fn invalid_timestamp_tuple(
    order: Option<i64>,
    presence: Option<i64>,
    inactive: Option<i64>,
    device: Option<i64>,
) -> StorageError {
    StorageError::InvalidStoredValue {
        field: "ride inactivity evidence",
        value: format!(
            "order={order:?}, presence={presence:?}, inactive={inactive:?}, device={device:?}"
        ),
    }
}

fn order_from_sql(value: Option<i64>) -> Result<Option<RideAppPresenceOrder>, StorageError> {
    value
        .map(|value| {
            u64::try_from(value)
                .ok()
                .and_then(RideAppPresenceOrder::new)
                .ok_or_else(|| StorageError::InvalidStoredValue {
                    field: "app presence order",
                    value: value.to_string(),
                })
        })
        .transpose()
}

fn order_to_sql(value: Option<RideAppPresenceOrder>) -> Result<Option<i64>, StorageError> {
    value
        .map(|value| {
            i64::try_from(value.get()).map_err(|_| StorageError::InvalidStoredValue {
                field: "app presence order",
                value: value.get().to_string(),
            })
        })
        .transpose()
}
