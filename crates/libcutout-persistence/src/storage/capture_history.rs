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

/// One published row whose stored metadata could not be decoded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnreadablePevcapCapture {
    /// SHA-256 identity from the durable row.
    pub artifact_digest: String,
    /// Original byte length from the durable row.
    pub artifact_size: u64,
}

/// A bounded capture-history page and continuation only when another row exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapCapturePage {
    /// Published captures, newest import first with descending digest for ties.
    pub captures: Vec<StoredPevcapCapture>,
    /// Rows on this page whose stored metadata could not be decoded.
    pub(crate) unreadable: Vec<UnreadablePevcapCapture>,
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
        FROM capture_submissions AS candidate
        WHERE candidate.source_artifact_id = (
                SELECT recording.artifact_id FROM pevcap_recordings AS recording
                WHERE recording.artifact_digest = i.artifact_digest LIMIT 1)
           OR candidate.source_artifact_id IN (
                SELECT alias.source_artifact_id FROM capture_submission_source_aliases AS alias
                WHERE alias.artifact_digest = i.artifact_digest)
           OR candidate.snapshot_digest = i.artifact_digest
           OR json_extract(candidate.status_json, '$.metadata.provenance.source_sha256') = i.artifact_digest)
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
    struct ListedCapture {
        imported_at_milliseconds: u64,
        artifact_digest: String,
        artifact_size: u64,
        decoded: Option<StoredPevcapCapture>,
    }

    let mut listed = Vec::with_capacity(count + 1);
    while let Some(row) = rows.next()? {
        let artifact_digest: String = row.get(0)?;
        let artifact_size: u64 = row.get(2)?;
        let imported_at_milliseconds: u64 = row.get(3)?;
        let decoded = match read_capture(connection, row) {
            Ok(capture) => Some(capture),
            Err(StorageError::InvalidStoredValue { .. }) => None,
            Err(error) => return Err(error),
        };
        listed.push(ListedCapture {
            imported_at_milliseconds,
            artifact_digest,
            artifact_size,
            decoded,
        });
    }
    let has_more = listed.len() > count;
    listed.truncate(count);
    let next_cursor = if has_more {
        let Some(last) = listed.last() else {
            return Err(StorageError::InvalidStoredValue {
                field: "capture history page",
                value: "empty".to_owned(),
            });
        };
        Some(PevcapCaptureCursor::new(
            last.imported_at_milliseconds,
            &last.artifact_digest,
        )?)
    } else {
        None
    };
    let mut captures = Vec::new();
    let mut unreadable = Vec::new();
    for row in listed {
        match row.decoded {
            Some(capture) => captures.push(capture),
            None => unreadable.push(UnreadablePevcapCapture {
                artifact_digest: row.artifact_digest,
                artifact_size: row.artifact_size,
            }),
        }
    }
    Ok(PevcapCapturePage {
        captures,
        unreadable,
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
    let encoding_text = row.get::<_, Option<String>>(1)?;
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
    let joined_status = status_json
        .as_deref()
        .map(super::capture_submission::decode_status)
        .transpose()?;
    let recording = super::recorded_capture::lookup(connection, &artifact_digest)?;
    let (status, source_artifact_id) = if let Some(status) = joined_status {
        let source_artifact_id = status.metadata.provenance.source_artifact_id.clone();
        (Some(status), source_artifact_id)
    } else {
        resolve_unjoined_owner(connection, &artifact_digest, recording.as_ref())?
    };
    let encoding = resolve_encoding(encoding_text.as_deref(), status.as_ref())?;
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

/// One digest can belong to both the live capture id and the recording id.
/// A receipt on either identity covers the bytes. Otherwise keep the identity
/// live history already uses, so review does not list the capture twice.
fn resolve_unjoined_owner(
    connection: &Connection,
    digest: &str,
    recording: Option<&super::RecordedCapture>,
) -> Result<
    (
        Option<super::capture_submission::CaptureSubmissionStatus>,
        String,
    ),
    StorageError,
> {
    let mut owners = submission_owners(connection, digest)?;
    if owners.is_empty() {
        let source_artifact_id = recording
            .map(|recording| recording.artifact_id.to_string())
            .map_or_else(
                || {
                    super::capture_submission::legacy_capture_artifact_id(digest)
                        .map(|id| id.to_string())
                        .map_err(|_| StorageError::InvalidStoredValue {
                            field: "capture source identity",
                            value: digest.to_owned(),
                        })
                },
                Ok,
            )?;
        return Ok((None, source_artifact_id));
    }
    owners.sort_by(|left, right| {
        left.metadata
            .provenance
            .source_artifact_id
            .cmp(&right.metadata.provenance.source_artifact_id)
    });
    if let Some(index) = owners
        .iter()
        .position(super::capture_submission::status_has_valid_receipt)
    {
        let status = owners.swap_remove(index);
        let source_artifact_id = status.metadata.provenance.source_artifact_id.clone();
        return Ok((Some(status), source_artifact_id));
    }
    let live_source_id = matching_live_source_id(connection, &owners)?;
    if let Some(live_source_id) = live_source_id
        && let Some(index) = owners
            .iter()
            .position(|status| status.metadata.provenance.source_artifact_id == live_source_id)
    {
        let status = owners.swap_remove(index);
        let source_artifact_id = status.metadata.provenance.source_artifact_id.clone();
        return Ok((Some(status), source_artifact_id));
    }
    if let Some(recording) = recording {
        let recording_id = recording.artifact_id.to_string();
        if let Some(index) = owners
            .iter()
            .position(|status| status.metadata.provenance.source_artifact_id == recording_id)
        {
            let status = owners.swap_remove(index);
            let source_artifact_id = status.metadata.provenance.source_artifact_id.clone();
            return Ok((Some(status), source_artifact_id));
        }
    }
    let status = owners.remove(0);
    let source_artifact_id = status.metadata.provenance.source_artifact_id.clone();
    Ok((Some(status), source_artifact_id))
}

fn submission_owners(
    connection: &Connection,
    digest: &str,
) -> Result<Vec<super::capture_submission::CaptureSubmissionStatus>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT status_json FROM capture_submissions
         WHERE source_artifact_id IN (
            SELECT source_artifact_id FROM capture_submission_source_aliases
            WHERE artifact_digest = ?1
            UNION
            SELECT source_artifact_id FROM capture_submissions
            WHERE coalesce(
                json_extract(status_json, '$.metadata.evidence.capture_sha256'),
                json_extract(status_json, '$.metadata.sha256')
            ) = ?1
               OR json_extract(status_json, '$.metadata.provenance.source_sha256') = ?1
         )",
    )?;
    let rows = statement.query_map([digest], |row| row.get::<_, String>(0))?;
    let mut owners = Vec::new();
    for row in rows {
        let Ok(status) = super::capture_submission::decode_status(&row?) else {
            continue;
        };
        owners.push(status);
    }
    Ok(owners)
}

fn matching_live_source_id(
    connection: &Connection,
    owners: &[super::capture_submission::CaptureSubmissionStatus],
) -> Result<Option<String>, StorageError> {
    let mut matches = Vec::new();
    for owner in owners {
        let id = &owner.metadata.provenance.source_artifact_id;
        let mut statement = connection.prepare(
            "SELECT capture_id,
                    CASE WHEN json_valid(recording_context_json)
                         THEN json_extract(recording_context_json, '$.artifact_id')
                    END
             FROM live_capture_sessions
             WHERE capture_id = ?1
                OR (json_valid(recording_context_json)
                    AND json_extract(recording_context_json, '$.artifact_id') = ?1)",
        )?;
        let rows = statement.query_map([id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (capture_id, artifact_id) = row?;
            let source_id = artifact_id
                .filter(|value| !value.is_empty())
                .unwrap_or(capture_id);
            if owners
                .iter()
                .any(|owner| owner.metadata.provenance.source_artifact_id == source_id)
            {
                matches.push(source_id);
            }
        }
    }
    matches.sort();
    matches.dedup();
    Ok(matches.into_iter().next())
}

fn resolve_encoding(
    encoding: Option<&str>,
    status: Option<&super::capture_submission::CaptureSubmissionStatus>,
) -> Result<PevcapEncoding, StorageError> {
    let from_status = status.map(|status| match status.metadata.encoding {
        PevcapEncoding::Jsonl => "jsonl",
        PevcapEncoding::Binary => "binary",
    });
    match encoding.or(from_status) {
        Some("jsonl") => Ok(PevcapEncoding::Jsonl),
        Some("binary") => Ok(PevcapEncoding::Binary),
        Some(encoding) => Err(StorageError::InvalidStoredValue {
            field: "capture encoding",
            value: encoding.to_owned(),
        }),
        None => Err(StorageError::InvalidStoredValue {
            field: "capture encoding",
            value: "missing".to_owned(),
        }),
    }
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
        // create_current_schema does not create live sessions. Owner resolution reads that table.
        connection
            .execute_batch(super::super::live_capture::SCHEMA)
            .unwrap();
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

    fn insert_owner(
        connection: &Connection,
        digest: &str,
        source_artifact_id: &str,
        operation_id: &str,
        sent: bool,
    ) {
        let receipt = if sent {
            serde_json::json!({
                "operation_id": operation_id,
                "sha256": digest,
                "bytes": 1,
                "received_at_milliseconds": 1_700_000_000_000_u64
            })
        } else {
            serde_json::Value::Null
        };
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
            "phase": if sent { "sent" } else { "retryable" },
            "local_payload_state": if sent { "removed" } else { "retained" },
            "account_id": null,
            "receipt": receipt,
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
        let writer_artifact_id = "00000000-0000-4000-8000-000000000077";
        connection
            .execute(
                "INSERT INTO pevcap_recordings
                    (artifact_id, artifact_digest, origin, platform_identifier, started_at_ms)
                 VALUES (?1, ?2, 'manual', 'test-platform', 1)",
                params![writer_artifact_id, digest],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO capture_submission_source_aliases
                    (artifact_digest, source_artifact_id) VALUES (?1, ?2)",
                params![digest, source_artifact_id],
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
    fn capture_history_keeps_one_owner_when_a_digest_has_two_submissions() {
        let connection = connection();
        let digest = receipt(&connection, 'b', 10);
        stored(&connection, &digest, true);
        for (source_artifact_id, operation_id) in [
            (
                "00000000-0000-4000-8000-000000000081",
                "00000000-0000-4000-8000-000000000091",
            ),
            (
                "00000000-0000-4000-8000-000000000082",
                "00000000-0000-4000-8000-000000000092",
            ),
        ] {
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
                "receipt": null,
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
        }

        let page = list(&connection, None, QueryLimit::new(10).unwrap()).unwrap();
        assert!(page.unreadable.is_empty());
        assert_eq!(page.captures.len(), 1);
        assert_eq!(
            page.captures[0].submission.source_artifact_id,
            "00000000-0000-4000-8000-000000000081"
        );
    }

    #[test]
    fn capture_history_keeps_the_live_capture_id_when_neither_submission_was_sent() {
        let connection = connection();
        let digest = receipt(&connection, 'c', 11);
        stored(&connection, &digest, true);
        for (source_artifact_id, operation_id) in [
            (
                "00000000-0000-4000-8000-000000000083",
                "00000000-0000-4000-8000-000000000093",
            ),
            (
                "00000000-0000-4000-8000-000000000084",
                "00000000-0000-4000-8000-000000000094",
            ),
        ] {
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
                "phase": "retryable",
                "local_payload_state": "retained",
                "account_id": null,
                "receipt": null,
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
        }
        connection
            .execute(
                "INSERT INTO live_capture_sessions
                    (capture_id, state, integrity, header_json, started_at_ms, finished_at_ms,
                     next_sequence, stored_bytes)
                 VALUES (?1, 'finished', 'complete', ?2, 1, 2, 1, 1)",
                params!["00000000-0000-4000-8000-000000000084", vec![b'{', b'}']],
            )
            .unwrap();

        let page = list(&connection, None, QueryLimit::new(10).unwrap()).unwrap();
        assert!(page.unreadable.is_empty());
        assert_eq!(page.captures.len(), 1);
        assert_eq!(page.captures[0].encoding, PevcapEncoding::Jsonl);
        assert_eq!(
            page.captures[0].submission.source_artifact_id,
            "00000000-0000-4000-8000-000000000084"
        );
    }

    #[test]
    fn capture_history_keeps_the_receipted_owner_when_a_digest_has_two_submissions() {
        let connection = connection();
        let digest = receipt(&connection, 'd', 12);
        stored(&connection, &digest, true);
        insert_owner(
            &connection,
            &digest,
            "00000000-0000-4000-8000-000000000085",
            "00000000-0000-4000-8000-000000000095",
            false,
        );
        insert_owner(
            &connection,
            &digest,
            "00000000-0000-4000-8000-000000000086",
            "00000000-0000-4000-8000-000000000096",
            true,
        );

        let page = list(&connection, None, QueryLimit::new(10).unwrap()).unwrap();
        assert!(page.unreadable.is_empty());
        assert_eq!(page.captures.len(), 1);
        assert_eq!(
            page.captures[0].submission.source_artifact_id,
            "00000000-0000-4000-8000-000000000086"
        );
        assert!(
            page.captures[0]
                .submission
                .status
                .as_ref()
                .unwrap()
                .receipt
                .is_some()
        );
    }

    #[test]
    fn capture_history_skips_an_import_with_two_owners_and_no_payload() {
        let connection = connection();
        let digest = receipt(&connection, 'e', 13);
        insert_owner(
            &connection,
            &digest,
            "00000000-0000-4000-8000-000000000087",
            "00000000-0000-4000-8000-000000000097",
            false,
        );
        insert_owner(
            &connection,
            &digest,
            "00000000-0000-4000-8000-000000000088",
            "00000000-0000-4000-8000-000000000098",
            false,
        );

        let page = list(&connection, None, QueryLimit::new(10).unwrap()).unwrap();
        assert!(page.captures.is_empty());
        assert!(page.unreadable.is_empty());
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
