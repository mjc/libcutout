//! Local per-device drivetrain facts in the existing database worker.
use super::{Reply, RideDatabase, StorageError, normalize_stored_text};
use cutout_core::{
    VescDrivetrain, VescDrivetrainProfile, VescDrivetrainRevision, VescDrivetrainTarget,
};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) const SCHEMA_V51: &str = "CREATE TABLE IF NOT EXISTS device_vesc_drivetrains (
 platform_identifier TEXT PRIMARY KEY NOT NULL CHECK(length(platform_identifier) BETWEEN 1 AND 512),
 diameter_micrometres INTEGER NOT NULL CHECK(diameter_micrometres BETWEEN 160 AND 4294967295),
 pole_pairs INTEGER NOT NULL CHECK(pole_pairs BETWEEN 1 AND 255),
 ratio_milli INTEGER NOT NULL CHECK(ratio_milli BETWEEN 1 AND 4294967295),
 updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0)
);";

#[cfg(test)]
pub(super) const SCHEMA_V52: &str = "CREATE TABLE IF NOT EXISTS device_vesc_drivetrains (
 platform_identifier TEXT PRIMARY KEY NOT NULL CHECK(length(platform_identifier) BETWEEN 1 AND 512),
 diameter_micrometres INTEGER NOT NULL CHECK(diameter_micrometres BETWEEN 160 AND 4294967295),
 pole_pairs INTEGER NOT NULL CHECK(pole_pairs BETWEEN 1 AND 255),
 ratio_milli INTEGER NOT NULL CHECK(ratio_milli BETWEEN 1 AND 4294967295),
 updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
 revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0)
);";

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS device_vesc_drivetrains (
 platform_identifier TEXT NOT NULL CHECK(length(platform_identifier) BETWEEN 1 AND 512),
 diameter_micrometres INTEGER NOT NULL CHECK(diameter_micrometres BETWEEN 160 AND 4294967295),
 pole_pairs INTEGER NOT NULL CHECK(pole_pairs BETWEEN 1 AND 255),
 ratio_milli INTEGER NOT NULL CHECK(ratio_milli BETWEEN 1 AND 4294967295),
 updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
 revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
 target_key TEXT NOT NULL DEFAULT 'local' CHECK(length(target_key) BETWEEN 1 AND 40),
 PRIMARY KEY (platform_identifier, target_key)
);";

fn target_key(target: VescDrivetrainTarget) -> Result<String, StorageError> {
    match (target.request_target(), target.controller_uuid()) {
        (cutout_core::RequestTarget::Local, _) => Ok("local".to_owned()),
        (cutout_core::RequestTarget::VescCanController { controller_id }, Some(uuid)) => {
            let uuid = hex::encode(uuid);
            Ok(format!("can:{}:{uuid}", controller_id.get()))
        }
        // Checked targets cannot contain an unassociated CAN address.
        (cutout_core::RequestTarget::VescCanController { .. }, None) => {
            Err(StorageError::InvalidDatabaseIdentity)
        }
    }
}

pub(super) enum Command {
    Save {
        identifier: String,
        target: VescDrivetrainTarget,
        value: VescDrivetrain,
        at: u64,
        reply: Reply<VescDrivetrainProfile>,
    },
    Load {
        identifier: String,
        target: VescDrivetrainTarget,
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
        self.save_user_vesc_controller_drivetrain(
            identifier,
            VescDrivetrainTarget::LOCAL,
            value,
            at,
        )
    }
    /// Commits geometry for one explicit controller association; never writes to hardware.
    ///
    /// # Errors
    /// Returns invalid device identity, timestamp, exhausted revision or database failure.
    pub fn save_user_vesc_controller_drivetrain(
        &self,
        identifier: &str,
        target: VescDrivetrainTarget,
        value: VescDrivetrain,
        at: u64,
    ) -> Result<VescDrivetrainProfile, StorageError> {
        let identifier = normalize_stored_text(identifier, "platform identifier")?;
        self.request(move |reply| {
            super::Command::DeviceDrivetrain(Command::Save {
                identifier,
                target,
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
        self.user_vesc_controller_drivetrain(identifier, VescDrivetrainTarget::LOCAL)
    }
    /// Loads only the exact target association; reused CAN addresses do not share profiles.
    ///
    /// # Errors
    /// Returns invalid identity, corrupt stored geometry or database failure.
    pub fn user_vesc_controller_drivetrain(
        &self,
        identifier: &str,
        target: VescDrivetrainTarget,
    ) -> Result<Option<VescDrivetrainProfile>, StorageError> {
        let identifier = normalize_stored_text(identifier, "platform identifier")?;
        self.request(move |reply| {
            super::Command::DeviceDrivetrain(Command::Load {
                identifier,
                target,
                reply,
            })
        })
    }
}
pub(super) fn dispatch(connection: &Connection, command: Command) {
    match command {
        Command::Save {
            identifier,
            target,
            value,
            at,
            reply,
        } => {
            let result = save_target(connection, &identifier, target, value, at);
            let _ = reply.send(result);
        }
        Command::Load {
            identifier,
            target,
            reply,
        } => {
            let _ = reply.send(load_target(connection, &identifier, target));
        }
    }
}
#[cfg(test)]
fn save(
    connection: &Connection,
    identifier: &str,
    value: VescDrivetrain,
    at: u64,
) -> Result<VescDrivetrainProfile, StorageError> {
    save_target(
        connection,
        identifier,
        VescDrivetrainTarget::LOCAL,
        value,
        at,
    )
}

fn save_target(
    connection: &Connection,
    identifier: &str,
    target: VescDrivetrainTarget,
    value: VescDrivetrain,
    at: u64,
) -> Result<VescDrivetrainProfile, StorageError> {
    let transaction = connection.unchecked_transaction()?;
    let previous = load_target(&transaction, identifier, target)?;
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
    transaction.execute("INSERT INTO device_vesc_drivetrains (platform_identifier,diameter_micrometres,pole_pairs,ratio_milli,updated_at_ms,revision,target_key) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(platform_identifier,target_key) DO UPDATE SET diameter_micrometres=excluded.diameter_micrometres,pole_pairs=excluded.pole_pairs,ratio_milli=excluded.ratio_milli,updated_at_ms=excluded.updated_at_ms,revision=excluded.revision",
        params![identifier,value.diameter_micrometres(),value.geometry().motor_pole_pairs.get(),value.geometry().gear_ratio_denominator.as_milli(),at,stored_revision,target_key(target)?])?;
    transaction.commit()?;
    Ok(VescDrivetrainProfile::for_target(target, value, revision))
}

fn revision_exhausted() -> StorageError {
    StorageError::InvalidStoredValue {
        field: "VESC drivetrain revision",
        value: "revision exhausted".to_owned(),
    }
}

#[cfg(test)]
fn load(
    connection: &Connection,
    identifier: &str,
) -> Result<Option<VescDrivetrainProfile>, StorageError> {
    load_target(connection, identifier, VescDrivetrainTarget::LOCAL)
}

fn load_target(
    connection: &Connection,
    identifier: &str,
    target: VescDrivetrainTarget,
) -> Result<Option<VescDrivetrainProfile>, StorageError> {
    let raw: Option<(u32,u8,u32,u64)> = connection.query_row("SELECT diameter_micrometres,pole_pairs,ratio_milli,revision FROM device_vesc_drivetrains WHERE platform_identifier=?1 AND target_key=?2", params![identifier, target_key(target)?], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
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
        Ok(VescDrivetrainProfile::for_target(target, value, revision))
    })
    .transpose()
}

pub(super) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
    verify_columns(connection, &[DrivetrainSchema::Targeted])
}
pub(super) fn verify_v52_schema(connection: &Connection) -> Result<(), StorageError> {
    verify_columns(connection, &[DrivetrainSchema::Revisioned])
}
pub(super) fn verify_migration_schema(connection: &Connection) -> Result<(), StorageError> {
    verify_columns(
        connection,
        &[
            DrivetrainSchema::Legacy,
            DrivetrainSchema::Revisioned,
            DrivetrainSchema::Targeted,
        ],
    )
}

#[derive(Clone, Copy)]
enum DrivetrainSchema {
    Legacy,
    Revisioned,
    Targeted,
}

impl DrivetrainSchema {
    fn columns(self) -> Vec<(String, String, i64, i64)> {
        let mut columns = [
            ("platform_identifier", "TEXT", 1, 1),
            ("diameter_micrometres", "INTEGER", 1, 0),
            ("pole_pairs", "INTEGER", 1, 0),
            ("ratio_milli", "INTEGER", 1, 0),
            ("updated_at_ms", "INTEGER", 1, 0),
        ]
        .map(|(name, kind, required, key)| (name.to_owned(), kind.to_owned(), required, key))
        .to_vec();
        match self {
            Self::Legacy => {}
            Self::Revisioned | Self::Targeted => {
                columns.push(("revision".to_owned(), "INTEGER".to_owned(), 1, 0));
            }
        }
        if let Self::Targeted = self {
            columns.push(("target_key".to_owned(), "TEXT".to_owned(), 1, 2));
        }
        columns
    }
}

fn verify_columns(
    connection: &Connection,
    accepted: &[DrivetrainSchema],
) -> Result<(), StorageError> {
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
    if !accepted.iter().any(|schema| columns == schema.columns()) {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_drivetrain_v52_migration_retains_local_revision_and_unknown_can() {
        use cutout_core::{VescControllerId, VescDrivetrainTarget};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        RideDatabase::open(&path).unwrap().shutdown().unwrap();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP TABLE device_vesc_drivetrains;")
            .unwrap();
        connection.execute_batch(SCHEMA_V52).unwrap();
        connection.execute_batch("INSERT INTO device_vesc_drivetrains VALUES ('scooter',250000,15,2500,17,9); PRAGMA user_version=52;").unwrap();
        drop(connection);
        let database = RideDatabase::open(&path).unwrap();
        let local = database.user_vesc_drivetrain("scooter").unwrap().unwrap();
        assert_eq!(local.target(), VescDrivetrainTarget::LOCAL);
        assert_eq!(local.revision().get(), 9);
        assert_eq!(
            local.value(),
            VescDrivetrain::parse("250", "15", "2.5").unwrap()
        );
        let can = VescDrivetrainTarget::can(VescControllerId::new(7), [1; 12]).unwrap();
        assert_eq!(
            database
                .user_vesc_controller_drivetrain("scooter", can)
                .unwrap(),
            None
        );
        database.shutdown().unwrap();
        let connection = Connection::open(path).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT updated_at_ms FROM device_vesc_drivetrains",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            17
        );
    }

    #[test]
    fn controller_drivetrain_storage_is_target_and_installation_scoped() {
        use cutout_core::{VescControllerId, VescDrivetrainTarget};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        let database = RideDatabase::open(&path).unwrap();
        let can = VescDrivetrainTarget::can(VescControllerId::new(7), [1; 12]).unwrap();
        let replacement = VescDrivetrainTarget::can(VescControllerId::new(7), [2; 12]).unwrap();
        let local = VescDrivetrain::parse("250", "15", "1").unwrap();
        let secondary = VescDrivetrain::parse("200", "7", "2.5").unwrap();
        let first = database
            .save_user_vesc_drivetrain("scooter", local, 1)
            .unwrap();
        let second = database
            .save_user_vesc_controller_drivetrain("scooter", can, secondary, 2)
            .unwrap();
        assert_eq!(first.revision().get(), 1);
        assert_eq!(second.revision().get(), 1);
        assert_eq!(second.target(), can);
        assert_eq!(
            database
                .save_user_vesc_controller_drivetrain("scooter", can, secondary, 3)
                .unwrap(),
            second
        );
        let changed = database
            .save_user_vesc_controller_drivetrain("scooter", can, local, 4)
            .unwrap();
        assert_eq!(changed.revision().get(), 2);
        assert_eq!(
            database.user_vesc_drivetrain("scooter").unwrap(),
            Some(first)
        );
        assert_eq!(
            database
                .user_vesc_controller_drivetrain("scooter", replacement)
                .unwrap(),
            None
        );
        assert_eq!(
            database
                .user_vesc_controller_drivetrain("other-scooter", can)
                .unwrap(),
            None
        );
        database.shutdown().unwrap();
        let reopened = RideDatabase::open(&path).unwrap();
        assert_eq!(
            reopened
                .user_vesc_controller_drivetrain("scooter", can)
                .unwrap(),
            Some(changed)
        );
        assert_eq!(
            reopened.user_vesc_drivetrain("scooter").unwrap(),
            Some(first)
        );
        reopened.shutdown().unwrap();
    }

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
            super::super::migrations::CURRENT_SCHEMA_VERSION
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
    fn main_v50_migration_adds_vehicle_and_drivetrain_without_changing_capture_schema() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        let database = RideDatabase::open(&path).unwrap();
        database
            .remember_selected_device("controller-a", Some("My board"), 1)
            .unwrap();
        database.shutdown().unwrap();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "DROP TABLE device_vehicle_classifications;
                 DROP TABLE device_vesc_drivetrains;
                 PRAGMA user_version = 50;",
            )
            .unwrap();
        let capture_schema: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'capture_submissions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(capture_schema.contains("evidence_json"));
        drop(connection);

        let database = RideDatabase::open(&path).unwrap();
        assert_eq!(
            database.selected_device().unwrap().as_deref(),
            Some("controller-a")
        );
        assert_eq!(
            database.device_name("controller-a").unwrap().as_deref(),
            Some("My board")
        );
        assert_eq!(
            database
                .device_vehicle_classification("controller-a")
                .unwrap(),
            None
        );
        let geometry = VescDrivetrain::parse("90", "7", "2.5").unwrap();
        database
            .save_user_vehicle_kind("controller-a", cutout_core::VehicleKind::Eskate, 2)
            .unwrap();
        database
            .save_user_vesc_drivetrain("controller-a", geometry, 3)
            .unwrap();
        database.shutdown().unwrap();

        let database = RideDatabase::open(&path).unwrap();
        assert_eq!(
            database
                .device_vehicle_classification("controller-a")
                .unwrap()
                .unwrap()
                .kind(),
            cutout_core::VehicleKind::Eskate
        );
        assert_eq!(
            database
                .user_vesc_drivetrain("controller-a")
                .unwrap()
                .unwrap()
                .value(),
            geometry
        );
        database.shutdown().unwrap();
        let connection = Connection::open(&path).unwrap();
        let retained_schema: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'capture_submissions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retained_schema, capture_schema);
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            super::super::migrations::CURRENT_SCHEMA_VERSION
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
            super::super::migrations::CURRENT_SCHEMA_VERSION
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
