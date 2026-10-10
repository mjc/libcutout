use super::{MapPointId, SpatialRowId, StorageError};
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) const CURRENT_SCHEMA_VERSION: i64 = 39;
const APPLICATION_ID: i64 = 0x4355_544f;

fn schema_pragmas(version: i64) -> String {
    format!("PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = {version};")
}

fn current_schema_pragmas() -> String {
    schema_pragmas(CURRENT_SCHEMA_VERSION)
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
        15 => migrate_v15_to_current(connection)?,
        16 => migrate_v16_to_current(connection)?,
        17 => migrate_v17_to_current(connection)?,
        18 => migrate_v18_to_current(connection)?,
        19 => migrate_v19_to_current(connection)?,
        20 => migrate_v20_to_current(connection)?,
        21 => migrate_v21_to_current(connection)?,
        22 => migrate_v22_to_current(connection)?,
        23 => migrate_v23_to_current(connection)?,
        24 => migrate_v24_to_current(connection)?,
        25 => migrate_v25_to_current(connection)?,
        26 => migrate_v26_to_current(connection)?,
        27 => migrate_v27_to_current(connection)?,
        28 => migrate_v28_to_current(connection)?,
        29 => migrate_v29_to_current(connection)?,
        30 => migrate_v30_to_current(connection)?,
        31 => migrate_v31_to_current(connection)?,
        32 => migrate_v32_to_current(connection)?,
        33 => migrate_v33_to_current(connection)?,
        34 => migrate_v34_to_current(connection)?,
        35 => migrate_v35_to_current(connection)?,
        36 => migrate_v36_to_current(connection)?,
        37 => migrate_v37_to_current(connection)?,
        38 => migrate_v38_to_current(connection)?,
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
    if let Err(error) = create_current_schema(connection)
        .and_then(|()| {
            connection
                .execute_batch(super::live_capture::SCHEMA)
                .map_err(Into::into)
        })
        .and_then(|()| {
            connection
                .execute_batch(super::live_capture::LOCATION_SCHEMA)
                .map_err(Into::into)
        })
        .and_then(|()| {
            connection
                .execute_batch(super::live_capture::BLE_SCHEMA)
                .map_err(Into::into)
        })
        .and_then(|()| {
            connection
                .execute_batch(super::live_capture::BLE_RAW_TELEMETRY_SCHEMA)
                .map_err(Into::into)
        })
        .and_then(|()| {
            connection
                .execute_batch(super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA)
                .map_err(Into::into)
        })
        .and_then(|()| {
            connection
                .execute_batch(RIDE_RECORDING_PREFERENCES_SCHEMA)
                .map_err(Into::into)
        })
        .and_then(|()| ensure_capture_payload_encoding(connection))
    {
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
    connection.execute_batch(RIDE_RECORDING_PREFERENCES_SCHEMA)?;
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
            last_telemetry_at_ms INTEGER CHECK (last_telemetry_at_ms IS NULL OR last_telemetry_at_ms >= 0),
            last_location_observed_monotonic_ms INTEGER,
            last_location_observed_wall_clock_ms INTEGER
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
        CREATE TABLE ride_music_history (
            ride_id TEXT PRIMARY KEY NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
            policy TEXT NOT NULL CHECK (policy IN ('disabled', 'opaque_item', 'human_readable')),
            state TEXT NOT NULL DEFAULT 'disabled'
                CHECK (state IN ('disabled', 'opaque_item', 'human_readable', 'deleted')),
            deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
            last_observed_at_ms INTEGER CHECK (last_observed_at_ms IS NULL OR last_observed_at_ms >= 0)
        );
        CREATE TABLE ride_music_event (
            ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            provider TEXT NOT NULL CHECK (provider IN ('apple_music', 'spotify')),
            item_identifier TEXT CHECK (item_identifier IS NULL OR length(CAST(item_identifier AS BLOB)) BETWEEN 1 AND 256),
            title TEXT CONSTRAINT ride_music_event_title_bytes
                CHECK (title IS NULL OR length(CAST(title AS BLOB)) BETWEEN 1 AND 512),
            artist TEXT CHECK (artist IS NULL OR length(CAST(artist AS BLOB)) BETWEEN 1 AND 512),
            kind TEXT NOT NULL CHECK (kind IN ('play', 'pause', 'skip', 'item_changed', 'stopped', 'provider_disconnected')),
            monotonic_at_ms INTEGER NOT NULL CHECK (monotonic_at_ms >= 0),
            wall_clock_at_ms INTEGER NOT NULL CHECK (wall_clock_at_ms >= 0),
            clock_uncertainty_milliseconds INTEGER NOT NULL CHECK (clock_uncertainty_milliseconds >= 0),
            observed_at_ms INTEGER CHECK (observed_at_ms IS NULL OR observed_at_ms BETWEEN 0 AND monotonic_at_ms),
            PRIMARY KEY (ride_id, sequence)
        );
        CREATE TABLE bms_voltage_samples (
            device_identity TEXT NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
            session_identifier TEXT NOT NULL CHECK (length(session_identifier) BETWEEN 1 AND 512),
            event_sequence INTEGER NOT NULL CHECK (event_sequence >= 0),
            monotonic_ms INTEGER NOT NULL CHECK (monotonic_ms >= 0),
            wall_clock_ms INTEGER NOT NULL CHECK (wall_clock_ms >= 0),
            observation_index INTEGER NOT NULL CHECK (observation_index BETWEEN 0 AND 65535),
            pack_index INTEGER CHECK (pack_index IS NULL OR pack_index BETWEEN 0 AND 65535),
            pack_observation_index INTEGER
                CHECK (pack_observation_index IS NULL OR pack_observation_index BETWEEN 0 AND 65535),
            millivolts INTEGER NOT NULL,
            PRIMARY KEY (device_identity, session_identifier, event_sequence, observation_index)
        );
        CREATE INDEX bms_voltage_samples_history
            ON bms_voltage_samples(device_identity, observation_index, wall_clock_ms DESC);
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
        CREATE TABLE last_connected_device (
            singleton_key BLOB PRIMARY KEY NOT NULL CHECK (length(singleton_key) = 16),
            platform_identifier TEXT NOT NULL CHECK (length(platform_identifier) BETWEEN 1 AND 512),
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
        );
        CREATE TABLE phone_alarm_preferences (
            device_identity TEXT PRIMARY KEY NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
            enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
            pwm_duty_percent INTEGER NOT NULL CHECK (pwm_duty_percent BETWEEN 1 AND 100)
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
    connection.execute_batch(super::capture_data::SCHEMA)?;
    connection.execute_batch(super::capture_history::HISTORY_INDEX)?;
    connection.execute_batch(super::recorded_capture::SCHEMA)?;
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
    transaction.execute_batch(super::live_capture::SCHEMA)?;
    transaction.execute_batch(super::live_capture::LOCATION_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_RAW_TELEMETRY_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA)?;
    transaction.execute_batch(&current_schema_pragmas())?;
    transaction.commit()?;
    Ok(())
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
        connection.execute_batch("PRAGMA user_version = 15;")?;
        return migrate_v15_to_current(connection);
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
    transaction.execute_batch("PRAGMA user_version = 15;")?;
    transaction.commit()?;
    migrate_v15_to_current(connection)
}

fn migrate_v15_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS ride_music_history (
             ride_id TEXT PRIMARY KEY NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
             policy TEXT NOT NULL CHECK (policy IN ('disabled', 'opaque_item', 'human_readable'))
         );
         CREATE TABLE IF NOT EXISTS ride_music_event (
             ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
             sequence INTEGER NOT NULL CHECK (sequence >= 0),
             provider TEXT NOT NULL CHECK (provider IN ('apple_music', 'spotify')),
             item_identifier TEXT CHECK (item_identifier IS NULL OR length(CAST(item_identifier AS BLOB)) BETWEEN 1 AND 256),
             title TEXT CHECK (title IS NULL OR length(CAST(title AS BLOB)) BETWEEN 1 AND 512),
             artist TEXT CHECK (artist IS NULL OR length(CAST(artist AS BLOB)) BETWEEN 1 AND 512),
             kind TEXT NOT NULL CHECK (kind IN ('play', 'pause', 'skip', 'item_changed', 'stopped', 'provider_disconnected')),
             monotonic_at_ms INTEGER NOT NULL CHECK (monotonic_at_ms >= 0),
             wall_clock_at_ms INTEGER NOT NULL CHECK (wall_clock_at_ms >= 0),
             clock_uncertainty_milliseconds INTEGER NOT NULL CHECK (clock_uncertainty_milliseconds >= 0),
             PRIMARY KEY (ride_id, sequence)
         );
         ",
    )?;
    transaction.execute_batch("PRAGMA user_version = 16;")?;
    transaction.commit()?;
    migrate_v16_to_current(connection)
}

fn migrate_v16_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    if !table_exists(connection, "ride_music_event")? {
        return migrate_pre_music_v16_to_current(connection);
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE ride_music_event RENAME TO ride_music_event_v16;
         CREATE TABLE ride_music_event (
             ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
             sequence INTEGER NOT NULL CHECK (sequence >= 0),
             provider TEXT NOT NULL CHECK (provider IN ('apple_music', 'spotify')),
             item_identifier TEXT CHECK (item_identifier IS NULL OR length(CAST(item_identifier AS BLOB)) BETWEEN 1 AND 256),
             title TEXT CHECK (title IS NULL OR length(CAST(title AS BLOB)) BETWEEN 1 AND 512),
             artist TEXT CHECK (artist IS NULL OR length(CAST(artist AS BLOB)) BETWEEN 1 AND 512),
             kind TEXT NOT NULL CHECK (kind IN ('play', 'pause', 'skip', 'item_changed', 'stopped', 'provider_disconnected')),
             monotonic_at_ms INTEGER NOT NULL CHECK (monotonic_at_ms >= 0),
             wall_clock_at_ms INTEGER NOT NULL CHECK (wall_clock_at_ms >= 0),
             clock_uncertainty_milliseconds INTEGER NOT NULL CHECK (clock_uncertainty_milliseconds >= 0),
             observed_at_ms INTEGER CHECK (observed_at_ms IS NULL OR observed_at_ms BETWEEN 0 AND monotonic_at_ms),
             PRIMARY KEY (ride_id, sequence)
         );
         INSERT INTO ride_music_event
             (ride_id, sequence, provider, item_identifier, title, artist, kind,
              monotonic_at_ms, wall_clock_at_ms, clock_uncertainty_milliseconds)
         SELECT ride_id, sequence, provider,
                CASE WHEN item_identifier IS NULL
                          OR length(CAST(item_identifier AS BLOB)) BETWEEN 1 AND 256
                     THEN item_identifier END,
                CASE WHEN title IS NULL OR length(CAST(title AS BLOB)) BETWEEN 1 AND 512
                     THEN title END,
                CASE WHEN artist IS NULL OR length(CAST(artist AS BLOB)) BETWEEN 1 AND 512
                     THEN artist END,
                kind,
                monotonic_at_ms, wall_clock_at_ms, clock_uncertainty_milliseconds
           FROM ride_music_event_v16;
         DROP TABLE ride_music_event_v16;",
    )?;
    for (table, column, definition) in [
        (
            "ride_music_history",
            "deleted",
            "INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))",
        ),
        (
            "ride_music_history",
            "last_observed_at_ms",
            "INTEGER CHECK (last_observed_at_ms IS NULL OR last_observed_at_ms >= 0)",
        ),
        (
            "ride_music_history",
            "state",
            "TEXT NOT NULL DEFAULT 'disabled' CHECK (state IN ('disabled', 'opaque_item', 'human_readable', 'deleted'))",
        ),
        (
            "ride_music_event",
            "observed_at_ms",
            "INTEGER CHECK (observed_at_ms IS NULL OR observed_at_ms BETWEEN 0 AND monotonic_at_ms)",
        ),
    ] {
        if !table_has_column(&transaction, table, column)? {
            transaction.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition};"
            ))?;
        }
    }
    transaction.execute_batch(
        "UPDATE ride_music_history
         SET state = CASE
             WHEN deleted = 1 THEN 'deleted'
             WHEN policy = 'opaque_item' THEN 'opaque_item'
             WHEN policy = 'human_readable' THEN 'human_readable'
             ELSE 'disabled'
         END;",
    )?;
    transaction.execute_batch("PRAGMA application_id = 1129665615; PRAGMA user_version = 18;")?;
    transaction.commit()?;
    migrate_v18_to_current(connection)
}

fn migrate_pre_music_v16_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    if table_exists(connection, "ride_music_history")? {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE ride_music_history (
             ride_id TEXT PRIMARY KEY NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
             policy TEXT NOT NULL CHECK (policy IN ('disabled', 'opaque_item', 'human_readable')),
             deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
             last_observed_at_ms INTEGER CHECK (last_observed_at_ms IS NULL OR last_observed_at_ms >= 0)
         );
         CREATE TABLE ride_music_event (
             ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
             sequence INTEGER NOT NULL CHECK (sequence >= 0),
             provider TEXT NOT NULL CHECK (provider IN ('apple_music', 'spotify')),
             item_identifier TEXT CHECK (item_identifier IS NULL OR length(CAST(item_identifier AS BLOB)) BETWEEN 1 AND 256),
             title TEXT CHECK (title IS NULL OR length(CAST(title AS BLOB)) BETWEEN 1 AND 512),
             artist TEXT CHECK (artist IS NULL OR length(CAST(artist AS BLOB)) BETWEEN 1 AND 512),
             kind TEXT NOT NULL CHECK (kind IN ('play', 'pause', 'skip', 'item_changed', 'stopped', 'provider_disconnected')),
             monotonic_at_ms INTEGER NOT NULL CHECK (monotonic_at_ms >= 0),
             wall_clock_at_ms INTEGER NOT NULL CHECK (wall_clock_at_ms >= 0),
             clock_uncertainty_milliseconds INTEGER NOT NULL CHECK (clock_uncertainty_milliseconds >= 0),
             observed_at_ms INTEGER CHECK (observed_at_ms IS NULL OR observed_at_ms BETWEEN 0 AND monotonic_at_ms),
             PRIMARY KEY (ride_id, sequence)
         );
         PRAGMA user_version = 18;",
    )?;
    transaction.commit()?;
    migrate_v18_to_current(connection)
}

fn migrate_v17_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE ride_music_event RENAME TO ride_music_event_v17;
         CREATE TABLE ride_music_event (
             ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
             sequence INTEGER NOT NULL CHECK (sequence >= 0),
             provider TEXT NOT NULL CHECK (provider IN ('apple_music', 'spotify')),
             item_identifier TEXT CHECK (item_identifier IS NULL OR length(CAST(item_identifier AS BLOB)) BETWEEN 1 AND 256),
             title TEXT CHECK (title IS NULL OR length(CAST(title AS BLOB)) BETWEEN 1 AND 512),
             artist TEXT CHECK (artist IS NULL OR length(CAST(artist AS BLOB)) BETWEEN 1 AND 512),
             kind TEXT NOT NULL CHECK (kind IN ('play', 'pause', 'skip', 'item_changed', 'stopped', 'provider_disconnected')),
             monotonic_at_ms INTEGER NOT NULL CHECK (monotonic_at_ms >= 0),
             wall_clock_at_ms INTEGER NOT NULL CHECK (wall_clock_at_ms >= 0),
             clock_uncertainty_milliseconds INTEGER NOT NULL CHECK (clock_uncertainty_milliseconds >= 0),
             observed_at_ms INTEGER CHECK (observed_at_ms IS NULL OR observed_at_ms BETWEEN 0 AND monotonic_at_ms),
             PRIMARY KEY (ride_id, sequence)
         );
         INSERT INTO ride_music_event
             (ride_id, sequence, provider, item_identifier, title, artist, kind,
              monotonic_at_ms, wall_clock_at_ms, clock_uncertainty_milliseconds, observed_at_ms)
         SELECT ride_id, sequence, provider, item_identifier, title, artist, kind,
                monotonic_at_ms, wall_clock_at_ms, clock_uncertainty_milliseconds, observed_at_ms
           FROM ride_music_event_v17;
         DROP TABLE ride_music_event_v17;",
    )?;
    transaction.execute_batch("PRAGMA application_id = 1129665615; PRAGMA user_version = 18;")?;
    transaction.commit()?;
    migrate_v18_to_current(connection)
}

fn migrate_v18_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let captures_exists = table_exists(connection, "pevcap_captures")?;
    let chunks_exists = table_exists(connection, "pevcap_capture_chunks")?;
    if captures_exists != chunks_exists {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    let transaction = connection.transaction()?;
    if !captures_exists {
        transaction.execute_batch(super::capture_data::SCHEMA)?;
    }
    if table_exists(&transaction, "ride_music_history")?
        && !table_has_column(&transaction, "ride_music_history", "state")?
    {
        transaction.execute_batch(
            "ALTER TABLE ride_music_history
                 ADD COLUMN state TEXT NOT NULL DEFAULT 'disabled'
                 CHECK (state IN ('disabled', 'opaque_item', 'human_readable', 'deleted'));
             UPDATE ride_music_history
             SET state = CASE
                 WHEN deleted = 1 THEN 'deleted'
                 WHEN policy = 'opaque_item' THEN 'opaque_item'
                 WHEN policy = 'human_readable' THEN 'human_readable'
                 ELSE 'disabled'
             END;",
        )?;
    }
    transaction.execute_batch(&format!(
        "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = 20;"
    ))?;
    transaction.commit()?;
    migrate_v20_to_current(connection)
}

fn migrate_v19_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    migrate_v18_to_current(connection)
}

fn migrate_v20_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    if table_exists(connection, "bms_voltage_samples")? {
        if table_has_column(connection, "bms_voltage_samples", "session_identifier")? {
            connection.execute_batch(&format!(
                "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = 22;"
            ))?;
            return migrate_v22_to_current(connection);
        }
        return migrate_v21_to_current(connection);
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS bms_voltage_samples (
             device_identity TEXT NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
             monotonic_ms INTEGER NOT NULL CHECK (monotonic_ms >= 0),
             wall_clock_ms INTEGER NOT NULL CHECK (wall_clock_ms >= 0),
             observation_index INTEGER NOT NULL CHECK (observation_index BETWEEN 0 AND 65535),
             pack_index INTEGER CHECK (pack_index IS NULL OR pack_index BETWEEN 0 AND 65535),
             pack_observation_index INTEGER
                 CHECK (pack_observation_index IS NULL OR pack_observation_index BETWEEN 0 AND 65535),
             millivolts INTEGER NOT NULL,
             PRIMARY KEY (device_identity, monotonic_ms, wall_clock_ms, observation_index)
         );
         CREATE INDEX IF NOT EXISTS bms_voltage_samples_history
             ON bms_voltage_samples(device_identity, observation_index, wall_clock_ms DESC);",
    )?;
    transaction.execute_batch(&format!(
        "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = 21;"
    ))?;
    transaction.commit()?;
    migrate_v21_to_current(connection)
}

fn migrate_v21_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE bms_voltage_samples RENAME TO bms_voltage_samples_legacy;
         CREATE TABLE bms_voltage_samples (
             device_identity TEXT NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
             session_identifier TEXT NOT NULL CHECK (length(session_identifier) BETWEEN 1 AND 512),
             event_sequence INTEGER NOT NULL CHECK (event_sequence >= 0),
             monotonic_ms INTEGER NOT NULL CHECK (monotonic_ms >= 0),
             wall_clock_ms INTEGER NOT NULL CHECK (wall_clock_ms >= 0),
             observation_index INTEGER NOT NULL CHECK (observation_index BETWEEN 0 AND 65535),
             pack_index INTEGER CHECK (pack_index IS NULL OR pack_index BETWEEN 0 AND 65535),
             pack_observation_index INTEGER CHECK (pack_observation_index IS NULL OR pack_observation_index BETWEEN 0 AND 65535),
             millivolts INTEGER NOT NULL,
             PRIMARY KEY (device_identity, session_identifier, event_sequence, observation_index)
         );
         INSERT INTO bms_voltage_samples
             (device_identity, session_identifier, event_sequence, monotonic_ms, wall_clock_ms,
              observation_index, pack_index, pack_observation_index, millivolts)
         SELECT device_identity, 'legacy', rowid, monotonic_ms, wall_clock_ms,
                observation_index, pack_index, pack_observation_index, millivolts
         FROM bms_voltage_samples_legacy;
         DROP TABLE bms_voltage_samples_legacy;
         CREATE INDEX bms_voltage_samples_history
             ON bms_voltage_samples(device_identity, observation_index, wall_clock_ms DESC);",
    )?;
    transaction.execute_batch(&format!(
        "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = 22;"
    ))?;
    transaction.commit()?;
    migrate_v22_to_current(connection)
}

fn migrate_v22_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS last_connected_device (
             singleton_key BLOB PRIMARY KEY NOT NULL CHECK (length(singleton_key) = 16),
             platform_identifier TEXT NOT NULL CHECK (length(platform_identifier) BETWEEN 1 AND 512),
             updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
         );",
    )?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS phone_alarm_preferences (
             device_identity TEXT PRIMARY KEY NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
             enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
             pwm_duty_percent INTEGER NOT NULL CHECK (pwm_duty_percent BETWEEN 1 AND 100)
         );",
    )?;
    transaction.execute_batch("PRAGMA user_version = 24;")?;
    transaction.commit()?;
    migrate_v24_to_current(connection)
}

fn migrate_v23_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS phone_alarm_preferences (
             device_identity TEXT PRIMARY KEY NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
             enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
             pwm_duty_percent INTEGER NOT NULL CHECK (pwm_duty_percent BETWEEN 1 AND 100)
         );",
    )?;
    transaction.execute_batch("PRAGMA user_version = 24;")?;
    transaction.commit()?;
    migrate_v24_to_current(connection)
}

fn migrate_v24_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(super::capture_history::HISTORY_INDEX)?;
    transaction.execute_batch("PRAGMA user_version = 25;")?;
    transaction.commit()?;
    migrate_v25_to_current(connection)
}

fn migrate_v25_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(super::recorded_capture::SCHEMA)?;
    transaction.execute_batch("PRAGMA user_version = 26;")?;
    transaction.commit()?;
    migrate_v26_to_current(connection)
}

const RIDE_RECORDING_PREFERENCES_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS ride_recording_preferences (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        autostart_enabled INTEGER NOT NULL CHECK (autostart_enabled IN (0, 1))
    );";

fn migrate_v26_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(RIDE_RECORDING_PREFERENCES_SCHEMA)?;
    ensure_live_capture_sessions_schema(&transaction)?;
    transaction.execute_batch(&schema_pragmas(28))?;
    transaction.commit()?;
    migrate_v28_to_current(connection)
}

fn migrate_v27_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    ensure_live_capture_sessions_schema(&transaction)?;
    transaction.execute_batch(RIDE_RECORDING_PREFERENCES_SCHEMA)?;
    transaction.execute_batch(&schema_pragmas(28))?;
    transaction.commit()?;
    migrate_v28_to_current(connection)
}

fn migrate_v28_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(super::live_capture::LOCATION_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_RAW_TELEMETRY_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA)?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn ensure_live_capture_sessions_schema(connection: &Connection) -> Result<(), StorageError> {
    if table_exists(connection, "live_capture_sessions")? {
        if !table_has_column(connection, "live_capture_sessions", "integrity")? {
            connection.execute_batch(
                "ALTER TABLE live_capture_sessions
                     ADD COLUMN integrity TEXT NOT NULL DEFAULT 'unknown'
                         CHECK (integrity IN ('complete', 'incomplete', 'unknown'));",
            )?;
        }
        if !table_has_column(connection, "live_capture_sessions", "dropped_messages")? {
            connection.execute_batch(
                "ALTER TABLE live_capture_sessions
                     ADD COLUMN dropped_messages INTEGER NOT NULL DEFAULT 0
                         CHECK (dropped_messages >= 0);",
            )?;
        }
    }
    connection.execute_batch(super::live_capture::SCHEMA)?;
    Ok(())
}

fn migrate_v29_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE live_capture_location_observations
             RENAME TO live_capture_location_observations_v29;",
    )?;
    transaction.execute_batch(super::live_capture::LOCATION_SCHEMA)?;
    transaction.execute_batch(
        "INSERT INTO live_capture_location_observations
         (capture_id, sequence, latitude_degrees, longitude_degrees, altitude_meters,
          horizontal_accuracy_meters, vertical_accuracy_meters, speed_meters_per_second,
          speed_accuracy_meters_per_second, course_degrees, course_accuracy_degrees,
          simulated, produced_by_accessory, validation_state, validation_reason, route_admission)
         SELECT capture_id, sequence, latitude_degrees, longitude_degrees, altitude_meters,
                horizontal_accuracy_meters, vertical_accuracy_meters, speed_meters_per_second,
                speed_accuracy_meters_per_second, course_degrees, course_accuracy_degrees,
                simulated, produced_by_accessory, validation_state, validation_reason,
                route_admission
         FROM live_capture_location_observations_v29;
         DROP TABLE live_capture_location_observations_v29;",
    )?;
    transaction.execute_batch(super::live_capture::BLE_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_RAW_TELEMETRY_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA)?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v30_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE live_capture_location_observations
             ADD COLUMN raw_source_timestamp_bits BLOB
                 CHECK (raw_source_timestamp_bits IS NULL OR length(raw_source_timestamp_bits) = 8);",
    )?;
    transaction.execute_batch(super::live_capture::BLE_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_RAW_TELEMETRY_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA)?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v32_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(super::live_capture::BLE_RAW_TELEMETRY_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA)?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v33_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA)?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v31_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(super::live_capture::BLE_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_RAW_TELEMETRY_SCHEMA)?;
    transaction.execute_batch(super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA)?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v34_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v35_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v36_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v37_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn ensure_location_observation_clocks(connection: &Connection) -> Result<(), StorageError> {
    if !table_exists(connection, "rides")? {
        return Ok(());
    }
    for column in [
        "last_location_observed_monotonic_ms",
        "last_location_observed_wall_clock_ms",
    ] {
        if !table_has_column(connection, "rides", column)? {
            // Nullable metadata adds no row rewrites or CHECK validation scan.
            connection.execute_batch(&format!("ALTER TABLE rides ADD COLUMN {column} INTEGER;"))?;
        }
    }
    Ok(())
}

const CAPTURE_PAYLOAD_ENCODING_TRIGGERS: &str = "
    CREATE TRIGGER IF NOT EXISTS live_capture_payload_encoding_insert
    BEFORE INSERT ON live_capture_events
    WHEN NEW.payload_encoding NOT IN (0, 1)
        OR (NEW.payload_encoding = 0 AND NEW.payload_original_bytes IS NOT NULL)
        OR (NEW.payload_encoding = 1 AND (
            NEW.payload_original_bytes IS NULL
            OR typeof(NEW.payload_original_bytes) != 'integer'
            OR NEW.payload_original_bytes NOT BETWEEN 1 AND 65536
            OR length(NEW.payload) >= NEW.payload_original_bytes))
    BEGIN
        SELECT RAISE(ABORT, 'invalid live capture payload encoding');
    END;
    CREATE TRIGGER IF NOT EXISTS live_capture_payload_encoding_update
    BEFORE UPDATE OF payload, payload_encoding, payload_original_bytes ON live_capture_events
    WHEN NEW.payload_encoding NOT IN (0, 1)
        OR (NEW.payload_encoding = 0 AND NEW.payload_original_bytes IS NOT NULL)
        OR (NEW.payload_encoding = 1 AND (
            NEW.payload_original_bytes IS NULL
            OR typeof(NEW.payload_original_bytes) != 'integer'
            OR NEW.payload_original_bytes NOT BETWEEN 1 AND 65536
            OR length(NEW.payload) >= NEW.payload_original_bytes))
    BEGIN
        SELECT RAISE(ABORT, 'invalid live capture payload encoding');
    END;
";

fn ensure_capture_payload_encoding(connection: &Connection) -> Result<(), StorageError> {
    if !table_exists(connection, "live_capture_events")? {
        return Ok(());
    }
    // ADD COLUMN with CHECK makes SQLite scan every existing event. Add only
    // metadata here and enforce the shared invariant with write triggers instead.
    // Both ALTERs remain transactional; old payloads stay raw with default zero.
    if !table_has_column(connection, "live_capture_events", "payload_encoding")? {
        connection.execute_batch(
            "ALTER TABLE live_capture_events ADD COLUMN payload_encoding INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    if !table_has_column(connection, "live_capture_events", "payload_original_bytes")? {
        connection.execute_batch(
            "ALTER TABLE live_capture_events ADD COLUMN payload_original_bytes INTEGER;",
        )?;
    }
    connection.execute_batch(CAPTURE_PAYLOAD_ENCODING_TRIGGERS)?;
    Ok(())
}

fn migrate_v38_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    finish_migration(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn ensure_live_capture_recording_context(connection: &Connection) -> Result<(), StorageError> {
    if table_exists(connection, "live_capture_sessions")?
        && !table_has_column(
            connection,
            "live_capture_sessions",
            "recording_context_json",
        )?
    {
        // Nullable addition: no event scan, data copy or retroactive provenance claim.
        connection.execute_batch(
            "ALTER TABLE live_capture_sessions ADD COLUMN recording_context_json TEXT;",
        )?;
    }
    Ok(())
}

fn finish_migration(transaction: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    // Capture reads page by the existing (capture_id, sequence) primary keys.
    // These unused secondary indexes add writes to every admitted observation.
    transaction.execute_batch(
        "DROP INDEX IF EXISTS live_capture_events_receipt_order;
         DROP INDEX IF EXISTS live_capture_events_source_wall_clock;
         DROP INDEX IF EXISTS live_capture_ble_characteristic;
         DROP INDEX IF EXISTS live_capture_ble_raw_telemetry_field_id;
         DROP INDEX IF EXISTS live_capture_ble_semantic_telemetry_time;",
    )?;
    ensure_capture_payload_encoding(transaction)?;
    ensure_live_capture_recording_context(transaction)?;
    ensure_location_observation_clocks(transaction)?;
    transaction.execute_batch(RIDE_RECORDING_PREFERENCES_SCHEMA)?;
    transaction.execute_batch(&current_schema_pragmas())?;
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
        "phone_alarm_preferences",
        "ride_recording_preferences",
        "selected_device",
        "voltage_sag_models",
        "ride_session_marker",
        "pevcap_imports",
        "pevcap_import_work",
        "pevcap_captures",
        "pevcap_capture_chunks",
        "pevcap_recordings",
        "live_capture_sessions",
        "live_capture_events",
        "live_capture_location_observations",
        "live_capture_ble_observations",
        "live_capture_ble_raw_telemetry_fields",
        "live_capture_ble_semantic_telemetry",
        "trails",
        "trail_segments",
        "trail_segment_spatial_keys",
        "trail_segments_rtree",
        "map_points",
        "map_point_spatial_keys",
        "map_points_rtree",
        "ride_music_history",
        "ride_music_event",
        "bms_voltage_samples",
        "last_connected_device",
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
    if !table_has_column(
        connection,
        "live_capture_sessions",
        "recording_context_json",
    )? || !table_has_column(connection, "live_capture_sessions", "integrity")?
        || !table_has_column(connection, "live_capture_sessions", "dropped_messages")?
        || !table_has_column(connection, "live_capture_events", "payload_encoding")?
        || !table_has_column(connection, "live_capture_events", "payload_original_bytes")?
    {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    verify_singleton_schema(connection, "selected_device", "platform_identifier")?;
    verify_singleton_schema(connection, "last_connected_device", "platform_identifier")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE_SCHEMA_V36: &str = "
        CREATE TABLE IF NOT EXISTS live_capture_sessions (
            capture_id TEXT PRIMARY KEY NOT NULL CHECK (length(capture_id) = 36),
            state TEXT NOT NULL CHECK (state IN ('active', 'finished', 'interrupted')),
            integrity TEXT NOT NULL CHECK (integrity IN ('complete', 'incomplete', 'unknown')),
            dropped_messages INTEGER NOT NULL DEFAULT 0 CHECK (dropped_messages >= 0),
            header_json BLOB NOT NULL CHECK (length(header_json) BETWEEN 1 AND 65536),
            started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
            finished_at_ms INTEGER CHECK (finished_at_ms IS NULL OR finished_at_ms >= started_at_ms),
            next_sequence INTEGER NOT NULL DEFAULT 0 CHECK (next_sequence >= 0),
            stored_bytes INTEGER NOT NULL CHECK (stored_bytes BETWEEN 1 AND 536870912),
            CHECK ((state = 'finished') = (finished_at_ms IS NOT NULL)),
            CHECK ((integrity = 'complete' AND dropped_messages = 0)
                OR (integrity = 'incomplete' AND dropped_messages > 0)
                OR integrity = 'unknown')
        );
        CREATE TABLE IF NOT EXISTS live_capture_events (
            capture_id TEXT NOT NULL REFERENCES live_capture_sessions(capture_id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            event_kind TEXT NOT NULL CHECK (event_kind IN
                ('link_up', 'link_down', 'write', 'notification', 'location', 'music', 'metadata')),
            receipt_monotonic_ms INTEGER NOT NULL CHECK (receipt_monotonic_ms >= 0),
            source_monotonic_offset_ms INTEGER,
            source_wall_clock_unix_ms INTEGER CHECK
                (source_wall_clock_unix_ms IS NULL OR source_wall_clock_unix_ms >= 0),
            payload BLOB NOT NULL CHECK (length(payload) BETWEEN 1 AND 65536),
            PRIMARY KEY (capture_id, sequence)
        ) WITHOUT ROWID;
    ";

    fn initialize_capture_schema_v36(connection: &Connection) {
        create_current_schema(connection).unwrap();
        for schema in [
            CAPTURE_SCHEMA_V36,
            super::super::live_capture::LOCATION_SCHEMA,
            super::super::live_capture::BLE_SCHEMA,
            super::super::live_capture::BLE_RAW_TELEMETRY_SCHEMA,
            super::super::live_capture::BLE_SEMANTIC_TELEMETRY_SCHEMA,
        ] {
            connection.execute_batch(schema).unwrap();
        }
        connection.execute_batch(&schema_pragmas(36)).unwrap();
    }

    fn capture_rows_without_encoding(
        connection: &Connection,
    ) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
        let mut rows = capture_rows(connection);
        // Compare the original schema fields; provenance is a nullable metadata addition.
        for session in &mut rows[0] {
            session.truncate(9);
        }
        if table_has_column(connection, "live_capture_events", "payload_encoding").unwrap() {
            for event in &mut rows[1] {
                event.truncate(7);
            }
        }
        rows
    }

    fn assert_payload_encoding_constraints(connection: &Connection) {
        for (encoding, original_bytes) in [
            ("2", "NULL"),
            ("NULL", "NULL"),
            ("1", "NULL"),
            ("0", "1"),
            ("1", "0"),
            ("1", "65537"),
            ("1", "4"),
            ("1", "3"),
            ("1", "5.5"),
        ] {
            for statement in [
                format!(
                    "UPDATE live_capture_events SET payload_encoding = {encoding},
                         payload_original_bytes = {original_bytes} WHERE sequence = 0"
                ),
                format!(
                    "INSERT INTO live_capture_events
                         (capture_id, sequence, event_kind, receipt_monotonic_ms, payload,
                          payload_encoding, payload_original_bytes)
                     SELECT capture_id, 99, event_kind, receipt_monotonic_ms, payload,
                            {encoding}, {original_bytes}
                     FROM live_capture_events WHERE sequence = 0"
                ),
            ] {
                assert_eq!(
                    connection
                        .execute_batch(&statement)
                        .unwrap_err()
                        .sqlite_error_code(),
                    Some(rusqlite::ErrorCode::ConstraintViolation),
                    "must reject {statement}"
                );
            }
        }
        connection.execute_batch(
            "UPDATE live_capture_events SET payload_encoding = 1, payload_original_bytes = 5 WHERE sequence = 0;"
        ).unwrap();
        assert_eq!(
            connection
                .execute_batch(
                    "UPDATE live_capture_events SET payload = X'0001020304' WHERE sequence = 0;"
                )
                .unwrap_err()
                .sqlite_error_code(),
            Some(rusqlite::ErrorCode::ConstraintViolation)
        );
        connection.execute_batch(
            "UPDATE live_capture_events SET payload_encoding = 0, payload_original_bytes = NULL WHERE sequence = 0;"
        ).unwrap();
    }

    #[test]
    fn payload_encoding_metadata_migration_preserves_existing_capture_values() {
        // Older direct-to-current routes share the same metadata ensure boundary.
        for version in [31, 32, 33, 34, 35, 36] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("capture.sqlite");
            let mut connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "foreign_keys", "ON")
                .unwrap();
            initialize_capture_schema_v36(&connection);
            populate_indexed_capture(&connection);
            connection
                .pragma_update(None, "user_version", version)
                .unwrap();
            let before = capture_rows_without_encoding(&connection);

            migrate(&mut connection).unwrap();

            assert!(
                table_has_column(&connection, "live_capture_events", "payload_encoding").unwrap()
            );
            assert!(
                table_has_column(&connection, "live_capture_events", "payload_original_bytes")
                    .unwrap()
            );
            assert_eq!(capture_rows_without_encoding(&connection), before);
            let metadata: Vec<(i64, Option<i64>)> = connection.prepare(
                "SELECT payload_encoding, payload_original_bytes FROM live_capture_events ORDER BY sequence"
            ).unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap();
            assert_eq!(metadata, [(0, None), (0, None)]);
            assert_payload_encoding_constraints(&connection);
            assert_capture_constraints(&mut connection);
            assert_eq!(capture_rows_without_encoding(&connection), before);
            assert_eq!(
                connection
                    .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                    .unwrap(),
                CURRENT_SCHEMA_VERSION
            );
            drop(connection);
            let mut reopened = Connection::open(&path).unwrap();
            reopened.pragma_update(None, "foreign_keys", "ON").unwrap();
            migrate(&mut reopened).unwrap();
            verify_current_schema(&reopened).unwrap();
            assert_eq!(capture_rows_without_encoding(&reopened), before);
            assert_payload_encoding_constraints(&reopened);
        }
    }

    #[test]
    fn location_observation_clock_migration_preserves_rows_without_a_scan() {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
        let mut connection = old_location_clock_fixture();
        let before: Vec<rusqlite::types::Value> = connection
            .query_row("SELECT * FROM rides", [], |row| {
                (0..row.as_ref().column_count())
                    .map(|i| row.get(i))
                    .collect()
            })
            .unwrap();
        let changes = connection.total_changes();
        connection.authorizer(Some(|context: AuthContext<'_>| match context.action {
            AuthAction::Read {
                table_name: "rides" | "pragma_quick_check",
                ..
            }
            | AuthAction::Pragma {
                pragma_name: "quick_check",
                ..
            } => Authorization::Deny,
            _ => Authorization::Allow,
        }));
        migrate(&mut connection).unwrap();
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        let after: Vec<rusqlite::types::Value> = connection
            .query_row("SELECT * FROM rides", [], |row| {
                (0..row.as_ref().column_count())
                    .map(|i| row.get(i))
                    .collect()
            })
            .unwrap();
        assert_eq!(&after[..before.len()], before.as_slice());
        assert_eq!(
            &after[before.len()..],
            [rusqlite::types::Value::Null, rusqlite::types::Value::Null]
        );
        assert_eq!(connection.total_changes(), changes);
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn location_observation_clock_migration_rolls_back_both_columns() {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let mut connection = old_location_clock_fixture();
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&attempts);
        connection.authorizer(Some(move |context: AuthContext<'_>| match context.action {
            AuthAction::AlterTable {
                table_name: "rides",
                ..
            } if observed.fetch_add(1, Ordering::SeqCst) == 1 => Authorization::Deny,
            _ => Authorization::Allow,
        }));
        assert!(migrate(&mut connection).is_err());
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert!(
            !table_has_column(&connection, "rides", "last_location_observed_monotonic_ms").unwrap()
        );
        assert!(
            !table_has_column(&connection, "rides", "last_location_observed_wall_clock_ms")
                .unwrap()
        );
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            37
        );
        migrate(&mut connection).unwrap();
        assert!(
            table_has_column(&connection, "rides", "last_location_observed_monotonic_ms").unwrap()
        );
        assert!(
            table_has_column(&connection, "rides", "last_location_observed_wall_clock_ms").unwrap()
        );
    }

    fn old_location_clock_fixture() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        initialize_current_schema(&connection).unwrap();
        for column in [
            "last_location_observed_monotonic_ms",
            "last_location_observed_wall_clock_ms",
        ] {
            if table_has_column(&connection, "rides", column).unwrap() {
                connection
                    .execute_batch(&format!("ALTER TABLE rides DROP COLUMN {column};"))
                    .unwrap();
            }
        }
        connection.execute_batch("INSERT INTO rides (id, source, state, created_at_ms, updated_at_ms, monotonic_created_at_ms, monotonic_last_event_ms, point_count, distance_mm) VALUES ('00000000-0000-0000-0000-000000000001', 'live', 'active', 100, 200, 10, 100, 0, 0); PRAGMA user_version = 37;").unwrap();
        connection
    }

    #[test]
    fn payload_encoding_metadata_migration_does_not_scan_existing_events() {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_capture_schema_v36(&connection);
        populate_indexed_capture(&connection);
        let before = capture_rows(&connection);
        let reads = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&reads);
        connection.authorizer(Some(move |context: AuthContext<'_>| match context.action {
            AuthAction::Read {
                table_name: "live_capture_events" | "pragma_quick_check",
                ..
            }
            | AuthAction::Pragma {
                pragma_name: "quick_check",
                ..
            } => {
                observed.fetch_add(1, Ordering::SeqCst);
                Authorization::Deny
            }
            _ => Authorization::Allow,
        }));
        migrate(&mut connection).unwrap();
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(capture_rows_without_encoding(&connection), before);
    }

    #[test]
    fn payload_encoding_metadata_fresh_schema_constraints() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        migrate(&mut connection).unwrap();
        populate_indexed_capture(&connection);
        assert_payload_encoding_constraints(&connection);
    }

    #[test]
    fn payload_encoding_metadata_migration_rolls_back_partial_alter() {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        initialize_capture_schema_v36(&connection);
        populate_indexed_capture(&connection);
        let before = capture_rows(&connection);
        let indexes_before = capture_secondary_indexes(&connection);
        let alters = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&alters);
        connection.authorizer(Some(move |context: AuthContext<'_>| match context.action {
            AuthAction::AlterTable {
                table_name: "live_capture_events",
                ..
            } if observed.fetch_add(1, Ordering::SeqCst) == 1 => Authorization::Deny,
            _ => Authorization::Allow,
        }));
        assert!(migrate(&mut connection).is_err());
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert_eq!(alters.load(Ordering::SeqCst), 2);
        assert!(!table_has_column(&connection, "live_capture_events", "payload_encoding").unwrap());
        assert!(
            !table_has_column(&connection, "live_capture_events", "payload_original_bytes")
                .unwrap()
        );
        assert_eq!(capture_rows(&connection), before);
        assert_eq!(capture_secondary_indexes(&connection), indexes_before);
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            36
        );
        migrate(&mut connection).unwrap();
        assert_eq!(capture_rows_without_encoding(&connection), before);
    }

    const LEGACY_CAPTURE_INDEXES: &str = "
        CREATE INDEX IF NOT EXISTS live_capture_events_receipt_order
            ON live_capture_events(capture_id, receipt_monotonic_ms, sequence);
        CREATE INDEX IF NOT EXISTS live_capture_events_source_wall_clock
            ON live_capture_events(capture_id, source_wall_clock_unix_ms, sequence);
        CREATE INDEX IF NOT EXISTS live_capture_ble_characteristic
            ON live_capture_ble_observations(characteristic_uuid, capture_id, sequence);
        CREATE INDEX IF NOT EXISTS live_capture_ble_raw_telemetry_field_id
            ON live_capture_ble_raw_telemetry_fields(field_kind, field_id, capture_id, sequence);
        CREATE INDEX IF NOT EXISTS live_capture_ble_semantic_telemetry_time
            ON live_capture_ble_semantic_telemetry(provenance, observed_at_ms, capture_id, sequence);
    ";

    const CAPTURE_TABLES: [&str; 6] = [
        "live_capture_sessions",
        "live_capture_events",
        "live_capture_location_observations",
        "live_capture_ble_observations",
        "live_capture_ble_raw_telemetry_fields",
        "live_capture_ble_semantic_telemetry",
    ];

    fn capture_rows(connection: &Connection) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
        CAPTURE_TABLES
            .iter()
            .map(|table| {
                let ordering = if *table == "live_capture_sessions" {
                    "capture_id"
                } else if *table == "live_capture_ble_raw_telemetry_fields" {
                    "capture_id, sequence, field_kind, field_index"
                } else {
                    "capture_id, sequence"
                };
                let mut statement = connection
                    .prepare(&format!("SELECT * FROM {table} ORDER BY {ordering}"))
                    .unwrap();
                let columns = statement.column_count();
                statement
                    .query_map([], |row| {
                        (0..columns).map(|column| row.get(column)).collect()
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap()
            })
            .collect()
    }

    fn capture_secondary_indexes(connection: &Connection) -> Vec<String> {
        connection
            .prepare(
                "SELECT name FROM sqlite_schema
                 WHERE type = 'index' AND name NOT LIKE 'sqlite_%'
                   AND tbl_name LIKE 'live_capture_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn populate_indexed_capture(connection: &Connection) {
        connection
            .execute_batch(
                r#"INSERT INTO live_capture_sessions
                     (capture_id, state, integrity, header_json, started_at_ms,
                      next_sequence, stored_bytes)
                 VALUES ('00000000-0000-0000-0000-000000000001', 'active', 'complete',
                         X'7B7D', 100, 2, 8);
                 INSERT INTO live_capture_events
                     (capture_id, sequence, event_kind, receipt_monotonic_ms,
                      source_monotonic_offset_ms, source_wall_clock_unix_ms, payload)
                 VALUES ('00000000-0000-0000-0000-000000000001', 0, 'notification',
                         101, -9, 9001, X'0001FEFF'),
                        ('00000000-0000-0000-0000-000000000001', 1, 'location',
                         102, NULL, NULL, X'7B7D');
                 INSERT INTO live_capture_location_observations
                     (capture_id, sequence, latitude_degrees, longitude_degrees,
                      validation_state, route_admission, raw_float_bits, raw_source_timestamp_bits)
                 VALUES ('00000000-0000-0000-0000-000000000001', 1, 40.125, -105.25,
                         'valid', 'accepted', zeroblob(73), X'FFFFFFFFFFFFFFFF');
                 INSERT INTO live_capture_ble_observations
                     (capture_id, sequence, direction, characteristic_uuid, service_uuid,
                      transport_payload, raw_telemetry_json)
                 VALUES ('00000000-0000-0000-0000-000000000001', 0, 'inbound',
                         X'00112233445566778899AABBCCDDEEFF',
                         X'FFEEDDCCBBAA99887766554433221100', X'0001FEFF', X'7B7D');
                 INSERT INTO live_capture_ble_raw_telemetry_fields
                     (capture_id, sequence, field_kind, field_index, field_id,
                      integer_value, float_value_bits)
                 VALUES ('00000000-0000-0000-0000-000000000001', 0, 'integer', 0,
                         65535, -9223372036854775808, NULL),
                        ('00000000-0000-0000-0000-000000000001', 0, 'float', 0,
                         42, NULL, X'0100C07F');
                 INSERT INTO live_capture_ble_semantic_telemetry
                     (capture_id, sequence, observed_at_ms, provenance, snapshot_schema_version,
                      library_version, snapshot_json)
                 VALUES ('00000000-0000-0000-0000-000000000001', 0, 99, 'live_session',
                         1, 'test-version', '{"unknown":[1,null,{"bits":"7fc00001"}]}');"#,
            )
            .unwrap();
        connection.execute_batch(LEGACY_CAPTURE_INDEXES).unwrap();
        assert_eq!(capture_secondary_indexes(connection).len(), 5);
    }

    fn assert_capture_constraints(connection: &mut Connection) {
        for invalid in [
            "INSERT INTO live_capture_events SELECT * FROM live_capture_events WHERE sequence = 0",
            "INSERT INTO live_capture_ble_observations
             (capture_id, sequence, direction, characteristic_uuid, transport_payload)
             VALUES ('00000000-0000-0000-0000-000000000001', 99, 'inbound', zeroblob(16), X'01')",
            "UPDATE live_capture_ble_raw_telemetry_fields SET float_value_bits = X'00'
             WHERE field_kind = 'float'",
            "UPDATE live_capture_sessions SET next_sequence = -1",
        ] {
            assert_eq!(
                connection
                    .execute_batch(invalid)
                    .unwrap_err()
                    .sqlite_error_code(),
                Some(rusqlite::ErrorCode::ConstraintViolation),
                "constraint must reject {invalid}"
            );
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        let integrity: String = connection
            .pragma_query_value(None, "integrity_check", |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
        let transaction = connection.transaction().unwrap();
        transaction
            .execute("DELETE FROM live_capture_sessions", [])
            .unwrap();
        assert!(capture_rows(&transaction).iter().all(Vec::is_empty));
        transaction.rollback().unwrap();
    }

    fn assert_capture_page_uses_primary_keys(connection: &Connection) {
        let mut statement = connection
            .prepare(
                "EXPLAIN QUERY PLAN SELECT event.sequence, event.payload, location.raw_float_bits
                 FROM live_capture_events AS event
                 LEFT JOIN live_capture_location_observations AS location
                   ON location.capture_id = event.capture_id AND location.sequence = event.sequence
                 WHERE event.capture_id = ?1 AND event.sequence > COALESCE(?2, -1)
                 ORDER BY event.sequence LIMIT ?3",
            )
            .unwrap();
        let plan = statement
            .query_map(
                params!["00000000-0000-0000-0000-000000000001", 0, 10],
                |row| row.get::<_, String>(3),
            )
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            plan.iter()
                .any(|step| step.contains("SEARCH event USING PRIMARY KEY"))
        );
        assert!(
            plan.iter()
                .any(|step| step.contains("SEARCH location USING PRIMARY KEY"))
        );
        assert!(plan.iter().all(|step| !step.contains("TEMP B-TREE")));
    }

    #[test]
    fn fresh_schema_omits_unused_capture_indexes() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        assert_eq!(capture_secondary_indexes(&connection), Vec::<String>::new());
        assert_capture_page_uses_primary_keys(&connection);
    }

    #[test]
    fn capture_indexes_migration_preserves_every_value_and_constraint() {
        // v34 exercises a prior direct-to-current entry as well as the v35 upgrade.
        for version in [34, 35] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("capture.sqlite3");
            let mut connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "foreign_keys", "ON")
                .unwrap();
            initialize_current_schema(&connection).unwrap();
            populate_indexed_capture(&connection);
            let before = capture_rows(&connection);
            connection
                .pragma_update(None, "user_version", version)
                .unwrap();

            migrate(&mut connection).unwrap();

            assert_eq!(capture_secondary_indexes(&connection), Vec::<String>::new());
            assert_eq!(capture_rows(&connection), before);
            assert_capture_constraints(&mut connection);
            assert_eq!(capture_rows(&connection), before);
            assert_capture_page_uses_primary_keys(&connection);
            assert_eq!(
                connection
                    .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                    .unwrap(),
                CURRENT_SCHEMA_VERSION
            );
            drop(connection);
            let mut reopened = Connection::open(&path).unwrap();
            reopened.pragma_update(None, "foreign_keys", "ON").unwrap();
            migrate(&mut reopened).unwrap();
            assert_eq!(capture_secondary_indexes(&reopened), Vec::<String>::new());
            assert_eq!(capture_rows(&reopened), before);
            assert_capture_constraints(&mut reopened);
            assert_eq!(capture_rows(&reopened), before);
        }
    }

    #[test]
    fn capture_indexes_migration_rolls_back_partial_index_removal() {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        initialize_current_schema(&connection).unwrap();
        populate_indexed_capture(&connection);
        connection.pragma_update(None, "user_version", 35).unwrap();
        let before = capture_rows(&connection);
        let indexes_before = capture_secondary_indexes(&connection);
        let drops = Arc::new(AtomicUsize::new(0));
        let observed_drops = Arc::clone(&drops);
        connection.authorizer(Some(move |context: AuthContext<'_>| match context.action {
            AuthAction::DropIndex { .. } if observed_drops.fetch_add(1, Ordering::SeqCst) == 1 => {
                Authorization::Deny
            }
            _ => Authorization::Allow,
        }));

        assert!(migrate(&mut connection).is_err());

        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert_eq!(drops.load(Ordering::SeqCst), 2);
        assert_eq!(capture_secondary_indexes(&connection), indexes_before);
        assert_eq!(capture_rows(&connection), before);
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            35
        );
        migrate(&mut connection).unwrap();
        assert_eq!(capture_secondary_indexes(&connection), Vec::<String>::new());
        assert_eq!(capture_rows(&connection), before);
    }

    #[test]
    fn schema_v30_migration_adds_raw_location_source_timestamp_bits() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE live_capture_location_observations (raw_float_bits BLOB);
                 PRAGMA application_id = {APPLICATION_ID};
                 PRAGMA user_version = 30;"
            ))
            .unwrap();

        migrate(&mut connection).unwrap();

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert!(
            table_has_column(
                &connection,
                "live_capture_location_observations",
                "raw_source_timestamp_bits"
            )
            .unwrap()
        );
        assert!(table_exists(&connection, "live_capture_ble_observations").unwrap());
    }

    #[test]
    fn schema_v31_migration_adds_structured_live_ble_rows() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!(
                "PRAGMA application_id = {APPLICATION_ID};
                 PRAGMA user_version = 31;"
            ))
            .unwrap();

        migrate(&mut connection).unwrap();

        assert!(table_exists(&connection, "live_capture_ble_observations").unwrap());
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn schema_v32_migration_adds_queryable_raw_telemetry_fields() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_current_schema(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE live_capture_ble_raw_telemetry_fields;
                 DROP TABLE live_capture_ble_semantic_telemetry;
                 PRAGMA user_version = 32;",
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert!(table_exists(&connection, "live_capture_ble_raw_telemetry_fields").unwrap());
        assert!(table_exists(&connection, "live_capture_ble_semantic_telemetry").unwrap());
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn schema_v33_migration_adds_semantic_capture_telemetry() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_current_schema(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE live_capture_ble_semantic_telemetry;
                 PRAGMA user_version = 33;",
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert!(table_exists(&connection, "live_capture_ble_semantic_telemetry").unwrap());
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn schema_v27_migration_preserves_rows_with_unknown_integrity() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE live_capture_sessions (
                     capture_id TEXT PRIMARY KEY NOT NULL,
                     state TEXT NOT NULL,
                     header_json BLOB NOT NULL,
                     started_at_ms INTEGER NOT NULL,
                     finished_at_ms INTEGER,
                     next_sequence INTEGER NOT NULL,
                     stored_bytes INTEGER NOT NULL
                 );
                 INSERT INTO live_capture_sessions
                     (capture_id, state, header_json, started_at_ms, finished_at_ms,
                      next_sequence, stored_bytes)
                 VALUES ('00000000-0000-0000-0000-000000000001', 'finished', X'7B7D',
                         100, 120, 0, 2);
                 PRAGMA application_id = 1129665615;
                 PRAGMA user_version = 27;",
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        let migrated: (String, i64, String, i64) = connection
            .query_row(
                "SELECT capture_id, started_at_ms, integrity, dropped_messages
                 FROM live_capture_sessions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            migrated,
            (
                "00000000-0000-0000-0000-000000000001".to_owned(),
                100,
                "unknown".to_owned(),
                0
            )
        );
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn main_v27_schema_preserves_ride_recording_preference_when_capture_tables_are_added() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_current_schema(&connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO ride_recording_preferences (id, autostart_enabled) VALUES (1, 0);
                 DROP TABLE live_capture_ble_semantic_telemetry;
                 DROP TABLE live_capture_ble_raw_telemetry_fields;
                 DROP TABLE live_capture_ble_observations;
                 DROP TABLE live_capture_location_observations;
                 DROP TABLE live_capture_events;
                 DROP TABLE live_capture_sessions;
                 PRAGMA user_version = 27;",
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(
            connection
                .query_row(
                    "SELECT autostart_enabled FROM ride_recording_preferences WHERE id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert!(table_exists(&connection, "live_capture_sessions").unwrap());
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn v26_live_capture_schema_repairs_missing_session_columns_and_tables() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE live_capture_sessions (
                     capture_id TEXT PRIMARY KEY NOT NULL,
                     state TEXT NOT NULL,
                     header_json BLOB NOT NULL,
                     started_at_ms INTEGER NOT NULL,
                     finished_at_ms INTEGER,
                     next_sequence INTEGER NOT NULL,
                     stored_bytes INTEGER NOT NULL
                 );",
            )
            .unwrap();

        ensure_live_capture_sessions_schema(&connection).unwrap();

        assert!(table_has_column(&connection, "live_capture_sessions", "integrity").unwrap());
        assert!(
            table_has_column(&connection, "live_capture_sessions", "dropped_messages").unwrap()
        );
        assert!(table_exists(&connection, "live_capture_events").unwrap());
    }

    #[test]
    fn schema_v34_migration_adds_ride_recording_preferences() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_current_schema(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE ride_recording_preferences;
                 PRAGMA user_version = 34;",
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert!(table_exists(&connection, "ride_recording_preferences").unwrap());
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn schema_v28_migration_adds_structured_live_location_rows() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!(
                "{CAPTURE_SCHEMA_V36}
                     PRAGMA application_id = 1129665615;
                     PRAGMA user_version = 28;"
            ))
            .unwrap();

        migrate(&mut connection).unwrap();

        assert!(table_exists(&connection, "live_capture_location_observations").unwrap());
        assert!(table_exists(&connection, "live_capture_ble_observations").unwrap());
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }
}
