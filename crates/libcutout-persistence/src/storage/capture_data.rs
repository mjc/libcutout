use super::{Command, PevcapImportPreview, RideDatabase, StorageError};
use cutout_core::PevcapEncoding;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

const CHUNK_BYTES: usize = 64 * 1024;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CaptureStart {
    Stored,
    Started,
}

pub(super) fn is_stored(connection: &Connection, digest: &str) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pevcap_captures WHERE receipt_digest = ?1)",
        [digest],
        |row| row.get(0),
    )?)
}

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

pub(super) fn append(
    connection: &mut Connection,
    digest: &str,
    sequence: u64,
    bytes: &[u8],
) -> Result<(), StorageError> {
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

pub(super) fn abort(connection: &Connection, digest: &str) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM pevcap_captures WHERE artifact_digest = ?1 AND receipt_digest IS NULL",
        [digest],
    )?;
    Ok(())
}

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
        loop {
            let mut bytes = vec![0; CHUNK_BYTES];
            let count = source.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            bytes.truncate(count);
            total += count as u64;
            if total > preview.artifact_size {
                return Err(StorageError::PevcapPreviewChanged);
            }
            digest.update(&bytes);
            database.request(|reply| Command::AppendCaptureData {
                digest: preview.artifact_digest.clone(),
                sequence,
                bytes,
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
