//! Local per-device drivetrain facts in the existing database worker.
use super::{Reply, RideDatabase, StorageError, normalize_stored_text};
use cutout_core::VescDrivetrain;
use rusqlite::{Connection, OptionalExtension, params};

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS device_vesc_drivetrains (
 platform_identifier TEXT PRIMARY KEY NOT NULL CHECK(length(platform_identifier) BETWEEN 1 AND 512),
 diameter_micrometres INTEGER NOT NULL CHECK(diameter_micrometres BETWEEN 160 AND 4294967295),
 pole_pairs INTEGER NOT NULL CHECK(pole_pairs BETWEEN 1 AND 255),
 ratio_milli INTEGER NOT NULL CHECK(ratio_milli BETWEEN 1 AND 4294967295),
 updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0)
);";

pub(super) enum Command {
    Save {
        identifier: String,
        value: VescDrivetrain,
        at: u64,
        reply: Reply<()>,
    },
    Load {
        identifier: String,
        reply: Reply<Option<VescDrivetrain>>,
    },
}
impl RideDatabase {
    /// Commits checked user geometry for one device; does not write to its controller.
    ///
    /// # Errors
    /// Returns invalid device identity, timestamp range or database failure.
    pub fn save_user_vesc_drivetrain(
        &self,
        identifier: &str,
        value: VescDrivetrain,
        at: u64,
    ) -> Result<(), StorageError> {
        let identifier = normalize_stored_text(identifier, "platform identifier")?;
        self.request(move |reply| {
            super::Command::DeviceDrivetrain(Command::Save {
                identifier,
                value,
                at,
                reply,
            })
        })
    }
    /// Restores checked user geometry for this exact platform-local device.
    ///
    /// # Errors
    /// Returns invalid device identity, corrupt stored geometry or database failure.
    pub fn user_vesc_drivetrain(
        &self,
        identifier: &str,
    ) -> Result<Option<VescDrivetrain>, StorageError> {
        let identifier = normalize_stored_text(identifier, "platform identifier")?;
        self.request(move |reply| {
            super::Command::DeviceDrivetrain(Command::Load { identifier, reply })
        })
    }
}
pub(super) fn dispatch(connection: &Connection, command: Command) {
    match command {
        Command::Save {
            identifier,
            value,
            at,
            reply,
        } => {
            let result = connection.execute("INSERT INTO device_vesc_drivetrains (platform_identifier,diameter_micrometres,pole_pairs,ratio_milli,updated_at_ms) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(platform_identifier) DO UPDATE SET diameter_micrometres=excluded.diameter_micrometres,pole_pairs=excluded.pole_pairs,ratio_milli=excluded.ratio_milli,updated_at_ms=excluded.updated_at_ms",
                params![identifier,value.diameter_micrometres(),value.geometry().motor_pole_pairs.get(),value.geometry().gear_ratio_denominator.as_milli(),at]).map(|_| ()).map_err(Into::into);
            let _ = reply.send(result);
        }
        Command::Load { identifier, reply } => {
            let _ = reply.send(load(connection, &identifier));
        }
    }
}
fn load(connection: &Connection, identifier: &str) -> Result<Option<VescDrivetrain>, StorageError> {
    let raw: Option<(u32,u8,u32)> = connection.query_row("SELECT diameter_micrometres,pole_pairs,ratio_milli FROM device_vesc_drivetrains WHERE platform_identifier=?1", [identifier], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    raw.map(|(diameter, poles, ratio)| {
        VescDrivetrain::from_parts(diameter, poles, ratio).map_err(|_| {
            StorageError::InvalidStoredValue {
                field: "VESC drivetrain",
                value: "invalid geometry".to_owned(),
            }
        })
    })
    .transpose()
}

pub(super) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(device_vesc_drivetrains)")?;
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
        ("platform_identifier", "TEXT", 1, 1),
        ("diameter_micrometres", "INTEGER", 1, 0),
        ("pole_pairs", "INTEGER", 1, 0),
        ("ratio_milli", "INTEGER", 1, 0),
        ("updated_at_ms", "INTEGER", 1, 0),
    ]
    .map(|(name, kind, required, key)| (name.to_owned(), kind.to_owned(), required, key));
    if columns != expected {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_drivetrain_storage_roundtrips_after_reopen_and_is_device_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        let value = VescDrivetrain::parse("90.125", "7", "2.5000").unwrap();
        let database = RideDatabase::open(&path).unwrap();
        database
            .save_user_vesc_drivetrain("controller-a", value, 1)
            .unwrap();
        assert_eq!(database.user_vesc_drivetrain("controller-b").unwrap(), None);
        database.shutdown().unwrap();
        let reopened = RideDatabase::open(&path).unwrap();
        assert_eq!(
            reopened.user_vesc_drivetrain("controller-a").unwrap(),
            Some(value)
        );
        reopened.shutdown().unwrap();
    }

    #[test]
    fn user_drivetrain_migration_from_50_preserves_device_classification() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        let database = RideDatabase::open(&path).unwrap();
        database
            .save_user_vehicle_kind("controller-a", cutout_core::VehicleKind::Eskate, 1)
            .unwrap();
        database.shutdown().unwrap();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP TABLE device_vesc_drivetrains; PRAGMA user_version=50;")
            .unwrap();
        drop(connection);
        let migrated = RideDatabase::open(&path).unwrap();
        assert_eq!(
            migrated
                .device_vehicle_classification("controller-a")
                .unwrap()
                .unwrap()
                .kind(),
            cutout_core::VehicleKind::Eskate
        );
        assert_eq!(migrated.user_vesc_drivetrain("controller-a").unwrap(), None);
        let value = VescDrivetrain::parse("90", "7", "2.5").unwrap();
        migrated
            .save_user_vesc_drivetrain("controller-a", value, 2)
            .unwrap();
        migrated.shutdown().unwrap();
        let connection = Connection::open(path).unwrap();
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            51
        );
    }

    #[test]
    fn user_drivetrain_corrupt_geometry_is_rejected_on_read() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection.execute_batch("PRAGMA ignore_check_constraints=ON; INSERT INTO device_vesc_drivetrains VALUES ('controller-a',0,7,2500,1);").unwrap();
        assert!(load(&connection, "controller-a").is_err());
    }
}
