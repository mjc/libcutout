//! Retirement of exact duplicates of published canonical capture bytes.

use super::{RideDatabase, StorageError, normalization};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const FILE_CHUNK_BYTES: usize = 64 * 1024;

/// Files scanned and retired by one managed capture cleanup pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CaptureFileDeduplication {
    /// Number of recognized regular capture files compared with canonical data.
    pub scanned_files: u64,
    /// Number of exact duplicates successfully removed.
    pub removed_files: u64,
    /// Bytes reclaimed from successfully removed files.
    pub removed_bytes: u64,
}

impl RideDatabase {
    /// Removes exact copies of published capture bytes from a caller-owned directory.
    ///
    /// Only recognized flat capture filenames are considered. The caller should invoke this
    /// synchronous filesystem operation away from UI and callback threads.
    ///
    /// # Errors
    /// Returns [`StorageError`] when directory enumeration or canonical database reads fail.
    pub fn deduplicate_managed_capture_files(
        &self,
        directory: &Path,
    ) -> Result<CaptureFileDeduplication, StorageError> {
        deduplicate_managed_capture_files_inner(self, directory, |_| {})
    }
}

fn deduplicate_managed_capture_files_inner(
    database: &RideDatabase,
    directory: &Path,
    mut before_unlink: impl FnMut(&Path),
) -> Result<CaptureFileDeduplication, StorageError> {
    let directory_metadata = match fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CaptureFileDeduplication::default());
        }
        Err(error) => return Err(StorageError::Io(error)),
    };
    if !directory_metadata.file_type().is_dir() || directory_metadata.file_type().is_symlink() {
        return Ok(CaptureFileDeduplication::default());
    }
    let canonical_directory = fs::canonicalize(directory)?;

    let mut result = CaptureFileDeduplication::default();
    for entry in fs::read_dir(directory)? {
        if !directory_still_matches(directory, &canonical_directory)? {
            break;
        }
        let entry = entry?;
        let path = entry.path();
        let Some(file_kind) = capture_file_kind(&entry.file_name()) else {
            continue;
        };
        let path_metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(StorageError::Io(error)),
        };
        if !path_metadata.file_type().is_file() || path_metadata.file_type().is_symlink() {
            continue;
        }
        result.scanned_files = result
            .scanned_files
            .checked_add(1)
            .ok_or(StorageError::InvalidDatabaseIdentity)?;
        let initial_fingerprint = normalization::FileFingerprint::from_metadata(&path_metadata);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(StorageError::Io(error)),
        };
        let opened_metadata = file.metadata()?;
        if !opened_metadata.is_file()
            || !normalization::same_file_identity(
                initial_fingerprint,
                normalization::FileFingerprint::from_metadata(&opened_metadata),
            )
        {
            continue;
        }
        let Some(digest) = hash_file(&mut file, initial_fingerprint.len())? else {
            continue;
        };
        if !path_still_matches(&path, &file, initial_fingerprint)? {
            continue;
        }
        let canonical_match = file_kind == CaptureFileKind::ManagedOriginal
            && matches_published_capture(database, &mut file, &digest, initial_fingerprint.len())?;
        let receipt_authorized = !canonical_match
            && database
                .managed_capture_file_removal_authorized(&digest, initial_fingerprint.len())?;
        if !canonical_match && !receipt_authorized {
            continue;
        }

        before_unlink(&path);
        if !directory_still_matches(directory, &canonical_directory)?
            || !path_still_matches(&path, &file, initial_fingerprint)?
        {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => {
                result.removed_files = result
                    .removed_files
                    .checked_add(1)
                    .ok_or(StorageError::InvalidDatabaseIdentity)?;
                result.removed_bytes = result
                    .removed_bytes
                    .checked_add(initial_fingerprint.len())
                    .ok_or(StorageError::InvalidDatabaseIdentity)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(StorageError::Io(error)),
        }
    }
    Ok(result)
}

fn directory_still_matches(directory: &Path, canonical: &Path) -> Result<bool, StorageError> {
    let metadata = match fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(StorageError::Io(error)),
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    Ok(fs::canonicalize(directory)? == canonical)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureFileKind {
    ManagedOriginal,
    ExportCache,
}

fn capture_file_kind(filename: &std::ffi::OsStr) -> Option<CaptureFileKind> {
    let filename = filename.to_str()?;
    if let Some(stem) = filename
        .strip_prefix("cutout-btle-capture-")
        .and_then(|name| name.strip_suffix(".jsonl"))
    {
        let (timestamp, uuid) = stem.split_once('-')?;
        return (!timestamp.is_empty()
            && timestamp.bytes().all(|byte| byte.is_ascii_digit())
            && is_canonical_uuid(uuid))
        .then_some(CaptureFileKind::ManagedOriginal);
    }
    if filename
        .strip_prefix("recovered-")
        .and_then(|name| name.strip_suffix(".pevcap.jsonl"))
        .is_some_and(|name| is_canonical_uuid(name) || is_iso_date(name))
    {
        return Some(CaptureFileKind::ManagedOriginal);
    }
    if filename
        .strip_prefix("capture-")
        .and_then(|name| {
            name.strip_suffix(".jsonl")
                .or_else(|| name.strip_suffix(".pevcap"))
        })
        .is_some_and(is_canonical_uuid)
        || filename
            .strip_prefix("cutout-capture-")
            .and_then(|name| name.strip_suffix(".jsonl"))
            .is_some_and(is_canonical_uuid)
    {
        return Some(CaptureFileKind::ExportCache);
    }
    None
}

fn is_canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .is_ok_and(|parsed| parsed.hyphenated().to_string().eq_ignore_ascii_case(value))
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u16>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u8>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u8>() else {
        return false;
    };
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}

fn hash_file(file: &mut File, expected_size: u64) -> Result<Option<String>, StorageError> {
    file.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0; FILE_CHUNK_BYTES];
    loop {
        let count = read_chunk(file, &mut buffer)?;
        if count == 0 {
            break;
        }
        total = match total.checked_add(u64::try_from(count).map_err(|_| {
            StorageError::PevcapCaptureIntegrity("managed file size overflowed".to_owned())
        })?) {
            Some(total) if total <= expected_size => total,
            _ => return Ok(None),
        };
        hasher.update(&buffer[..count]);
    }
    if total != expected_size || total == 0 {
        return Ok(None);
    }
    Ok(Some(hex::encode(hasher.finalize())))
}

fn matches_published_capture(
    database: &RideDatabase,
    file: &mut File,
    digest: &str,
    expected_size: u64,
) -> Result<bool, StorageError> {
    file.seek(SeekFrom::Start(0))?;
    let mut sequence = 0_u64;
    let mut total = 0_u64;
    let mut buffer = vec![0; FILE_CHUNK_BYTES];
    loop {
        let count = read_chunk(file, &mut buffer)?;
        if count == 0 {
            break;
        }
        let Some(canonical_chunk) = database.pevcap_capture_chunk(digest, sequence)? else {
            return Ok(false);
        };
        if canonical_chunk.as_slice() != &buffer[..count] {
            return Ok(false);
        }
        total = match total.checked_add(u64::try_from(count).map_err(|_| {
            StorageError::PevcapCaptureIntegrity("managed file size overflowed".to_owned())
        })?) {
            Some(total) if total <= expected_size => total,
            _ => return Ok(false),
        };
        sequence = sequence
            .checked_add(1)
            .ok_or(StorageError::InvalidDatabaseIdentity)?;
    }
    if total != expected_size || database.pevcap_capture_chunk(digest, sequence)?.is_some() {
        return Ok(false);
    }
    Ok(true)
}

fn read_chunk(file: &mut File, buffer: &mut [u8]) -> Result<usize, StorageError> {
    let mut total = 0;
    while total < buffer.len() {
        let count = file.read(&mut buffer[total..])?;
        if count == 0 {
            break;
        }
        total += count;
    }
    Ok(total)
}

fn path_still_matches(
    path: &Path,
    file: &File,
    expected: normalization::FileFingerprint,
) -> Result<bool, StorageError> {
    let path_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(StorageError::Io(error)),
    };
    if !path_metadata.file_type().is_file() || path_metadata.file_type().is_symlink() {
        return Ok(false);
    }
    let opened_metadata = file.metadata()?;
    Ok(normalization::same_file_identity(
        expected,
        normalization::FileFingerprint::from_metadata(&path_metadata),
    ) && normalization::same_file_identity(
        expected,
        normalization::FileFingerprint::from_metadata(&opened_metadata),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaptureMetadata, CaptureWriter, CaptureWriterFinish, RideDatabase};
    use cutout_core::{
        CaptureOrigin, GattChannel, MonotonicTimestamp, PevcapCapture, PevcapEncoding,
        PevcapHeader, PevcapRecord, WallClockUnixTimestamp,
    };
    use rusqlite::Connection;
    use std::io::{Seek, SeekFrom, Write};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn published_capture() -> (TempDir, RideDatabase, String, Vec<u8>) {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ride.sqlite");
        let source = directory.path().join("source.jsonl");
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "darwin",
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
        let channel = GattChannel::from_bytes([0x11; 16]);
        let bytes = PevcapCapture::new(
            header,
            vec![PevcapRecord::inbound_notification(
                MonotonicTimestamp::new(1),
                channel,
                GattChannel::from_bytes([0x22; 16]),
                vec![0x01, 0x02, 0x03],
            )],
        )
        .to_jsonl()
        .unwrap()
        .into_bytes();
        fs::write(&source, &bytes).unwrap();
        let database = RideDatabase::open(&database_path).unwrap();
        let preview = database
            .preflight_pevcap(&source, PevcapEncoding::Jsonl)
            .unwrap();
        let digest = preview.artifact_digest().to_owned();
        database
            .confirm_pevcap_import(&preview, 1_700_000_000_001)
            .unwrap();
        (directory, database, digest, bytes)
    }

    fn retained_writer_capture() -> (TempDir, RideDatabase, crate::CaptureArtifactId, Vec<u8>) {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let writer = CaptureWriter::start_with_database(
            directory.path().join("private-original.jsonl"),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test-device",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec![],
            },
            database.clone(),
        )
        .unwrap();
        assert_eq!(
            writer.try_send_record(PevcapRecord::inbound_notification(
                MonotonicTimestamp::new(1),
                GattChannel::from_bytes([0x11; 16]),
                GattChannel::from_bytes([0x22; 16]),
                vec![0x01, 0x02, 0x03],
            )),
            crate::CaptureWriteOutcome::Accepted,
        );
        let CaptureWriterFinish::DatabaseFinished {
            live_capture_id,
            receipt,
            ..
        } = writer.finish_without_export().unwrap()
        else {
            panic!("database-backed capture must finish in storage");
        };
        let artifact_id = receipt.artifact_id();
        database
            .retain_finished_database_capture(
                &receipt,
                CaptureOrigin::Manual,
                None,
                WallClockUnixTimestamp::new(1_700_000_000_001),
            )
            .unwrap();
        let exported = directory.path().join("canonical.jsonl");
        database
            .export_live_capture(live_capture_id, &exported)
            .unwrap();
        let bytes = fs::read(exported).unwrap();
        (directory, database, artifact_id, bytes)
    }

    fn retained_file_writer_capture() -> (TempDir, RideDatabase, crate::CaptureArtifactId, Vec<u8>)
    {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let source = directory.path().join("private-original.jsonl");
        let writer = CaptureWriter::start(
            source.clone(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test-device",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec![],
            },
        )
        .unwrap();
        assert_eq!(
            writer.try_send_record(PevcapRecord::inbound_notification(
                MonotonicTimestamp::new(1),
                GattChannel::from_bytes([0x11; 16]),
                GattChannel::from_bytes([0x22; 16]),
                vec![0x01, 0x02, 0x03],
            )),
            crate::CaptureWriteOutcome::Accepted,
        );
        let artifact = writer.finish_exported().unwrap();
        let bytes = fs::read(&source).unwrap();
        let receipt = database
            .retain_finished_capture(
                &artifact,
                CaptureOrigin::Manual,
                None,
                WallClockUnixTimestamp::new(1_700_000_000_001),
            )
            .unwrap();
        (
            directory,
            database,
            receipt.recording.unwrap().artifact_id,
            bytes,
        )
    }

    fn capture_file(directory: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        fs::create_dir_all(directory).unwrap();
        let path = directory.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    fn btle_name(uuid: &str) -> String {
        format!("cutout-btle-capture-1700000000-{uuid}.jsonl")
    }

    fn recovered_name(uuid: &str) -> String {
        format!("recovered-{uuid}.pevcap.jsonl")
    }

    fn dated_recovered_name(date: &str) -> String {
        format!("recovered-{date}.pevcap.jsonl")
    }

    fn ios_export_name(uuid: &str) -> String {
        format!("capture-{uuid}.jsonl")
    }

    fn android_export_name(uuid: &str) -> String {
        format!("cutout-capture-{uuid}.jsonl")
    }

    #[test]
    fn removes_exact_managed_duplicates_and_preserves_canonical_bytes_after_reopen() {
        let (directory, database, digest, bytes) = published_capture();
        let documents = directory.path().join("Documents");
        let uuid_a = uuid::Uuid::new_v4().hyphenated().to_string();
        let uuid_b = uuid::Uuid::new_v4().hyphenated().to_string();
        let first = capture_file(&documents, &btle_name(&uuid_a), &bytes);
        let second = capture_file(&documents, &recovered_name(&uuid_b), &bytes);
        let third = capture_file(&documents, &dated_recovered_name("2026-10-04"), &bytes);
        let unknown = capture_file(&documents, "cutout-btle-capture-old.jsonl", &bytes);
        let invalid_date = capture_file(&documents, &dated_recovered_name("2026-02-30"), &bytes);

        let result = database
            .deduplicate_managed_capture_files(&documents)
            .unwrap();

        assert_eq!(result.scanned_files, 3);
        assert_eq!(result.removed_files, 3);
        assert_eq!(
            result.removed_bytes,
            u64::try_from(bytes.len()).unwrap() * 3
        );
        assert!(!first.exists());
        assert!(!second.exists());
        assert!(!third.exists());
        assert_eq!(fs::read(&unknown).unwrap(), bytes);
        assert_eq!(fs::read(&invalid_date).unwrap(), bytes);
        database.shutdown().unwrap();

        let reopened = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let exported = directory.path().join("reopened.jsonl");
        reopened.export_pevcap_capture(&digest, &exported).unwrap();
        assert_eq!(fs::read(exported).unwrap(), bytes);
        reopened.shutdown().unwrap();
    }

    #[test]
    fn preserves_candidate_when_canonical_chunk_is_corrupt() {
        let (directory, database, digest, bytes) = published_capture();
        let database_path = directory.path().join("ride.sqlite");
        database.shutdown().unwrap();
        Connection::open(&database_path)
            .unwrap()
            .execute(
                "UPDATE pevcap_capture_chunks SET payload = zeroblob(length(payload))
                 WHERE artifact_digest = ?1 AND sequence = 0",
                [&digest],
            )
            .unwrap();
        let reopened = RideDatabase::open(&database_path).unwrap();
        let documents = directory.path().join("Documents");
        let candidate = capture_file(
            &documents,
            &btle_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let result = reopened
            .deduplicate_managed_capture_files(&documents)
            .unwrap();

        assert_eq!(result.scanned_files, 1);
        assert_eq!(result.removed_files, 0);
        assert_eq!(result.removed_bytes, 0);
        assert_eq!(fs::read(candidate).unwrap(), bytes);
        reopened.shutdown().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinked_capture_candidates() {
        use std::os::unix::fs::symlink;

        let (directory, database, _digest, bytes) = published_capture();
        let documents = directory.path().join("Documents");
        fs::create_dir_all(&documents).unwrap();
        let source = directory.path().join("outside.jsonl");
        fs::write(&source, &bytes).unwrap();
        let candidate = documents.join(btle_name(&uuid::Uuid::new_v4().hyphenated().to_string()));
        symlink(&source, &candidate).unwrap();

        let result = database
            .deduplicate_managed_capture_files(&documents)
            .unwrap();

        assert_eq!(result.scanned_files, 0);
        assert_eq!(result.removed_files, 0);
        assert!(candidate.symlink_metadata().is_ok());
        assert_eq!(fs::read(&source).unwrap(), bytes);
        database.shutdown().unwrap();
    }

    #[test]
    fn preserves_candidate_changed_after_comparison_before_unlink() {
        let (directory, database, _digest, bytes) = published_capture();
        let documents = directory.path().join("Documents");
        let candidate = capture_file(
            &documents,
            &btle_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let mut changed_bytes = bytes.clone();
        changed_bytes[0] ^= 1;

        let result = deduplicate_managed_capture_files_inner(&database, &documents, |path| {
            let mut file = fs::OpenOptions::new().write(true).open(path).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&changed_bytes[..1]).unwrap();
            file.sync_all().unwrap();
        })
        .unwrap();

        assert_eq!(result.scanned_files, 1);
        assert_eq!(result.removed_files, 0);
        assert_eq!(fs::read(candidate).unwrap(), changed_bytes);
        database.shutdown().unwrap();
    }

    #[test]
    fn recorded_export_uses_canonical_bytes_after_managed_duplicate_is_removed() {
        let (directory, database, artifact_id, bytes) = retained_file_writer_capture();
        let documents = directory.path().join("Documents");
        let duplicate = capture_file(
            &documents,
            &btle_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let result = database
            .deduplicate_managed_capture_files(&documents)
            .unwrap();
        assert_eq!(result.removed_files, 1);
        assert!(!duplicate.exists());
        database.shutdown().unwrap();

        let reopened = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let destination = directory.path().join("exported-from-canonical.jsonl");
        reopened
            .export_recorded_capture(artifact_id, &destination)
            .unwrap();
        assert_eq!(fs::read(destination).unwrap(), bytes);
        reopened.shutdown().unwrap();
    }

    #[test]
    fn recorded_export_uses_sqlite_only_live_capture_alias() {
        let (directory, database, artifact_id, bytes) = retained_writer_capture();
        database.shutdown().unwrap();

        let reopened = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let destination = directory.path().join("exported-live-capture.jsonl");
        reopened
            .export_recorded_capture(artifact_id, &destination)
            .unwrap();
        assert_eq!(fs::read(destination).unwrap(), bytes);
        reopened.shutdown().unwrap();
    }

    #[test]
    fn recorded_export_reports_unpublished_artifact_as_not_found() {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let artifact_id =
            crate::CaptureArtifactId::parse("3f2504e0-4f89-41d3-9a0c-0305e82c3301").unwrap();
        let destination = directory.path().join("not-written.jsonl");

        let result = database.export_recorded_capture(artifact_id, &destination);

        match result {
            Err(StorageError::PevcapCaptureNotFound) => {}
            other => panic!("unpublished artifact should remain not found: {other:?}"),
        }
        assert!(!destination.exists());
        database.shutdown().unwrap();
    }

    #[test]
    fn sent_receipt_retires_exact_managed_file_after_canonical_cleanup() {
        let (directory, database, artifact_id, bytes) = retained_file_writer_capture();
        let documents = directory.path().join("Documents");
        let matching = capture_file(
            &documents,
            &btle_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let mut different_bytes = bytes.clone();
        different_bytes[0] ^= 1;
        let mismatched = capture_file(
            &documents,
            &btle_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &different_bytes,
        );
        let unrecognized = capture_file(&documents, "old-capture.jsonl", &bytes);
        let ios_exports = directory.path().join("CaptureExports");
        let ios_export = capture_file(
            &ios_exports,
            &ios_export_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let android_exports = directory.path().join("diagnostic-exports");
        let android_export = capture_file(
            &android_exports,
            &android_export_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        database
            .save_capture_evidence(
                crate::CaptureSubmissionSource::RecordedArtifact(artifact_id),
                cutout_core::CaptureAttachments {
                    note: "sent with context".to_owned(),
                    screenshots: vec![],
                },
            )
            .unwrap();
        let prepared = database
            .prepare_capture_submission(crate::CaptureSubmissionSource::RecordedArtifact(
                artifact_id,
            ))
            .unwrap();
        assert_ne!(
            prepared.metadata.sha256,
            hex::encode(Sha256::digest(&bytes))
        );
        database.shutdown().unwrap();

        seed_sent_status_and_cleanup(&directory.path().join("ride.sqlite"), &prepared);
        let reopened = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let result = reopened
            .deduplicate_managed_capture_files(&documents)
            .unwrap();

        assert_eq!(result.scanned_files, 2);
        assert_eq!(result.removed_files, 1);
        assert_eq!(result.removed_bytes, u64::try_from(bytes.len()).unwrap());
        assert!(!matching.exists());
        assert!(mismatched.exists(), "a different file is never retired");
        assert!(unrecognized.exists(), "unknown names are never retired");
        assert_eq!(
            reopened
                .deduplicate_managed_capture_files(&ios_exports)
                .unwrap()
                .removed_files,
            1
        );
        assert_eq!(
            reopened
                .deduplicate_managed_capture_files(&android_exports)
                .unwrap()
                .removed_files,
            1
        );
        assert!(!ios_export.exists());
        assert!(!android_export.exists());
        reopened.shutdown().unwrap();
    }

    #[test]
    fn unsent_export_cache_survives_even_when_it_matches_canonical_bytes() {
        let (directory, database, artifact_id, bytes) = retained_file_writer_capture();
        let cache = directory.path().join("CaptureExports");
        let export = capture_file(
            &cache,
            &ios_export_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        database
            .prepare_capture_submission(crate::CaptureSubmissionSource::RecordedArtifact(
                artifact_id,
            ))
            .unwrap();

        let result = database.deduplicate_managed_capture_files(&cache).unwrap();

        assert_eq!(result.scanned_files, 1);
        assert_eq!(result.removed_files, 0);
        assert_eq!(fs::read(export).unwrap(), bytes);
        database.shutdown().unwrap();
    }

    #[test]
    fn valid_receipt_authorizes_export_cache_before_database_payload_cleanup() {
        let (directory, database, artifact_id, bytes) = retained_file_writer_capture();
        let cache = directory.path().join("CaptureExports");
        let export = capture_file(
            &cache,
            &ios_export_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let prepared = database
            .prepare_capture_submission(crate::CaptureSubmissionSource::RecordedArtifact(
                artifact_id,
            ))
            .unwrap();
        let mut status = serde_json::to_value(&prepared).unwrap();
        status["phase"] = serde_json::json!("sent");
        status["local_payload_state"] = serde_json::json!("removal_pending");
        status["account_id"] = serde_json::json!("account-1");
        status["receipt"] = serde_json::json!({
            "operation_id": prepared.operation_id,
            "sha256": prepared.metadata.sha256,
            "bytes": prepared.metadata.bytes,
            "received_at_milliseconds": 1_700_000_002_000_u64,
        });
        Connection::open(directory.path().join("ride.sqlite"))
            .unwrap()
            .execute(
                "UPDATE capture_submissions SET status_json = ?2 WHERE operation_id = ?1",
                rusqlite::params![prepared.operation_id, status.to_string()],
            )
            .unwrap();

        let before: i64 = Connection::open(directory.path().join("ride.sqlite"))
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM pevcap_capture_chunks WHERE artifact_digest = ?1",
                [prepared
                    .metadata
                    .provenance
                    .source_sha256
                    .as_deref()
                    .unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            before > 0,
            "canonical chunks remain before background cleanup"
        );
        let result = database.deduplicate_managed_capture_files(&cache).unwrap();

        assert_eq!(result.removed_files, 1);
        assert!(!export.exists());
        let after: i64 = Connection::open(directory.path().join("ride.sqlite"))
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM pevcap_capture_chunks WHERE artifact_digest = ?1",
                [prepared
                    .metadata
                    .provenance
                    .source_sha256
                    .as_deref()
                    .unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(after > 0, "deduplication does not run payload cleanup");
        database.shutdown().unwrap();
    }

    #[test]
    fn invalid_receipt_does_not_authorize_managed_file_removal_after_cleanup() {
        let (directory, database, artifact_id, bytes) = retained_file_writer_capture();
        let documents = directory.path().join("Documents");
        let candidate = capture_file(
            &documents,
            &btle_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let cache = directory.path().join("CaptureExports");
        let cache_candidate = capture_file(
            &cache,
            &ios_export_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let prepared = database
            .prepare_capture_submission(crate::CaptureSubmissionSource::RecordedArtifact(
                artifact_id,
            ))
            .unwrap();
        database.shutdown().unwrap();

        let database_path = directory.path().join("ride.sqlite");
        let connection = Connection::open(&database_path).unwrap();
        let mut status = serde_json::to_value(&prepared).unwrap();
        status["phase"] = serde_json::json!("sent");
        status["local_payload_state"] = serde_json::json!("removed");
        status["account_id"] = serde_json::json!("account-1");
        status["receipt"] = serde_json::json!({
            "operation_id": "wrong-operation",
            "sha256": prepared.metadata.sha256,
            "bytes": prepared.metadata.bytes,
            "received_at_milliseconds": 1_700_000_002_000_u64,
        });
        connection
            .execute(
                "UPDATE capture_submissions SET status_json = ?2 WHERE operation_id = ?1",
                rusqlite::params![prepared.operation_id, status.to_string()],
            )
            .unwrap();
        connection
            .execute(
                "DELETE FROM pevcap_capture_chunks WHERE artifact_digest = ?1",
                [prepared.metadata.capture_sha256()],
            )
            .unwrap();
        drop(connection);

        let reopened = RideDatabase::open(&database_path).unwrap();
        let result = reopened
            .deduplicate_managed_capture_files(&documents)
            .unwrap();
        assert_eq!(result.removed_files, 0);
        assert!(candidate.exists());
        assert_eq!(
            reopened
                .deduplicate_managed_capture_files(&cache)
                .unwrap()
                .removed_files,
            0
        );
        assert!(cache_candidate.exists());
        reopened.shutdown().unwrap();
    }

    #[test]
    fn absent_receipt_does_not_authorize_managed_file_removal_after_cleanup() {
        let (directory, database, _artifact_id, bytes) = retained_file_writer_capture();
        let documents = directory.path().join("Documents");
        let candidate = capture_file(
            &documents,
            &btle_name(&uuid::Uuid::new_v4().hyphenated().to_string()),
            &bytes,
        );
        let digest = hex::encode(Sha256::digest(&bytes));
        database.shutdown().unwrap();

        let database_path = directory.path().join("ride.sqlite");
        Connection::open(&database_path)
            .unwrap()
            .execute(
                "DELETE FROM pevcap_capture_chunks WHERE artifact_digest = ?1",
                [&digest],
            )
            .unwrap();
        let reopened = RideDatabase::open(&database_path).unwrap();
        let result = reopened
            .deduplicate_managed_capture_files(&documents)
            .unwrap();
        assert_eq!(result.removed_files, 0);
        assert!(candidate.exists());
        reopened.shutdown().unwrap();
    }

    #[test]
    fn statusless_export_io_failure_is_not_mislabeled_as_retired_payload() {
        let (directory, database, artifact_id, _bytes) = retained_writer_capture();
        let destination = directory.path().join("missing-parent/export.jsonl");

        assert!(matches!(
            database.export_recorded_capture(artifact_id, &destination),
            Err(StorageError::PevcapCaptureIntegrity(_))
        ));

        database.shutdown().unwrap();
    }

    fn seed_sent_status_and_cleanup(
        database_path: &Path,
        prepared: &crate::CaptureSubmissionStatus,
    ) {
        let mut status = serde_json::to_value(prepared).unwrap();
        status["phase"] = serde_json::json!("sent");
        status["local_payload_state"] = serde_json::json!("removal_pending");
        status["account_id"] = serde_json::json!("account-1");
        status["receipt"] = serde_json::json!({
            "operation_id": prepared.operation_id,
            "sha256": prepared.metadata.sha256,
            "bytes": prepared.metadata.bytes,
            "received_at_milliseconds": 1_700_000_002_000_u64,
        });
        let mut connection = Connection::open(database_path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        connection
            .execute(
                "UPDATE capture_submissions SET status_json = ?2 WHERE operation_id = ?1",
                rusqlite::params![prepared.operation_id, status.to_string()],
            )
            .unwrap();
        let mut cursor = None;
        for _ in 0..8 {
            if super::super::capture_submission::cleanup_sent_capture_batch(
                &mut connection,
                &mut cursor,
            )
            .unwrap()
                == normalization::BatchState::Finished
            {
                break;
            }
        }
        let state: String = connection
            .query_row(
                "SELECT json_extract(status_json, '$.local_payload_state')
                 FROM capture_submissions WHERE operation_id = ?1",
                [&prepared.operation_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "removed");
    }
}
