//! Compact, lossless storage for raw BMS voltage observations.

use super::{BmsVoltageSampleRecord, StorageError, bms_sqlite_integer};
use rusqlite::{Connection, TransactionBehavior, params};

const LEGACY_BATCH_ROWS: i64 = 256;

/// Creates the normalized tables and compatibility view after the original table has
/// been renamed to `bms_voltage_samples_legacy`. On a fresh database, creates the
/// empty legacy table as well so readers see the same view from the first schema.
pub(super) fn create_compact_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS bms_voltage_samples_legacy (
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
         CREATE TABLE bms_devices (
             id INTEGER PRIMARY KEY,
             identity TEXT NOT NULL UNIQUE CHECK (length(identity) BETWEEN 1 AND 512)
         );
         CREATE TABLE bms_sessions (
             id INTEGER PRIMARY KEY,
             device_id INTEGER NOT NULL REFERENCES bms_devices(id),
             identifier TEXT NOT NULL CHECK (length(identifier) BETWEEN 1 AND 512),
             UNIQUE (device_id, identifier),
             UNIQUE (id, device_id)
         );
         CREATE TABLE bms_cell_samples (
             session_id INTEGER NOT NULL,
             device_id INTEGER NOT NULL,
             event_sequence INTEGER NOT NULL CHECK (event_sequence >= 0),
             monotonic_ms INTEGER NOT NULL CHECK (monotonic_ms >= 0),
             wall_clock_ms INTEGER NOT NULL CHECK (wall_clock_ms >= 0),
             observation_index INTEGER NOT NULL CHECK (observation_index BETWEEN 0 AND 65535),
             pack_index INTEGER CHECK (pack_index IS NULL OR pack_index BETWEEN 0 AND 65535),
             pack_observation_index INTEGER CHECK (pack_observation_index IS NULL OR pack_observation_index BETWEEN 0 AND 65535),
             millivolts INTEGER NOT NULL,
             PRIMARY KEY (session_id, event_sequence, observation_index),
             FOREIGN KEY (session_id, device_id) REFERENCES bms_sessions(id, device_id)
         ) WITHOUT ROWID;
         CREATE INDEX bms_cell_samples_history
             ON bms_cell_samples(device_id, observation_index, wall_clock_ms DESC);
         CREATE TABLE bms_voltage_backfill_progress (
             singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
             last_rowid INTEGER CHECK (last_rowid IS NULL OR last_rowid >= 0),
             complete INTEGER NOT NULL DEFAULT 0 CHECK (complete IN (0, 1))
         );
         INSERT INTO bms_voltage_backfill_progress(singleton) VALUES (1);
         CREATE VIEW bms_voltage_samples AS
         SELECT d.identity AS device_identity, s.identifier AS session_identifier,
                c.event_sequence, c.monotonic_ms, c.wall_clock_ms,
                c.observation_index, c.pack_index, c.pack_observation_index, c.millivolts
         FROM bms_cell_samples c
         JOIN bms_sessions s ON s.id = c.session_id
         JOIN bms_devices d ON d.id = c.device_id
         UNION ALL
         SELECT l.device_identity, l.session_identifier, l.event_sequence,
                l.monotonic_ms, l.wall_clock_ms, l.observation_index,
                l.pack_index, l.pack_observation_index, l.millivolts
         FROM bms_voltage_samples_legacy l
         WHERE NOT EXISTS (
             SELECT 1 FROM bms_cell_samples c
             JOIN bms_sessions s ON s.id = c.session_id
             JOIN bms_devices d ON d.id = c.device_id
             WHERE d.identity = l.device_identity AND s.identifier = l.session_identifier
               AND c.event_sequence = l.event_sequence
               AND c.observation_index = l.observation_index
               AND c.monotonic_ms = l.monotonic_ms AND c.wall_clock_ms = l.wall_clock_ms
               AND c.pack_index IS l.pack_index
               AND c.pack_observation_index IS l.pack_observation_index
               AND c.millivolts = l.millivolts
         );",
    )?;
    Ok(())
}

/// Moves at most 256 legacy records into compact storage in one transaction.
pub(super) fn normalize_legacy_batch(
    connection: &mut Connection,
    own_backfill: Option<&str>,
) -> Result<super::normalization::BatchState, StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if super::normalization::has_active_work(&transaction, own_backfill)? {
        return Ok(super::normalization::BatchState::Deferred);
    }
    let (last_rowid, complete): (Option<i64>, bool) = transaction.query_row(
        "SELECT last_rowid, complete FROM bms_voltage_backfill_progress WHERE singleton = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if complete {
        return Ok(super::normalization::BatchState::Finished);
    }
    let start_rowid = last_rowid.unwrap_or(0);
    let (end_rowid, row_count): (Option<i64>, i64) = transaction.query_row(
        "SELECT MAX(rowid), COUNT(*) FROM (
            SELECT rowid FROM bms_voltage_samples_legacy
            WHERE rowid > ?1 ORDER BY rowid LIMIT ?2
         )",
        params![start_rowid, LEGACY_BATCH_ROWS],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let Some(end_rowid) = end_rowid else {
        transaction.execute(
            "UPDATE bms_voltage_backfill_progress SET complete = 1 WHERE singleton = 1",
            [],
        )?;
        transaction.commit()?;
        return Ok(super::normalization::BatchState::Finished);
    };

    transaction.execute(
        "INSERT OR IGNORE INTO bms_devices(identity)
         SELECT DISTINCT device_identity FROM bms_voltage_samples_legacy
         WHERE rowid > ?1 AND rowid <= ?2",
        params![start_rowid, end_rowid],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO bms_sessions(device_id, identifier)
         SELECT DISTINCT d.id, l.session_identifier
         FROM bms_voltage_samples_legacy l
         JOIN bms_devices d ON d.identity = l.device_identity
         WHERE l.rowid > ?1 AND l.rowid <= ?2",
        params![start_rowid, end_rowid],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO bms_cell_samples
         (session_id, device_id, event_sequence, monotonic_ms, wall_clock_ms,
          observation_index, pack_index, pack_observation_index, millivolts)
         SELECT s.id, d.id, l.event_sequence, l.monotonic_ms, l.wall_clock_ms,
                l.observation_index, l.pack_index, l.pack_observation_index, l.millivolts
         FROM bms_voltage_samples_legacy l
         JOIN bms_devices d ON d.identity = l.device_identity
         JOIN bms_sessions s ON s.device_id = d.id AND s.identifier = l.session_identifier
         WHERE l.rowid > ?1 AND l.rowid <= ?2",
        params![start_rowid, end_rowid],
    )?;
    let deleted_any = transaction.execute(
        "DELETE FROM bms_voltage_samples_legacy
         WHERE rowid > ?1 AND rowid <= ?2
           AND EXISTS (
                SELECT 1
                FROM bms_cell_samples c
                JOIN bms_sessions s ON s.id = c.session_id
                JOIN bms_devices d ON d.id = c.device_id
                WHERE d.identity = bms_voltage_samples_legacy.device_identity
                  AND s.identifier = bms_voltage_samples_legacy.session_identifier
                  AND c.event_sequence = bms_voltage_samples_legacy.event_sequence
                  AND c.monotonic_ms = bms_voltage_samples_legacy.monotonic_ms
                  AND c.wall_clock_ms = bms_voltage_samples_legacy.wall_clock_ms
                  AND c.observation_index = bms_voltage_samples_legacy.observation_index
                  AND c.pack_index IS bms_voltage_samples_legacy.pack_index
                  AND c.pack_observation_index IS bms_voltage_samples_legacy.pack_observation_index
                  AND c.millivolts = bms_voltage_samples_legacy.millivolts
           )",
        params![start_rowid, end_rowid],
    )? != 0;

    let complete = row_count < LEGACY_BATCH_ROWS;
    transaction.execute(
        "UPDATE bms_voltage_backfill_progress SET last_rowid = ?1, complete = ?2 WHERE singleton = 1",
        params![end_rowid, complete],
    )?;
    if deleted_any {
        transaction.execute(
            "UPDATE storage_normalization_progress
             SET compaction_pending = 1, compaction_complete = 0 WHERE singleton = 1",
            [],
        )?;
    }
    transaction.commit()?;
    Ok(if complete {
        super::normalization::BatchState::Finished
    } else {
        super::normalization::BatchState::Pending
    })
}

fn intern_ids(
    connection: &Connection,
    device: &str,
    session: &str,
) -> Result<(i64, i64), StorageError> {
    connection
        .prepare_cached("INSERT OR IGNORE INTO bms_devices(identity) VALUES (?1)")?
        .execute([device])?;
    let device_id: i64 = connection
        .prepare_cached("SELECT id FROM bms_devices WHERE identity = ?1")?
        .query_row([device], |row| row.get(0))?;
    connection
        .prepare_cached(
            "INSERT OR IGNORE INTO bms_sessions(device_id, identifier) VALUES (?1, ?2)",
        )?
        .execute(params![device_id, session])?;
    let session_id: i64 = connection
        .prepare_cached("SELECT id FROM bms_sessions WHERE device_id = ?1 AND identifier = ?2")?
        .query_row(params![device_id, session], |row| row.get(0))?;
    Ok((device_id, session_id))
}

pub(super) fn record_samples(
    connection: &mut Connection,
    samples: &[BmsVoltageSampleRecord],
) -> Result<(), StorageError> {
    if samples.is_empty() {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    let mut legacy_exists = transaction.prepare_cached(
        "SELECT EXISTS(
             SELECT 1 FROM bms_voltage_samples_legacy
             WHERE device_identity = ?1 AND session_identifier = ?2
               AND event_sequence = ?3 AND observation_index = ?4
         )",
    )?;
    let mut insert_sample = transaction.prepare_cached(
        "INSERT OR IGNORE INTO bms_cell_samples
         (session_id, device_id, event_sequence, monotonic_ms, wall_clock_ms,
          observation_index, pack_index, pack_observation_index, millivolts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for sample in samples {
        let sequence = bms_sqlite_integer(sample.event_sequence, "BMS event sequence")?;
        let monotonic =
            bms_sqlite_integer(sample.monotonic_milliseconds, "BMS monotonic timestamp")?;
        let wall = bms_sqlite_integer(sample.wall_clock_milliseconds, "BMS wall clock timestamp")?;
        let observation = i64::from(sample.observation_index);
        let pack = sample.pack_index.map(i64::from);
        let pack_observation = sample.pack_observation_index.map(i64::from);
        let old_exists: bool = legacy_exists.query_row(
            params![
                sample.device_identity,
                sample.session_identifier,
                sequence,
                observation
            ],
            |row| row.get(0),
        )?;
        if old_exists {
            continue;
        }
        let (device_id, session_id) = intern_ids(
            &transaction,
            &sample.device_identity,
            &sample.session_identifier,
        )?;
        insert_sample.execute(params![
            session_id,
            device_id,
            sequence,
            monotonic,
            wall,
            observation,
            pack,
            pack_observation,
            sample.millivolts,
        ])?;
    }
    drop(insert_sample);
    drop(legacy_exists);
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE storage_normalization_progress(singleton INTEGER PRIMARY KEY, compaction_pending INTEGER NOT NULL DEFAULT 0, compaction_complete INTEGER NOT NULL DEFAULT 1); INSERT INTO storage_normalization_progress(singleton) VALUES (1); CREATE TABLE rides(state TEXT); CREATE TABLE live_capture_sessions(state TEXT); CREATE TABLE pevcap_import_work(dummy INTEGER); CREATE TABLE pevcap_captures(receipt_digest TEXT, artifact_digest TEXT);").unwrap();
        create_compact_schema(&connection).unwrap();
        connection
    }

    fn sample(sequence: u64, millivolts: i32) -> BmsVoltageSampleRecord {
        BmsVoltageSampleRecord::new(
            "device",
            "session",
            sequence,
            sequence + 10,
            sequence + 20,
            3,
            millivolts,
        )
        .unwrap()
        .with_pack_identity(Some(2), Some(4))
    }

    #[test]
    fn compact_writer_is_idempotent_and_ignores_conflicting_duplicate() {
        let mut connection = db();
        record_samples(&mut connection, &[sample(7, 4100)]).unwrap();
        record_samples(&mut connection, &[sample(7, 4100)]).unwrap();
        record_samples(&mut connection, &[sample(7, 4200)]).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM bms_voltage_samples", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn mixed_writer_batch_ignores_legacy_and_compact_conflicts_but_commits_fresh_sample() {
        let mut connection = db();
        connection.execute(
            "INSERT INTO bms_voltage_samples_legacy VALUES ('device','session',1,11,21,3,2,4,4100)", [],
        ).unwrap();
        record_samples(&mut connection, &[sample(2, 4100)]).unwrap();

        record_samples(
            &mut connection,
            &[sample(1, 4200), sample(2, 4200), sample(3, 4300)],
        )
        .unwrap();

        let retained: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM bms_voltage_samples WHERE millivolts=4100",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let fresh: i64 = connection.query_row(
            "SELECT COUNT(*) FROM bms_voltage_samples WHERE event_sequence=3 AND millivolts=4300", [], |row| row.get(0),
        ).unwrap();
        let legacy: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM bms_voltage_samples_legacy",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((retained, fresh, legacy), (2, 1, 1));
    }

    #[test]
    fn compatibility_view_reconstructs_legacy_values_and_metadata() {
        let mut connection = db();
        connection.execute("INSERT INTO bms_voltage_samples_legacy VALUES ('old-device','old-session',8,18,28,3,2,4,4100)", []).unwrap();
        normalize_legacy_batch(&mut connection, None).unwrap();
        let row: (
            String,
            String,
            i64,
            i64,
            i64,
            i64,
            Option<i64>,
            Option<i64>,
            i64,
        ) = connection
            .query_row("SELECT * FROM bms_voltage_samples", [], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            })
            .unwrap();
        assert_eq!(
            row,
            (
                "old-device".into(),
                "old-session".into(),
                8,
                18,
                28,
                3,
                Some(2),
                Some(4),
                4100
            )
        );
    }

    #[test]
    fn backfill_moves_at_most_256_rows_per_call() {
        let mut connection = db();
        for sequence in 0..300_i64 {
            connection.execute("INSERT INTO bms_voltage_samples_legacy VALUES ('d','s',?1,?1,?1,0,NULL,NULL,4000)", [sequence]).unwrap();
        }
        assert_eq!(
            normalize_legacy_batch(&mut connection, None).unwrap(),
            super::super::normalization::BatchState::Pending
        );
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM bms_voltage_samples_legacy", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(remaining, 44);
        let progress: (Option<i64>, bool) = connection
            .query_row(
                "SELECT last_rowid, complete FROM bms_voltage_backfill_progress WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(progress, (Some(256), false));
        assert_eq!(
            normalize_legacy_batch(&mut connection, None).unwrap(),
            super::super::normalization::BatchState::Finished
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM bms_voltage_samples_legacy", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn failed_batch_rolls_back_copy_and_delete() {
        let mut connection = db();
        connection
            .execute(
                "INSERT INTO bms_voltage_samples_legacy VALUES ('d','s',1,1,1,0,NULL,NULL,4000)",
                [],
            )
            .unwrap();
        connection.execute_batch("CREATE TRIGGER stop_delete BEFORE DELETE ON bms_voltage_samples_legacy BEGIN SELECT RAISE(ABORT, 'blocked'); END;").unwrap();
        assert!(normalize_legacy_batch(&mut connection, None).is_err());
        let legacy: i64 = connection
            .query_row("SELECT COUNT(*) FROM bms_voltage_samples_legacy", [], |r| {
                r.get(0)
            })
            .unwrap();
        let compact: i64 = connection
            .query_row("SELECT COUNT(*) FROM bms_cell_samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!((legacy, compact), (1, 0));
    }

    #[test]
    fn empty_batch_does_not_request_database_vacuum() {
        let mut connection = db();
        assert_eq!(
            normalize_legacy_batch(&mut connection, None).unwrap(),
            super::super::normalization::BatchState::Finished
        );
        let flags: (bool, bool) = connection.query_row(
            "SELECT compaction_pending, compaction_complete FROM storage_normalization_progress WHERE singleton=1", [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(flags, (false, true));
    }

    #[test]
    fn active_database_work_defers_backfill_without_advancing_cursor() {
        let mut connection = db();
        connection
            .execute("INSERT INTO rides(state) VALUES ('active')", [])
            .unwrap();
        connection
            .execute(
                "INSERT INTO bms_voltage_samples_legacy VALUES ('d','s',1,1,1,0,NULL,NULL,4000)",
                [],
            )
            .unwrap();
        assert_eq!(
            normalize_legacy_batch(&mut connection, None).unwrap(),
            super::super::normalization::BatchState::Deferred
        );
        let cursor: Option<i64> = connection
            .query_row(
                "SELECT last_rowid FROM bms_voltage_backfill_progress WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cursor, None);
    }

    #[test]
    fn conflicting_legacy_duplicate_remains_visible_and_does_not_block_completion() {
        let mut connection = db();
        record_samples(&mut connection, &[sample(1, 4100)]).unwrap();
        connection.execute(
            "INSERT INTO bms_voltage_samples_legacy VALUES ('device','session',1,11,21,3,2,4,4200)", [],
        ).unwrap();
        assert_eq!(
            normalize_legacy_batch(&mut connection, None).unwrap(),
            super::super::normalization::BatchState::Finished
        );
        let (legacy, view): (i64, i64) = (
            connection
                .query_row("SELECT COUNT(*) FROM bms_voltage_samples_legacy", [], |r| {
                    r.get(0)
                })
                .unwrap(),
            connection
                .query_row(
                    "SELECT COUNT(*) FROM bms_voltage_samples WHERE millivolts=4200",
                    [],
                    |r| r.get(0),
                )
                .unwrap(),
        );
        assert_eq!((legacy, view), (1, 1));
    }

    #[test]
    fn writer_and_backfill_are_idempotent_across_legacy_and_compact_rows() {
        let mut connection = db();
        connection.execute("INSERT INTO bms_voltage_samples_legacy VALUES ('device','session',7,17,27,3,2,4,4100)", []).unwrap();
        record_samples(
            &mut connection,
            &[
                BmsVoltageSampleRecord::new("device", "session", 7, 17, 27, 3, 4100)
                    .unwrap()
                    .with_pack_identity(Some(2), Some(4)),
            ],
        )
        .unwrap();
        normalize_legacy_batch(&mut connection, None).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM bms_voltage_samples", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
