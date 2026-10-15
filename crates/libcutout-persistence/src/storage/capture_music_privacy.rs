//! Removal of obsolete music metadata from application-owned Bluetooth captures.

use super::StorageError;
mod artifacts;
mod live;
mod transform;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub(super) fn purge(
    connection: &mut Connection,
    database_path: &Path,
    directories: &[PathBuf],
) -> Result<(), StorageError> {
    live::check_active(connection)?;
    artifacts::check_imports(connection)?;
    artifacts::purge_imports(connection, database_path)?;
    live::purge(connection)?;
    artifacts::purge_files(database_path, directories)?;
    Ok(())
}
#[cfg(test)]
use cutout_core::PevcapEncoding;
pub(crate) use transform::remove_music as sanitize_capture_music;
#[cfg(test)]
use transform::remove_music;

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{
        GattChannel, MonotonicTimestamp, MusicProvider, PevcapCapture, PevcapHeader,
        PevcapMusicEvent, PevcapRecord, WallClockUnixTimestamp,
    };

    pub(super) fn capture() -> PevcapCapture {
        let music = PevcapMusicEvent::new(
            MusicProvider::Spotify,
            "spotify:track:private",
            MonotonicTimestamp::new(1),
            WallClockUnixTimestamp::new(1_000),
            0,
            None,
        )
        .unwrap();
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(1_000),
            "wheel",
            None,
            &[],
            &[],
            None,
            None,
            "test",
            [0; 32],
            &[],
        )
        .unwrap();
        let mut record = PevcapRecord::inbound_notification(
            MonotonicTimestamp::new(2),
            GattChannel::from_bytes([0x11; 16]),
            GattChannel::from_bytes([0x22; 16]),
            vec![0xff, 0x00, 0xa5],
        );
        record.music = Some(music.clone());
        PevcapCapture::new_with_locations_and_music(header, vec![record], vec![], vec![music])
    }

    #[test]
    fn legacy_capture_music_redaction_preserves_ble_in_both_encodings() {
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let original = capture();
            let source = original.encode(encoding).unwrap();
            let mut output = Vec::new();
            assert!(remove_music(&mut source.as_slice(), &mut output, encoding).unwrap());
            let retained = PevcapCapture::decode(&output, encoding).unwrap();
            assert_eq!(retained.music_events, Vec::<PevcapMusicEvent>::new());
            assert!(retained.records.iter().all(|record| record.music.is_none()));
            let mut expected = original.records;
            expected[0].music = None;
            assert_eq!(retained.records, expected);
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use rusqlite::params;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::Cursor;

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        crate::storage::migrations::migrate(&mut connection).unwrap();
        connection
    }

    #[test]
    fn capture_music_cleanup_preserves_unknown_raw_json_and_is_idempotent() {
        let input=b"{\"kind\":\"record\",\"future\":1.00,\"record\":{\"bytes\":\"ff00\",\"future\":[1.00, 9e2],\"music\":{\"private\":\"track\"}}}\n";
        let mut output = Vec::new();
        assert!(remove_music(&mut input.as_slice(), &mut output, PevcapEncoding::Jsonl).unwrap());
        let text = String::from_utf8(output.clone()).unwrap();
        assert!(text.contains("\"future\":1.00"));
        assert!(text.contains("[1.00, 9e2]"));
        assert!(text.contains("\"bytes\":\"ff00\""));
        assert!(!text.contains("private"));
        let escaped = br#"{"kind":"\u006dusic","music":{"private":"track"}}"#;
        let mut removed = Vec::new();
        assert!(
            remove_music(&mut escaped.as_slice(), &mut removed, PevcapEncoding::Jsonl).unwrap()
        );
        assert_eq!(removed, Vec::<u8>::new());
        let mut again = Vec::new();
        assert!(!remove_music(&mut output.as_slice(), &mut again, PevcapEncoding::Jsonl).unwrap());
        assert_eq!(again, output);
        assert!(
            remove_music(
                &mut Cursor::new(vec![b'x'; 1_048_577]),
                &mut Vec::new(),
                PevcapEncoding::Jsonl
            )
            .is_err()
        );
    }

    fn assert_clean_live_events(connection: &Connection, stored: i64) {
        let mut statement=connection.prepare("SELECT sequence,payload,payload_encoding,payload_original_bytes FROM live_capture_events ORDER BY sequence").unwrap();
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })
            .unwrap();
        let mut logical_bytes = 2_i64;
        for (expected, row) in rows.enumerate() {
            let (sequence, bytes, encoding, original) = row.unwrap();
            assert_eq!(sequence, i64::try_from(expected).unwrap());
            let bytes =
                crate::storage::live_capture::payload::decode(bytes, encoding, original).unwrap();
            logical_bytes += i64::try_from(bytes.len()).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("private"));
        }
        assert_eq!(logical_bytes, stored);
    }

    #[test]
    fn capture_music_cleanup_handles_compressed_events_reindexes_and_preserves_ble_children() {
        use crate::storage::live_capture::{
            self, LiveCaptureEventAppend, LiveCaptureEventKind, LiveCaptureId,
        };
        let mut connection = connection();
        let id = LiveCaptureId::new();
        live_capture::begin(&connection, id, b"{}", 0).unwrap();
        let raw=br#"{"kind":"record","record":{"bytes":[255,0,165],"music":{"provider":"spotify","track":"private"},"unknown":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}}"#;
        for (kind, bytes) in [
            (
                LiveCaptureEventKind::Music,
                br#"{"kind":"music","music":{"track":"private"}}"#.as_slice(),
            ),
            (LiveCaptureEventKind::Notification, raw.as_slice()),
            (
                LiveCaptureEventKind::Metadata,
                br#"{"kind":"metadata","future":1.00}"#.as_slice(),
            ),
        ] {
            live_capture::append(
                &mut connection,
                &LiveCaptureEventAppend {
                    id,
                    kind,
                    receipt_monotonic_ms: 0,
                    source_monotonic_offset_ms: None,
                    source_wall_clock_unix_ms: None,
                    payload: bytes,
                    location: None,
                    ble_record: None,
                },
            )
            .unwrap();
        }
        // Attach actual structured BLE evidence; all of these links must move with sequence 1.
        connection.execute("INSERT INTO live_capture_ble_observations(capture_id,sequence,direction,characteristic_uuid,transport_payload) VALUES(?1,1,'inbound',zeroblob(16),?2)",params![id.to_string(),vec![255u8,0,165]]).unwrap();
        connection.execute("INSERT INTO live_capture_ble_semantic_telemetry(capture_id,sequence,provenance,snapshot_schema_version,library_version,snapshot_json) VALUES(?1,1,'live_session',1,'test','{}')",[id.to_string()]).unwrap();
        assert!(live::check_active(&connection).is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM live_capture_events", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        connection
            .execute(
                "UPDATE live_capture_sessions SET state='finished',finished_at_ms=0",
                [],
            )
            .unwrap();
        live::purge(&mut connection).unwrap();
        let (count, stored): (i64, i64) = connection
            .query_row(
                "SELECT next_sequence,stored_bytes FROM live_capture_sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert_clean_live_events(&connection, stored);
        for table in [
            "live_capture_ble_observations",
            "live_capture_ble_semantic_telemetry",
        ] {
            assert_eq!(
                connection
                    .query_row(&format!("SELECT sequence FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        let ble: Vec<u8> = connection
            .query_row(
                "SELECT transport_payload FROM live_capture_ble_observations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ble, [255, 0, 165]);
        assert!(
            !connection
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap()
        );
    }

    #[test]
    fn capture_music_cleanup_relinks_indexed_bytes_and_recording_identity_without_touching_import_source()
     {
        let mut connection = connection();
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("rides.sqlite");
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = tests::capture().encode(encoding).unwrap();
            let old = hex::encode(Sha256::digest(&bytes));
            let extension = if encoding == PevcapEncoding::Jsonl {
                "jsonl"
            } else {
                "pevcap"
            };
            let source = directory.path().join(format!("external-{old}.{extension}"));
            fs::write(&source, &bytes).unwrap();
            let managed = crate::storage::pevcap_import_directory(&database);
            fs::create_dir_all(&managed).unwrap();
            let owned = managed.join(format!("{old}.{extension}"));
            // The canonical chunks and a divergent historical file both contain BLE
            // evidence; cleanup must retain both streams instead of unlinking blindly.
            let mut divergent = tests::capture();
            divergent.records[0].bytes = vec![254, 0, 165].into();
            fs::write(&owned, divergent.encode(encoding).unwrap()).unwrap();
            connection.execute("INSERT INTO pevcap_imports(artifact_digest,artifact_path,outcome,artifact_size,record_count,location_count,imported_at_ms) VALUES(?1,?2,'capture_only',?3,1,0,100)",params![old,source.to_str(),bytes.len()]).unwrap();
            connection.execute("INSERT INTO pevcap_captures(artifact_digest,receipt_digest,encoding,artifact_size,written_bytes,next_sequence) VALUES(?1,?1,?2,?3,?3,1)",params![old,if encoding==PevcapEncoding::Jsonl{"jsonl"}else{"binary"},bytes.len()]).unwrap();
            connection.execute("INSERT INTO pevcap_capture_chunks(artifact_digest,sequence,payload) VALUES(?1,0,?2)",params![old,bytes]).unwrap();
            let artifact_id = uuid::Uuid::new_v4().to_string();
            connection.execute("INSERT INTO pevcap_recordings(artifact_id,artifact_digest,origin,platform_identifier,started_at_ms) VALUES(?1,?2,'manual','wheel',0)",params![artifact_id,old]).unwrap();
            // Fail after the derivative file is durable, before the index transaction commits.
            connection.execute_batch("CREATE TEMP TRIGGER reject_cleanup BEFORE UPDATE OF artifact_digest ON pevcap_recordings BEGIN SELECT RAISE(ABORT,'injected relink failure'); END;").unwrap();
            assert!(purge(&mut connection, &database, &[]).is_err());
            assert!(owned.exists());
            assert_eq!(
                crate::storage::capture_data::chunk(&connection, &old, 0)
                    .unwrap()
                    .unwrap(),
                bytes
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT artifact_digest FROM pevcap_recordings WHERE artifact_id=?1",
                        [&artifact_id],
                        |r| r.get::<_, String>(0)
                    )
                    .unwrap(),
                old
            );
            connection
                .execute_batch("DROP TRIGGER reject_cleanup;")
                .unwrap();
            // Reuse the already-published sanitized derivative on retry; preserve original ID.
            purge(&mut connection, &database, &[]).unwrap();
            let (new,new_path):(String,String)=connection.query_row("SELECT artifact_digest,artifact_path FROM pevcap_imports JOIN pevcap_recordings USING(artifact_digest) WHERE artifact_id=?1",[artifact_id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
            assert_ne!(new, old);
            assert!(!owned.exists());
            assert_eq!(fs::read(&source).unwrap(), bytes);
            let rewritten = fs::read(new_path).unwrap();
            assert_eq!(hex::encode(Sha256::digest(&rewritten)), new);
            let capture = cutout_core::PevcapCapture::decode(&rewritten, encoding).unwrap();
            assert_eq!(
                capture.music_events,
                Vec::<cutout_core::PevcapMusicEvent>::new()
            );
            assert!(capture.records[0].music.is_none());
            assert_eq!(capture.records[0].bytes.as_ref(), [255, 0, 165]);
            assert!(
                fs::read_dir(&managed)
                    .unwrap()
                    .filter_map(Result::ok)
                    .any(|entry| {
                        let path = entry.path();
                        path.extension().is_some_and(|value| value == extension)
                            && fs::read(path)
                                .ok()
                                .and_then(|bytes| {
                                    cutout_core::PevcapCapture::decode(&bytes, encoding).ok()
                                })
                                .is_some_and(|capture| {
                                    capture.records[0].bytes.as_ref() == [254, 0, 165]
                                        && capture.records[0].music.is_none()
                                })
                    })
            );
            assert!(
                crate::storage::capture_data::chunk(&connection, &old, 0)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                crate::storage::capture_data::chunk(&connection, &new, 0)
                    .unwrap()
                    .unwrap(),
                rewritten
            );
            purge(&mut connection, &database, &[]).unwrap();
        }
        assert!(
            !connection
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap()
        );
    }

    #[test]
    fn capture_music_cleanup_rewrites_only_known_owned_names_and_breaks_hardlinks() {
        let mut connection = connection();
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("rides.sqlite");
        let owned = directory.path().join("Documents");
        fs::create_dir(&owned).unwrap();
        let bytes = tests::capture().encode(PevcapEncoding::Jsonl).unwrap();
        let id = uuid::Uuid::new_v4();
        let path = owned.join(format!("cutout-btle-capture-1700000000-{id}.jsonl"));
        let legacy = owned.join("cutout-btle-capture-1783308929.jsonl");
        fs::write(&legacy, &bytes).unwrap();
        let arbitrary = owned.join("my-import.jsonl");
        let external = directory.path().join("external.jsonl");
        fs::write(&external, &bytes).unwrap();
        fs::hard_link(&external, &path).unwrap();
        fs::write(&arbitrary, &bytes).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                &external,
                owned.join(format!("capture-{}.jsonl", uuid::Uuid::new_v4())),
            )
            .unwrap();
        }
        purge(&mut connection, &database, std::slice::from_ref(&owned)).unwrap();
        assert_eq!(fs::read(&external).unwrap(), bytes);
        assert_eq!(fs::read(&arbitrary).unwrap(), bytes);
        let changed = fs::read(path).unwrap();
        assert!(!String::from_utf8_lossy(&changed).contains("spotify:track:private"));
        let retained = cutout_core::PevcapCapture::decode(&changed, PevcapEncoding::Jsonl).unwrap();
        assert_eq!(retained.records[0].bytes.as_ref(), [255, 0, 165]);
        let legacy =
            cutout_core::PevcapCapture::decode(&fs::read(legacy).unwrap(), PevcapEncoding::Jsonl)
                .unwrap();
        assert_eq!(legacy.records[0].bytes.as_ref(), [255, 0, 165]);
        assert!(legacy.records[0].music.is_none());
        assert_eq!(
            legacy.music_events,
            Vec::<cutout_core::PevcapMusicEvent>::new()
        );
        purge(&mut connection, &database, &[owned]).unwrap();
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fs;

    #[test]
    fn capture_music_cleanup_preserves_original_when_sanitized_destination_is_corrupt_and_retries()
    {
        let directory = tempfile::tempdir().unwrap();
        let bytes = tests::capture().encode(PevcapEncoding::Jsonl).unwrap();
        let old = hex::encode(Sha256::digest(&bytes));
        let original = directory.path().join(format!("{old}.jsonl"));
        fs::write(&original, &bytes).unwrap();
        let mut sanitized = Vec::new();
        transform::remove_with_provenance(
            &mut bytes.as_slice(),
            &mut sanitized,
            PevcapEncoding::Jsonl,
            Some(&old),
        )
        .unwrap();
        let new = hex::encode(Sha256::digest(&sanitized));
        let destination = directory.path().join(format!("{new}.jsonl"));
        fs::write(&destination, b"corrupt").unwrap();
        assert!(artifacts::rewrite_file(&original, directory.path(), true).is_err());
        assert_eq!(fs::read(&original).unwrap(), bytes);
        fs::remove_file(&destination).unwrap();
        #[cfg(unix)]
        {
            let external = directory.path().join("external.jsonl");
            fs::write(&external, b"external").unwrap();
            std::os::unix::fs::symlink(&external, &destination).unwrap();
            assert!(artifacts::rewrite_file(&original, directory.path(), true).is_err());
            assert_eq!(fs::read(&original).unwrap(), bytes);
            assert_eq!(fs::read(&external).unwrap(), b"external");
            fs::remove_file(&destination).unwrap();
        }
        artifacts::rewrite_file(&original, directory.path(), true).unwrap();
        assert!(!original.exists());
        assert_eq!(fs::read(&destination).unwrap(), sanitized);
    }
}
