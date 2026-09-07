//! Bounded original-byte storage, hidden until a complete capture has a durable receipt.

use super::{Command, PevcapImportPreview, RideDatabase, StorageError};
use cutout_core::PevcapEncoding;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

const CHUNK_BYTES: usize = 64 * 1024;

/// A nonempty chunk that cannot exceed the database command's byte budget.
pub(super) struct CaptureChunk(Vec<u8>);

impl CaptureChunk {
    /// Reads at most one worker-sized chunk; EOF produces no write command.
    fn read(source: &mut impl Read) -> std::io::Result<Option<Self>> {
        let mut bytes = vec![0; CHUNK_BYTES];
        let count = source.read(&mut bytes)?;
        bytes.truncate(count);
        Ok((count != 0).then_some(Self(bytes)))
    }
}

/// Staged captures remain unlinked until complete; receipt deletion removes their bytes.
pub(super) const SCHEMA: &str = "
    CREATE TABLE pevcap_captures (
        artifact_digest TEXT PRIMARY KEY NOT NULL CHECK (length(artifact_digest) = 64),
        receipt_digest TEXT UNIQUE REFERENCES pevcap_imports(artifact_digest) ON DELETE CASCADE,
        encoding TEXT NOT NULL CHECK (encoding IN ('jsonl', 'binary')),
        artifact_size INTEGER NOT NULL CHECK (artifact_size BETWEEN 1 AND 536870912),
        written_bytes INTEGER NOT NULL DEFAULT 0 CHECK (written_bytes BETWEEN 0 AND artifact_size),
        next_sequence INTEGER NOT NULL DEFAULT 0 CHECK (next_sequence >= 0),
        CHECK (receipt_digest IS NULL OR (receipt_digest = artifact_digest AND written_bytes = artifact_size))
    );
    CREATE TABLE pevcap_capture_chunks (
        artifact_digest TEXT NOT NULL REFERENCES pevcap_captures(artifact_digest) ON DELETE CASCADE,
        sequence INTEGER NOT NULL CHECK (sequence >= 0),
        payload BLOB NOT NULL CHECK (length(payload) BETWEEN 1 AND 65536),
        PRIMARY KEY (artifact_digest, sequence)
    ) WITHOUT ROWID;
";

/// Distinguishes idempotent reuse from ownership of a new staged capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CaptureStart {
    Stored,
    Started,
}

/// Checks publication, not merely the presence of staged bytes.
pub(super) fn is_stored(connection: &Connection, digest: &str) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pevcap_captures WHERE receipt_digest = ?1)",
        [digest],
        |row| row.get(0),
    )?)
}

/// Claims a staged capture for existing import work or receipt backfill.
/// A competing partial write is rejected; published data is reused unchanged.
pub(super) fn begin(
    connection: &Connection,
    digest: &str,
    encoding: PevcapEncoding,
    artifact_size: u64,
) -> Result<CaptureStart, StorageError> {
    if is_stored(connection, digest)? {
        return Ok(CaptureStart::Stored);
    }
    let authorized: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pevcap_import_work WHERE artifact_digest = ?1)
             OR EXISTS(SELECT 1 FROM pevcap_imports WHERE artifact_digest = ?1)",
        [digest],
        |row| row.get(0),
    )?;
    if !authorized {
        return Err(StorageError::PevcapImportInProgress);
    }
    let encoding = match encoding {
        PevcapEncoding::Jsonl => "jsonl",
        PevcapEncoding::Binary => "binary",
    };
    let inserted = connection.execute(
        "INSERT INTO pevcap_captures (artifact_digest, encoding, artifact_size)
         VALUES (?1, ?2, ?3) ON CONFLICT(artifact_digest) DO NOTHING",
        params![digest, encoding, artifact_size],
    )?;
    if inserted != 1 {
        return Err(StorageError::PevcapImportInProgress);
    }
    Ok(CaptureStart::Started)
}

/// Atomically appends the next chunk and advances its byte/sequence counters.
/// Rejects gaps, repeated sequences, and writes exceeding the reviewed byte length.
pub(super) fn append(
    connection: &mut Connection,
    digest: &str,
    sequence: u64,
    chunk: &CaptureChunk,
) -> Result<(), StorageError> {
    let bytes = &chunk.0;
    let transaction = connection.transaction()?;
    let advanced = transaction.execute(
        "UPDATE pevcap_captures SET written_bytes = written_bytes + ?1,
             next_sequence = next_sequence + 1
         WHERE artifact_digest = ?2 AND receipt_digest IS NULL AND next_sequence = ?3",
        params![bytes.len(), digest, sequence],
    )?;
    if advanced != 1 {
        return Err(StorageError::PevcapImportInProgress);
    }
    transaction.execute(
        "INSERT INTO pevcap_capture_chunks (artifact_digest, sequence, payload) VALUES (?1, ?2, ?3)",
        params![digest, sequence, bytes],
    )?;
    transaction.commit()?;
    Ok(())
}

/// Links complete bytes to an existing receipt, making bounded reads visible.
/// New imports call this inside the transaction that publishes their receipt.
pub(super) fn publish(connection: &Connection, digest: &str) -> Result<(), StorageError> {
    let published = connection.execute(
        "UPDATE pevcap_captures SET receipt_digest = artifact_digest
         WHERE artifact_digest = ?1 AND written_bytes = artifact_size",
        [digest],
    )?;
    if published != 1 {
        return Err(StorageError::PevcapPreviewChanged);
    }
    Ok(())
}

/// Removes unpublished bytes without touching any successfully published capture.
pub(super) fn abort(connection: &Connection, digest: &str) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM pevcap_captures WHERE artifact_digest = ?1 AND receipt_digest IS NULL",
        [digest],
    )?;
    Ok(())
}

/// Reads one published chunk; absent, incomplete, and exhausted captures return `None`.
pub(super) fn chunk(
    connection: &Connection,
    digest: &str,
    sequence: u64,
) -> Result<Option<Vec<u8>>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT payload FROM pevcap_capture_chunks
         JOIN pevcap_captures USING (artifact_digest)
         WHERE receipt_digest = ?1 AND sequence = ?2",
            params![digest, sequence],
            |row| row.get(0),
        )
        .optional()?)
}

/// Streams and checks original bytes against the reviewed length and digest.
/// New bytes remain unpublished for the caller to commit; failures attempt cleanup,
/// and already-published captures are left unchanged.
pub(super) fn store(
    database: &RideDatabase,
    preview: &PevcapImportPreview,
    source: &Path,
) -> Result<(), StorageError> {
    let start = database.request(|reply| Command::BeginCaptureData {
        digest: preview.artifact_digest.clone(),
        encoding: preview.encoding(),
        artifact_size: preview.artifact_size,
        reply,
    })?;
    if start == CaptureStart::Stored {
        return Ok(());
    }
    let result = (|| {
        let mut source = File::open(source)?;
        let mut digest = Sha256::new();
        let mut sequence = 0_u64;
        let mut total = 0_u64;
        while let Some(chunk) = CaptureChunk::read(&mut source)? {
            total += chunk.0.len() as u64;
            if total > preview.artifact_size {
                return Err(StorageError::PevcapPreviewChanged);
            }
            digest.update(&chunk.0);
            database.request(|reply| Command::AppendCaptureData {
                digest: preview.artifact_digest.clone(),
                sequence,
                chunk,
                reply,
            })?;
            sequence += 1;
        }
        if total != preview.artifact_size
            || hex::encode(digest.finalize()) != preview.artifact_digest
        {
            return Err(StorageError::PevcapPreviewChanged);
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = database.request(|reply| Command::AbortCaptureData {
            digest: preview.artifact_digest.clone(),
            reply,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_chunks_bound_the_worker_message_and_exclude_eof() {
        let bytes = vec![42; CHUNK_BYTES + 1];
        let mut source = bytes.as_slice();
        assert_eq!(
            CaptureChunk::read(&mut source).unwrap().unwrap().0.len(),
            CHUNK_BYTES
        );
        assert_eq!(CaptureChunk::read(&mut source).unwrap().unwrap().0, [42]);
        assert!(CaptureChunk::read(&mut source).unwrap().is_none());
    }

    #[test]
    fn unpublished_capture_chunks_are_hidden_and_abort_rolls_back_data() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        super::super::migrations::create_current_schema(&connection).unwrap();
        let digest = "a".repeat(64);
        connection.execute("INSERT INTO pevcap_import_work (artifact_digest, artifact_path) VALUES (?1, '/unused')", [&digest]).unwrap();
        assert_eq!(
            begin(&connection, &digest, PevcapEncoding::Binary, 2).unwrap(),
            CaptureStart::Started
        );
        let payload = CaptureChunk::read(&mut [1_u8, 2].as_slice())
            .unwrap()
            .unwrap();
        assert!(append(&mut connection, &digest, 1, &payload).is_err());
        append(&mut connection, &digest, 0, &payload).unwrap();
        assert!(append(&mut connection, &digest, 0, &payload).is_err());
        assert!(chunk(&connection, &digest, 0).unwrap().is_none());
        assert!(publish(&connection, &digest).is_err());
        abort(&connection, &digest).unwrap();
        let remaining: u64 = connection
            .query_row("SELECT count(*) FROM pevcap_capture_chunks", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
