//! Private diagnostic evidence, with one atomic SQLite publication boundary.
use super::{Actor, QuotaWindow, Store, begin_write, db_error, ensure_actor, json, now_ms, quota};
use crate::Error;
use cutout_core::{
    CaptureSubmissionMetadata, CaptureSubmissionPayloadSummary, CaptureSubmissionReceipt,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

const OWNER_BYTES: u64 = 1024 * 1024 * 1024;
const GLOBAL_BYTES: u64 = 10 * OWNER_BYTES;
const STAGING_PREFIX: &str = "cutout-capture-";
pub(crate) const DOWNLOAD_CHUNK: usize = 64 * 1024;

/// One actor and one global slot, retired together by their sole owner.
#[derive(Debug)]
pub(crate) struct CaptureTransferLease {
    actor_id: String,
    actors: Arc<Mutex<HashSet<String>>>,
    global: Option<OwnedSemaphorePermit>,
}

impl Drop for CaptureTransferLease {
    fn drop(&mut self) {
        let mut actors = match self.actors.lock() {
            Ok(actors) => actors,
            Err(poisoned) => poisoned.into_inner(),
        };
        actors.remove(&self.actor_id);
        // Retire both slots before another transfer for this actor can enter.
        drop(self.global.take());
    }
}

/// Owns both temporary bytes and admission until all detached blocking work finishes.
#[derive(Debug)]
pub(crate) struct StagedCapture {
    pub(crate) path: PathBuf,
    _permit: CaptureTransferLease,
}
impl Drop for StagedCapture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
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
    pub(crate) fn capture_transfer(&self, actor: &Actor) -> Result<CaptureTransferLease, Error> {
        let mut actors = self.capture_actors.lock().map_err(|_| Error::Unavailable)?;
        if actors.contains(&actor.id) {
            return Err(Error::Throttled);
        }
        let global = self
            .capture_uploads
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Throttled)?;
        actors.insert(actor.id.clone());
        Ok(CaptureTransferLease {
            actor_id: actor.id.clone(),
            actors: self.capture_actors.clone(),
            global: Some(global),
        })
    }

    pub(crate) async fn capture_transfer_budget(&self, actor: Actor) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let now = now_ms()?;
            let principal = format!("capture-request:{}", actor.id);
            quota(&transaction, &principal, QuotaWindow::Minute, 6, now)?;
            quota(&transaction, &principal, QuotaWindow::Hour, 60, now)?;
            quota(
                &transaction,
                "capture-request-global",
                QuotaWindow::Minute,
                30,
                now,
            )?;
            transaction.commit().map_err(db_error)
        })
        .await
    }

    pub(crate) async fn capture_read_budget(&self, actor: Actor) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let now = now_ms()?;
            let principal = format!("capture-read:{}", actor.id);
            quota(&transaction, &principal, QuotaWindow::Minute, 60, now)?;
            quota(&transaction, &principal, QuotaWindow::Hour, 600, now)?;
            quota(
                &transaction,
                "capture-read-global",
                QuotaWindow::Minute,
                600,
                now,
            )?;
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

    pub(crate) async fn stage_capture(
        &self,
        actor: &Actor,
    ) -> Result<(StagedCapture, tokio::fs::File), Error> {
        let permit = self.capture_transfer(actor)?;
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
            Ok::<_, Error>((
                StagedCapture {
                    path,
                    _permit: permit,
                },
                file,
            ))
        })
        .await
        .map_err(|_| Error::Unavailable)??;
        Ok((stage, tokio::fs::File::from_std(file)))
    }

    /// Budget attempts before body ingestion; failed transfers still consume their budget.
    pub(crate) async fn admit_capture(
        &self,
        actor: Actor,
        operation: String,
        metadata: CaptureSubmissionMetadata,
    ) -> Result<(), Error> {
        self.call(move |connection| {
            admit_capture_at(connection, &actor, &operation, &metadata, now_ms()?)
        })
        .await
    }

    pub(crate) async fn commit_capture(
        &self,
        actor: Actor,
        operation: String,
        metadata: CaptureSubmissionMetadata,
        summary: CaptureSubmissionPayloadSummary,
        stage: StagedCapture,
    ) -> Result<(bool, CaptureSubmissionReceipt), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            if let Some(receipt) = existing(&transaction, &actor, &operation, &metadata)? { return Ok((false, receipt)); }
            check_storage(&transaction, &actor, metadata.bytes)?;
            if metadata.sha256 != summary.sha256 || metadata.bytes != summary.bytes { return Err(Error::CaptureCorrupt); }
            let receipt = CaptureSubmissionReceipt {operation_id:operation.clone(), sha256:summary.sha256.clone(), bytes:summary.bytes,
                received_at_milliseconds:now_ms()?};
            transaction.execute("INSERT INTO capture_submissions(operation_id,owner_id,metadata,receipt,summary,bytes,payload)
                VALUES(?1,?2,?3,?4,?5,?6,zeroblob(?6))",
                params![operation,actor.id,json(&metadata)?,json(&receipt)?,json(&summary)?,summary.bytes]).map_err(db_error)?;
            let row = transaction.last_insert_rowid();
            {
                let mut blob = transaction.blob_open(rusqlite::MAIN_DB, "capture_submissions", "payload", row, false).map_err(db_error)?;
                let mut file = File::open(&stage.path).map_err(|_| Error::Unavailable)?;
                let copied = std::io::copy(&mut file, &mut blob).map_err(|_| Error::Unavailable)?;
                if copied != summary.bytes { return Err(Error::CaptureCorrupt); }
                blob.close().map_err(db_error)?;
            }
            transaction.commit().map_err(db_error)?;
            drop(stage);
            Ok((true, receipt))
        }).await
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
        self.call(move |connection| {
            let transaction=begin_write(connection)?;
            ensure_actor(&transaction,&actor)?;
            if let Some(stored)=load(&transaction,&operation)? {
                if stored.owner != actor.id { return Err(Error::NotFound); }
                if stored.deleted { return Ok(()); }
                transaction.execute("UPDATE capture_submissions SET deleted=1,metadata=NULL,receipt=NULL,summary=NULL,bytes=0,payload=NULL WHERE operation_id=?1",[&operation]).map_err(db_error)?;
            } else {
                let now=now_ms()?;
                quota(&transaction,&format!("capture-cancel:{}",actor.id),QuotaWindow::Minute,3,now)?;
                quota(&transaction,&format!("capture-cancel:{}",actor.id),QuotaWindow::Hour,30,now)?;
                transaction.execute("INSERT INTO capture_submissions(operation_id,owner_id,deleted) VALUES(?1,?2,1)",params![operation,actor.id]).map_err(db_error)?;
            }
            transaction.commit().map_err(db_error)
        }).await
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

    pub(crate) async fn capture_chunk(
        &self,
        actor: Actor,
        operation: String,
        offset: u64,
    ) -> Result<Vec<u8>, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            authorize_content(&transaction, &actor, &operation)?;
            let rowid = transaction
                .query_row(
                    "SELECT rowid FROM capture_submissions WHERE operation_id=?1",
                    [operation],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            let blob = transaction
                .blob_open(
                    rusqlite::MAIN_DB,
                    "capture_submissions",
                    "payload",
                    rowid,
                    true,
                )
                .map_err(db_error)?;
            let position = usize::try_from(offset).map_err(|_| Error::Invalid)?;
            let mut chunk = vec![0; blob.len().saturating_sub(position).min(DOWNLOAD_CHUNK)];
            if !chunk.is_empty() {
                blob.read_at_exact(&mut chunk, position).map_err(db_error)?;
            }
            blob.close().map_err(db_error)?;
            Ok(chunk)
        })
        .await
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
    now: u64,
) -> Result<(), Error> {
    let transaction = begin_write(connection)?;
    ensure_actor(&transaction, actor)?;
    if existing(&transaction, actor, operation, metadata)?.is_some() {
        return Ok(());
    }
    check_storage(&transaction, actor, metadata.bytes)?;
    let principal = format!("capture-upload:{}", actor.id);
    quota(&transaction, &principal, QuotaWindow::Minute, 3, now)?;
    quota(&transaction, &principal, QuotaWindow::Hour, 30, now)?;
    quota(
        &transaction,
        "capture-upload-global",
        QuotaWindow::Minute,
        30,
        now,
    )?;
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

    async fn contributor(store: &Store) -> Actor {
        let identity = store
            .identity(Registration::Operator {
                role: Role::Contributor,
            })
            .await
            .expect("contributor");
        store.authenticate(&identity.token).await.expect("actor")
    }

    #[tokio::test]
    async fn capture_staging_leases_bound_uploads_and_cleanup_partial_files() {
        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("server.sqlite")).expect("store");
        let first_actor = contributor(&store).await;
        let second_actor = contributor(&store).await;
        let third_actor = contributor(&store).await;
        let (first, file) = store.stage_capture(&first_actor).await.expect("first");
        assert_eq!(
            store.stage_capture(&first_actor).await.err(),
            Some(Error::Throttled)
        );
        let (second, second_file) = store.stage_capture(&second_actor).await.expect("second");
        assert_eq!(
            store.stage_capture(&third_actor).await.err(),
            Some(Error::Throttled)
        );
        let path = first.path.clone();
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
        let (replacement, replacement_file) = store
            .stage_capture(&first_actor)
            .await
            .expect("released lease");
        drop((second_file, second, replacement_file, replacement));
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
                    admit_capture_at(
                        connection,
                        &actor,
                        &Uuid::new_v4().to_string(),
                        &metadata,
                        1_000,
                    )
                })
                .await
                .expect("attempt");
        }
        drop(store);
        let store = Store::open(&path).expect("reopen");
        let stale_actor = actor.clone();
        assert_eq!(
            store
                .call(move |connection| admit_capture_at(
                    connection,
                    &stale_actor,
                    &Uuid::new_v4().to_string(),
                    &metadata,
                    1_001
                ))
                .await,
            Err(Error::Throttled)
        );
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
        let actor = contributor(&store).await;
        let (stage, file) = store.stage_capture(&actor).await.expect("stage");
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
}
