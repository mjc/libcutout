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
    use super::*;

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
}
