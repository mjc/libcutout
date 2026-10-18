//! Local per-device drivetrain facts in the existing database worker.
use super::{Reply, RideDatabase, StorageError, normalize_stored_text};
use cutout_core::{VescDrivetrain, VescDrivetrainProfile, VescDrivetrainRevision};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) const SCHEMA_V51: &str = "CREATE TABLE IF NOT EXISTS device_vesc_drivetrains (
 platform_identifier TEXT PRIMARY KEY NOT NULL CHECK(length(platform_identifier) BETWEEN 1 AND 512),
 diameter_micrometres INTEGER NOT NULL CHECK(diameter_micrometres BETWEEN 160 AND 4294967295),
 pole_pairs INTEGER NOT NULL CHECK(pole_pairs BETWEEN 1 AND 255),
 ratio_milli INTEGER NOT NULL CHECK(ratio_milli BETWEEN 1 AND 4294967295),
 updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0)
);";

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS device_vesc_drivetrains (
 platform_identifier TEXT PRIMARY KEY NOT NULL CHECK(length(platform_identifier) BETWEEN 1 AND 512),
 diameter_micrometres INTEGER NOT NULL CHECK(diameter_micrometres BETWEEN 160 AND 4294967295),
 pole_pairs INTEGER NOT NULL CHECK(pole_pairs BETWEEN 1 AND 255),
 ratio_milli INTEGER NOT NULL CHECK(ratio_milli BETWEEN 1 AND 4294967295),
 updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
 revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0)
);";

pub(super) enum Command {
    Save {
        identifier: String,
        value: VescDrivetrain,
        at: u64,
        reply: Reply<VescDrivetrainProfile>,
    },
    Load {
        identifier: String,
        reply: Reply<Option<VescDrivetrainProfile>>,
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
    ) -> Result<VescDrivetrainProfile, StorageError> {
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
    ) -> Result<Option<VescDrivetrainProfile>, StorageError> {
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
            let result = save(connection, &identifier, value, at);
            let _ = reply.send(result);
        }
        Command::Load { identifier, reply } => {
            let _ = reply.send(load(connection, &identifier));
        }
    }
}
fn save(
    connection: &Connection,
    identifier: &str,
    value: VescDrivetrain,
    at: u64,
) -> Result<VescDrivetrainProfile, StorageError> {
    let transaction = connection.unchecked_transaction()?;
    let previous = load(&transaction, identifier)?;
    let revision = match previous {
        Some(previous) if previous.value() == value => previous.revision(),
        Some(previous) => previous
            .revision()
            .checked_next()
            .ok_or_else(revision_exhausted)?,
        None => VescDrivetrainRevision::new(1).ok_or(StorageError::InvalidDatabaseIdentity)?,
    };
    // SQLite INTEGER is signed; never let an increment promote to REAL or lose precision.
    let stored_revision = i64::try_from(revision.get()).map_err(|_| revision_exhausted())?;
    transaction.execute("INSERT INTO device_vesc_drivetrains (platform_identifier,diameter_micrometres,pole_pairs,ratio_milli,updated_at_ms,revision) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(platform_identifier) DO UPDATE SET diameter_micrometres=excluded.diameter_micrometres,pole_pairs=excluded.pole_pairs,ratio_milli=excluded.ratio_milli,updated_at_ms=excluded.updated_at_ms,revision=excluded.revision",
        params![identifier,value.diameter_micrometres(),value.geometry().motor_pole_pairs.get(),value.geometry().gear_ratio_denominator.as_milli(),at,stored_revision])?;
    transaction.commit()?;
    Ok(VescDrivetrainProfile::user_configured(value, revision))
}

fn revision_exhausted() -> StorageError {
    StorageError::InvalidStoredValue {
        field: "VESC drivetrain revision",
        value: "revision exhausted".to_owned(),
    }
}

fn load(
    connection: &Connection,
    identifier: &str,
) -> Result<Option<VescDrivetrainProfile>, StorageError> {
    let raw: Option<(u32,u8,u32,u64)> = connection.query_row("SELECT diameter_micrometres,pole_pairs,ratio_milli,revision FROM device_vesc_drivetrains WHERE platform_identifier=?1", [identifier], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
    raw.map(|(diameter, poles, ratio, revision)| {
        let value = VescDrivetrain::from_parts(diameter, poles, ratio).map_err(|_| {
            StorageError::InvalidStoredValue {
                field: "VESC drivetrain",
                value: "invalid geometry".to_owned(),
            }
        })?;
        let revision = VescDrivetrainRevision::new(revision).ok_or_else(|| {
            StorageError::InvalidStoredValue {
                field: "VESC drivetrain revision",
                value: "invalid revision".to_owned(),
            }
        })?;
        Ok(VescDrivetrainProfile::user_configured(value, revision))
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
        ("revision", "INTEGER", 1, 0),
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
    fn user_drivetrain_revision_changes_only_for_changed_canonical_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        let database = RideDatabase::open(&path).unwrap();
        let original = VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let same = VescDrivetrain::parse("90.000", "07", "2.5000").unwrap();
        let changed = VescDrivetrain::parse("100", "7", "2.5").unwrap();
        let first = database
            .save_user_vesc_drivetrain("controller-a", original, 1)
            .unwrap();
        assert_eq!(first.revision().get(), 1);
        assert_eq!(
            first.source(),
            cutout_core::VescDrivetrainSource::UserConfigured
        );
        assert_eq!(
            database
                .save_user_vesc_drivetrain("controller-a", same, 2)
                .unwrap(),
            first
        );
        let second = database
            .save_user_vesc_drivetrain("controller-a", changed, 3)
            .unwrap();
        assert_eq!(second.revision().get(), 2);
        assert_eq!(second.value(), changed);
        assert_eq!(
            database
                .save_user_vesc_drivetrain("controller-b", changed, 4)
                .unwrap()
                .revision()
                .get(),
            1
        );
        database.shutdown().unwrap();
        let reopened = RideDatabase::open(&path).unwrap();
        assert_eq!(
            reopened.user_vesc_drivetrain("controller-a").unwrap(),
            Some(second)
        );
        reopened.shutdown().unwrap();
    }

    #[test]
    fn user_drivetrain_storage_roundtrips_after_reopen_and_is_device_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        let value = VescDrivetrain::parse("90.125", "7", "2.5000").unwrap();
        let database = RideDatabase::open(&path).unwrap();
        let saved = database
            .save_user_vesc_drivetrain("controller-a", value, 1)
            .unwrap();
        assert_eq!(database.user_vesc_drivetrain("controller-b").unwrap(), None);
        database.shutdown().unwrap();
        let reopened = RideDatabase::open(&path).unwrap();
        assert_eq!(
            reopened.user_vesc_drivetrain("controller-a").unwrap(),
            Some(saved)
        );
        reopened.shutdown().unwrap();
    }

    #[test]
    fn user_drivetrain_revision_migration_from_51_keeps_existing_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        RideDatabase::open(&path).unwrap().shutdown().unwrap();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP TABLE device_vesc_drivetrains;")
            .unwrap();
        connection.execute_batch(SCHEMA_V51).unwrap();
        connection
            .execute(
                "INSERT INTO device_vesc_drivetrains VALUES (?1,?2,?3,?4,?5)",
                params!["controller-a", 90_000, 7, 2_500, 17],
            )
            .unwrap();
        connection.pragma_update(None, "user_version", 51).unwrap();
        drop(connection);
        let migrated = RideDatabase::open(&path).unwrap();
        let profile = migrated
            .user_vesc_drivetrain("controller-a")
            .unwrap()
            .unwrap();
        assert_eq!(
            profile.value(),
            VescDrivetrain::parse("90", "7", "2.5").unwrap()
        );
        assert_eq!(profile.revision().get(), 1);
        migrated.shutdown().unwrap();
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            52
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT updated_at_ms FROM device_vesc_drivetrains",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            17
        );
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
            52
        );
    }

    #[test]
    fn user_drivetrain_revision_overflow_and_write_failure_preserve_prior_profile() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        let first = VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let changed = VescDrivetrain::parse("100", "7", "2.5").unwrap();
        let committed = save(&connection, "controller-a", first, 1).unwrap();
        assert!(save(&connection, "controller-a", changed, u64::MAX).is_err());
        assert_eq!(load(&connection, "controller-a").unwrap(), Some(committed));
        connection
            .execute("UPDATE device_vesc_drivetrains SET revision=?1", [i64::MAX])
            .unwrap();
        let last = load(&connection, "controller-a").unwrap().unwrap();
        assert_eq!(last.revision().get(), u64::try_from(i64::MAX).unwrap());
        assert_eq!(
            save(&connection, "controller-a", first, 2).unwrap(),
            last,
            "unchanged geometry does not exhaust a revision"
        );
        assert!(save(&connection, "controller-a", changed, 3).is_err());
        assert_eq!(load(&connection, "controller-a").unwrap(), Some(last));
        assert_eq!(
            connection
                .query_row(
                    "SELECT updated_at_ms FROM device_vesc_drivetrains",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
    }

    #[test]
    fn user_drivetrain_corrupt_revision_is_not_repaired_by_saving_geometry() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        let value = VescDrivetrain::parse("90", "7", "2.5").unwrap();
        save(&connection, "controller-a", value, 1).unwrap();
        connection
            .execute_batch("PRAGMA ignore_check_constraints=ON;")
            .unwrap();
        for corrupt in [
            rusqlite::types::Value::Integer(0),
            rusqlite::types::Value::Integer(-1),
            rusqlite::types::Value::Text("corrupt".to_owned()),
        ] {
            connection
                .execute("UPDATE device_vesc_drivetrains SET revision=?1", [&corrupt])
                .unwrap();
            assert!(load(&connection, "controller-a").is_err());
            assert!(save(&connection, "controller-a", value, 2).is_err());
            let retained = connection
                .query_row("SELECT revision FROM device_vesc_drivetrains", [], |row| {
                    row.get::<_, rusqlite::types::Value>(0)
                })
                .unwrap();
            assert_eq!(retained, corrupt);
        }
    }

    #[test]
    fn user_drivetrain_corrupt_geometry_is_rejected_on_read() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection.execute_batch("PRAGMA ignore_check_constraints=ON; INSERT INTO device_vesc_drivetrains (platform_identifier,diameter_micrometres,pole_pairs,ratio_milli,updated_at_ms) VALUES ('controller-a',0,7,2500,1);").unwrap();
        assert!(load(&connection, "controller-a").is_err());
    }
}
