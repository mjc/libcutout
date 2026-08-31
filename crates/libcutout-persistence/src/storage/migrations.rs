use super::{MapPointId, SpatialRowId, StorageError};
use rusqlite::{Connection, OptionalExtension, params};

const CURRENT_SCHEMA_VERSION: i64 = 15;
const APPLICATION_ID: i64 = 0x4355_544f;
fn current_schema_pragmas() -> String {
    format!(
        "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"
    )
}

pub(super) fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchemaVersion(version));
    }
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != 0 && application_id != APPLICATION_ID {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    match version {
        0 => initialize_current_schema(connection)?,
        1 => migrate_v1_to_current(connection)?,
        2 => migrate_v2_to_current(connection)?,
        3 => migrate_v3_to_current(connection)?,
        4 => migrate_v4_to_current(connection)?,
        5 => migrate_v5_to_current(connection)?,
        6 => migrate_v6_to_current(connection)?,
        7 => migrate_v7_to_current(connection)?,
        8 => migrate_v8_to_current(connection)?,
        9 => migrate_v9_to_current(connection)?,
        10 => migrate_v10_to_current(connection)?,
        11 => migrate_v11_to_current(connection)?,
        12 => migrate_v12_to_current(connection)?,
        13 => migrate_v13_to_current(connection)?,
        14 => migrate_v14_to_current(connection)?,
        CURRENT_SCHEMA_VERSION => {
            if application_id != APPLICATION_ID {
                return Err(StorageError::InvalidDatabaseIdentity);
            }
        }
        _ => return Err(StorageError::InvalidDatabaseIdentity),
    }
    Ok(())
}

fn initialize_current_schema(connection: &Connection) -> Result<(), StorageError> {
    let user_table_count: u64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_schema
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    if user_table_count != 0 {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    connection.execute_batch("BEGIN IMMEDIATE;")?;
    if let Err(error) = create_current_schema(connection) {
        let _ = connection.execute_batch("ROLLBACK;");
        return Err(error);
    }
    connection.execute_batch(&format!("{} COMMIT;", current_schema_pragmas()))?;
    Ok(())
}

fn migrate_v1_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE selected_device (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             platform_identifier TEXT NOT NULL,
             updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE voltage_sag_models (
             device_identity TEXT PRIMARY KEY NOT NULL,
             schema_version INTEGER NOT NULL,
             effective_resistance_milliohms INTEGER NOT NULL,
             observations INTEGER NOT NULL,
             hardware_verified INTEGER NOT NULL CHECK (hardware_verified IN (0, 1)),
             last_learned_wall_clock_ms INTEGER NOT NULL
         );
         CREATE TABLE ride_session_marker (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             marker BLOB NOT NULL
         );
         PRAGMA user_version = 2;
         COMMIT;",
    )?;
    migrate(connection)
}

fn migrate_v2_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE pevcap_imports (
             artifact_digest TEXT PRIMARY KEY NOT NULL,
             artifact_path TEXT NOT NULL,
             ride_id TEXT NOT NULL REFERENCES rides(id),
             record_count INTEGER NOT NULL,
             location_count INTEGER NOT NULL,
             imported_at_ms INTEGER NOT NULL
         );
         PRAGMA user_version = 3;
         COMMIT;",
    )?;
    migrate(connection)
}

#[allow(
    clippy::too_many_lines,
    reason = "the declarative schema stays in one transaction"
)]
pub(crate) fn create_current_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "
        CREATE TABLE rides (
            id TEXT PRIMARY KEY NOT NULL,
            source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
            state TEXT NOT NULL CHECK (state IN ('draft', 'active', 'paused', 'stopped', 'interrupted', 'discarded', 'saved', 'imported')),
            created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
            monotonic_created_at_ms INTEGER CHECK (monotonic_created_at_ms IS NULL OR monotonic_created_at_ms >= 0),
            monotonic_last_event_ms INTEGER CHECK (monotonic_last_event_ms IS NULL OR monotonic_last_event_ms >= 0),
            paused_at_ms INTEGER CHECK (paused_at_ms IS NULL OR paused_at_ms >= 0),
            paused_duration_ms INTEGER NOT NULL DEFAULT 0 CHECK (paused_duration_ms >= 0),
            completed_duration_ms INTEGER NOT NULL DEFAULT 0 CHECK (completed_duration_ms >= 0),
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
            point_count INTEGER NOT NULL CHECK (point_count >= 0),
            distance_mm INTEGER NOT NULL CHECK (distance_mm >= 0),
            candidate_vehicle TEXT CHECK (candidate_vehicle IS NULL OR length(candidate_vehicle) BETWEEN 1 AND 512),
            associated_vehicle TEXT CHECK (associated_vehicle IS NULL OR length(associated_vehicle) BETWEEN 1 AND 512),
            associated_at_ms INTEGER CHECK (associated_at_ms IS NULL OR associated_at_ms >= 0),
            last_telemetry_at_ms INTEGER CHECK (last_telemetry_at_ms IS NULL OR last_telemetry_at_ms >= 0)
        );
        CREATE INDEX rides_history_order ON rides(created_at_ms DESC, id DESC);
        CREATE TABLE ride_segments (
            ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
            segment_id INTEGER NOT NULL CHECK (segment_id >= 0),
            point_count INTEGER NOT NULL DEFAULT 0 CHECK (point_count >= 0),
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            start_reason TEXT NOT NULL CHECK (start_reason IN ('initial', 'resume', 'background_gap', 'import_boundary')),
            source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
            started_monotonic_ms INTEGER NOT NULL CHECK (started_monotonic_ms >= 0),
            ended_monotonic_ms INTEGER CHECK (ended_monotonic_ms IS NULL OR ended_monotonic_ms >= started_monotonic_ms),
            started_wall_clock_ms INTEGER NOT NULL CHECK (started_wall_clock_ms >= 0),
            ended_wall_clock_ms INTEGER CHECK (ended_wall_clock_ms IS NULL OR ended_wall_clock_ms >= started_wall_clock_ms),
            PRIMARY KEY (ride_id, segment_id),
            UNIQUE (ride_id, sequence)
        );
        CREATE TABLE ride_points (
            ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            segment_id INTEGER NOT NULL CHECK (segment_id >= 0),
            telemetry_state INTEGER NOT NULL DEFAULT 0 CHECK (telemetry_state BETWEEN 0 AND 3),
            monotonic_ms INTEGER NOT NULL CHECK (monotonic_ms >= 0),
            wall_clock_ms INTEGER NOT NULL CHECK (wall_clock_ms >= 0),
            latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
            longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000),
            horizontal_accuracy_mm INTEGER CHECK (horizontal_accuracy_mm IS NULL OR horizontal_accuracy_mm >= 0),
            source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
            PRIMARY KEY (ride_id, sequence),
            UNIQUE (ride_id, monotonic_ms, wall_clock_ms, latitude_e7, longitude_e7),
            FOREIGN KEY (ride_id, segment_id) REFERENCES ride_segments(ride_id, segment_id)
        );
        CREATE TABLE selected_device (
            singleton_key BLOB PRIMARY KEY NOT NULL CHECK (length(singleton_key) = 16),
            platform_identifier TEXT NOT NULL CHECK (length(platform_identifier) BETWEEN 1 AND 512),
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
        );
        CREATE TABLE devices (
            platform_identifier TEXT PRIMARY KEY NOT NULL CHECK (length(platform_identifier) BETWEEN 1 AND 512),
            display_name TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 512),
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
        );
        CREATE TABLE voltage_sag_models (
            device_identity TEXT PRIMARY KEY NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
            schema_version INTEGER NOT NULL CHECK (schema_version = 1),
            effective_resistance_milliohms INTEGER NOT NULL CHECK (effective_resistance_milliohms <= 10000),
            observations INTEGER NOT NULL CHECK (observations <= 65535),
            hardware_verified INTEGER NOT NULL CHECK (hardware_verified IN (0, 1)),
            last_learned_wall_clock_ms INTEGER NOT NULL CHECK (last_learned_wall_clock_ms >= 0)
        );
        CREATE TABLE ride_session_marker (
            singleton_key BLOB PRIMARY KEY NOT NULL CHECK (length(singleton_key) = 16),
            marker BLOB NOT NULL CHECK (length(marker) BETWEEN 1 AND 4096)
        );
        CREATE TABLE pevcap_imports (
            artifact_digest TEXT PRIMARY KEY NOT NULL CHECK (length(artifact_digest) = 64),
            artifact_path TEXT NOT NULL CHECK (length(artifact_path) BETWEEN 1 AND 4096),
            ride_id TEXT REFERENCES rides(id),
            outcome TEXT NOT NULL CHECK (outcome IN ('ride_and_capture', 'capture_only')),
            artifact_size INTEGER NOT NULL CHECK (artifact_size >= 0),
            record_count INTEGER NOT NULL CHECK (record_count >= 0),
            location_count INTEGER NOT NULL CHECK (location_count >= 0),
            imported_at_ms INTEGER NOT NULL CHECK (imported_at_ms >= 0)
        );
        CREATE TABLE pevcap_import_work (
            artifact_digest TEXT PRIMARY KEY NOT NULL CHECK (length(artifact_digest) = 64),
            artifact_path TEXT NOT NULL CHECK (length(artifact_path) BETWEEN 1 AND 4096),
            ride_id TEXT REFERENCES rides(id) ON DELETE CASCADE
        );
        CREATE TABLE trails (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512)
        );
        CREATE TABLE trail_segments (
            trail_id TEXT NOT NULL REFERENCES trails(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            start_lat_e7 INTEGER NOT NULL CHECK (start_lat_e7 BETWEEN -900000000 AND 900000000),
            start_lon_e7 INTEGER NOT NULL CHECK (start_lon_e7 BETWEEN -1800000000 AND 1800000000),
            end_lat_e7 INTEGER NOT NULL CHECK (end_lat_e7 BETWEEN -900000000 AND 900000000),
            end_lon_e7 INTEGER NOT NULL CHECK (end_lon_e7 BETWEEN -1800000000 AND 1800000000),
            PRIMARY KEY (trail_id, sequence)
        );
        CREATE TABLE trail_segment_spatial_keys (
            rtree_id INTEGER PRIMARY KEY CHECK (rtree_id BETWEEN 1 AND 2147483647),
            trail_id TEXT NOT NULL,
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            FOREIGN KEY (trail_id, sequence)
                REFERENCES trail_segments(trail_id, sequence) ON DELETE CASCADE,
            UNIQUE (trail_id, sequence)
        );
        CREATE VIRTUAL TABLE trail_segments_rtree
            USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
        CREATE TABLE map_points (
            id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
            name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512),
            latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
            longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000)
        );
        CREATE TABLE map_point_spatial_keys (
            rtree_id INTEGER PRIMARY KEY CHECK (rtree_id BETWEEN 1 AND 2147483647),
            point_id BLOB NOT NULL UNIQUE CHECK (length(point_id) = 16),
            FOREIGN KEY (point_id) REFERENCES map_points(id) ON DELETE CASCADE
        );
        CREATE VIRTUAL TABLE map_points_rtree
            USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
        ",
    )?;
    Ok(())
}

fn verify_legacy_schema(connection: &Connection) -> Result<(), StorageError> {
    for table in ["rides", "ride_points"] {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(StorageError::InvalidDatabaseIdentity);
        }
    }
    Ok(())
}

pub(super) fn table_exists(connection: &Connection, table: &str) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type IN ('table', 'view') AND name = ?1)",
            [table],
            |row| row.get(0),
        )
        .map_err(StorageError::from)
}

fn copy_legacy_spatial_rows(transaction: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    let segments = {
        let mut statement = transaction.prepare(
            "SELECT id, trail_id, sequence, start_lat_e7, start_lon_e7,
                    end_lat_e7, end_lon_e7
             FROM trail_segments_legacy
             ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (rtree_id, trail_id, sequence, start_lat_e7, start_lon_e7, end_lat_e7, end_lon_e7) in
        segments
    {
        let rtree_id = SpatialRowId::from_sqlite(rtree_id)?;
        transaction.execute(
            "INSERT INTO trail_segments
                (trail_id, sequence, start_lat_e7, start_lon_e7, end_lat_e7, end_lon_e7)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                trail_id,
                sequence,
                start_lat_e7,
                start_lon_e7,
                end_lat_e7,
                end_lon_e7,
            ],
        )?;
        transaction.execute(
            "INSERT INTO trail_segment_spatial_keys (rtree_id, trail_id, sequence)
             VALUES (?1, ?2, ?3)",
            params![rtree_id.get(), trail_id, sequence],
        )?;
        transaction.execute(
            "INSERT INTO trail_segments_rtree
                (id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7)
             VALUES (?1, min(?2, ?3), max(?2, ?3), min(?4, ?5), max(?4, ?5))",
            params![
                rtree_id.get(),
                start_lat_e7,
                end_lat_e7,
                start_lon_e7,
                end_lon_e7,
            ],
        )?;
    }

    let points = {
        let mut statement = transaction.prepare(
            "SELECT id, name, latitude_e7, longitude_e7
             FROM map_points_legacy
             ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (rtree_id, name, latitude_e7, longitude_e7) in points {
        let rtree_id = SpatialRowId::from_sqlite(rtree_id)?;
        let point_id = MapPointId::new();
        transaction.execute(
            "INSERT INTO map_points (id, name, latitude_e7, longitude_e7)
             VALUES (?1, ?2, ?3, ?4)",
            params![point_id.uuid().as_bytes(), name, latitude_e7, longitude_e7],
        )?;
        transaction.execute(
            "INSERT INTO map_point_spatial_keys (rtree_id, point_id)
             VALUES (?1, ?2)",
            params![rtree_id.get(), point_id.uuid().as_bytes()],
        )?;
        transaction.execute(
            "INSERT INTO map_points_rtree
                (id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7)
             VALUES (?1, ?2, ?2, ?3, ?3)",
            params![rtree_id.get(), latitude_e7, longitude_e7],
        )?;
    }
    Ok(())
}

fn migrate_v3_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    let has_spatial = table_exists(connection, "trails")?
        && table_exists(connection, "trail_segments")?
        && table_exists(connection, "map_points")?;
    let transaction = connection.transaction()?;
    if has_spatial {
        transaction.execute_batch(
            "
            DROP TABLE IF EXISTS trail_segments_rtree;
            DROP TABLE IF EXISTS map_points_rtree;
            ALTER TABLE trail_segments RENAME TO trail_segments_legacy;
            ALTER TABLE trails RENAME TO trails_legacy;
            ALTER TABLE map_points RENAME TO map_points_legacy;
            ",
        )?;
    }
    transaction.execute_batch(
        "
        ALTER TABLE pevcap_imports RENAME TO pevcap_imports_legacy;
        ALTER TABLE ride_points RENAME TO ride_points_legacy;
        ALTER TABLE rides RENAME TO rides_legacy;
        ALTER TABLE selected_device RENAME TO selected_device_legacy;
        ALTER TABLE voltage_sag_models RENAME TO voltage_sag_models_legacy;
        ALTER TABLE ride_session_marker RENAME TO ride_session_marker_legacy;
        ",
    )?;
    create_current_schema(&transaction)?;
    if has_spatial {
        transaction.execute_batch("INSERT INTO trails SELECT * FROM trails_legacy;")?;
        copy_legacy_spatial_rows(&transaction)?;
        transaction.execute_batch(
            "DROP TABLE trail_segments_legacy;
             DROP TABLE trails_legacy;
             DROP TABLE map_points_legacy;",
        )?;
    }
    transaction.execute_batch(
        "
        INSERT INTO rides
            (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
        SELECT id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm
        FROM rides_legacy;
        INSERT INTO ride_segments
            (ride_id, segment_id, sequence, start_reason, source,
             started_monotonic_ms, started_wall_clock_ms)
        SELECT id, 0, 0, 'initial', source, 0, created_at_ms
        FROM rides_legacy;
        INSERT INTO ride_points
            (ride_id, sequence, segment_id, telemetry_state, monotonic_ms, wall_clock_ms, latitude_e7,
             longitude_e7, horizontal_accuracy_mm, source)
        SELECT ride_id, sequence, 0, 0, monotonic_ms, wall_clock_ms, latitude_e7,
               longitude_e7, horizontal_accuracy_mm, source
        FROM ride_points_legacy;
        INSERT INTO selected_device (singleton_key, platform_identifier, updated_at_ms)
        SELECT X'00000000000000000000000000000001', platform_identifier, updated_at_ms
        FROM selected_device_legacy
        WHERE id = 1;
        INSERT INTO voltage_sag_models SELECT * FROM voltage_sag_models_legacy;
        INSERT INTO ride_session_marker (singleton_key, marker)
        SELECT X'00000000000000000000000000000002', marker
        FROM ride_session_marker_legacy
        WHERE id = 1;
        INSERT INTO pevcap_imports
            (artifact_digest, artifact_path, ride_id, outcome, artifact_size,
             record_count, location_count, imported_at_ms)
        SELECT artifact_digest, artifact_path, ride_id, 'ride_and_capture', 0,
               record_count, location_count, imported_at_ms
        FROM pevcap_imports_legacy;
        DROP TABLE pevcap_imports_legacy;
        DROP TABLE ride_points_legacy;
        DROP TABLE rides_legacy;
        DROP TABLE selected_device_legacy;
        DROP TABLE voltage_sag_models_legacy;
        DROP TABLE ride_session_marker_legacy;
        ",
    )?;
    transaction.commit()?;
    migrate_v14_to_current(connection)
}

fn migrate_v4_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    let has_spatial = table_exists(connection, "trails")?
        && table_exists(connection, "trail_segments")?
        && table_exists(connection, "map_points")?;
    let transaction = connection.transaction()?;
    if has_spatial {
        transaction.execute_batch(
            "
            DROP TABLE IF EXISTS trail_segments_rtree;
            DROP TABLE IF EXISTS map_points_rtree;
            CREATE VIRTUAL TABLE trail_segments_rtree
                USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
            CREATE VIRTUAL TABLE map_points_rtree
                USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
            ",
        )?;
    } else {
        transaction.execute_batch(
            "
            CREATE TABLE trails (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512)
            );
            CREATE TABLE trail_segments (
                id INTEGER PRIMARY KEY,
                trail_id TEXT NOT NULL REFERENCES trails(id) ON DELETE CASCADE,
                sequence INTEGER NOT NULL CHECK (sequence >= 0),
                start_lat_e7 INTEGER NOT NULL CHECK (start_lat_e7 BETWEEN -900000000 AND 900000000),
                start_lon_e7 INTEGER NOT NULL CHECK (start_lon_e7 BETWEEN -1800000000 AND 1800000000),
                end_lat_e7 INTEGER NOT NULL CHECK (end_lat_e7 BETWEEN -900000000 AND 900000000),
                end_lon_e7 INTEGER NOT NULL CHECK (end_lon_e7 BETWEEN -1800000000 AND 1800000000),
                UNIQUE (trail_id, sequence)
            );
            CREATE VIRTUAL TABLE trail_segments_rtree
                USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
            CREATE TABLE map_points (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512),
                latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
                longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000)
            );
            CREATE VIRTUAL TABLE map_points_rtree
                USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
            ",
        )?;
    }
    if has_spatial {
        transaction.execute_batch(
            "
            INSERT INTO trail_segments_rtree
            SELECT id,
                   min(start_lat_e7, end_lat_e7), max(start_lat_e7, end_lat_e7),
                   min(start_lon_e7, end_lon_e7), max(start_lon_e7, end_lon_e7)
            FROM trail_segments;
            INSERT INTO map_points_rtree
            SELECT id, latitude_e7, latitude_e7, longitude_e7, longitude_e7 FROM map_points;
            PRAGMA user_version = 5;
            ",
        )?;
    } else {
        transaction.execute_batch("PRAGMA user_version = 5;")?;
    }
    transaction.commit()?;
    migrate_v5_to_current(connection)
}

fn migrate_v5_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    connection.execute_batch(
        "
        BEGIN IMMEDIATE;
        ALTER TABLE ride_points ADD COLUMN segment_id INTEGER NOT NULL DEFAULT 0 CHECK (segment_id >= 0);
        PRAGMA user_version = 6;
        COMMIT;
        ",
    )?;
    migrate_v6_to_current(connection)
}

fn migrate_v6_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    connection.execute_batch(
        "
        BEGIN IMMEDIATE;
        ALTER TABLE rides ADD COLUMN candidate_vehicle TEXT
            CHECK (candidate_vehicle IS NULL OR length(candidate_vehicle) BETWEEN 1 AND 512);
        ALTER TABLE rides ADD COLUMN associated_vehicle TEXT
            CHECK (associated_vehicle IS NULL OR length(associated_vehicle) BETWEEN 1 AND 512);
        ALTER TABLE rides ADD COLUMN associated_at_ms INTEGER
            CHECK (associated_at_ms IS NULL OR associated_at_ms >= 0);
        ALTER TABLE rides ADD COLUMN last_telemetry_at_ms INTEGER
            CHECK (last_telemetry_at_ms IS NULL OR last_telemetry_at_ms >= 0);
        PRAGMA user_version = 7;
        COMMIT;
        ",
    )?;
    migrate_v7_to_current(connection)
}

fn migrate_v7_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    connection.execute_batch(
        "
        BEGIN IMMEDIATE;
        ALTER TABLE ride_points ADD COLUMN telemetry_state INTEGER NOT NULL DEFAULT 0
            CHECK (telemetry_state BETWEEN 0 AND 3);
        PRAGMA user_version = 8;
        COMMIT;
        ",
    )?;
    migrate_v8_to_current(connection)
}

fn migrate_v8_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "
        BEGIN IMMEDIATE;
        CREATE TABLE devices (
            platform_identifier TEXT PRIMARY KEY NOT NULL CHECK (length(platform_identifier) BETWEEN 1 AND 512),
            display_name TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 512),
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
        );
        PRAGMA user_version = 9;
        COMMIT;
        ",
    )?;
    migrate_v9_to_current(connection)
}

fn migrate_v9_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    connection.execute_batch(
        "
        BEGIN IMMEDIATE;
        ALTER TABLE rides ADD COLUMN monotonic_created_at_ms INTEGER
            CHECK (monotonic_created_at_ms IS NULL OR monotonic_created_at_ms >= 0);
        PRAGMA user_version = 10;
        COMMIT;
        ",
    )?;
    migrate_v10_to_current(connection)
}

fn migrate_v10_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    connection.execute_batch(
        "
        BEGIN IMMEDIATE;
        ALTER TABLE rides ADD COLUMN monotonic_last_event_ms INTEGER
            CHECK (monotonic_last_event_ms IS NULL OR monotonic_last_event_ms >= 0);
        ALTER TABLE rides ADD COLUMN paused_at_ms INTEGER
            CHECK (paused_at_ms IS NULL OR paused_at_ms >= 0);
        ALTER TABLE rides ADD COLUMN paused_duration_ms INTEGER NOT NULL DEFAULT 0
            CHECK (paused_duration_ms >= 0);
        ALTER TABLE rides ADD COLUMN completed_duration_ms INTEGER NOT NULL DEFAULT 0
            CHECK (completed_duration_ms >= 0);
        PRAGMA user_version = 11;
        COMMIT;
        ",
    )?;
    migrate_v11_to_current(connection)
}

fn migrate_v11_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    if table_exists(connection, "ride_segments")? {
        connection
            .execute_batch("PRAGMA application_id = 1129665615; PRAGMA user_version = 12;")?;
        return migrate_v12_to_current(connection);
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE ride_segments (
             ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
             segment_id INTEGER NOT NULL CHECK (segment_id >= 0),
             sequence INTEGER NOT NULL CHECK (sequence >= 0),
             start_reason TEXT NOT NULL CHECK (start_reason IN ('initial', 'resume', 'background_gap', 'import_boundary')),
             source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
             started_monotonic_ms INTEGER NOT NULL CHECK (started_monotonic_ms >= 0),
             ended_monotonic_ms INTEGER CHECK (ended_monotonic_ms IS NULL OR ended_monotonic_ms >= started_monotonic_ms),
             started_wall_clock_ms INTEGER NOT NULL CHECK (started_wall_clock_ms >= 0),
             ended_wall_clock_ms INTEGER CHECK (ended_wall_clock_ms IS NULL OR ended_wall_clock_ms >= started_wall_clock_ms),
             PRIMARY KEY (ride_id, segment_id), UNIQUE (ride_id, sequence)
         );
         INSERT INTO ride_segments
             (ride_id, segment_id, sequence, start_reason, source,
              started_monotonic_ms, ended_monotonic_ms, started_wall_clock_ms, ended_wall_clock_ms)
         SELECT ride_id, segment_id, segment_id,
                CASE WHEN segment_id = 0 THEN 'initial' ELSE 'background_gap' END,
                MIN(source), MIN(monotonic_ms), MAX(monotonic_ms), MIN(wall_clock_ms), MAX(wall_clock_ms)
         FROM ride_points GROUP BY ride_id, segment_id;
         ALTER TABLE ride_points RENAME TO ride_points_legacy;
         CREATE TABLE ride_points (
             ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
             sequence INTEGER NOT NULL CHECK (sequence >= 0),
             segment_id INTEGER NOT NULL CHECK (segment_id >= 0),
             telemetry_state INTEGER NOT NULL DEFAULT 0 CHECK (telemetry_state BETWEEN 0 AND 3),
             monotonic_ms INTEGER NOT NULL CHECK (monotonic_ms >= 0),
             wall_clock_ms INTEGER NOT NULL CHECK (wall_clock_ms >= 0),
             latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
             longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000),
             horizontal_accuracy_mm INTEGER CHECK (horizontal_accuracy_mm IS NULL OR horizontal_accuracy_mm >= 0),
             source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
             PRIMARY KEY (ride_id, sequence),
             UNIQUE (ride_id, monotonic_ms, wall_clock_ms, latitude_e7, longitude_e7),
             FOREIGN KEY (ride_id, segment_id) REFERENCES ride_segments(ride_id, segment_id)
         );
         INSERT INTO ride_points
             (ride_id, sequence, segment_id, telemetry_state, monotonic_ms, wall_clock_ms,
              latitude_e7, longitude_e7, horizontal_accuracy_mm, source)
         SELECT ride_id, sequence, segment_id, telemetry_state, monotonic_ms, wall_clock_ms,
                latitude_e7, longitude_e7, horizontal_accuracy_mm, source
         FROM ride_points_legacy;
         DROP TABLE ride_points_legacy;
         PRAGMA application_id = 1129665615; PRAGMA user_version = 12;",
    )?;
    transaction.commit()?;
    migrate_v12_to_current(connection)
}

fn migrate_v12_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let selected_device_has_uuid_key =
        table_has_column(connection, "selected_device", "singleton_key")?;
    let ride_session_marker_has_uuid_key =
        table_has_column(connection, "ride_session_marker", "singleton_key")?;
    if !selected_device_has_uuid_key {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "ALTER TABLE selected_device RENAME TO selected_device_legacy;
             CREATE TABLE selected_device (
                 singleton_key BLOB PRIMARY KEY NOT NULL CHECK (length(singleton_key) = 16),
                 platform_identifier TEXT NOT NULL CHECK (length(platform_identifier) BETWEEN 1 AND 512),
                 updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
             );
             INSERT INTO selected_device (singleton_key, platform_identifier, updated_at_ms)
             SELECT X'00000000000000000000000000000001', platform_identifier, updated_at_ms
             FROM selected_device_legacy WHERE id = 1;
             DROP TABLE selected_device_legacy;",
        )?;
        if !ride_session_marker_has_uuid_key {
            transaction.execute_batch(
                "ALTER TABLE ride_session_marker RENAME TO ride_session_marker_legacy;
                 CREATE TABLE ride_session_marker (
                     singleton_key BLOB PRIMARY KEY NOT NULL CHECK (length(singleton_key) = 16),
                     marker BLOB NOT NULL CHECK (length(marker) BETWEEN 1 AND 4096)
                 );
                 INSERT INTO ride_session_marker (singleton_key, marker)
                 SELECT X'00000000000000000000000000000002', marker
                 FROM ride_session_marker_legacy WHERE id = 1;
                 DROP TABLE ride_session_marker_legacy;",
            )?;
        }
        transaction.commit()?;
    } else if !ride_session_marker_has_uuid_key {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "ALTER TABLE ride_session_marker RENAME TO ride_session_marker_legacy;
             CREATE TABLE ride_session_marker (
                 singleton_key BLOB PRIMARY KEY NOT NULL CHECK (length(singleton_key) = 16),
                 marker BLOB NOT NULL CHECK (length(marker) BETWEEN 1 AND 4096)
             );
             INSERT INTO ride_session_marker (singleton_key, marker)
             SELECT X'00000000000000000000000000000002', marker
             FROM ride_session_marker_legacy WHERE id = 1;
             DROP TABLE ride_session_marker_legacy;",
        )?;
        transaction.commit()?;
    }
    connection.execute_batch("PRAGMA application_id = 1129665615; PRAGMA user_version = 13;")?;
    migrate_v13_to_current(connection)
}

fn migrate_v13_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let map_points_are_uuid_backed: bool = connection.query_row(
        "SELECT COALESCE((SELECT type = 'BLOB' FROM pragma_table_info('map_points') WHERE name = 'id'), 0)",
        [], |row| row.get(0),
    )?;
    let trail_segments_are_composite = !table_has_column(connection, "trail_segments", "id")?
        && table_exists(connection, "trail_segment_spatial_keys")?
        && table_exists(connection, "map_point_spatial_keys")?;
    if map_points_are_uuid_backed && trail_segments_are_composite {
        connection
            .execute_batch("PRAGMA application_id = 1129665615; PRAGMA user_version = 14;")?;
        return migrate_v14_to_current(connection);
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "DROP TABLE IF EXISTS trail_segments_rtree;
         DROP TABLE IF EXISTS map_points_rtree;
         ALTER TABLE trail_segments RENAME TO trail_segments_legacy;
         ALTER TABLE map_points RENAME TO map_points_legacy;
         CREATE TABLE trail_segments (
             trail_id TEXT NOT NULL REFERENCES trails(id) ON DELETE CASCADE,
             sequence INTEGER NOT NULL CHECK (sequence >= 0),
             start_lat_e7 INTEGER NOT NULL CHECK (start_lat_e7 BETWEEN -900000000 AND 900000000),
             start_lon_e7 INTEGER NOT NULL CHECK (start_lon_e7 BETWEEN -1800000000 AND 1800000000),
             end_lat_e7 INTEGER NOT NULL CHECK (end_lat_e7 BETWEEN -900000000 AND 900000000),
             end_lon_e7 INTEGER NOT NULL CHECK (end_lon_e7 BETWEEN -1800000000 AND 1800000000),
             PRIMARY KEY (trail_id, sequence)
         );
         CREATE TABLE trail_segment_spatial_keys (
             rtree_id INTEGER PRIMARY KEY CHECK (rtree_id BETWEEN 1 AND 2147483647),
             trail_id TEXT NOT NULL,
             sequence INTEGER NOT NULL CHECK (sequence >= 0),
             FOREIGN KEY (trail_id, sequence) REFERENCES trail_segments(trail_id, sequence) ON DELETE CASCADE,
             UNIQUE (trail_id, sequence)
         );
         CREATE VIRTUAL TABLE trail_segments_rtree USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
         CREATE TABLE map_points (
             id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
             name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512),
             latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
             longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000)
         );
         CREATE TABLE map_point_spatial_keys (
             rtree_id INTEGER PRIMARY KEY CHECK (rtree_id BETWEEN 1 AND 2147483647),
             point_id BLOB NOT NULL UNIQUE CHECK (length(point_id) = 16),
             FOREIGN KEY (point_id) REFERENCES map_points(id) ON DELETE CASCADE
         );
         CREATE VIRTUAL TABLE map_points_rtree USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);",
    )?;
    copy_legacy_spatial_rows(&transaction)?;
    transaction.execute_batch("DROP TABLE trail_segments_legacy; DROP TABLE map_points_legacy; PRAGMA application_id = 1129665615; PRAGMA user_version = 14;")?;
    transaction.commit()?;
    migrate_v14_to_current(connection)
}

fn migrate_v14_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    if table_has_column(connection, "ride_segments", "point_count")? {
        connection.execute_batch(&current_schema_pragmas())?;
        return Ok(());
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE ride_segments
             ADD COLUMN point_count INTEGER NOT NULL DEFAULT 0 CHECK (point_count >= 0);
         UPDATE ride_segments
         SET point_count = counts.point_count
         FROM (
             SELECT ride_id, segment_id, COUNT(*) AS point_count
             FROM ride_points
             GROUP BY ride_id, segment_id
         ) AS counts
         WHERE ride_segments.ride_id = counts.ride_id
           AND ride_segments.segment_id = counts.segment_id;
         ",
    )?;
    transaction.execute_batch(&current_schema_pragmas())?;
    transaction.commit()?;
    Ok(())
}

fn table_has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, StorageError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    Ok(columns
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == column))
}

pub(super) fn verify_current_schema(connection: &Connection) -> Result<(), StorageError> {
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    for table in [
        "rides",
        "ride_points",
        "ride_segments",
        "devices",
        "selected_device",
        "voltage_sag_models",
        "ride_session_marker",
        "pevcap_imports",
        "pevcap_import_work",
        "trails",
        "trail_segments",
        "trail_segment_spatial_keys",
        "trail_segments_rtree",
        "map_points",
        "map_point_spatial_keys",
        "map_points_rtree",
    ] {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(StorageError::InvalidDatabaseIdentity);
        }
    }
    verify_singleton_schema(connection, "selected_device", "platform_identifier")?;
    verify_singleton_schema(connection, "ride_session_marker", "marker")?;
    verify_device_schema(connection)?;
    verify_ride_segment_schema(connection)?;
    verify_spatial_identity_schema(connection)?;
    Ok(())
}

fn verify_ride_segment_schema(connection: &Connection) -> Result<(), StorageError> {
    let point_count: Option<(String, i64)> = connection
        .query_row(
            "SELECT type, \"notnull\"
             FROM pragma_table_info('ride_segments')
             WHERE name = 'point_count'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if point_count != Some(("INTEGER".to_owned(), 1)) {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

fn verify_spatial_identity_schema(connection: &Connection) -> Result<(), StorageError> {
    let map_point_id_type: Option<String> = connection
        .query_row(
            "SELECT type FROM pragma_table_info('map_points') WHERE name = 'id'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if map_point_id_type.as_deref() != Some("BLOB")
        || table_has_column(connection, "trail_segments", "id")?
    {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    for table in ["map_point_spatial_keys", "trail_segment_spatial_keys"] {
        let columns: Vec<String> = connection
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |row| row.get(1))?
            .collect::<Result<_, _>>()?;
        if !columns.iter().any(|column| column == "rtree_id") {
            return Err(StorageError::InvalidDatabaseIdentity);
        }
    }
    Ok(())
}

fn verify_singleton_schema(
    connection: &Connection,
    table: &str,
    value_column: &str,
) -> Result<(), StorageError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
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
    let valid = if value_column == "marker" {
        columns.len() == 2
            && columns[0] == ("singleton_key".to_owned(), "BLOB".to_owned(), 1, 1)
            && columns[1] == ("marker".to_owned(), "BLOB".to_owned(), 1, 0)
    } else {
        columns.len() == 3
            && columns[0] == ("singleton_key".to_owned(), "BLOB".to_owned(), 1, 1)
            && columns[1] == (value_column.to_owned(), "TEXT".to_owned(), 1, 0)
            && columns[2] == ("updated_at_ms".to_owned(), "INTEGER".to_owned(), 1, 0)
    };
    if !valid {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}

fn verify_device_schema(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(devices)")?;
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
    if columns.len() != 3
        || columns[0] != ("platform_identifier".to_owned(), "TEXT".to_owned(), 1, 1)
        || columns[1] != ("display_name".to_owned(), "TEXT".to_owned(), 1, 0)
        || columns[2] != ("updated_at_ms".to_owned(), "INTEGER".to_owned(), 1, 0)
    {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    Ok(())
}
