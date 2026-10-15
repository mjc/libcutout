//! Lossless physical encoding; public capture payloads remain their original bytes.

use super::{LIVE_CAPTURE_EVENT_LIMIT_BYTES, StorageError};
use miniz_oxide::inflate::{
    TINFLStatus,
    core::{DecompressorOxide, decompress, inflate_flags},
};
use std::borrow::Cow;

#[derive(Debug)]
pub(super) struct Encoded<'a> {
    pub(super) bytes: Cow<'a, [u8]>,
    pub(super) encoding: i64,
    pub(super) original_bytes: Option<i64>,
}

pub(super) fn encode(bytes: &[u8]) -> Result<Encoded<'_>, StorageError> {
    if !(1..=LIVE_CAPTURE_EVENT_LIMIT_BYTES).contains(&bytes.len()) {
        return Err(StorageError::LiveCaptureInputInvalid("event payload size"));
    }
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(bytes, 1);
    if compressed.len() < bytes.len() {
        return Ok(Encoded {
            bytes: Cow::Owned(compressed),
            encoding: 1,
            original_bytes: Some(
                i64::try_from(bytes.len())
                    .map_err(|_| StorageError::LiveCaptureInputInvalid("event payload size"))?,
            ),
        });
    }
    Ok(Encoded {
        bytes: Cow::Borrowed(bytes),
        encoding: 0,
        original_bytes: None,
    })
}

pub(super) fn decode(
    bytes: Vec<u8>,
    encoding: i64,
    original_bytes: Option<i64>,
) -> Result<Vec<u8>, StorageError> {
    match (encoding, original_bytes) {
        (0, None) if (1..=LIVE_CAPTURE_EVENT_LIMIT_BYTES).contains(&bytes.len()) => Ok(bytes),
        (1, Some(original_bytes)) => {
            let length = usize::try_from(original_bytes).map_err(|_| invalid("original length"))?;
            if !(1..=LIVE_CAPTURE_EVENT_LIMIT_BYTES).contains(&length)
                || bytes.is_empty()
                || bytes.len() >= length
            {
                return Err(invalid("compressed size or original length"));
            }
            let mut output = vec![0; length];
            let mut state = Box::<DecompressorOxide>::default();
            // The complete input is available. Parse its zlib header and checksum,
            // use bounded nonwrapping output, and reject trailing bytes below.
            let flags = inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER
                | inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
            let (status, consumed, written) = decompress(&mut state, &bytes, &mut output, 0, flags);
            if status != TINFLStatus::Done || consumed != bytes.len() || written != length {
                return Err(invalid("invalid or incomplete zlib payload"));
            }
            Ok(output)
        }
        _ => Err(invalid("encoding or original length")),
    }
}

fn invalid(value: &str) -> StorageError {
    StorageError::InvalidStoredValue {
        field: "live capture payload encoding",
        value: value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LOCATION_SCHEMA, LiveCaptureId, SCHEMA, compress_legacy_payloads, read};
    use super::*;
    use crate::storage::QueryLimit;
    use rusqlite::{Connection, params};

    type SessionSignature = (String, Vec<u8>, i64, i64, i64);

    struct LegacyFixture {
        connection: Connection,
        id: LiveCaptureId,
        header: Vec<u8>,
        original: Vec<u8>,
        event_count: usize,
        session_before: SessionSignature,
    }

    fn legacy_fixture(event_count: usize) -> LegacyFixture {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection.execute_batch(LOCATION_SCHEMA).unwrap();
        let id = LiveCaptureId::new();
        let header = b"{\"capture\":\"legacy\"}".to_vec();
        let original = [0xff, 0x00, 0x7f, b'\n', 0x80].repeat(160);
        connection
            .execute(
                "INSERT INTO live_capture_sessions
             (capture_id, state, integrity, header_json, started_at_ms, stored_bytes,
              next_sequence)
             VALUES (?1, 'interrupted', 'unknown', ?2, 100, ?3, ?4)",
                params![
                    id.as_string(),
                    header.as_slice(),
                    header.len() + original.len() * event_count,
                    event_count,
                ],
            )
            .unwrap();
        for sequence in 0..event_count {
            connection
                .execute(
                    "INSERT INTO live_capture_events
                 (capture_id, sequence, event_kind, receipt_monotonic_ms, payload)
                 VALUES (?1, ?2, 'notification', ?3, ?4)",
                    params![id.as_string(), sequence, sequence * 3, original.as_slice()],
                )
                .unwrap();
        }
        let session_before = session_signature(&connection);
        LegacyFixture {
            connection,
            id,
            header,
            original,
            event_count,
            session_before,
        }
    }

    fn session_signature(connection: &Connection) -> SessionSignature {
        connection
            .query_row(
                "SELECT capture_id, header_json, started_at_ms, next_sequence, stored_bytes
             FROM live_capture_sessions",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap()
    }

    fn assert_compacted_readback(fixture: &LegacyFixture) {
        let pages = [500, 500, 25];
        let mut after_sequence = None;
        let mut expected_sequence = 0_usize;
        for page_size in pages {
            let page = read(
                &fixture.connection,
                fixture.id,
                after_sequence,
                QueryLimit::new(page_size).unwrap(),
            )
            .unwrap();
            assert_eq!(page.id, fixture.id);
            assert_eq!(page.header_json, fixture.header);
            assert_eq!(page.events.len(), usize::try_from(page_size).unwrap());
            for event in &page.events {
                assert_eq!(event.sequence, u64::try_from(expected_sequence).unwrap());
                assert_eq!(
                    event.receipt_monotonic_ms,
                    u64::try_from(expected_sequence * 3).unwrap()
                );
                assert_eq!(event.payload, fixture.original);
                expected_sequence += 1;
            }
            after_sequence = page
                .events
                .last()
                .map(|event| i64::try_from(event.sequence).unwrap());
        }
        assert_eq!(expected_sequence, fixture.event_count);
    }

    #[test]
    fn database_compresses_new_rows_reads_mixed_rows_and_accounts_original_bytes() {
        use super::super::{
            LOCATION_SCHEMA, LiveCaptureEventAppend, LiveCaptureEventKind, LiveCaptureId, SCHEMA,
            append, begin, read,
        };
        use crate::storage::QueryLimit;
        use rusqlite::{Connection, params};

        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection.execute_batch(LOCATION_SCHEMA).unwrap();
        let id = LiveCaptureId::new();
        let header = b"{}";
        begin(&connection, id, header, 0).unwrap();
        let original = b"{\"unknown\":1.00, \"fields\":[0,0,0,0]}\n".repeat(50);
        assert_eq!(
            append(
                &mut connection,
                &LiveCaptureEventAppend {
                    id,
                    kind: LiveCaptureEventKind::Notification,
                    receipt_monotonic_ms: 0,
                    source_monotonic_offset_ms: None,
                    source_wall_clock_unix_ms: None,
                    payload: &original,
                    location: None,
                    ble_record: None,
                }
            )
            .unwrap(),
            0
        );
        let encoding: i64 = connection
            .query_row(
                "SELECT payload_encoding FROM live_capture_events",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(encoding, 1);
        let legacy = b"{ \"legacy\":1.00 }\n";
        connection.execute("INSERT INTO live_capture_events(capture_id, sequence, event_kind, receipt_monotonic_ms, payload) VALUES (?1, 1, 'metadata', 1, ?2)", params![id.as_string(), legacy.as_slice()]).unwrap();
        connection
            .execute(
                "UPDATE live_capture_sessions SET next_sequence=2, stored_bytes=stored_bytes+?1",
                [legacy.len()],
            )
            .unwrap();
        assert_eq!(
            append(
                &mut connection,
                &LiveCaptureEventAppend {
                    id,
                    kind: LiveCaptureEventKind::Metadata,
                    receipt_monotonic_ms: 2,
                    source_monotonic_offset_ms: None,
                    source_wall_clock_unix_ms: None,
                    payload: b"x",
                    location: None,
                    ble_record: None,
                }
            )
            .unwrap(),
            2
        );
        let snapshot = read(&connection, id, None, QueryLimit::new(3).unwrap()).unwrap();
        assert_eq!(
            snapshot
                .events
                .iter()
                .map(|event| event.payload.as_slice())
                .collect::<Vec<_>>(),
            vec![original.as_slice(), legacy.as_slice(), b"x".as_slice()]
        );
        let (stored, physical): (usize, usize) = connection.query_row("SELECT stored_bytes,(SELECT SUM(length(payload)) FROM live_capture_events) FROM live_capture_sessions", [], |row| Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(stored, header.len() + original.len() + legacy.len() + 1);
        assert!(physical < original.len() + legacy.len() + 1);
        let page = read(&connection, id, Some(0), QueryLimit::new(1).unwrap()).unwrap();
        assert_eq!(page.events[0].sequence, 1);
        assert_eq!(page.events[0].payload, legacy);
    }

    #[test]
    fn repetitive_payload_is_compressed_and_round_trips_exact_bytes() {
        let original = b"{\"unknown\":1.00, \"fields\":[0,0,0,0],\"duplicate\":true}\n".repeat(50);
        let encoded = encode(&original).unwrap();
        assert_eq!(encoded.encoding, 1);
        assert_eq!(
            encoded.original_bytes,
            Some(i64::try_from(original.len()).unwrap())
        );
        assert!(encoded.bytes.len() < original.len());
        assert_eq!(
            decode(
                encoded.bytes.into_owned(),
                encoded.encoding,
                encoded.original_bytes
            )
            .unwrap(),
            original
        );
    }

    #[test]
    fn small_and_incompressible_payloads_stay_raw() {
        let mut state = 0x1234_5678_u32;
        let random: Vec<_> = (0..LIVE_CAPTURE_EVENT_LIMIT_BYTES)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state.to_le_bytes()[0]
            })
            .collect();
        for original in [b"x".as_slice(), random.as_slice()] {
            let encoded = encode(original).unwrap();
            assert_eq!(encoded.encoding, 0);
            assert_eq!(encoded.original_bytes, None);
            assert_eq!(
                decode(encoded.bytes.into_owned(), 0, None).unwrap(),
                original
            );
        }
    }

    #[test]
    fn maximum_payload_round_trips_and_oversize_is_rejected() {
        let original = vec![0xaa; LIVE_CAPTURE_EVENT_LIMIT_BYTES];
        let encoded = encode(&original).unwrap();
        assert_eq!(
            decode(
                encoded.bytes.into_owned(),
                encoded.encoding,
                encoded.original_bytes
            )
            .unwrap(),
            original
        );
        assert!(encode(&vec![0; LIVE_CAPTURE_EVENT_LIMIT_BYTES + 1]).is_err());
        assert!(encode(&[]).is_err());
    }

    #[test]
    fn decoder_rejects_corruption_truncation_checksum_and_trailing_input() {
        let original = vec![0xaa; 4096];
        let encoded = encode(&original).unwrap();
        let bytes = encoded.bytes.into_owned();
        let mut corrupted_header = bytes.clone();
        corrupted_header[0] ^= 0xff;
        let mut corrupted_checksum = bytes.clone();
        *corrupted_checksum.last_mut().unwrap() ^= 1;
        let mut trailing = bytes.clone();
        trailing.push(0);
        let mut concatenated = bytes.clone();
        concatenated.extend_from_slice(&bytes);
        for damaged in [
            corrupted_header,
            corrupted_checksum,
            bytes[..bytes.len() - 1].to_vec(),
            trailing,
            concatenated,
        ] {
            assert!(decode(damaged, 1, Some(4096)).is_err());
        }
    }

    #[test]
    fn decoder_rejects_unknown_codec_and_invalid_or_dishonest_lengths() {
        let original = vec![0xaa; 4096];
        let encoded = encode(&original).unwrap();
        let bytes = encoded.bytes.into_owned();
        for (encoding, length) in [
            (2, None),
            (0, Some(4096)),
            (1, None),
            (1, Some(-1)),
            (1, Some(0)),
            (1, Some(65537)),
            (1, Some(4095)),
            (1, Some(4097)),
        ] {
            assert!(decode(bytes.clone(), encoding, length).is_err());
        }
        assert!(decode(vec![], 0, None).is_err());
        assert!(decode(vec![0; 65537], 0, None).is_err());
        let bomb = miniz_oxide::deflate::compress_to_vec_zlib(&vec![0; 65537], 1);
        assert!(decode(bomb, 1, Some(65536)).is_err());
    }

    #[test]
    fn legacy_compaction_is_lossless_bounded_and_idempotent() {
        const EVENT_COUNT: usize = 1025;
        let mut fixture = legacy_fixture(EVENT_COUNT);
        assert_eq!(
            compress_legacy_payloads(&mut fixture.connection).unwrap(),
            u64::try_from(EVENT_COUNT).unwrap()
        );
        assert_eq!(
            compress_legacy_payloads(&mut fixture.connection).unwrap(),
            0
        );
        assert_compacted_readback(&fixture);

        let event_rows: Vec<(i64, i64, Vec<u8>, i64, i64)> = {
            let mut statement = fixture
                .connection
                .prepare(
                    "SELECT sequence, receipt_monotonic_ms, payload, payload_encoding,
                        payload_original_bytes
                 FROM live_capture_events ORDER BY sequence",
                )
                .unwrap();
            statement
                .query_map([], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(event_rows.len(), EVENT_COUNT);
        for (sequence, receipt, compressed, encoding, original_bytes) in event_rows {
            assert_eq!(receipt, sequence * 3);
            assert_eq!(encoding, 1);
            assert_eq!(
                original_bytes,
                i64::try_from(fixture.original.len()).unwrap()
            );
            assert_eq!(
                decode(compressed, encoding, Some(original_bytes)).unwrap(),
                fixture.original
            );
        }
        assert_eq!(
            session_signature(&fixture.connection),
            fixture.session_before
        );
    }

    #[test]
    fn legacy_compaction_leaves_incompressible_and_compressed_payloads_untouched() {
        use super::super::{LOCATION_SCHEMA, LiveCaptureId, SCHEMA, compress_legacy_payloads};
        use rusqlite::{Connection, params};

        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection.execute_batch(LOCATION_SCHEMA).unwrap();
        let id = LiveCaptureId::new();
        let small = b"x";
        let mut state = 0x1234_5678_u32;
        let incompressible: Vec<_> = (0..4096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state.to_le_bytes()[0]
            })
            .collect();
        let already_compressed = encode(&[0x42; 4096]).unwrap();
        let compressed_bytes = already_compressed.bytes.into_owned();
        connection
            .execute(
                "INSERT INTO live_capture_sessions
             (capture_id, state, integrity, header_json, started_at_ms, stored_bytes,
              next_sequence)
             VALUES (?1, 'interrupted', 'unknown', X'7b7d', 0, ?2, 3)",
                params![
                    id.as_string(),
                    2 + small.len() + incompressible.len() + 4096
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO live_capture_events
             (capture_id, sequence, event_kind, receipt_monotonic_ms, payload)
             VALUES (?1, 0, 'metadata', 0, ?2), (?1, 1, 'metadata', 1, ?3)",
                params![id.as_string(), small.as_slice(), incompressible.as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO live_capture_events
             (capture_id, sequence, event_kind, receipt_monotonic_ms, payload,
              payload_encoding, payload_original_bytes)
             VALUES (?1, 2, 'metadata', 2, ?2, 1, 4096)",
                params![id.as_string(), compressed_bytes.as_slice()],
            )
            .unwrap();

        assert_eq!(compress_legacy_payloads(&mut connection).unwrap(), 0);
        let encodings: Vec<(Vec<u8>, i64, Option<i64>)> = {
            let mut statement = connection
                .prepare(
                    "SELECT payload, payload_encoding, payload_original_bytes
                 FROM live_capture_events ORDER BY sequence",
                )
                .unwrap();
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(encodings[0], (small.to_vec(), 0, None));
        assert_eq!(encodings[1], (incompressible, 0, None));
        assert_eq!(encodings[2], (compressed_bytes, 1, Some(4096)));
    }

    #[test]
    fn legacy_compaction_rolls_back_when_a_raw_row_has_invalid_encoding_metadata() {
        use super::super::{LOCATION_SCHEMA, LiveCaptureId, SCHEMA, compress_legacy_payloads};
        use rusqlite::{Connection, params};

        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection.execute_batch(LOCATION_SCHEMA).unwrap();
        let id = LiveCaptureId::new();
        let original = [0x5a; 1024];
        connection
            .execute(
                "INSERT INTO live_capture_sessions
             (capture_id, state, integrity, header_json, started_at_ms, stored_bytes,
              next_sequence)
             VALUES (?1, 'interrupted', 'unknown', X'7b7d', 0, ?2, 2)",
                params![id.as_string(), 2 + original.len() * 2],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO live_capture_events
                 (capture_id, sequence, event_kind, receipt_monotonic_ms, payload)
                 VALUES (?1, 0, 'metadata', 0, ?2)",
                params![id.as_string(), original.as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO live_capture_events
                 (capture_id, sequence, event_kind, receipt_monotonic_ms, payload,
                  payload_original_bytes)
                 VALUES (?1, 1, 'metadata', 1, ?2, ?3)",
                params![id.as_string(), original.as_slice(), original.len()],
            )
            .unwrap();

        assert!(compress_legacy_payloads(&mut connection).is_err());
        let rows: Vec<(Vec<u8>, i64, Option<i64>)> = {
            let mut statement = connection
                .prepare(
                    "SELECT payload, payload_encoding, payload_original_bytes
                 FROM live_capture_events ORDER BY sequence",
                )
                .unwrap();
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(
            rows,
            vec![
                (original.to_vec(), 0, None),
                (
                    original.to_vec(),
                    0,
                    Some(i64::try_from(original.len()).unwrap())
                ),
            ]
        );
    }

    #[test]
    fn legacy_compaction_rolls_back_the_entire_batch_on_update_error() {
        use super::super::{LOCATION_SCHEMA, LiveCaptureId, SCHEMA, compress_legacy_payloads};
        use rusqlite::{Connection, params};

        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection.execute_batch(LOCATION_SCHEMA).unwrap();
        let id = LiveCaptureId::new();
        let original = [0x5a; 1024];
        connection
            .execute(
                "INSERT INTO live_capture_sessions
             (capture_id, state, integrity, header_json, started_at_ms, stored_bytes,
              next_sequence)
             VALUES (?1, 'interrupted', 'unknown', X'7b7d', 0, ?2, 2)",
                params![id.as_string(), 2 + original.len() * 2],
            )
            .unwrap();
        for sequence in 0..2 {
            connection
                .execute(
                    "INSERT INTO live_capture_events
                 (capture_id, sequence, event_kind, receipt_monotonic_ms, payload)
                 VALUES (?1, ?2, 'metadata', ?2, ?3)",
                    params![id.as_string(), sequence, original.as_slice()],
                )
                .unwrap();
        }
        connection
            .execute_batch(
                "CREATE TRIGGER reject_second_compression
             BEFORE UPDATE OF payload_encoding ON live_capture_events
             WHEN OLD.sequence = 1
             BEGIN SELECT RAISE(ABORT, 'injected update failure'); END;",
            )
            .unwrap();

        assert!(compress_legacy_payloads(&mut connection).is_err());
        let rows: Vec<(Vec<u8>, i64, Option<i64>)> = {
            let mut statement = connection
                .prepare(
                    "SELECT payload, payload_encoding, payload_original_bytes
                 FROM live_capture_events ORDER BY sequence",
                )
                .unwrap();
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(rows, vec![(original.to_vec(), 0, None); 2]);
    }
}
