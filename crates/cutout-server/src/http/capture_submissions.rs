//! Explicit private diagnostic transfer. No capture replay or transport execution.
use super::actor;
use crate::{
    Error, Store,
    model::id,
    store::capture_submissions::{CaptureTransferLease, ReviewPage},
};
use axum::{
    Json, Router,
    body::{Body, Bytes, HttpBody},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use cutout_core::{
    CaptureSubmissionMetadata, CaptureSubmissionReceipt, CaptureSubmissionValidationError,
    MAX_CAPTURE_SUBMISSION_BYTES, PevcapEncoding, validate_capture_submission_payload,
};
use serde::Deserialize;
use std::{
    future::poll_fn,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::AsyncWriteExt;

/// Retains the shared transfer slot even when a response body is not polled.
/// The independent watchdog retires stalled bodies; Drop retires abandoned ones.
#[derive(Debug)]
struct DownloadLease {
    permit: Arc<Mutex<Option<CaptureTransferLease>>>,
    expires: tokio::time::Instant,
    watchdog: tokio::task::JoinHandle<()>,
}

impl DownloadLease {
    fn new(permit: CaptureTransferLease) -> Self {
        let permit = Arc::new(Mutex::new(Some(permit)));
        let expires = tokio::time::Instant::now() + Duration::from_secs(600);
        let retained = permit.clone();
        let watchdog = tokio::spawn(async move {
            tokio::time::sleep_until(expires).await;
            release_download_permit(&retained);
        });
        Self {
            permit,
            expires,
            watchdog,
        }
    }
}

impl Drop for DownloadLease {
    fn drop(&mut self) {
        self.watchdog.abort();
        release_download_permit(&self.permit);
    }
}

fn release_download_permit(permit: &Mutex<Option<CaptureTransferLease>>) {
    let mut slot = match permit.lock() {
        Ok(slot) => slot,
        Err(poisoned) => poisoned.into_inner(),
    };
    drop(slot.take());
}

pub(super) fn router() -> Router<Store> {
    Router::new()
        .route("/v1/capture-submissions", get(list))
        .route(
            "/v1/capture-submissions/{operation}",
            get(receipt).put(upload).delete(delete),
        )
        .route("/v1/capture-submissions/{operation}/content", get(content))
        .layer(axum::middleware::from_fn(deadline))
}

async fn deadline(request: Request, next: axum::middleware::Next) -> Response {
    let mut response = match tokio::time::timeout(Duration::from_secs(600), next.run(request)).await
    {
        Ok(response) => response,
        Err(_) => Error::Unavailable.into_response(),
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

fn validation(error: CaptureSubmissionValidationError) -> Error {
    match error {
        CaptureSubmissionValidationError::InvalidMetadata => Error::Invalid,
        CaptureSubmissionValidationError::InvalidPevcap => Error::CaptureInvalid,
        CaptureSubmissionValidationError::UnsupportedFormat => Error::CaptureUnsupported,
        CaptureSubmissionValidationError::MusicPresent => Error::CaptureMusic,
        CaptureSubmissionValidationError::TooLarge
        | CaptureSubmissionValidationError::TooManyEvents
        | CaptureSubmissionValidationError::DurationTooLong => Error::CaptureTooLarge,
        CaptureSubmissionValidationError::ReadFailure => Error::Unavailable,
    }
}

fn metadata(headers: &HeaderMap) -> Result<CaptureSubmissionMetadata, Error> {
    let mut values = headers.get_all("x-cutout-capture").iter();
    let value = values.next().ok_or(Error::Invalid)?;
    if values.next().is_some() || value.as_bytes().len() > 4096 || !value.as_bytes().is_ascii() {
        return Err(Error::Invalid);
    }
    let metadata: CaptureSubmissionMetadata =
        serde_json::from_slice(value.as_bytes()).map_err(|_| Error::Invalid)?;
    metadata.validate().map_err(validation)?;
    let mut content_types = headers.get_all(header::CONTENT_TYPE).iter();
    let content_type = content_types
        .next()
        .ok_or(Error::Invalid)?
        .to_str()
        .map_err(|_| Error::Invalid)?;
    let expected = if metadata.evidence.is_some() {
        "application/vnd.cutout.capture-evidence"
    } else {
        match metadata.encoding {
            PevcapEncoding::Jsonl => "application/x-ndjson",
            PevcapEncoding::Binary => "application/octet-stream",
        }
    };
    if content_types.next().is_some()
        || !content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case(expected)
    {
        return Err(Error::Invalid);
    }
    if let Some(length) = headers.get(header::CONTENT_LENGTH) {
        let length: u64 = length
            .to_str()
            .map_err(|_| Error::Invalid)?
            .parse()
            .map_err(|_| Error::Invalid)?;
        if length > MAX_CAPTURE_SUBMISSION_BYTES {
            return Err(Error::CaptureTooLarge);
        }
        if length != metadata.bytes {
            return Err(Error::CaptureCorrupt);
        }
    }
    Ok(metadata)
}

async fn upload(
    State(store): State<Store>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    mut body: Body,
) -> Result<(StatusCode, Json<CaptureSubmissionReceipt>), Error> {
    let actor = actor(&store, &headers).await?;
    let operation = id(&operation)?;
    let metadata = metadata(&headers)?;
    store.capture_transfer_budget(actor.clone()).await?;
    store
        .admit_capture(actor.clone(), operation.clone(), metadata.clone())
        .await?;
    let (stage, mut file) = store.stage_capture(&actor).await?;
    let mut bytes = 0_u64;
    while let Some(frame) = tokio::time::timeout(
        Duration::from_secs(30),
        poll_fn(|context| Pin::new(&mut body).poll_frame(context)),
    )
    .await
    .map_err(|_| Error::Unavailable)?
    {
        let frame = frame.map_err(|_| Error::CaptureCorrupt)?;
        if let Ok(data) = frame.into_data() {
            bytes = bytes
                .checked_add(data.len() as u64)
                .ok_or(Error::CaptureTooLarge)?;
            if bytes > MAX_CAPTURE_SUBMISSION_BYTES {
                return Err(Error::CaptureTooLarge);
            }
            if bytes > metadata.bytes {
                return Err(Error::CaptureCorrupt);
            }
            file.write_all(&data)
                .await
                .map_err(|_| Error::Unavailable)?;
        }
    }
    if bytes != metadata.bytes {
        return Err(Error::CaptureCorrupt);
    }
    file.flush().await.map_err(|_| Error::Unavailable)?;
    drop(file);
    let validated_metadata = metadata.clone();
    let (stage, summary) = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&stage.path).map_err(|_| Error::Unavailable)?;
        let summary =
            validate_capture_submission_payload(file, &validated_metadata).map_err(validation)?;
        Ok::<_, Error>((stage, summary))
    })
    .await
    .map_err(|_| Error::Unavailable)??;
    if summary.sha256 != metadata.sha256 || summary.bytes != metadata.bytes {
        return Err(Error::CaptureCorrupt);
    }
    let (created, receipt) = store
        .commit_capture(actor, operation, metadata, summary, stage)
        .await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(receipt),
    ))
}

async fn receipt(
    State(store): State<Store>,
    Path(operation): Path<String>,
    headers: HeaderMap,
) -> Result<Json<CaptureSubmissionReceipt>, Error> {
    let actor = actor(&store, &headers).await?;
    store.capture_read_budget(actor.clone()).await?;
    Ok(Json(store.capture_receipt(actor, id(&operation)?).await?))
}
async fn delete(
    State(store): State<Store>,
    Path(operation): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, Error> {
    let actor = actor(&store, &headers).await?;
    store.capture_transfer_budget(actor.clone()).await?;
    store.delete_capture(actor, id(&operation)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewQuery {
    after: Option<String>,
    limit: Option<u32>,
}
async fn list(
    State(store): State<Store>,
    headers: HeaderMap,
    query: Result<Query<ReviewQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<ReviewPage>, Error> {
    let actor = actor(&store, &headers).await?;
    let query = query.map_err(|_| Error::Invalid)?.0;
    store.capture_read_budget(actor.clone()).await?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(Error::Invalid);
    }
    let after = query
        .after
        .map(|value| id(&value))
        .transpose()?
        .unwrap_or_default();
    Ok(Json(store.review_captures(actor, after, limit).await?))
}

async fn content(
    State(store): State<Store>,
    Path(operation): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Error> {
    let actor = actor(&store, &headers).await?;
    let operation = id(&operation)?;
    store.capture_read_budget(actor.clone()).await?;
    let metadata = store
        .capture_content_metadata(actor.clone(), operation.clone())
        .await?;
    let total = metadata.bytes;
    let (content_type, disposition) = if metadata.evidence.is_some() {
        (
            "application/vnd.cutout.capture-evidence",
            "attachment; filename=\"bluetooth-capture.capture-evidence\"",
        )
    } else {
        (
            "application/octet-stream",
            "attachment; filename=\"bluetooth-capture.pevcap\"",
        )
    };
    let permit = store.capture_transfer(&actor)?;
    let lease = DownloadLease::new(permit);
    let stream = futures_util::stream::unfold(
        Some((store, actor, operation, 0_u64, lease)),
        move |state| async move {
            let (store, actor, operation, offset, lease) = state?;
            if offset >= total {
                return None;
            }
            if tokio::time::Instant::now() >= lease.expires {
                return Some((Err(Error::Unavailable), None));
            }
            match tokio::time::timeout_at(
                lease.expires,
                store.capture_chunk(actor.clone(), operation.clone(), offset),
            )
            .await
            {
                Ok(Ok(chunk))
                    if !chunk.is_empty() && tokio::time::Instant::now() < lease.expires =>
                {
                    let next = offset + chunk.len() as u64;
                    Some((
                        Ok::<Bytes, Error>(Bytes::from(chunk)),
                        Some((store, actor, operation, next, lease)),
                    ))
                }
                Ok(Err(error)) => Some((Err(error), None)),
                Ok(Ok(_)) | Err(_) => Some((Err(Error::Unavailable), None)),
            }
        },
    );
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Registration, Role};
    use cutout_core::{
        CaptureOrigin, CaptureSubmissionIntegrity, CaptureSubmissionProvenance,
        CaptureSubmissionTransformation,
    };
    use uuid::Uuid;

    async fn stored_capture() -> (tempfile::TempDir, Store, HeaderMap, String, Vec<u8>) {
        stored_capture_bytes(
            include_bytes!("../../../cutout-cli/fixtures/pevcap/falcon-begode-banner.jsonl")
                .to_vec(),
        )
        .await
    }

    async fn stored_capture_bytes(
        bytes: Vec<u8>,
    ) -> (tempfile::TempDir, Store, HeaderMap, String, Vec<u8>) {
        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("server.sqlite")).expect("store");
        let identity = store
            .identity(Registration::Operator {
                role: Role::Contributor,
            })
            .await
            .expect("identity");
        let digest = crate::store::hash(&bytes);
        let metadata = CaptureSubmissionMetadata {
            evidence: None,
            sha256: digest.clone(),
            bytes: u64::try_from(bytes.len()).expect("size"),
            encoding: PevcapEncoding::Jsonl,
            origin: CaptureOrigin::Manual,
            integrity: CaptureSubmissionIntegrity::Complete,
            provenance: CaptureSubmissionProvenance {
                source_artifact_id: Uuid::new_v4().to_string(),
                live_capture_id: None,
                source_sha256: Some(digest),
                transformation: CaptureSubmissionTransformation::Unmodified,
            },
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", identity.token)
                .parse()
                .expect("bearer"),
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/x-ndjson"),
        );
        headers.insert(
            "x-cutout-capture",
            serde_json::to_string(&metadata)
                .expect("metadata")
                .parse()
                .expect("header"),
        );
        let operation = Uuid::new_v4().to_string();
        let (status, _) = upload(
            State(store.clone()),
            Path(operation.clone()),
            headers.clone(),
            Body::from(bytes.clone()),
        )
        .await
        .expect("upload");
        assert_eq!(status, StatusCode::CREATED);
        (directory, store, headers, operation, bytes)
    }

    async fn reviewer_headers(store: &Store, template: &HeaderMap) -> HeaderMap {
        let reviewer = store
            .operator_capture_reviewer(Role::Contributor)
            .await
            .expect("reviewer");
        let mut headers = template.clone();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", reviewer.token)
                .parse()
                .expect("reviewer bearer"),
        );
        headers
    }

    #[tokio::test]
    async fn private_note_and_four_screenshots_commit_and_read_back_atomically() {
        use cutout_core::{CaptureAttachments, CaptureEvidenceManifest, CaptureScreenshot};
        let (_directory, store, mut headers, _original, raw) = stored_capture().await;
        let evidence = CaptureAttachments {
            note: "Stationary by the workbench — é".into(),
            screenshots: vec![
                CaptureScreenshot {
                    jpeg: vec![0xff, 0xd8, 0xff, 0xd9]
                };
                4
            ],
        };
        let mut body = raw.clone();
        body.extend(serde_json::to_vec(&evidence).unwrap());
        let mut metadata: CaptureSubmissionMetadata =
            serde_json::from_slice(headers["x-cutout-capture"].as_bytes()).unwrap();
        metadata.evidence = Some(
            CaptureEvidenceManifest::new(crate::store::hash(&raw), raw.len() as u64, &evidence)
                .unwrap(),
        );
        metadata.sha256 = crate::store::hash(&body);
        metadata.bytes = body.len() as u64;
        headers.insert(
            "x-cutout-capture",
            serde_json::to_string(&metadata).unwrap().parse().unwrap(),
        );
        headers.insert(
            header::CONTENT_TYPE,
            "application/vnd.cutout.capture-evidence".parse().unwrap(),
        );
        let operation = Uuid::new_v4().to_string();
        let (status, Json(received)) = upload(
            State(store.clone()),
            Path(operation.clone()),
            headers.clone(),
            Body::from(body.clone()),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(received.sha256, metadata.sha256);
        let reviewer = reviewer_headers(&store, &headers).await;
        let response = content(State(store.clone()), Path(operation.clone()), reviewer)
            .await
            .unwrap();
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/vnd.cutout.capture-evidence"
        );
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"bluetooth-capture.capture-evidence\""
        );
        let downloaded = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        assert_eq!(downloaded.as_ref(), body);
        let readback: CaptureAttachments =
            serde_json::from_slice(&downloaded[raw.len()..]).unwrap();
        assert_eq!(readback, evidence);
        let failed = Uuid::new_v4().to_string();
        let mut incomplete = body.clone();
        incomplete.pop();
        assert!(
            upload(
                State(store.clone()),
                Path(failed.clone()),
                headers.clone(),
                Body::from(incomplete)
            )
            .await
            .is_err()
        );
        assert!(
            receipt(State(store.clone()), Path(failed), headers.clone())
                .await
                .is_err()
        );
        delete(
            State(store.clone()),
            Path(operation.clone()),
            headers.clone(),
        )
        .await
        .unwrap();
        assert!(
            content(State(store), Path(operation), headers)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn private_downloads_share_transfer_slots_and_release_on_body_drop_or_eof() {
        let (_directory, store, headers, operation, bytes) = stored_capture().await;
        let third_headers = reviewer_headers(&store, &headers).await;
        let third_actor = actor(&store, &third_headers).await.expect("third actor");
        let first = content(
            State(store.clone()),
            Path(operation.clone()),
            headers.clone(),
        )
        .await
        .expect("first download");
        assert_eq!(
            first.headers()[header::CONTENT_TYPE],
            "application/octet-stream"
        );
        assert_eq!(
            first.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"bluetooth-capture.pevcap\""
        );
        assert_eq!(
            content(
                State(store.clone()),
                Path(operation.clone()),
                headers.clone()
            )
            .await
            .err(),
            Some(Error::Throttled),
            "one actor cannot occupy both global transfer slots"
        );
        let reviewer = reviewer_headers(&store, &headers).await;
        let second = content(State(store.clone()), Path(operation.clone()), reviewer)
            .await
            .expect("second download");
        assert_eq!(store.capture_uploads.available_permits(), 0);
        assert_eq!(
            content(
                State(store.clone()),
                Path(operation.clone()),
                headers.clone()
            )
            .await
            .err(),
            Some(Error::Throttled)
        );
        assert_eq!(
            store.stage_capture(&third_actor).await.err(),
            Some(Error::Throttled),
            "downloads also bound upload admission"
        );
        drop(first);
        assert_eq!(store.capture_uploads.available_permits(), 1);
        let replacement = content(
            State(store.clone()),
            Path(operation.clone()),
            headers.clone(),
        )
        .await
        .expect("released transfer slot");
        let actual = axum::body::to_bytes(replacement.into_body(), bytes.len())
            .await
            .expect("complete body");
        assert_eq!(actual.as_ref(), bytes);
        assert_eq!(
            store.capture_uploads.available_permits(),
            1,
            "EOF releases its slot"
        );
        drop(second);
        assert_eq!(store.capture_uploads.available_permits(), 2);
        drop(
            content(State(store.clone()), Path(operation), headers)
                .await
                .expect("EOF releases the actor lease"),
        );
    }

    #[tokio::test]
    async fn private_download_unpolled_body_expires_and_cannot_resume_after_deadline() {
        let (_directory, store, headers, operation, _) = stored_capture().await;
        let owner = actor(&store, &headers).await.expect("owner");
        let response = content(
            State(store.clone()),
            Path(operation.clone()),
            headers.clone(),
        )
        .await
        .expect("download");
        assert_eq!(store.capture_uploads.available_permits(), 1);
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(601)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            store.capture_uploads.available_permits(),
            2,
            "unpolled response cannot retain a transfer slot forever"
        );
        let replacement = store
            .capture_transfer(&owner)
            .expect("deadline releases the actor lease");
        let mut body = response.into_body();
        let frame = poll_fn(|context| Pin::new(&mut body).poll_frame(context))
            .await
            .expect("terminal body error");
        assert!(
            frame.is_err(),
            "expired body must not emit more capture bytes"
        );
        drop(body);
        assert_eq!(store.capture_uploads.available_permits(), 1);
        assert_eq!(
            store.capture_transfer(&owner).err(),
            Some(Error::Throttled),
            "dropping the expired body cannot retire a replacement actor lease"
        );
        drop(replacement);
        assert_eq!(store.capture_uploads.available_permits(), 2);
        drop(
            store
                .capture_transfer(&owner)
                .expect("replacement releases the actor lease"),
        );
    }

    #[tokio::test]
    async fn private_download_body_error_releases_its_transfer_slot() {
        let (_directory, store, headers, operation, bytes) = stored_capture().await;
        let response = content(
            State(store.clone()),
            Path(operation.clone()),
            headers.clone(),
        )
        .await
        .expect("download");
        let owner = actor(&store, &headers).await.expect("owner");
        store
            .delete_capture(owner, operation)
            .await
            .expect("delete before body consumption");
        assert_eq!(store.capture_uploads.available_permits(), 1);
        let mut body = response.into_body();
        let frame = poll_fn(|context| Pin::new(&mut body).poll_frame(context))
            .await
            .expect("terminal error");
        assert!(frame.is_err());
        assert_eq!(store.capture_uploads.available_permits(), 2);
        let (status, _) = upload(
            State(store),
            Path(Uuid::new_v4().to_string()),
            headers,
            Body::from(bytes),
        )
        .await
        .expect("body error releases the actor lease");
        assert_eq!(status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn private_upload_and_download_share_one_actor_lease() {
        let (_directory, store, headers, operation, _) = stored_capture().await;
        let (started, polled) = tokio::sync::oneshot::channel();
        let mut started = Some(started);
        let pending = futures_util::stream::poll_fn(move |_| {
            if let Some(started) = started.take() {
                let _ = started.send(());
            }
            std::task::Poll::<Option<Result<Bytes, std::io::Error>>>::Pending
        });
        let uploading = tokio::spawn(upload(
            State(store.clone()),
            Path(Uuid::new_v4().to_string()),
            headers.clone(),
            Body::from_stream(pending),
        ));
        tokio::time::timeout(Duration::from_secs(5), polled)
            .await
            .expect("body poll deadline")
            .expect("upload reached body ingestion");
        assert_eq!(store.capture_uploads.available_permits(), 1);
        assert_eq!(
            content(
                State(store.clone()),
                Path(operation.clone()),
                headers.clone()
            )
            .await
            .err(),
            Some(Error::Throttled),
            "a pending upload cannot be combined with a download from the same actor"
        );
        let reviewer = reviewer_headers(&store, &headers).await;
        let other = content(State(store.clone()), Path(operation.clone()), reviewer)
            .await
            .expect("another actor can use the second slot");
        assert_eq!(store.capture_uploads.available_permits(), 0);
        drop(other);
        uploading.abort();
        assert!(uploading.await.expect_err("aborted upload").is_cancelled());
        assert_eq!(store.capture_uploads.available_permits(), 2);
        drop(
            content(State(store.clone()), Path(operation), headers)
                .await
                .expect("canceled upload releases the actor lease"),
        );
    }

    #[tokio::test]
    async fn private_download_blocks_same_actor_upload_but_not_another_actor() {
        let (_directory, store, headers, operation, bytes) = stored_capture().await;
        let downloading = content(State(store.clone()), Path(operation), headers.clone())
            .await
            .expect("download");
        assert_eq!(
            upload(
                State(store.clone()),
                Path(Uuid::new_v4().to_string()),
                headers.clone(),
                Body::from(bytes.clone())
            )
            .await
            .err(),
            Some(Error::Throttled),
            "the per-actor bound covers upload after download as well"
        );
        let other = reviewer_headers(&store, &headers).await;
        let (status, _) = upload(
            State(store.clone()),
            Path(Uuid::new_v4().to_string()),
            other,
            Body::from(bytes.clone()),
        )
        .await
        .expect("another actor can upload");
        assert_eq!(status, StatusCode::CREATED);
        drop(downloading);
        let (status, _) = upload(
            State(store.clone()),
            Path(Uuid::new_v4().to_string()),
            headers,
            Body::from(bytes),
        )
        .await
        .expect("body drop releases the actor lease");
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(store.capture_uploads.available_permits(), 2);
    }

    #[tokio::test]
    async fn private_capture_chunks_stay_bounded_below_sqlite_value_limit() {
        use crate::store::capture_submissions::DOWNLOAD_CHUNK;
        use rusqlite::limits::Limit;
        let fixture =
            include_str!("../../../cutout-cli/fixtures/pevcap/falcon-begode-banner.jsonl");
        let record = format!("{}\n", fixture.lines().last().expect("record"));
        let mut payload = fixture.as_bytes().to_vec();
        while payload.len() < DOWNLOAD_CHUNK * 4 {
            payload.extend_from_slice(record.as_bytes());
        }
        let (_directory, store, headers, operation, bytes) = stored_capture_bytes(payload).await;
        let owner = actor(&store, &headers).await.expect("owner");
        let reviewer_headers = reviewer_headers(&store, &headers).await;
        let reviewer = actor(&store, &reviewer_headers).await.expect("reviewer");
        let stranger = store
            .identity(Registration::Operator {
                role: Role::Contributor,
            })
            .await
            .expect("stranger");
        let stranger = store
            .authenticate(stranger.token)
            .await
            .expect("stranger actor");
        let old_limit = store
            .call(|connection| {
                connection
                    .set_limit(
                        Limit::SQLITE_LIMIT_LENGTH,
                        i32::try_from(DOWNLOAD_CHUNK * 2).expect("bounded SQLite value limit"),
                    )
                    .map_err(crate::store::db_error)
            })
            .await
            .expect("lower value limit after publication");
        for offset in [
            0,
            DOWNLOAD_CHUNK,
            bytes.len() - 5,
            bytes.len(),
            bytes.len() + 1,
        ] {
            let chunk = store
                .capture_chunk(
                    owner.clone(),
                    operation.clone(),
                    u64::try_from(offset).expect("offset"),
                )
                .await
                .expect("bounded chunk must not materialize the complete payload");
            let expected = if offset >= bytes.len() {
                &[][..]
            } else {
                &bytes[offset..bytes.len().min(offset + DOWNLOAD_CHUNK)]
            };
            assert_eq!(chunk, expected);
            assert!(chunk.len() <= DOWNLOAD_CHUNK);
        }
        assert_eq!(
            store
                .capture_chunk(reviewer.clone(), operation.clone(), 0)
                .await
                .expect("reviewer bounded read"),
            bytes[..DOWNLOAD_CHUNK]
        );
        assert_eq!(
            store.capture_chunk(stranger, operation.clone(), 0).await,
            Err(Error::NotFound)
        );
        store
            .call(move |connection| {
                connection
                    .set_limit(Limit::SQLITE_LIMIT_LENGTH, old_limit)
                    .map_err(crate::store::db_error)
            })
            .await
            .expect("restore the write limit");
        assert_chunk_permission_retirement(&store, &operation, owner, reviewer).await;
    }

    async fn assert_chunk_permission_retirement(
        store: &Store,
        operation: &str,
        owner: crate::store::Actor,
        reviewer: crate::store::Actor,
    ) {
        let reviewer_id = reviewer.id.clone();
        store
            .call(move |connection| {
                connection
                    .execute(
                        "UPDATE contributors SET private_capture_review=0 WHERE id=?1",
                        [reviewer_id],
                    )
                    .map_err(crate::store::db_error)
                    .map(|_| ())
            })
            .await
            .expect("remove review permission");
        assert_eq!(
            store.capture_chunk(reviewer, operation.to_owned(), 0).await,
            Err(Error::NotFound)
        );
        store
            .delete_capture(owner.clone(), operation.to_owned())
            .await
            .expect("delete between reads");
        assert_eq!(
            store
                .capture_chunk(owner.clone(), operation.to_owned(), 0)
                .await,
            Err(Error::NotFound)
        );
        store.revoke(owner.id.clone()).await.expect("revoke owner");
        assert_eq!(
            store.capture_chunk(owner, operation.to_owned(), 0).await,
            Err(Error::Unauthorized)
        );
    }
}
