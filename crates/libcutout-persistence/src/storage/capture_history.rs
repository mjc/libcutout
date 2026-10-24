//! Bounded metadata projection of published original-byte captures.

use super::{QueryLimit, RideId, StorageError};
use cutout_core::PevcapEncoding;
use rusqlite::{Connection, Row, params};
use uuid::Uuid;

/// Checked keyset cursor; import time is not the recording's start time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PevcapCaptureCursor {
    imported_at_milliseconds: u64,
    digest: [u8; 32],
}

impl PevcapCaptureCursor {
    /// Restores a cursor from a prior page's import timestamp and lowercase SHA-256 digest.
    ///
    /// # Errors
    /// Returns [`StorageError::InvalidPevcapCaptureCursor`] for malformed digests or
    /// timestamps outside SQLite's nonnegative signed-integer range.
    pub fn new(imported_at_milliseconds: u64, digest: &str) -> Result<Self, StorageError> {
        if i64::try_from(imported_at_milliseconds).is_err()
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StorageError::InvalidPevcapCaptureCursor);
        }
        let mut bytes = [0; 32];
        hex::decode_to_slice(digest, &mut bytes)
            .map_err(|_| StorageError::InvalidPevcapCaptureCursor)?;
        Ok(Self {
            imported_at_milliseconds,
            digest: bytes,
        })
    }

    /// Returns the durable import time, in Unix milliseconds.
    #[must_use]
    pub const fn imported_at_milliseconds(self) -> u64 {
        self.imported_at_milliseconds
    }

    /// Returns the lowercase SHA-256 tie-breaker.
    #[must_use]
    pub fn artifact_digest(self) -> String {
        hex::encode(self.digest)
    }
}

/// Metadata for an original capture whose complete bytes have been published in SQLite.
/// No file path is exposed: historical managed copies may no longer exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredPevcapCapture {
    /// Finished-writer provenance, absent for externally imported captures.
    pub recording: Option<super::RecordedCapture>,
    /// SHA-256 identity of the original byte stream.
    pub artifact_digest: String,
    /// Encoding of the retained original bytes.
    pub encoding: PevcapEncoding,
    /// Original byte length, not a decoded-memory estimate.
    pub artifact_size: u64,
    /// Durable import time; not the capture's recording time.
    pub imported_at_milliseconds: u64,
    /// Number of transport records in the source.
    pub record_count: u64,
    /// Number of locations admitted to the derived ride, not all raw observations.
    pub location_count: u64,
    /// Optional derived ride; GPS-free captures remain independently queryable.
    pub ride_id: Option<RideId>,
    /// Stable Send source identity and its durable private receipt-backed status, if any.
    pub submission: CaptureSubmissionHistory,
}

/// The canonical source alias and optional private Send status for one history row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureSubmissionHistory {
    /// Stable supported locator for evidence, preparation and receipt lookup.
    pub source: super::capture_submission::CaptureSubmissionSource,
    /// Stable artifact identity used to prepare or look up private Send.
    pub source_artifact_id: String,
    /// Durable private Send operation; distinct from a PEVCAP import receipt.
    pub status: Option<super::capture_submission::CaptureSubmissionStatus>,
}

/// A bounded capture-history page and continuation only when another row exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapCapturePage {
    /// Published captures, newest import first with descending digest for ties.
    pub captures: Vec<StoredPevcapCapture>,
    /// Continue after the last returned entry; refresh without a cursor for new imports.
    pub next_cursor: Option<PevcapCaptureCursor>,
}

pub(super) const HISTORY_INDEX: &str = "CREATE INDEX IF NOT EXISTS pevcap_imports_history
     ON pevcap_imports(imported_at_ms DESC, artifact_digest DESC);";

const SELECT_CAPTURES: &str = "SELECT i.artifact_digest,
            coalesce(c.encoding, json_extract(s.status_json, '$.metadata.encoding')),
            coalesce(c.artifact_size, i.artifact_size), i.imported_at_ms,
            i.record_count, i.location_count, i.ride_id, s.status_json
     FROM pevcap_imports AS i
     LEFT JOIN pevcap_captures AS c ON c.receipt_digest = i.artifact_digest
     LEFT JOIN capture_submissions AS s ON s.source_artifact_id = (
        SELECT CASE WHEN min(candidate.source_artifact_id) = max(candidate.source_artifact_id)
                    THEN min(candidate.source_artifact_id) END
        FROM (
            SELECT recording.artifact_id AS source_artifact_id
            FROM pevcap_recordings AS recording
            WHERE recording.artifact_digest = i.artifact_digest
            UNION ALL
            SELECT alias.source_artifact_id
            FROM capture_submission_source_aliases AS alias
            WHERE alias.artifact_digest = i.artifact_digest
            UNION ALL
            SELECT candidate.source_artifact_id
            FROM capture_submissions AS candidate
            WHERE candidate.snapshot_digest = i.artifact_digest
               OR json_extract(candidate.status_json, '$.metadata.provenance.source_sha256') = i.artifact_digest
        ) AS candidate)
     ";

pub(super) fn list(
    connection: &Connection,
    cursor: Option<PevcapCaptureCursor>,
    limit: QueryLimit,
) -> Result<PevcapCapturePage, StorageError> {
    let count =
        usize::try_from(limit.get()).map_err(|_| StorageError::InvalidQueryLimit(limit.get()))?;
    let fetch_limit = i64::from(limit.get()) + 1;
    let predicate = if cursor.is_some() {
        "WHERE (i.imported_at_ms, i.artifact_digest) < (?2, ?3)
           AND (c.receipt_digest = i.artifact_digest OR s.operation_id IS NOT NULL)"
    } else {
        "WHERE c.receipt_digest = i.artifact_digest OR s.operation_id IS NOT NULL"
    };
    let sql = format!(
        "{SELECT_CAPTURES} {predicate}
        ORDER BY i.imported_at_ms DESC, i.artifact_digest DESC LIMIT ?1"
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = if let Some(cursor) = cursor {
        statement.query(params![
            fetch_limit,
            cursor.imported_at_milliseconds,
            cursor.artifact_digest()
        ])?
    } else {
        statement.query([fetch_limit])?
    };
    let mut captures = Vec::with_capacity(count + 1);
    while let Some(row) = rows.next()? {
        captures.push(read_capture(connection, row)?);
    }
    let has_more = captures.len() > count;
    captures.truncate(count);
    let next_cursor = if has_more {
        captures
            .last()
            .map(|capture| {
                PevcapCaptureCursor::new(capture.imported_at_milliseconds, &capture.artifact_digest)
            })
            .transpose()?
    } else {
        None
    };
    Ok(PevcapCapturePage {
        captures,
        next_cursor,
    })
}

fn read_capture(
    connection: &Connection,
    row: &Row<'_>,
) -> Result<StoredPevcapCapture, StorageError> {
    let artifact_digest: String = row.get(0)?;
    let imported_at_milliseconds = row.get(3)?;
    PevcapCaptureCursor::new(imported_at_milliseconds, &artifact_digest).map_err(|_| {
        StorageError::InvalidStoredValue {
            field: "capture digest",
            value: artifact_digest.clone(),
        }
    })?;
    let encoding: String = row.get(1)?;
    let encoding = match encoding.as_str() {
        "jsonl" => PevcapEncoding::Jsonl,
        "binary" => PevcapEncoding::Binary,
        _ => {
            return Err(StorageError::InvalidStoredValue {
                field: "capture encoding",
                value: encoding,
            });
        }
    };
    let ride_id = row
        .get::<_, Option<String>>(6)?
        .map(|value| {
            Uuid::parse_str(&value).map(RideId::from_uuid).map_err(|_| {
                StorageError::InvalidStoredValue {
                    field: "PEVCAP ride identifier",
                    value,
                }
            })
        })
        .transpose()?;
    let status_json = row.get::<_, Option<String>>(7)?;
    let status = status_json
        .as_deref()
        .map(super::capture_submission::decode_status)
        .transpose()?;
    let recording = super::recorded_capture::lookup(connection, &artifact_digest)?;
    let alias =
        super::capture_submission::lookup_submission_source_alias(connection, &artifact_digest)?;
    let source_artifact_id = status
        .as_ref()
        .map(|status| status.metadata.provenance.source_artifact_id.clone())
        .map_or_else(
            || {
                alias.map_or_else(
                    || {
                        recording.as_ref().map_or_else(
                            || {
                                super::capture_submission::legacy_capture_artifact_id(
                                    &artifact_digest,
                                )
                                .map(|id| id.to_string())
                                .map_err(|_| {
                                    StorageError::InvalidStoredValue {
                                        field: "capture source identity",
                                        value: artifact_digest.clone(),
                                    }
                                })
                            },
                            |recording| Ok(recording.artifact_id.to_string()),
                        )
                    },
                    |alias| Ok(alias.to_string()),
                )
            },
            Ok,
        )?;
    let source = super::capture_submission::PevcapDigest::parse(&artifact_digest)
        .map(super::capture_submission::CaptureSubmissionSource::PevcapDigest)
        .map_err(|_| StorageError::InvalidStoredValue {
            field: "capture source identity",
            value: artifact_digest.clone(),
        })?;
    Ok(StoredPevcapCapture {
        recording,
        artifact_digest,
        encoding,
        artifact_size: row.get(2)?,
        imported_at_milliseconds,
        record_count: row.get(4)?,
        location_count: row.get(5)?,
        ride_id,
        submission: CaptureSubmissionHistory {
            source,
            source_artifact_id,
            status,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        super::super::migrations::create_current_schema(&connection).unwrap();
        connection
    }

    fn receipt(connection: &Connection, digit: char, at: u64) -> String {
        let digest = digit.to_string().repeat(64);
        connection.execute(
            "INSERT INTO pevcap_imports (artifact_digest, artifact_path, outcome, artifact_size,
             record_count, location_count, imported_at_ms)
             VALUES (?1, '/not-opened', 'capture_only', 1, 0, 0, ?2)",
            params![digest, at],
        ).unwrap();
        digest
    }

    fn stored(connection: &Connection, digest: &str, published: bool) {
        connection.execute(
            "INSERT INTO pevcap_captures (artifact_digest, receipt_digest, encoding, artifact_size,
             written_bytes, next_sequence) VALUES (?1, ?2, 'jsonl', 1, 1, 1)",
            params![digest, published.then_some(digest)],
        ).unwrap();
        connection
            .execute(
                "INSERT INTO pevcap_capture_chunks VALUES (?1, 0, X'0a')",
                [digest],
            )
            .unwrap();
    }

    #[test]
    fn capture_history_keysets_exclude_staging_and_do_not_repeat_new_imports() {
        let connection = connection();
        let limit = QueryLimit::new(1).unwrap();
        assert_eq!(
            list(&connection, None, limit).unwrap().captures,
            Vec::<StoredPevcapCapture>::new()
        );
        for (digit, at) in [('0', 0), ('1', 10), ('2', 10)] {
            let digest = receipt(&connection, digit, at);
            stored(&connection, &digest, true);
        }
        let _legacy = receipt(&connection, '3', 20);
        let staged = receipt(&connection, '4', 30);
        stored(&connection, &staged, false);
        let first = list(&connection, None, limit).unwrap();
        assert_eq!(first.captures.len(), 1);
        assert_eq!(first.captures[0].artifact_digest, "2".repeat(64));
        let newer = receipt(&connection, '5', 40);
        stored(&connection, &newer, true);
        let second = list(&connection, first.next_cursor, limit).unwrap();
        assert_eq!(second.captures[0].artifact_digest, "1".repeat(64));
        let third = list(&connection, second.next_cursor, limit).unwrap();
        assert_eq!(third.captures[0].artifact_digest, "0".repeat(64));
        assert_eq!(third.next_cursor, None);
        let refreshed = list(&connection, None, QueryLimit::new(10).unwrap()).unwrap();
        assert_eq!(refreshed.captures.len(), 4);
        assert_eq!(refreshed.captures[0].artifact_digest, newer);
        assert_eq!(refreshed.next_cursor, None);
    }

    #[test]
    fn capture_history_cursor_checks_timestamp_and_canonical_digest() {
        for digest in ["", "a", &"g".repeat(64), &"A".repeat(64), &"é".repeat(32)] {
            assert!(PevcapCaptureCursor::new(0, digest).is_err());
        }
        assert!(PevcapCaptureCursor::new(u64::MAX, &"a".repeat(64)).is_err());
        let cursor = PevcapCaptureCursor::new(1234, &"ab".repeat(32)).unwrap();
        assert_eq!(cursor.imported_at_milliseconds(), 1234);
        assert_eq!(cursor.artifact_digest(), "ab".repeat(32));
    }

    #[test]
    fn capture_history_retains_private_send_receipt_after_payload_retirement() {
        let connection = connection();
        let digest = receipt(&connection, 'a', 10);
        stored(&connection, &digest, true);
        // The durable operation's provenance is authoritative even when the current
        // catalog row can resolve to a different legacy/recording identity.
        let source_artifact_id = "00000000-0000-4000-8000-000000000088".to_owned();
        let operation_id = "00000000-0000-4000-8000-000000000099";
        let status = serde_json::json!({
            "operation_id": operation_id,
            "metadata": {
                "evidence": null,
                "sha256": digest,
                "bytes": 1,
                "encoding": "jsonl",
                "origin": "unknown",
                "integrity": "unknown",
                "provenance": {
                    "source_artifact_id": source_artifact_id,
                    "live_capture_id": null,
                    "source_sha256": digest,
                    "transformation": "unmodified"
                }
            },
            "categories": {
                "device_identifiers": false,
                "location": false,
                "notes": false,
                "annotations": false,
                "settings_observations": false
            },
            "phase": "sent",
            "local_payload_state": "removed",
            "account_id": null,
            "receipt": {
                "operation_id": operation_id,
                "sha256": digest,
                "bytes": 1,
                "received_at_milliseconds": 1_700_000_000_000_u64
            },
            "failure": null,
            "revision": 0
        });
        connection
            .execute(
                "INSERT INTO capture_submissions
                    (source_artifact_id, operation_id, snapshot_digest, status_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![source_artifact_id, operation_id, digest, status.to_string()],
            )
            .unwrap();
        connection
            .execute(
                "DELETE FROM pevcap_capture_chunks WHERE artifact_digest = ?1",
                [&digest],
            )
            .unwrap();
        connection
            .execute(
                "DELETE FROM pevcap_captures WHERE artifact_digest = ?1",
                [&digest],
            )
            .unwrap();

        let page = list(&connection, None, QueryLimit::new(10).unwrap()).unwrap();

        assert_eq!(page.captures.len(), 1);
        let capture = &page.captures[0];
        assert_eq!(capture.artifact_digest, digest);
        assert_eq!(capture.submission.source_artifact_id, source_artifact_id);
        let submission = capture.submission.status.as_ref().unwrap();
        assert_eq!(
            submission.phase,
            super::super::capture_submission::CaptureSubmissionPhase::Sent
        );
        assert_eq!(
            submission.local_payload_state,
            super::super::capture_submission::CaptureSubmissionLocalPayloadState::Removed
        );
        assert_eq!(
            submission.receipt.as_ref().unwrap().operation_id,
            operation_id
        );
    }

    #[test]
    fn capture_history_uses_the_time_digest_index_without_a_sort_buffer() {
        let connection = connection();
        let sql = format!(
            "EXPLAIN QUERY PLAN {SELECT_CAPTURES}
            WHERE (i.imported_at_ms, i.artifact_digest) < (10, '{}')
            ORDER BY i.imported_at_ms DESC, i.artifact_digest DESC LIMIT 2",
            "a".repeat(64)
        );
        let mut statement = connection.prepare(&sql).unwrap();
        let plan = statement
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        assert!(plan.contains("pevcap_imports_history"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
}
