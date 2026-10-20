//! Per-device user classification within the existing device database owner.

use super::{Reply, RideDatabase, StorageError, normalize_stored_text};
use cutout_core::{VehicleClassification, VehicleKind};
use rusqlite::{Connection, OptionalExtension, params};

// Device names are optional metadata in practice, but the existing devices table
// requires a name. Keep classification beside that metadata without fabricating one.
pub(super) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS device_vehicle_classifications (
        platform_identifier TEXT PRIMARY KEY NOT NULL CHECK (length(platform_identifier) BETWEEN 1 AND 512),
        kind TEXT NOT NULL CHECK (kind IN ('electric_unicycle', 'onewheel', 'scooter', 'eskate', 'ebike', 'emoto')),
        provenance TEXT NOT NULL CHECK (provenance = 'user_provided'),
        updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
    );";

pub(super) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(device_vehicle_classifications)")?;
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
        ("platform_identifier".to_owned(), "TEXT".to_owned(), 1, 1),
        ("kind".to_owned(), "TEXT".to_owned(), 1, 0),
        ("provenance".to_owned(), "TEXT".to_owned(), 1, 0),
        ("updated_at_ms".to_owned(), "INTEGER".to_owned(), 1, 0),
    ];
    if columns != expected {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

pub(super) enum Command {
    Save {
        platform_identifier: String,
        classification: VehicleClassification,
        updated_at_ms: u64,
        reply: Reply<()>,
    },
    Load {
        platform_identifier: String,
        reply: Reply<Option<VehicleClassification>>,
    },
    Clear {
        platform_identifier: String,
        reply: Reply<()>,
    },
}

impl RideDatabase {
    /// Saves a physical type chosen by the user for one platform-local device.
    ///
    /// This does not select the device, invent its name, identify a model, or retain protocol proof.
    ///
    /// # Errors
    /// Returns [`StorageError`] for an invalid identifier, unknown type or failed database operation.
    pub fn save_user_vehicle_kind(
        &self,
        platform_identifier: &str,
        kind: VehicleKind,
        updated_at_ms: u64,
    ) -> Result<(), StorageError> {
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        let classification = VehicleClassification::user_provided(kind).ok_or_else(|| {
            StorageError::InvalidStoredValue {
                field: "vehicle kind",
                value: "unknown".to_owned(),
            }
        })?;
        self.request(move |reply| {
            super::Command::DeviceVehicle(Command::Save {
                platform_identifier,
                classification,
                updated_at_ms,
                reply,
            })
        })
    }

    /// Loads the user's physical-type choice for this device, with its provenance.
    ///
    /// # Errors
    /// Returns [`StorageError`] for an invalid identifier, corrupt classification or failed query.
    pub fn device_vehicle_classification(
        &self,
        platform_identifier: &str,
    ) -> Result<Option<VehicleClassification>, StorageError> {
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        self.request(move |reply| {
            super::Command::DeviceVehicle(Command::Load {
                platform_identifier,
                reply,
            })
        })
    }

    /// Clears only this device's saved physical type, preserving its name and selection.
    ///
    /// # Errors
    /// Returns [`StorageError`] for an invalid identifier or failed database operation.
    pub fn clear_user_vehicle_kind(&self, platform_identifier: &str) -> Result<(), StorageError> {
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        self.request(move |reply| {
            super::Command::DeviceVehicle(Command::Clear {
                platform_identifier,
                reply,
            })
        })
    }
}

pub(super) fn dispatch(connection: &Connection, command: Command) {
    match command {
        Command::Save {
            platform_identifier,
            classification,
            updated_at_ms,
            reply,
        } => {
            let result = connection.execute(
                "INSERT INTO device_vehicle_classifications (platform_identifier, kind, provenance, updated_at_ms)
                 VALUES (?1, ?2, 'user_provided', ?3)
                 ON CONFLICT(platform_identifier) DO UPDATE SET kind = excluded.kind,
                    provenance = excluded.provenance, updated_at_ms = excluded.updated_at_ms",
                params![platform_identifier, kind_key(classification.kind()), updated_at_ms],
            ).map(|_| ()).map_err(Into::into);
            let _ = reply.send(result);
        }
        Command::Load {
            platform_identifier,
            reply,
        } => {
            let _ = reply.send(load(connection, &platform_identifier));
        }
        Command::Clear {
            platform_identifier,
            reply,
        } => {
            let result = connection
                .execute(
                    "DELETE FROM device_vehicle_classifications WHERE platform_identifier = ?1",
                    [platform_identifier],
                )
                .map(|_| ())
                .map_err(Into::into);
            let _ = reply.send(result);
        }
    }
}

fn kind_key(kind: VehicleKind) -> &'static str {
    match kind {
        VehicleKind::Unknown => "unknown",
        VehicleKind::ElectricUnicycle => "electric_unicycle",
        VehicleKind::Onewheel => "onewheel",
        VehicleKind::Scooter => "scooter",
        VehicleKind::Eskate => "eskate",
        VehicleKind::Ebike => "ebike",
        VehicleKind::Emoto => "emoto",
    }
}

fn load(
    connection: &Connection,
    platform_identifier: &str,
) -> Result<Option<VehicleClassification>, StorageError> {
    let stored: Option<(String, String)> = connection.query_row(
        "SELECT kind, provenance FROM device_vehicle_classifications WHERE platform_identifier = ?1",
        [platform_identifier], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    let Some((kind, provenance)) = stored else {
        return Ok(None);
    };
    let kind = match (kind.as_str(), provenance.as_str()) {
        ("electric_unicycle", "user_provided") => VehicleKind::ElectricUnicycle,
        ("onewheel", "user_provided") => VehicleKind::Onewheel,
        ("scooter", "user_provided") => VehicleKind::Scooter,
        ("eskate", "user_provided") => VehicleKind::Eskate,
        ("ebike", "user_provided") => VehicleKind::Ebike,
        ("emoto", "user_provided") => VehicleKind::Emoto,
        _ => {
            return Err(StorageError::InvalidStoredValue {
                field: "vehicle classification",
                value: "invalid physical type or provenance".to_owned(),
            });
        }
    };
    Ok(VehicleClassification::user_provided(kind))
}
