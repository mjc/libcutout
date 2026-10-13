use crate::model::{
    AuditPage, ModerationAction, ModerationRequest, ModeratorSpot, ReportReason, ReportReceipt,
    ReportRequest, ReportsPage, ReportsQuery, ResolveRequest, optional_text,
};
use crate::model::{
    ChangesPage, ChangesQuery, EditRequest, SpotMutation, VoteReceipt, VoteRequest, VoteState,
    WithdrawRequest,
};
use crate::{
    Error, Store,
    model::{CreateRequest, ListQuery, MutationReceipt, PublicSpot, SpotPage, id},
    store::{Actor, Identity},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        #[derive(Serialize)]
        struct Body {
            error: &'static str,
        }
        let (status, code) = match self {
            Self::Invalid => (StatusCode::UNPROCESSABLE_ENTITY, "invalid"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict => (StatusCode::CONFLICT, "conflict"),
            Self::Throttled => (StatusCode::TOO_MANY_REQUESTS, "throttled"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        };
        let mut response = (status, Json(Body { error: code })).into_response();
        if self == Self::Unauthorized {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer"),
            );
        }
        if self == Self::Throttled {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, header::HeaderValue::from_static("60"));
        }
        response
    }
}

/// Builds the versioned HTTP directory, using the supplied durable store.
pub fn router(store: Store) -> Router {
    Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route("/readyz", get(ready))
        .route("/v1/identities", post(identity))
        .route("/v1/charging-spots", get(list).post(create))
        .route("/v1/charging-spots/changes", get(changes))
        .route(
            "/v1/charging-spots/{id}",
            get(detail).put(edit).delete(withdraw),
        )
        .route("/v1/charging-spots/{id}/vote", get(current_vote).put(vote))
        .route("/v1/charging-spots/{id}/reports", post(report))
        .route("/v1/charging-spots/{id}/verification", post(verify))
        .route("/v1/moderation/reports", get(reports))
        .route("/v1/moderation/reports/{id}", axum::routing::put(resolve))
        .route(
            "/v1/moderation/charging-spots/{id}",
            get(moderator_spot).put(moderate),
        )
        .route("/v1/moderation/audit", get(audit))
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(axum::middleware::from_fn(deadline))
        .with_state(store)
}

async fn deadline(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut response =
        match tokio::time::timeout(std::time::Duration::from_secs(5), next.run(request)).await {
            Ok(response) => response,
            Err(_) => Error::Unavailable.into_response(),
        };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

pub(crate) async fn actor(store: &Store, headers: &HeaderMap) -> Result<Actor, Error> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(Error::Unauthorized)?;
    let token = value.strip_prefix("Bearer ").ok_or(Error::Unauthorized)?;
    store.authenticate(token.to_owned()).await
}

async fn ready(State(store): State<Store>) -> Result<StatusCode, Error> {
    store.ready().await?;
    Ok(StatusCode::OK)
}
async fn identity(
    State(store): State<Store>,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<(StatusCode, Json<Identity>), Error> {
    if !body.map_err(|_| Error::Invalid)?.is_empty() {
        return Err(Error::Invalid);
    }
    Ok((
        StatusCode::CREATED,
        Json(store.identity(false, true).await?),
    ))
}

async fn verify(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<WithdrawRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<MutationReceipt>, Error> {
    let actor = actor(&store, &headers).await?;
    let mut request = body.map_err(|_| Error::Invalid)?.0;
    request.operation_id = id(&request.operation_id)?;
    Ok(Json(
        store.verify_access(actor, id(&target)?, request).await?,
    ))
}
async fn create(
    State(store): State<Store>,
    headers: HeaderMap,
    body: Result<Json<CreateRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<MutationReceipt>), Error> {
    let actor = actor(&store, &headers).await?;
    let mut request = body.map_err(|_| Error::Invalid)?.0;
    request.operation_id = id(&request.operation_id)?;
    request.spot = request.spot.validate()?;
    Ok((
        StatusCode::CREATED,
        Json(store.create(actor, request).await?),
    ))
}
async fn detail(
    State(store): State<Store>,
    Path(target): Path<String>,
) -> Result<Json<PublicSpot>, Error> {
    Ok(Json(store.detail(id(&target)?).await?))
}
async fn list(
    State(store): State<Store>,
    query: Result<Query<ListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<SpotPage>, Error> {
    let query = query.map_err(|_| Error::Invalid)?.0;
    query.validate()?;
    Ok(Json(store.list(query).await?))
}

async fn edit(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<EditRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<MutationReceipt>, Error> {
    let actor = actor(&store, &headers).await?;
    let mut request = body.map_err(|_| Error::Invalid)?.0;
    request.operation_id = id(&request.operation_id)?;
    request.spot = request.spot.validate()?;
    Ok(Json(
        store
            .mutate(actor, id(&target)?, SpotMutation::Edit(request))
            .await?,
    ))
}
async fn withdraw(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<WithdrawRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<MutationReceipt>, Error> {
    let actor = actor(&store, &headers).await?;
    let mut request = body.map_err(|_| Error::Invalid)?.0;
    request.operation_id = id(&request.operation_id)?;
    Ok(Json(
        store
            .mutate(actor, id(&target)?, SpotMutation::Withdraw(request))
            .await?,
    ))
}
async fn vote(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<VoteRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<VoteReceipt>, Error> {
    let actor = actor(&store, &headers).await?;
    let mut request = body.map_err(|_| Error::Invalid)?.0;
    request.operation_id = id(&request.operation_id)?;
    Ok(Json(store.vote(actor, id(&target)?, request).await?))
}
async fn current_vote(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
) -> Result<Json<VoteState>, Error> {
    let actor = actor(&store, &headers).await?;
    Ok(Json(store.current_vote(actor, id(&target)?).await?))
}
async fn changes(
    State(store): State<Store>,
    query: Result<Query<ChangesQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<ChangesPage>, Error> {
    let query = query.map_err(|_| Error::Invalid)?.0;
    if !(1..=100).contains(&query.limit.unwrap_or(50))
        || query.after.unwrap_or_default() > i64::MAX as u64
    {
        return Err(Error::Invalid);
    }
    Ok(Json(store.changes(query).await?))
}

async fn moderator_actor(store: &Store, headers: &HeaderMap) -> Result<Actor, Error> {
    let actor = actor(store, headers).await?;
    if !actor.moderator {
        return Err(Error::Forbidden);
    }
    Ok(actor)
}

async fn report(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<ReportRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<ReportReceipt>), Error> {
    let actor = actor(&store, &headers).await?;
    let mut request = body.map_err(|_| Error::Invalid)?.0;
    request.operation_id = id(&request.operation_id)?;
    request.message = optional_text(request.message, 1_000)?;
    if let ReportReason::Other = request.reason
        && request.message.is_none()
    {
        return Err(Error::Invalid);
    }
    Ok((
        StatusCode::CREATED,
        Json(store.report(actor, id(&target)?, request).await?),
    ))
}

async fn reports(
    State(store): State<Store>,
    headers: HeaderMap,
    query: Result<Query<ReportsQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<ReportsPage>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    let query = query.map_err(|_| Error::Invalid)?.0;
    if !(1..=100).contains(&query.limit.unwrap_or(50)) {
        return Err(Error::Invalid);
    }
    Ok(Json(store.reports(actor, query).await?))
}

async fn moderator_spot(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ModeratorSpot>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    Ok(Json(store.moderator_spot(actor, id(&target)?).await?))
}

async fn moderate(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<ModerationRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<MutationReceipt>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    let mut request = body.map_err(|_| Error::Invalid)?.0;
    request.operation_id = id(&request.operation_id)?;
    request.note = optional_text(Some(request.note), 500)?.ok_or(Error::Invalid)?;
    request.action = match request.action {
        ModerationAction::Correct { spot } => ModerationAction::Correct {
            spot: spot.validate()?,
        },
        ModerationAction::Duplicate { canonical_id } => ModerationAction::Duplicate {
            canonical_id: id(&canonical_id)?,
        },
        action => action,
    };
    Ok(Json(store.moderate(actor, id(&target)?, request).await?))
}

async fn resolve(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<ResolveRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<ReportReceipt>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    let mut request = body.map_err(|_| Error::Invalid)?.0;
    request.operation_id = id(&request.operation_id)?;
    request.note = optional_text(Some(request.note), 500)?.ok_or(Error::Invalid)?;
    Ok(Json(store.resolve(actor, id(&target)?, request).await?))
}

async fn audit(
    State(store): State<Store>,
    headers: HeaderMap,
    query: Result<Query<ChangesQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<AuditPage>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    let query = query.map_err(|_| Error::Invalid)?.0;
    if !(1..=100).contains(&query.limit.unwrap_or(50))
        || query.after.unwrap_or_default() > i64::MAX as u64
    {
        return Err(Error::Invalid);
    }
    Ok(Json(store.audit(actor, query).await?))
}
