use crate::model::{
    AuditPage, ModerationRequest, ModeratorSpot, ReportReceipt, ReportRequest, ReportsPage,
    ReportsQuery, ResolveRequest,
};
use crate::model::{
    ChangesPage, ChangesQuery, EditRequest, SpotMutation, VoteReceipt, VoteRequest, VoteState,
    WithdrawRequest,
};
use crate::{
    Error, Store,
    model::{CreateRequest, ListQuery, MutationReceipt, PublicSpot, SpotPage, id},
    store::{Actor, ModeratorActor},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Serialize;
use std::net::IpAddr;

pub(crate) mod registration;

/// A proxy peer allowed to supply one client address through `X-Real-IP`.
/// The proxy must overwrite that header rather than forwarding caller input.
#[derive(Clone, Copy, Debug)]
pub struct TrustedProxy(pub IpAddr);

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

/// Builds the HTTP directory and website, using the supplied durable store.
/// Serve with `into_make_service_with_connect_info::<SocketAddr>()` so registration
/// budgets use the transport peer rather than caller-supplied headers.
/// Behind a proxy, add `Extension(TrustedProxy(proxy_ip))` and configure the proxy
/// to overwrite `X-Real-IP` with the client address. Other peers cannot select
/// their budget by sending forwarding headers.
pub fn router(store: Store) -> Router {
    router_with_signup_state(store, crate::signup::SignupState::from_env())
}

pub(crate) fn router_with_signup_state(store: Store, signup: crate::signup::SignupState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route("/readyz", get(ready))
        .route("/v1/alpha-signup/config", get(crate::signup::public_config))
        .route("/v1/alpha-signup/requests", post(crate::signup::submit))
        .route(
            "/v1/alpha-signup-removals",
            post(crate::signup::submit_removal),
        )
        .route("/v1/identities", post(registration::identity))
        .route(
            "/v1/identities/current",
            delete(registration::delete_current),
        )
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
        .route("/v1/moderation/alpha-signups", get(alpha_signups))
        .route(
            "/v1/moderation/alpha-signup-removals",
            get(alpha_signup_removals),
        )
        .route(
            "/v1/moderation/alpha-signups/{id}/retry",
            post(retry_alpha_signup),
        )
        .route(
            "/v1/moderation/alpha-signups/{id}",
            delete(withdraw_alpha_signup),
        )
        .merge(crate::site::router())
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(axum::Extension(signup))
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
    let token = bearer_token(headers)?;
    store.authenticate(token.to_owned()).await
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Result<&str, Error> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(Error::Unauthorized)?;
    value.strip_prefix("Bearer ").ok_or(Error::Unauthorized)
}

async fn ready(State(store): State<Store>) -> Result<StatusCode, Error> {
    store.ready().await?;
    Ok(StatusCode::OK)
}

async fn verify(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<WithdrawRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<MutationReceipt>, Error> {
    let actor = actor(&store, &headers).await?;
    let request = body.map_err(|_| Error::Invalid)?.0;
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
    let request = body.map_err(|_| Error::Invalid)?.0;
    let request = CreateRequest {
        operation_id: request.operation_id,
        spot: request.spot.validate()?,
    };
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
    let query = query.into_validated()?;
    Ok(Json(store.list(query).await?))
}

async fn edit(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<EditRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<MutationReceipt>, Error> {
    let actor = actor(&store, &headers).await?;
    let request = body.map_err(|_| Error::Invalid)?.0;
    let request = EditRequest {
        operation_id: request.operation_id,
        expected_revision: request.expected_revision,
        spot: request.spot.validate()?,
    };
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
    let request = body.map_err(|_| Error::Invalid)?.0;
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
    let request = body.map_err(|_| Error::Invalid)?.0;
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
    let query = query.map_err(|_| Error::Invalid)?.0.into_validated()?;
    Ok(Json(store.changes(query).await?))
}

async fn moderator_actor(store: &Store, headers: &HeaderMap) -> Result<ModeratorActor, Error> {
    actor(store, headers).await?.into_moderator()
}

async fn report(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<ReportRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<ReportReceipt>), Error> {
    let actor = actor(&store, &headers).await?;
    let request = body.map_err(|_| Error::Invalid)?.0.into_validated()?;
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
    let query = query.into_validated()?;
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
    let request = body.map_err(|_| Error::Invalid)?.0.into_validated()?;
    Ok(Json(store.moderate(actor, id(&target)?, request).await?))
}

async fn resolve(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
    body: Result<Json<ResolveRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<ReportReceipt>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    let request = body.map_err(|_| Error::Invalid)?.0.into_validated()?;
    Ok(Json(store.resolve(actor, id(&target)?, request).await?))
}

async fn audit(
    State(store): State<Store>,
    headers: HeaderMap,
    query: Result<Query<ChangesQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<AuditPage>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    let query = query.map_err(|_| Error::Invalid)?.0.into_validated()?;
    Ok(Json(store.audit(actor, query).await?))
}

async fn alpha_signups(
    State(store): State<Store>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    Ok(Json(store.moderator_signup_sync_status(actor).await?))
}

async fn alpha_signup_removals(
    State(store): State<Store>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    Ok(Json(store.moderator_signup_removal_status(actor).await?))
}

async fn retry_alpha_signup(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    store
        .moderator_retry_signup_sync(actor, id(&target)?)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

async fn withdraw_alpha_signup(
    State(store): State<Store>,
    Path(target): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, Error> {
    let actor = moderator_actor(&store, &headers).await?;
    store
        .moderator_request_signup_withdrawal(actor, id(&target)?)
        .await?;
    Ok(StatusCode::ACCEPTED)
}
