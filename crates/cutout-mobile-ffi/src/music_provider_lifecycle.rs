//! Thin mobile projection of the Rust-owned provider lifecycle.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_music::callback_epoch::{
    AuthorizationTransactionKind, AuthorizationTransactionMatch, CallbackEpochMatch,
};
use cutout_music::connection::MusicConnectionCallback;
use cutout_music::player_request::MusicPlayerRequestCompletion;
use cutout_music::provider_lifecycle::{
    MusicProviderLifecycle, MusicTransportCompletion, MusicTransportOutcome,
};
use cutout_music::{MusicMonitorRequest, MusicMonitorResume, MusicMonitorStart};

/// User intent for the next foreground provider start.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicProviderMonitorRequest {
    /// Observe using existing provider authorization.
    Observe,
    /// Consume one explicit user authorization grant.
    Authorize,
}

/// Work admitted for one foreground provider start.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicProviderMonitorStart {
    /// Observe using existing provider authorization.
    Observe,
    /// Launch authorization when credentials are unavailable.
    Authorize,
}

/// One foreground monitor effect admitted by the Rust reducer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicProviderMonitorEffect {
    /// Rust-owned task identity.
    pub generation: u64,
    /// Provider start mode.
    pub start: MobileMusicProviderMonitorStart,
}

/// Result of restoring the provider lifecycle to the foreground.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicProviderMonitorResume {
    /// The scene was already active.
    AlreadyActive,
    /// No monitor intent was retained.
    NoRequest,
    /// Passive observation should restart.
    Restored,
}

/// Kind of authorization operation begun by the provider adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicProviderAuthorizationKind {
    /// Explicit user authorization.
    Authorizing,
    /// Silent renewal of retained credentials.
    Renewing,
}

/// Classification of an authorization callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicProviderAuthorizationMatch {
    /// Current explicit authorization.
    Authorizing,
    /// Current silent renewal.
    Renewing,
    /// Retired authorization callback.
    Stale,
}

/// Classification of a provider SDK callback generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicProviderCallbackMatch {
    /// The callback belongs to the active provider object.
    Current,
    /// The callback belongs to a retired provider object.
    Stale,
}

/// Classification of a connection callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicProviderConnectionCallback {
    /// The callback belongs to the current attempt or session.
    Accepted,
    /// The callback belongs to a retired attempt or session.
    Stale,
}

/// Classification of a player-state or artwork callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicRequestCompletion {
    /// The callback completed the current request.
    Accepted,
    /// The callback belongs to a completed or retired request.
    Stale,
}

/// Terminal transport result supplied by the SDK boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicTransportOutcome {
    /// Provider accepted the command.
    Accepted,
    /// Provider rejected or failed the command.
    Failed,
    /// Provider did not call back before the deadline.
    TimedOut,
    /// Provider session ended before the callback.
    Cancelled,
}

/// State of a transport completion attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicTransportCompletionState {
    /// Request remains pending.
    Pending,
    /// This transition completed the request.
    Finished,
    /// Request was already completed or retired.
    Stale,
}

/// Result of a transport callback, deadline, or cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicTransportCompletion {
    /// Completion classification.
    pub state: MobileMusicTransportCompletionState,
    /// Completed request identity, only for `Finished`.
    pub request_id: Option<u64>,
    /// Terminal outcome, only for `Finished`.
    pub outcome: Option<MobileMusicTransportOutcome>,
}

/// Work stopped when the app enters the background.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicProviderSuspension {
    /// Whether listening history must show an unknown observation interval.
    pub observation_gap: bool,
    /// Transport request the platform must resume as cancelled.
    pub cancelled_transport_request_id: Option<u64>,
}

/// One thread-safe Rust owner shared by app scene and provider adapter code.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileMusicProviderLifecycle {
    inner: Mutex<MusicProviderLifecycle>,
}

#[uniffi::export]
impl MobileMusicProviderLifecycle {
    /// Creates an idle provider lifecycle.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Records provider monitoring intent.
    pub fn request_monitor(&self, request: MobileMusicProviderMonitorRequest) {
        self.lock_inner().request_monitor(request.into());
    }

    /// Cancels monitoring and all provider work.
    #[must_use]
    pub fn cancel_monitor(&self) -> MobileMusicTransportCompletion {
        self.lock_inner().cancel_monitor().into()
    }

    /// Admits one foreground monitor task with a Rust-owned identity.
    #[must_use]
    pub fn begin_monitor(&self) -> Option<MobileMusicProviderMonitorEffect> {
        self.lock_inner()
            .begin_monitor()
            .map(|effect| MobileMusicProviderMonitorEffect {
                generation: effect.generation,
                start: effect.start.into(),
            })
    }

    /// Classifies a monitor task without ending it.
    #[must_use]
    pub fn classify_monitor(&self, generation: u64) -> MobileMusicProviderCallbackMatch {
        self.lock_inner().classify_monitor(generation).into()
    }

    /// Ends only the matching monitor task.
    #[must_use]
    pub fn finish_monitor(&self, generation: u64) -> MobileMusicProviderCallbackMatch {
        self.lock_inner().finish_monitor(generation).into()
    }

    /// Whether provider work is allowed in the current scene.
    #[must_use]
    pub fn is_scene_active(&self) -> bool {
        self.lock_inner().is_scene_active()
    }

    /// Suspends active provider work while retaining passive intent and in-flight authorization.
    #[must_use]
    pub fn suspend(&self) -> MobileMusicProviderSuspension {
        let suspension = self.lock_inner().suspend();
        MobileMusicProviderSuspension {
            observation_gap: suspension.observation_gap,
            cancelled_transport_request_id: suspension.cancelled_transport_request_id,
        }
    }

    /// Restores the foreground scene without granting authorization.
    #[must_use]
    pub fn resume(&self) -> MobileMusicProviderMonitorResume {
        self.lock_inner().resume().into()
    }

    /// Begins a replaceable provider SDK object generation.
    #[must_use]
    pub fn begin_provider_session(&self) -> u64 {
        self.lock_inner().begin_provider_session()
    }

    /// Classifies a provider callback without ending its generation.
    #[must_use]
    pub fn classify_provider_session(&self, id: u64) -> MobileMusicProviderCallbackMatch {
        self.lock_inner().classify_provider_session(id).into()
    }

    /// Ends the current provider generation and cancels its transport.
    #[must_use]
    pub fn retire_provider_session(&self, id: u64) -> MobileMusicTransportCompletion {
        self.lock_inner().retire_provider_session(id).into()
    }

    /// Begins one authorization transaction.
    #[must_use]
    pub fn begin_authorization(&self, kind: MobileMusicProviderAuthorizationKind) -> u64 {
        self.lock_inner().begin_authorization(kind.into())
    }

    /// Classifies a provider authorization callback.
    #[must_use]
    pub fn classify_authorization(&self, id: u64) -> MobileMusicProviderAuthorizationMatch {
        self.lock_inner().classify_authorization(id).into()
    }

    /// Finishes the current provider authorization callback exactly once.
    #[must_use]
    pub fn finish_authorization(&self, id: u64) -> MobileMusicProviderAuthorizationMatch {
        self.lock_inner().finish_authorization(id).into()
    }

    /// Explicitly retires provider authorization.
    pub fn invalidate_authorization(&self) {
        self.lock_inner().invalidate_authorization();
    }

    /// Begins one bounded provider connection attempt.
    #[must_use]
    pub fn begin_connection_attempt(&self, now_ms: u64) -> Option<u64> {
        self.lock_inner().begin_connection_attempt(now_ms)
    }

    /// Accepts success only for the current connection attempt.
    #[must_use]
    pub fn connection_established(&self, id: u64) -> MobileMusicProviderConnectionCallback {
        self.lock_inner().connection_established(id).into()
    }

    /// Accepts failure only for the current connection attempt or session.
    #[must_use]
    pub fn connection_failed(&self, id: u64, now_ms: u64) -> MobileMusicProviderConnectionCallback {
        self.lock_inner().connection_failed(id, now_ms).into()
    }

    /// Accepts disconnection only for the current connection attempt or session.
    #[must_use]
    pub fn connection_disconnected(
        &self,
        id: u64,
        now_ms: u64,
    ) -> MobileMusicProviderConnectionCallback {
        self.lock_inner().connection_disconnected(id, now_ms).into()
    }

    /// Begins or replaces an expired player-state request.
    #[must_use]
    pub fn begin_player_state_request(&self, now_ms: u64) -> Option<u64> {
        self.lock_inner().begin_player_state_request(now_ms)
    }

    /// Completes only the matching player-state request.
    #[must_use]
    pub fn complete_player_state_request(&self, id: u64) -> MobileMusicRequestCompletion {
        self.lock_inner().complete_player_state_request(id).into()
    }

    /// Refreshes the current player-state freshness window.
    pub fn mark_player_state_observed(&self, now_ms: u64) {
        self.lock_inner().mark_player_state_observed(now_ms);
    }

    /// Whether the cached state exceeded the portable freshness deadline.
    #[must_use]
    pub fn is_player_state_stale(&self, now_ms: u64) -> bool {
        self.lock_inner().is_player_state_stale(now_ms)
    }

    /// Begins one bounded artwork request.
    #[must_use]
    pub fn begin_artwork_request(&self) -> Option<u64> {
        self.lock_inner().begin_artwork_request()
    }

    /// Completes only the matching artwork request.
    #[must_use]
    pub fn complete_artwork_request(&self, id: u64) -> MobileMusicRequestCompletion {
        self.lock_inner().complete_artwork_request(id).into()
    }

    /// Whether another artwork attempt remains for the current item.
    #[must_use]
    pub fn can_retry_artwork(&self) -> bool {
        self.lock_inner().can_retry_artwork()
    }

    /// Starts a fresh artwork budget for a new item.
    pub fn reset_artwork(&self) {
        self.lock_inner().reset_artwork();
    }

    /// Begins one transport request.
    #[must_use]
    pub fn begin_transport(&self, now_ms: u64) -> Option<u64> {
        self.lock_inner().begin_transport(now_ms)
    }

    /// Finishes the matching transport request exactly once.
    #[must_use]
    pub fn finish_transport(
        &self,
        request_id: u64,
        outcome: MobileMusicTransportOutcome,
    ) -> MobileMusicTransportCompletion {
        self.lock_inner()
            .finish_transport(request_id, outcome.into())
            .into()
    }

    /// Applies the portable transport timeout deadline.
    #[must_use]
    pub fn expire_transport(&self, now_ms: u64) -> MobileMusicTransportCompletion {
        self.lock_inner().expire_transport(now_ms).into()
    }

    /// Cancels the pending transport request, if any.
    #[must_use]
    pub fn cancel_transport(&self) -> MobileMusicTransportCompletion {
        self.lock_inner().cancel_transport().into()
    }
}

impl MobileMusicProviderLifecycle {
    fn lock_inner(&self) -> MutexGuard<'_, MusicProviderLifecycle> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl From<MobileMusicProviderMonitorRequest> for MusicMonitorRequest {
    fn from(value: MobileMusicProviderMonitorRequest) -> Self {
        match value {
            MobileMusicProviderMonitorRequest::Observe => Self::Observe,
            MobileMusicProviderMonitorRequest::Authorize => Self::Authorize,
        }
    }
}

impl From<MusicMonitorStart> for MobileMusicProviderMonitorStart {
    fn from(value: MusicMonitorStart) -> Self {
        match value {
            MusicMonitorStart::Observe => Self::Observe,
            MusicMonitorStart::Authorize => Self::Authorize,
        }
    }
}

impl From<MusicMonitorResume> for MobileMusicProviderMonitorResume {
    fn from(value: MusicMonitorResume) -> Self {
        match value {
            MusicMonitorResume::AlreadyActive => Self::AlreadyActive,
            MusicMonitorResume::NoRequest => Self::NoRequest,
            MusicMonitorResume::Restored => Self::Restored,
        }
    }
}

impl From<MobileMusicProviderAuthorizationKind> for AuthorizationTransactionKind {
    fn from(value: MobileMusicProviderAuthorizationKind) -> Self {
        match value {
            MobileMusicProviderAuthorizationKind::Authorizing => Self::Authorizing,
            MobileMusicProviderAuthorizationKind::Renewing => Self::Renewing,
        }
    }
}

impl From<AuthorizationTransactionMatch> for MobileMusicProviderAuthorizationMatch {
    fn from(value: AuthorizationTransactionMatch) -> Self {
        match value {
            AuthorizationTransactionMatch::Authorizing => Self::Authorizing,
            AuthorizationTransactionMatch::Renewing => Self::Renewing,
            AuthorizationTransactionMatch::Stale => Self::Stale,
        }
    }
}

impl From<CallbackEpochMatch> for MobileMusicProviderCallbackMatch {
    fn from(value: CallbackEpochMatch) -> Self {
        match value {
            CallbackEpochMatch::Current => Self::Current,
            CallbackEpochMatch::Stale => Self::Stale,
        }
    }
}

impl From<MusicConnectionCallback> for MobileMusicProviderConnectionCallback {
    fn from(value: MusicConnectionCallback) -> Self {
        match value {
            MusicConnectionCallback::Accepted => Self::Accepted,
            MusicConnectionCallback::Stale => Self::Stale,
        }
    }
}

impl From<MusicPlayerRequestCompletion> for MobileMusicRequestCompletion {
    fn from(value: MusicPlayerRequestCompletion) -> Self {
        match value {
            MusicPlayerRequestCompletion::Accepted => Self::Accepted,
            MusicPlayerRequestCompletion::Stale => Self::Stale,
        }
    }
}

impl From<MobileMusicTransportOutcome> for MusicTransportOutcome {
    fn from(value: MobileMusicTransportOutcome) -> Self {
        match value {
            MobileMusicTransportOutcome::Accepted => Self::Accepted,
            MobileMusicTransportOutcome::Failed => Self::Failed,
            MobileMusicTransportOutcome::TimedOut => Self::TimedOut,
            MobileMusicTransportOutcome::Cancelled => Self::Cancelled,
        }
    }
}

impl From<MusicTransportOutcome> for MobileMusicTransportOutcome {
    fn from(value: MusicTransportOutcome) -> Self {
        match value {
            MusicTransportOutcome::Accepted => Self::Accepted,
            MusicTransportOutcome::Failed => Self::Failed,
            MusicTransportOutcome::TimedOut => Self::TimedOut,
            MusicTransportOutcome::Cancelled => Self::Cancelled,
        }
    }
}

impl From<MusicTransportCompletion> for MobileMusicTransportCompletion {
    fn from(value: MusicTransportCompletion) -> Self {
        match value {
            MusicTransportCompletion::Pending => Self {
                state: MobileMusicTransportCompletionState::Pending,
                request_id: None,
                outcome: None,
            },
            MusicTransportCompletion::Finished {
                request_id,
                outcome,
            } => Self {
                state: MobileMusicTransportCompletionState::Finished,
                request_id: Some(request_id),
                outcome: Some(outcome.into()),
            },
            MusicTransportCompletion::Stale => Self {
                state: MobileMusicTransportCompletionState::Stale,
                request_id: None,
                outcome: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MobileMusicProviderAuthorizationKind, MobileMusicProviderAuthorizationMatch,
        MobileMusicProviderCallbackMatch, MobileMusicProviderLifecycle,
        MobileMusicProviderMonitorRequest, MobileMusicProviderMonitorResume,
        MobileMusicProviderMonitorStart, MobileMusicTransportCompletionState,
        MobileMusicTransportOutcome,
    };

    #[test]
    fn binding_projects_one_shared_lifecycle() {
        let lifecycle = MobileMusicProviderLifecycle::new();
        lifecycle.request_monitor(MobileMusicProviderMonitorRequest::Authorize);
        let monitor = lifecycle.begin_monitor().expect("monitor effect");
        assert_eq!(monitor.start, MobileMusicProviderMonitorStart::Authorize);
        assert_eq!(
            lifecycle.classify_monitor(monitor.generation),
            MobileMusicProviderCallbackMatch::Current
        );
        let authorization =
            lifecycle.begin_authorization(MobileMusicProviderAuthorizationKind::Authorizing);
        let provider = lifecycle.begin_provider_session();
        let transport = lifecycle.begin_transport(100).expect("transport");

        let suspension = lifecycle.suspend();
        assert!(suspension.observation_gap);
        assert_eq!(suspension.cancelled_transport_request_id, Some(transport));
        assert_eq!(
            lifecycle.classify_provider_session(provider),
            MobileMusicProviderCallbackMatch::Stale
        );
        assert_eq!(
            lifecycle.finish_authorization(authorization),
            MobileMusicProviderAuthorizationMatch::Authorizing
        );
        assert_eq!(
            lifecycle.resume(),
            MobileMusicProviderMonitorResume::Restored
        );
        assert_eq!(
            lifecycle.classify_monitor(monitor.generation),
            MobileMusicProviderCallbackMatch::Stale
        );
        assert_eq!(
            lifecycle
                .finish_transport(transport, MobileMusicTransportOutcome::Accepted)
                .state,
            MobileMusicTransportCompletionState::Stale
        );
    }
}
