//! Private diagnostic evidence. The row records the publication; the bytes stay in a file beside the database.
use super::{Actor, Store, begin_write, db_error, ensure_actor, json, now_ms};
use crate::Error;
use cutout_core::{
    CaptureSubmissionMetadata, CaptureSubmissionPayloadSummary, CaptureSubmissionReceipt,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const OWNER_BYTES: u64 = 1024 * 1024 * 1024;
const GLOBAL_BYTES: u64 = 10 * OWNER_BYTES;
const STAGING_PREFIX: &str = "cutout-capture-";
pub(crate) const DOWNLOAD_CHUNK: usize = 64 * 1024;

/// Owns temporary bytes until all detached blocking work finishes.
#[derive(Debug)]
pub(crate) struct StagedCapture {
    pub(crate) path: PathBuf,
}
impl Drop for StagedCapture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Removes a capture file unless publication commits.
struct PublishedCapture {
    path: PathBuf,
}

impl PublishedCapture {
    fn keep(mut self) {
        self.path.clear();
    }
}

impl Drop for PublishedCapture {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(super) fn objects_directory(database: &Path) -> PathBuf {
    let mut name = database.as_os_str().to_owned();
    name.push(".capture-objects");
    PathBuf::from(name)
}

fn object_name(operation: &str) -> Result<String, Error> {
    Uuid::parse_str(operation)
        .map(|id| id.to_string())
        .map_err(|_| Error::Invalid)
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewRecord {
    pub(crate) receipt: CaptureSubmissionReceipt,
    pub(crate) metadata: CaptureSubmissionMetadata,
    pub(crate) summary: CaptureSubmissionPayloadSummary,
}
#[derive(Debug, Serialize)]
pub(crate) struct ReviewPage {
    pub(crate) submissions: Vec<ReviewRecord>,
    pub(crate) next_cursor: Option<String>,
}

fn staging_directory(path: &Path) -> Result<(), Error> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(Error::Unavailable),
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| Error::Unavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Error::Unavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Error::Unavailable);
        }
    }
    Ok(())
}

impl Store {
    pub(crate) async fn capture_transfer_budget(&self, actor: Actor) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            transaction.commit().map_err(db_error)
        })
        .await
    }

    pub(crate) async fn capture_read_budget(&self, actor: Actor) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            transaction.commit().map_err(db_error)
        })
        .await
    }

    /// Call only after the single service listener binds, before accepting requests.
    pub(crate) async fn cleanup_capture_staging(&self) -> Result<(), Error> {
        let directory = self.capture_staging.clone();
        tokio::task::spawn_blocking(move || {
            staging_directory(&directory)?;
            for entry in fs::read_dir(directory.as_ref()).map_err(|_| Error::Unavailable)? {
                let entry = entry.map_err(|_| Error::Unavailable)?;
                let name = entry.file_name();
                if name
                    .to_str()
                    .and_then(|name| name.strip_prefix(STAGING_PREFIX))
                    .is_some_and(|suffix| Uuid::parse_str(suffix).is_ok())
                {
                    fs::remove_file(entry.path()).map_err(|_| Error::Unavailable)?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| Error::Unavailable)?
    }

    pub(crate) async fn stage_capture(&self) -> Result<(StagedCapture, tokio::fs::File), Error> {
        let directory = self.capture_staging.clone();
        let (stage, file) = tokio::task::spawn_blocking(move || {
            staging_directory(&directory)?;
            let path = directory.join(format!("{STAGING_PREFIX}{}", Uuid::new_v4()));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(&path).map_err(|_| Error::Unavailable)?;
            Ok::<_, Error>((StagedCapture { path }, file))
        })
        .await
        .map_err(|_| Error::Unavailable)??;
        Ok((stage, tokio::fs::File::from_std(file)))
    }

    /// Checks storage before body ingestion. Capture uploads are not rate limited.
    pub(crate) async fn admit_capture(
        &self,
        actor: Actor,
        operation: String,
        metadata: CaptureSubmissionMetadata,
    ) -> Result<(), Error> {
        self.call(move |connection| admit_capture_at(connection, &actor, &operation, &metadata))
            .await
    }

    pub(crate) fn capture_object_path(&self, operation: &str) -> Result<PathBuf, Error> {
        Ok(self.capture_objects.join(object_name(operation)?))
    }

    pub(crate) async fn commit_capture(
        &self,
        actor: Actor,
        operation: String,
        metadata: CaptureSubmissionMetadata,
        summary: CaptureSubmissionPayloadSummary,
        stage: StagedCapture,
    ) -> Result<(bool, CaptureSubmissionReceipt), Error> {
        let stage_path = stage.path.clone();
        let objects = self.capture_objects.clone();
        tokio::task::spawn_blocking(move || {
            File::open(&stage_path)
                .and_then(|file| file.sync_all())
                .map_err(|_| Error::Unavailable)
        })
        .await
        .map_err(|_| Error::Unavailable)??;
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            if let Some(receipt) = existing(&transaction, &actor, &operation, &metadata)? {
                return Ok((false, receipt));
            }
            check_storage(&transaction, &actor, metadata.bytes)?;
            if metadata.sha256 != summary.sha256 || metadata.bytes != summary.bytes {
                return Err(Error::CaptureCorrupt);
            }
            let object = objects.join(object_name(&operation)?);
            let published = publish_staged_capture(&stage.path, &object)?;
            let durable = File::open(&object)
                .and_then(|file| file.metadata())
                .map_err(|_| Error::Unavailable)?;
            if durable.len() != summary.bytes {
                return Err(Error::CaptureCorrupt);
            }
            let receipt = CaptureSubmissionReceipt {
                operation_id: operation.clone(),
                sha256: summary.sha256.clone(),
                bytes: summary.bytes,
                received_at_milliseconds: now_ms()?,
            };
            transaction
                .execute(
                    "INSERT INTO capture_submissions(operation_id,owner_id,metadata,receipt,summary,bytes)
                     VALUES(?1,?2,?3,?4,?5,?6)",
                    params![
                        operation,
                        actor.id,
                        json(&metadata)?,
                        json(&receipt)?,
                        json(&summary)?,
                        summary.bytes
                    ],
                )
                .map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
            published.keep();
            drop(stage);
            Ok((true, receipt))
        })
        .await
    }

    pub(crate) async fn capture_receipt(
        &self,
        actor: Actor,
        operation: String,
    ) -> Result<CaptureSubmissionReceipt, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            ensure_actor(&transaction, &actor)?;
            let stored = load(&transaction, &operation)?.ok_or(Error::NotFound)?;
            if stored.owner != actor.id || stored.deleted {
                return Err(Error::NotFound);
            }
            decode(stored.receipt.as_deref().ok_or(Error::Unavailable)?)
        })
        .await
    }

    pub(crate) async fn delete_capture(
        &self,
        actor: Actor,
        operation: String,
    ) -> Result<(), Error> {
        let path = self.capture_object_path(&operation)?;
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            if let Some(stored) = load(&transaction, &operation)? {
                if stored.owner != actor.id {
                    return Err(Error::NotFound);
                }
                if !stored.deleted {
                    transaction
                        .execute(
                            "UPDATE capture_submissions SET deleted=1,metadata=NULL,receipt=NULL,summary=NULL,bytes=0 WHERE operation_id=?1",
                            [&operation],
                        )
                        .map_err(db_error)?;
                    transaction.commit().map_err(db_error)?;
                }
            } else {
                transaction
                    .execute(
                        "INSERT INTO capture_submissions(operation_id,owner_id,deleted) VALUES(?1,?2,1)",
                        params![operation, actor.id],
                    )
                    .map_err(db_error)?;
                transaction.commit().map_err(db_error)?;
            }
            Ok(())
        })
        .await?;
        if let Err(error) = fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }

    pub(crate) async fn capture_content_metadata(
        &self,
        actor: Actor,
        operation: String,
    ) -> Result<CaptureSubmissionMetadata, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            authorize_content(&transaction, &actor, &operation)?;
            let metadata: String = transaction
                .query_row(
                    "SELECT metadata FROM capture_submissions WHERE operation_id=?1",
                    [operation],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            decode(&metadata)
        })
        .await
    }

    #[cfg(test)]
    pub(crate) async fn capture_chunk(
        &self,
        actor: Actor,
        operation: String,
        offset: u64,
    ) -> Result<Vec<u8>, Error> {
        let path = self.capture_object_path(&operation)?;
        let bytes = self
            .call(move |connection| {
                let transaction = connection.transaction().map_err(db_error)?;
                authorize_content(&transaction, &actor, &operation)?;
                transaction
                    .query_row(
                        "SELECT bytes FROM capture_submissions WHERE operation_id=?1",
                        [operation],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .await?;
        if offset >= bytes {
            return Ok(Vec::new());
        }
        tokio::task::spawn_blocking(move || read_object_chunk(&path, offset, bytes))
            .await
            .map_err(|_| Error::Unavailable)?
    }

    pub(crate) async fn review_captures(
        &self,
        actor: Actor,
        after: String,
        limit: u32,
    ) -> Result<ReviewPage, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            ensure_actor(&transaction, &actor)?;
            if !is_reviewer(&transaction, &actor)? {
                return Err(Error::Forbidden);
            }
            let limit_usize = usize::try_from(limit).map_err(|_| Error::Invalid)?;
            let mut statement = transaction
                .prepare(
                    "SELECT operation_id,receipt,metadata,summary FROM capture_submissions
                WHERE deleted=0 AND operation_id>?1 ORDER BY operation_id LIMIT ?2",
                )
                .map_err(db_error)?;
            let rows = statement
                .query_map(params![after, limit + 1], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(db_error)?;
            let mut submissions = Vec::new();
            let mut last = None;
            let mut next_cursor = None;
            for row in rows {
                let (operation, receipt, metadata, summary) = row.map_err(db_error)?;
                if submissions.len() >= limit_usize {
                    next_cursor = last;
                    break;
                }
                submissions.push(ReviewRecord {
                    receipt: decode(&receipt)?,
                    metadata: decode(&metadata)?,
                    summary: decode(&summary)?,
                });
                last = Some(operation);
            }
            Ok(ReviewPage {
                submissions,
                next_cursor,
            })
        })
        .await
    }
}

struct Stored {
    owner: String,
    metadata: Option<String>,
    receipt: Option<String>,
    deleted: bool,
}
fn admit_capture_at(
    connection: &mut Connection,
    actor: &Actor,
    operation: &str,
    metadata: &CaptureSubmissionMetadata,
) -> Result<(), Error> {
    let transaction = begin_write(connection)?;
    ensure_actor(&transaction, actor)?;
    if existing(&transaction, actor, operation, metadata)?.is_some() {
        return Ok(());
    }
    check_storage(&transaction, actor, metadata.bytes)?;
    transaction.commit().map_err(db_error)
}
fn load(connection: &Connection, operation: &str) -> Result<Option<Stored>, Error> {
    connection.query_row("SELECT owner_id,metadata,receipt,deleted FROM capture_submissions WHERE operation_id=?1",[operation],|row|
        Ok(Stored{owner:row.get(0)?,metadata:row.get(1)?,receipt:row.get(2)?,deleted:row.get(3)?})).optional().map_err(db_error)
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Error> {
    serde_json::from_str(value).map_err(|_| Error::Unavailable)
}
fn existing(
    connection: &Connection,
    actor: &Actor,
    operation: &str,
    metadata: &CaptureSubmissionMetadata,
) -> Result<Option<CaptureSubmissionReceipt>, Error> {
    let Some(stored) = load(connection, operation)? else {
        return Ok(None);
    };
    if stored.owner != actor.id {
        return Err(Error::NotFound);
    }
    if stored.deleted {
        return Err(Error::Conflict);
    }
    let previous: CaptureSubmissionMetadata =
        decode(stored.metadata.as_deref().ok_or(Error::Unavailable)?)?;
    if previous != *metadata {
        return Err(Error::Conflict);
    }
    decode(stored.receipt.as_deref().ok_or(Error::Unavailable)?).map(Some)
}
fn check_storage(connection: &Connection, actor: &Actor, additional: u64) -> Result<(), Error> {
    let (owner,total):(u64,u64)=connection.query_row("SELECT coalesce(sum(CASE WHEN owner_id=?1 THEN bytes ELSE 0 END),0),coalesce(sum(bytes),0) FROM capture_submissions",[&actor.id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(db_error)?;
    if owner.saturating_add(additional) > OWNER_BYTES
        || total.saturating_add(additional) > GLOBAL_BYTES
    {
        return Err(Error::CaptureStorageFull);
    }
    Ok(())
}
fn is_reviewer(connection: &Connection, actor: &Actor) -> Result<bool, Error> {
    connection
        .query_row(
            "SELECT private_capture_review FROM contributors WHERE id=?1",
            [&actor.id],
            |row| row.get(0),
        )
        .map_err(db_error)
}
pub(super) fn migrate_capture_files(
    connection: &mut Connection,
    database: &Path,
) -> Result<(), Error> {
    extract_capture_blobs(connection, &objects_directory(database))?;
    connection
        .execute_batch("PRAGMA secure_delete=ON;")
        .map_err(db_error)?;
    let transaction = connection.transaction().map_err(db_error)?;
    transaction
        .execute_batch(include_str!("../../migrations/007_capture_files.sql"))
        .map_err(db_error)?;
    transaction.commit().map_err(db_error)
}

fn extract_capture_blobs(connection: &mut Connection, directory: &Path) -> Result<(), Error> {
    staging_directory(directory)?;
    let rows = {
        let mut statement = connection
            .prepare(
                "SELECT operation_id, rowid, bytes FROM capture_submissions
                 WHERE deleted=0 AND payload IS NOT NULL",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, u64>(2)?,
                ))
            })
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?
    };
    for (operation, rowid, bytes) in rows {
        let name = Uuid::parse_str(&operation)
            .map_err(|_| Error::Unavailable)?
            .to_string();
        let destination = directory.join(&name);
        if destination.is_file()
            && fs::metadata(&destination)
                .map_err(|_| Error::Unavailable)?
                .len()
                == bytes
        {
            continue;
        }
        let partial = directory.join(format!("{name}.partial"));
        let _ = fs::remove_file(&partial);
        {
            let transaction = connection.transaction().map_err(db_error)?;
            let blob = transaction
                .blob_open(
                    rusqlite::MAIN_DB,
                    "capture_submissions",
                    "payload",
                    rowid,
                    true,
                )
                .map_err(db_error)?;
            write_blob(&blob, &partial, bytes)?;
        }
        fs::rename(&partial, &destination).map_err(|_| Error::Unavailable)?;
        sync_directory(directory)?;
    }
    Ok(())
}

fn write_blob(blob: &rusqlite::blob::Blob<'_>, path: &Path, bytes: u64) -> Result<(), Error> {
    let blob_len = u64::try_from(blob.len()).map_err(|_| Error::Unavailable)?;
    if blob_len != bytes {
        return Err(Error::Unavailable);
    }
    let mut file = create_private(path)?;
    let mut offset = 0_usize;
    let total = blob.len();
    while offset < total {
        let end = total.min(offset.saturating_add(DOWNLOAD_CHUNK));
        let mut chunk = vec![0; end - offset];
        blob.read_at_exact(&mut chunk, offset).map_err(db_error)?;
        file.write_all(&chunk).map_err(|_| Error::Unavailable)?;
        offset = end;
    }
    file.sync_all().map_err(|_| Error::Unavailable)?;
    Ok(())
}

fn create_private(path: &Path) -> Result<File, Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|_| Error::Unavailable)
}

pub(super) fn reconcile_capture_objects(
    connection: &Connection,
    directory: &Path,
) -> Result<(), Error> {
    staging_directory(directory)?;
    let mut statement = connection
        .prepare("SELECT operation_id FROM capture_submissions WHERE deleted=0")
        .map_err(db_error)?;
    let live = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(db_error)?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(db_error)?;
    drop(statement);
    for entry in fs::read_dir(directory).map_err(|_| Error::Unavailable)? {
        let entry = entry.map_err(|_| Error::Unavailable)?;
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        if name.ends_with(".partial")
            || (Uuid::parse_str(&name).is_ok() && !live.contains(name.as_str()))
        {
            fs::remove_file(entry.path()).map_err(|_| Error::Unavailable)?;
        }
    }
    Ok(())
}

pub(super) fn copy_capture_objects(source: &Path, output: &Path) -> Result<(), Error> {
    if !source.exists() {
        return Ok(());
    }
    let destination = objects_directory(&output.canonicalize().map_err(|_| Error::Unavailable)?);
    staging_directory(&destination)?;
    for entry in fs::read_dir(source).map_err(|_| Error::Unavailable)? {
        let entry = entry.map_err(|_| Error::Unavailable)?;
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        if Uuid::parse_str(&name).is_err() {
            continue;
        }
        let target = destination.join(&name);
        fs::copy(entry.path(), &target).map_err(|_| Error::Unavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o600))
                .map_err(|_| Error::Unavailable)?;
        }
        File::open(&target)
            .and_then(|file| file.sync_all())
            .map_err(|_| Error::Unavailable)?;
    }
    sync_directory(&destination)
}

pub(crate) fn remove_capture_files(directory: &Path, operations: &[String]) {
    for operation in operations {
        let Ok(name) = Uuid::parse_str(operation) else {
            continue;
        };
        if let Err(error) = fs::remove_file(directory.join(name.to_string()))
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "capture file remained after the row was deleted");
        }
    }
}

fn publish_staged_capture(stage: &Path, object: &Path) -> Result<PublishedCapture, Error> {
    let directory = object
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or(Error::Unavailable)?;
    staging_directory(directory)?;
    let published = PublishedCapture {
        path: object.to_path_buf(),
    };
    let _ = fs::remove_file(object);
    if fs::hard_link(stage, object).is_err() {
        fs::copy(stage, object).map_err(|_| Error::Unavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(object, fs::Permissions::from_mode(0o600))
                .map_err(|_| Error::Unavailable)?;
        }
    }
    File::open(object)
        .and_then(|file| file.sync_all())
        .map_err(|_| Error::Unavailable)?;
    sync_directory(directory)?;
    Ok(published)
}

fn sync_directory(directory: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    {
        File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| Error::Unavailable)?;
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
    }
    Ok(())
}

#[cfg(test)]
fn read_object_chunk(path: &Path, offset: u64, bytes: u64) -> Result<Vec<u8>, Error> {
    use std::io::{Read, Seek, SeekFrom};

    let remaining = usize::try_from(bytes - offset).map_err(|_| Error::Invalid)?;
    let mut chunk = vec![0; remaining.min(DOWNLOAD_CHUNK)];
    let mut file = File::open(path).map_err(|_| Error::Unavailable)?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| Error::Unavailable)?;
    file.read_exact(&mut chunk)
        .map_err(|_| Error::Unavailable)?;
    Ok(chunk)
}

fn authorize_content(connection: &Connection, actor: &Actor, operation: &str) -> Result<(), Error> {
    ensure_actor(connection, actor)?;
    let stored = load(connection, operation)?.ok_or(Error::NotFound)?;
    if stored.deleted || (stored.owner != actor.id && !is_reviewer(connection, actor)?) {
        return Err(Error::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Registration, Role};

    #[tokio::test]
    async fn capture_staging_allows_overlapping_uploads_and_cleans_partial_files() {
        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("server.sqlite")).expect("store");
        let (first, file) = store.stage_capture().await.expect("first");
        let (second, second_file) = store.stage_capture().await.expect("overlapping stage");
        let (third, third_file) = store.stage_capture().await.expect("third stage");
        let path = first.path.clone();
        let second_path = second.path.clone();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
                0o600
            );
        }
        drop(file);
        drop(first);
        assert!(!path.exists());
        assert!(second_path.exists());
        drop((second_file, second, third_file, third));
        assert_eq!(
            fs::read_dir(store.capture_staging.as_ref())
                .expect("staging directory")
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn capture_attempt_budget_is_durable_and_reviewer_capability_is_independent() {
        let directory = tempfile::TempDir::new().expect("directory");
        let path = directory.path().join("server.sqlite");
        let store = Store::open(&path).expect("store");
        let identity = store
            .identity(Registration::Operator {
                role: Role::Moderator,
            })
            .await
            .expect("moderator");
        let actor = store.authenticate(&identity.token).await.expect("actor");
        let check_actor = actor.clone();
        store
            .call(move |connection| {
                assert!(!is_reviewer(connection, &check_actor)?);
                assert_eq!(
                    check_storage(connection, &check_actor, OWNER_BYTES + 1),
                    Err(Error::CaptureStorageFull)
                );
                Ok(())
            })
            .await
            .expect("access/storage checks");
        let reviewer = store
            .operator_capture_reviewer(Role::Contributor)
            .await
            .expect("reviewer");
        let reviewer = store
            .authenticate(&reviewer.token)
            .await
            .expect("reviewer actor");
        assert_eq!(reviewer.role, Role::Contributor);
        store
            .call(move |connection| {
                assert!(is_reviewer(connection, &reviewer)?);
                Ok(())
            })
            .await
            .expect("review capability");
        let metadata = CaptureSubmissionMetadata {
            evidence: None,
            sha256: "a".repeat(64),
            bytes: 42,
            encoding: cutout_core::PevcapEncoding::Jsonl,
            origin: cutout_core::CaptureOrigin::Automatic.into(),
            integrity: cutout_core::CaptureSubmissionIntegrity::Complete,
            provenance: cutout_core::CaptureSubmissionProvenance {
                source_artifact_id: Uuid::new_v4().to_string(),
                live_capture_id: None,
                source_sha256: Some("a".repeat(64)),
                transformation: cutout_core::CaptureSubmissionTransformation::Unmodified,
            },
        };
        for _ in 0..3 {
            let actor = actor.clone();
            let metadata = metadata.clone();
            store
                .call(move |connection| {
                    admit_capture_at(connection, &actor, &Uuid::new_v4().to_string(), &metadata)
                })
                .await
                .expect("attempt");
        }
        drop(store);
        let store = Store::open(&path).expect("reopen");
        let stale_actor = actor.clone();
        let metadata_for_reopen = metadata.clone();
        store
            .call(move |connection| {
                admit_capture_at(
                    connection,
                    &stale_actor,
                    &Uuid::new_v4().to_string(),
                    &metadata_for_reopen,
                )?;
                let counted: i64 = connection
                    .query_row(
                        "SELECT count(*) FROM quotas WHERE principal LIKE 'capture-%'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(db_error)?;
                assert_eq!(counted, 0, "capture uploads and reads are not rate limited");
                Ok(())
            })
            .await
            .expect("reopened attempt");
        store.revoke(actor.id.clone()).await.expect("revoke");
        assert_eq!(
            store
                .delete_capture(actor, Uuid::new_v4().to_string())
                .await,
            Err(Error::Unauthorized)
        );
    }

    #[tokio::test]
    async fn capture_restart_cleanup_removes_only_owned_staging_names() {
        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("server.sqlite")).expect("store");
        let (stage, file) = store.stage_capture().await.expect("stage");
        let staging = stage
            .path
            .parent()
            .expect("staging directory")
            .to_path_buf();
        drop(file);
        drop(stage);
        let abandoned = staging.join(format!("{STAGING_PREFIX}{}", Uuid::new_v4()));
        let unrelated = staging.join("preserve-this-file");
        fs::write(&abandoned, b"partial capture").expect("abandoned transfer");
        fs::write(&unrelated, b"unrelated").expect("unrelated file");
        store
            .cleanup_capture_staging()
            .await
            .expect("startup cleanup");
        assert!(!abandoned.exists());
        assert_eq!(fs::read(unrelated).expect("preserved file"), b"unrelated");
    }

    #[tokio::test]
    async fn capture_bytes_stay_in_a_file_and_leave_with_the_account() {
        use tokio::io::AsyncWriteExt;

        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("server.sqlite")).expect("store");
        let identity = store
            .identity(Registration::Operator {
                role: Role::Contributor,
            })
            .await
            .expect("identity");
        let actor = store.authenticate(&identity.token).await.expect("actor");
        let body = b"file-backed capture";
        let digest = crate::store::hash(body);
        let metadata = CaptureSubmissionMetadata {
            evidence: None,
            sha256: digest.clone(),
            bytes: u64::try_from(body.len()).expect("size"),
            encoding: cutout_core::PevcapEncoding::Jsonl,
            origin: cutout_core::CaptureOrigin::Manual.into(),
            integrity: cutout_core::CaptureSubmissionIntegrity::Complete,
            provenance: cutout_core::CaptureSubmissionProvenance {
                source_artifact_id: Uuid::new_v4().to_string(),
                live_capture_id: None,
                source_sha256: Some(digest.clone()),
                transformation: cutout_core::CaptureSubmissionTransformation::Unmodified,
            },
        };
        let summary = CaptureSubmissionPayloadSummary {
            sha256: digest,
            bytes: metadata.bytes,
            events: 0,
            annotation_count: 0,
            categories: cutout_core::CaptureSubmissionCategories::default(),
        };
        let (stage, mut file) = store.stage_capture().await.expect("stage");
        file.write_all(body).await.expect("stage bytes");
        drop(file);
        let operation = Uuid::new_v4().to_string();
        let (created, _) = store
            .commit_capture(actor, operation.clone(), metadata, summary, stage)
            .await
            .expect("commit");
        assert!(created);
        let path = store.capture_object_path(&operation).expect("object path");
        assert_eq!(fs::read(&path).expect("object"), body);
        let payload_column: u32 = store
            .call(|connection| {
                connection
                    .query_row(
                        "SELECT count(*) FROM pragma_table_info('capture_submissions') WHERE name='payload'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .await
            .expect("schema");
        assert_eq!(payload_column, 0);
        let chunk = store
            .capture_chunk(
                store.authenticate(&identity.token).await.expect("reader"),
                operation.clone(),
                0,
            )
            .await
            .expect("file chunk");
        assert_eq!(chunk, body);
        let backup = directory.path().join("backup.sqlite");
        store.backup(backup.clone()).await.expect("backup");
        store
            .delete_current(identity.token)
            .await
            .expect("delete account");
        assert!(!path.exists(), "account deletion removes the capture file");
        let restored = Store::open(&backup).expect("restored snapshot");
        let restored_path = restored
            .capture_object_path(&operation)
            .expect("restored object");
        assert_eq!(fs::read(restored_path).expect("backed up bytes"), body);
    }
}
