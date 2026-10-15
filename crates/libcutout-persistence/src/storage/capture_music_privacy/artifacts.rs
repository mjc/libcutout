//! Sanitized derivatives retain capture/ride identity; original imports outside our store stay untouched.
use super::{StorageError, transform};
use crate::storage::{capture_data, pevcap_import_directory};
use cutout_core::PevcapEncoding;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

fn owned_directory(path: &Path) -> Result<bool, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(StorageError::InvalidPath),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn regular(path: &Path) -> Result<bool, StorageError> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(metadata.is_file() && !metadata.file_type().is_symlink())
}
fn digest(file: &mut File) -> Result<(String, u64), StorageError> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut bytes = vec![0; 65536];
    let mut total = 0;
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hash.update(&bytes[..n]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok((hex::encode(hash.finalize()), total))
}
fn reusable_destination(path: &Path, expected: &str) -> Result<bool, StorageError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && digest(&mut File::open(path)?)?.0 == expected =>
        {
            Ok(true)
        }
        Ok(_) => Err(StorageError::CaptureIdentityConflict),
    }
}
fn encoding(path: &Path) -> Option<PevcapEncoding> {
    match path.extension()?.to_str()? {
        "jsonl" => Some(PevcapEncoding::Jsonl),
        "pevcap" => Some(PevcapEncoding::Binary),
        _ => None,
    }
}
fn managed_name(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        && encoding(path).is_some()
}
fn native_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if let Some(value) = name
        .strip_prefix("capture-")
        .and_then(|s| s.strip_suffix(".jsonl"))
    {
        return uuid::Uuid::parse_str(value).is_ok();
    }
    let Some(value) = name
        .strip_prefix("cutout-btle-capture-")
        .and_then(|s| s.strip_suffix(".jsonl"))
    else {
        return false;
    };
    // Recognize UUID exports and the current app recording timestamp plus UUID.
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) && value.parse::<u64>().is_ok())
        || uuid::Uuid::parse_str(value).is_ok()
        || value.split_once('-').is_some_and(|(seconds, id)| {
            !seconds.is_empty()
                && seconds.bytes().all(|b| b.is_ascii_digit())
                && seconds.parse::<u64>().is_ok()
                && uuid::Uuid::parse_str(id).is_ok()
        })
}

struct CheckedReader<R> {
    source: R,
    hash: Sha256,
}
impl<R: Read> Read for CheckedReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let count = self.source.read(bytes)?;
        self.hash.update(&bytes[..count]);
        Ok(count)
    }
}

struct ChunkReader<'a> {
    connection: &'a Connection,
    digest: &'a str,
    sequence: u64,
    bytes: std::io::Cursor<Vec<u8>>,
}
impl Read for ChunkReader<'_> {
    fn read(&mut self, destination: &mut [u8]) -> std::io::Result<usize> {
        if destination.is_empty() {
            return Ok(0);
        }
        if self.bytes.position() == self.bytes.get_ref().len() as u64 {
            let next = capture_data::chunk(self.connection, self.digest, self.sequence)
                .map_err(std::io::Error::other)?;
            let Some(next) = next else {
                return Ok(0);
            };
            self.sequence += 1;
            self.bytes = std::io::Cursor::new(next);
        }
        self.bytes.read(destination)
    }
}

/// Preflight rejects unfinished import work rather than racing a receipt publication.
pub(super) fn check_imports(connection: &Connection) -> Result<(), StorageError> {
    let unfinished: bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM pevcap_import_work) OR EXISTS(SELECT 1 FROM pevcap_captures WHERE receipt_digest IS NULL)",[],|r|r.get(0))?;
    if unfinished {
        return Err(StorageError::PevcapImportInProgress);
    }
    Ok(())
}

pub(super) fn purge_imports(
    connection: &mut Connection,
    database_path: &Path,
) -> Result<(), StorageError> {
    let directory = pevcap_import_directory(database_path);
    fs::create_dir_all(&directory)?;
    if !owned_directory(&directory)? {
        return Err(StorageError::InvalidPath);
    }
    let mut after = String::new();
    loop {
        let receipt=connection.query_row("SELECT artifact_digest,artifact_path FROM pevcap_imports WHERE artifact_digest>?1 ORDER BY artifact_digest LIMIT 1",[&after],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).optional()?;
        let Some((old, path)) = receipt else {
            break;
        };
        after.clone_from(&old);
        let stored: Option<String> = connection
            .query_row(
                "SELECT encoding FROM pevcap_captures WHERE receipt_digest=?1",
                [&old],
                |row| row.get(0),
            )
            .optional()?;
        let format = match stored.as_deref() {
            Some("binary") => PevcapEncoding::Binary,
            Some("jsonl") => PevcapEncoding::Jsonl,
            None => encoding(Path::new(&path)).ok_or_else(transform::invalid)?,
            _ => return Err(transform::invalid()),
        };
        let extension = if format == PevcapEncoding::Binary {
            "pevcap"
        } else {
            "jsonl"
        };
        let old_owned = directory.join(format!("{old}.{extension}"));
        let mut source: Box<dyn Read + '_> = if stored.is_some() {
            Box::new(ChunkReader {
                connection,
                digest: &old,
                sequence: 0,
                bytes: std::io::Cursor::new(Vec::new()),
            })
        } else {
            if !regular(&old_owned)? {
                return Err(StorageError::InvalidPath);
            }
            Box::new(File::open(&old_owned)?)
        };
        let mut stage = NamedTempFile::new_in(&directory)?;
        let mut checked = CheckedReader {
            source: &mut source,
            hash: Sha256::new(),
        };
        let changed =
            transform::remove_with_provenance(&mut checked, &mut stage, format, Some(&old))?;
        if hex::encode(checked.hash.finalize()) != old {
            return Err(StorageError::PevcapPreviewChanged);
        }
        drop(source);
        if !changed {
            continue;
        }
        stage.as_file().sync_all()?;
        let (new, size) = digest(stage.as_file_mut())?;
        let destination = directory.join(format!("{new}.{extension}"));
        // No music remains in this staged file. Publish it before relinking so a crash leaves
        // either the old indexed bytes or the new indexed bytes, never a missing artifact.
        if !reusable_destination(&destination, &new)? {
            stage
                .persist_noclobber(&destination)
                .map_err(|error| StorageError::Io(error.error))?;
        }
        File::open(&directory)?.sync_all()?;
        relink(connection, &old, &new, &destination, format, size)?;
        if old_owned.exists() {
            if !regular(&old_owned)? {
                return Err(StorageError::InvalidPath);
            }
            // A divergent managed file may contain distinct Bluetooth evidence. Leave it
            // for the recognized-orphan pass, which preserves its own sanitized bytes.
            if digest(&mut File::open(&old_owned)?)?.0 == old {
                fs::remove_file(&old_owned)?;
                File::open(&directory)?.sync_all()?;
            }
        }
    }
    Ok(())
}

/// Rewrite only recognized app file names. Symlink leaves and user import sources are excluded.
pub(super) fn purge_files(database_path: &Path, supplied: &[PathBuf]) -> Result<(), StorageError> {
    let managed = pevcap_import_directory(database_path);
    for (directory, is_managed) in
        std::iter::once((&managed, true)).chain(supplied.iter().map(|path| (path, false)))
    {
        if !owned_directory(directory)? {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if !(if is_managed {
                managed_name(&path)
            } else {
                native_name(&path)
            }) || !regular(&path)?
            {
                continue;
            }
            rewrite_file(&path, directory, is_managed)?;
        }
    }
    Ok(())
}

/// One recognized regular file, separately testable at the publication boundary.
pub(super) fn rewrite_file(
    path: &Path,
    directory: &Path,
    is_managed: bool,
) -> Result<(), StorageError> {
    let format = encoding(path).ok_or_else(transform::invalid)?;
    let mut source = File::open(path)?;
    let (old, _) = digest(&mut source)?;
    let mut stage = NamedTempFile::new_in(directory)?;
    if !transform::remove_with_provenance(&mut source, &mut stage, format, Some(&old))? {
        return Ok(());
    }
    stage.as_file().sync_all()?;
    if is_managed {
        let (new, _) = digest(stage.as_file_mut())?;
        let extension = path.extension().ok_or(StorageError::InvalidPath)?;
        let destination = directory.join(&new).with_extension(extension);
        if !reusable_destination(&destination, &new)? {
            stage
                .persist_noclobber(&destination)
                .map_err(|error| StorageError::Io(error.error))?;
        }
        fs::remove_file(path)?;
    } else {
        stage
            .persist(path)
            .map_err(|error| StorageError::Io(error.error))?;
    }
    File::open(directory)?.sync_all()?;
    Ok(())
}

/// The durable file exists before this atomic identity/chunk replacement begins.
fn relink(
    connection: &mut Connection,
    old: &str,
    new: &str,
    destination: &Path,
    format: PevcapEncoding,
    size: u64,
) -> Result<(), StorageError> {
    let mut file = File::open(destination)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
    transaction.execute("UPDATE pevcap_imports SET artifact_digest=?1,artifact_path=?2,artifact_size=?3 WHERE artifact_digest=?4",params![new,destination.to_string_lossy(),size,old])?;
    transaction.execute(
        "UPDATE pevcap_recordings SET artifact_digest=?1 WHERE artifact_digest=?2",
        params![new, old],
    )?;
    transaction.execute(
        "DELETE FROM pevcap_captures WHERE artifact_digest=?1",
        [&old],
    )?;
    transaction.execute("INSERT INTO pevcap_captures(artifact_digest,encoding,artifact_size,written_bytes,next_sequence) VALUES(?1,?2,?3,?3,0)",params![new,if format==PevcapEncoding::Binary{"binary"}else{"jsonl"},size])?;
    let mut sequence = 0_i64;
    let mut bytes = vec![0; 65536];
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        transaction.execute(
            "INSERT INTO pevcap_capture_chunks(artifact_digest,sequence,payload) VALUES(?1,?2,?3)",
            params![new, sequence, &bytes[..n]],
        )?;
        sequence += 1;
    }
    transaction.execute("UPDATE pevcap_captures SET receipt_digest=artifact_digest,next_sequence=?1 WHERE artifact_digest=?2",params![sequence,new])?;
    transaction.commit()?;
    Ok(())
}
