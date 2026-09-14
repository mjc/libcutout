//! Thin mobile projection of the Rust-owned provider lifecycle.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_music::callback_epoch::{
    AuthorizationTransactionKind, AuthorizationTransactionMatch, CallbackEpochMatch,
};
use cutout_music::connection::MusicConnectionCallback;
use cutout_music::ids::{
    ArtworkRequestId, ArtworkRetryId, AuthorizationId, CommandFeedbackId, ConnectionAttemptId,
    MonitorId, ObservationRevision, PlayerStateRequestId, ProviderSessionId, TransportRequestId,
};
use cutout_music::player_request::MusicPlayerRequestCompletion;
use cutout_music::provider_lifecycle::{
    MusicConnectionAttemptEffect, MusicConnectionEffect, MusicProviderLifecycle,
    MusicProviderWorkState, MusicTransportCompletion, MusicTransportOutcome,
};
use cutout_music::{MusicMonitorRequest, MusicMonitorResume, MusicMonitorStart};

use crate::{CoreMusicPlaybackState, MobileMusicPlaybackStateDto};

/// Matches callback paths with only empty/slash root equivalence.
#[uniffi::export]
#[must_use]
#[allow(clippy::needless_pass_by_value)] // UniFFI owns strings at this boundary.
pub fn music_callback_path_matches(expected: String, actual: String) -> bool {
    cutout_music::music_callback_path_matches(&expected, &actual)
}

/// Localized title key preserving playback state when track metadata is absent.
#[uniffi::export]
#[must_use]
pub fn music_playback_title_key(state: MobileMusicPlaybackStateDto) -> String {
    CoreMusicPlaybackState::from(state)
        .fallback_title_key()
        .to_owned()
}

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
    pub generation: MobileMusicMonitorId,
    /// Provider start mode.
    pub start: MobileMusicProviderMonitorStart,
}

/// Rust-owned monitor generation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicMonitorId {
    pub value: u64,
}

/// Rust-owned provider session identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicProviderSessionId {
    pub value: u64,
}

/// Rust-owned authorization transaction identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicAuthorizationId {
    pub value: u64,
}

/// Rust-owned connection attempt identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicConnectionAttemptId {
    pub value: u64,
}

/// Rust-owned player-state request identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicPlayerStateRequestId {
    pub value: u64,
}

/// Rust-owned artwork request identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicArtworkRequestId {
    pub value: u64,
}

/// Rust-owned artwork retry identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicArtworkRetryId {
    pub value: u64,
}

/// Rust-owned command feedback identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicCommandFeedbackId {
    pub value: u64,
}

/// Rust-owned transport request identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicTransportRequestId {
    pub value: u64,
}

/// Rust-owned observation revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicObservationRevision {
    pub value: u64,
}

/// One Rust-issued monitor poll effect and its absolute monotonic deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicMonitorPollEffect {
    /// Rust-owned effect identity.
    pub id: MobileMusicMonitorId,
    /// Absolute monotonic deadline in milliseconds.
    pub deadline_ms: u64,
}

macro_rules! timed_effect {
    ($name:ident, $id:ty) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
        pub struct $name {
            pub id: $id,
            pub deadline_ms: u64,
        }
    };
}

timed_effect!(MobileMusicAuthorizationEffect, MobileMusicAuthorizationId);
timed_effect!(
    MobileMusicPlayerStateEffect,
    MobileMusicPlayerStateRequestId
);
timed_effect!(MobileMusicArtworkEffect, MobileMusicArtworkRequestId);
timed_effect!(MobileMusicArtworkRetryEffect, MobileMusicArtworkRetryId);
timed_effect!(MobileMusicTransportEffect, MobileMusicTransportRequestId);

macro_rules! boundary_id {
    ($ffi:ty, $rust:ty) => {
        impl From<$rust> for $ffi {
            fn from(value: $rust) -> Self {
                Self { value: value.raw() }
            }
        }
    };
}

boundary_id!(MobileMusicMonitorId, MonitorId);
boundary_id!(MobileMusicProviderSessionId, ProviderSessionId);
boundary_id!(MobileMusicAuthorizationId, AuthorizationId);
boundary_id!(MobileMusicConnectionAttemptId, ConnectionAttemptId);
boundary_id!(MobileMusicPlayerStateRequestId, PlayerStateRequestId);
boundary_id!(MobileMusicArtworkRequestId, ArtworkRequestId);
boundary_id!(MobileMusicArtworkRetryId, ArtworkRetryId);
boundary_id!(MobileMusicCommandFeedbackId, CommandFeedbackId);
boundary_id!(MobileMusicTransportRequestId, TransportRequestId);
boundary_id!(MobileMusicObservationRevision, ObservationRevision);

/// Provider facts considered by Rust when scheduling another monitor poll.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicProviderWorkState {
    Active,
    AuthorizationPending,
    CredentialsAvailable,
    RequiresUserAction,
    Unavailable,
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

/// Result of ending a provider connection, including its owned work.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicProviderConnectionEffect {
    /// Whether the connection callback was current.
    pub callback: MobileMusicProviderConnectionCallback,
    /// Transport request retired with the connection, when present.
    pub transport: MobileMusicTransportCompletion,
}

/// One connection attempt and transport retired when it replaced an owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicProviderConnectionAttemptEffect {
    /// Identity for callbacks from this attempt.
    pub attempt_id: MobileMusicConnectionAttemptId,
    /// Transport request retired with the replaced attempt, when present.
    pub transport: MobileMusicTransportCompletion,
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
    pub request_id: Option<MobileMusicTransportRequestId>,
    /// Terminal outcome, only for `Finished`.
    pub outcome: Option<MobileMusicTransportOutcome>,
}

/// Work stopped when the app enters the background.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicProviderSuspension {
    /// Whether listening history must show an unknown observation interval.
    pub observation_gap: bool,
    /// Transport request the platform must resume as cancelled.
    pub cancelled_transport_request_id: Option<MobileMusicTransportRequestId>,
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

    /// Begins one replaceable command-feedback presentation.
    #[must_use]
    pub fn begin_command_feedback(&self) -> Option<MobileMusicCommandFeedbackId> {
        self.lock_inner().begin_command_feedback().map(Into::into)
    }

    /// Classifies a command completion without changing the visible request.
    #[must_use]
    pub fn classify_command_feedback(
        &self,
        id: MobileMusicCommandFeedbackId,
    ) -> MobileMusicProviderCallbackMatch {
        self.lock_inner()
            .classify_command_feedback(CommandFeedbackId::from_raw(id.value))
            .into()
    }

    /// Dismisses only the matching visible command feedback.
    #[must_use]
    pub fn dismiss_command_feedback(
        &self,
        id: MobileMusicCommandFeedbackId,
    ) -> MobileMusicProviderCallbackMatch {
        self.lock_inner()
            .dismiss_command_feedback(CommandFeedbackId::from_raw(id.value))
            .into()
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
                generation: effect.generation.into(),
                start: effect.start.into(),
            })
    }

    /// Classifies a monitor task without ending it.
    #[must_use]
    pub fn classify_monitor(
        &self,
        generation: MobileMusicMonitorId,
    ) -> MobileMusicProviderCallbackMatch {
        self.lock_inner()
            .classify_monitor(MonitorId::from_raw(generation.value))
            .into()
    }

    /// Ends only the matching monitor task.
    #[must_use]
    pub fn finish_monitor(
        &self,
        generation: MobileMusicMonitorId,
    ) -> MobileMusicProviderCallbackMatch {
        self.lock_inner()
            .finish_monitor(MonitorId::from_raw(generation.value))
            .into()
    }

    /// Schedules another poll only while Rust considers the monitor useful.
    #[must_use]
    pub fn next_monitor_poll(
        &self,
        generation: MobileMusicMonitorId,
        work_state: MobileMusicProviderWorkState,
        now_ms: u64,
    ) -> Option<MobileMusicMonitorPollEffect> {
        self.lock_inner()
            .next_monitor_poll(
                MonitorId::from_raw(generation.value),
                work_state.into(),
                now_ms,
            )
            .map(|effect| MobileMusicMonitorPollEffect {
                id: effect.id.into(),
                deadline_ms: effect.deadline_ms,
            })
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
            cancelled_transport_request_id: suspension
                .cancelled_transport_request_id
                .map(Into::into),
        }
    }

    /// Restores the foreground scene without granting authorization.
    #[must_use]
    pub fn resume(&self) -> MobileMusicProviderMonitorResume {
        self.lock_inner().resume().into()
    }

    /// Begins a replaceable provider SDK object generation.
    #[must_use]
    pub fn begin_provider_session(&self) -> Option<MobileMusicProviderSessionId> {
        self.lock_inner().begin_provider_session().map(Into::into)
    }

    /// Invalidates command feedback when the provider session changes.
    pub fn invalidate_command_feedback(&self) {
        self.lock_inner().invalidate_command_feedback();
    }

    /// Classifies a provider callback without ending its generation.
    #[must_use]
    pub fn classify_provider_session(
        &self,
        id: MobileMusicProviderSessionId,
    ) -> MobileMusicProviderCallbackMatch {
        self.lock_inner()
            .classify_provider_session(ProviderSessionId::from_raw(id.value))
            .into()
    }

    /// Ends the current provider generation and cancels its transport.
    #[must_use]
    pub fn retire_provider_session(
        &self,
        id: MobileMusicProviderSessionId,
    ) -> MobileMusicTransportCompletion {
        self.lock_inner()
            .retire_provider_session(ProviderSessionId::from_raw(id.value))
            .into()
    }

    /// Begins one authorization transaction.
    #[must_use]
    pub fn begin_authorization_effect(
        &self,
        kind: MobileMusicProviderAuthorizationKind,
        now_ms: u64,
    ) -> Option<MobileMusicAuthorizationEffect> {
        let effect = self
            .lock_inner()
            .begin_authorization_effect(kind.into(), now_ms)?;
        Some(MobileMusicAuthorizationEffect {
            id: effect.id.into(),
            deadline_ms: effect.deadline_ms,
        })
    }

    /// Classifies a provider authorization callback.
    #[must_use]
    pub fn classify_authorization(
        &self,
        id: MobileMusicAuthorizationId,
    ) -> MobileMusicProviderAuthorizationMatch {
        self.lock_inner()
            .classify_authorization(AuthorizationId::from_raw(id.value))
            .into()
    }

    /// Finishes the current provider authorization callback exactly once.
    #[must_use]
    pub fn finish_authorization(
        &self,
        id: MobileMusicAuthorizationId,
    ) -> MobileMusicProviderAuthorizationMatch {
        self.lock_inner()
            .finish_authorization(AuthorizationId::from_raw(id.value))
            .into()
    }

    /// Explicitly retires provider authorization.
    pub fn invalidate_authorization(&self) {
        self.lock_inner().invalidate_authorization();
    }

    /// Begins one bounded provider connection attempt.
    #[must_use]
    pub fn begin_connection_attempt(
        &self,
        now_ms: u64,
    ) -> Option<MobileMusicProviderConnectionAttemptEffect> {
        self.lock_inner()
            .begin_connection_attempt(now_ms)
            .map(Into::into)
    }

    /// Classifies a connection or player callback without changing retry state.
    #[must_use]
    pub fn classify_connection(
        &self,
        id: MobileMusicConnectionAttemptId,
        now_ms: u64,
    ) -> MobileMusicProviderConnectionCallback {
        self.lock_inner()
            .classify_connection(ConnectionAttemptId::from_raw(id.value), now_ms)
            .into()
    }

    /// Accepts success only for the current connection attempt.
    #[must_use]
    pub fn connection_established(
        &self,
        id: MobileMusicConnectionAttemptId,
        now_ms: u64,
    ) -> MobileMusicProviderConnectionCallback {
        self.lock_inner()
            .connection_established(ConnectionAttemptId::from_raw(id.value), now_ms)
            .into()
    }

    /// Accepts failure and returns the transport work retired with it.
    #[must_use]
    pub fn connection_failed_effect(
        &self,
        id: MobileMusicConnectionAttemptId,
        now_ms: u64,
    ) -> MobileMusicProviderConnectionEffect {
        self.lock_inner()
            .connection_failed_effect(ConnectionAttemptId::from_raw(id.value), now_ms)
            .into()
    }

    /// Accepts disconnection and returns the transport work retired with it.
    #[must_use]
    pub fn connection_disconnected_effect(
        &self,
        id: MobileMusicConnectionAttemptId,
        now_ms: u64,
    ) -> MobileMusicProviderConnectionEffect {
        self.lock_inner()
            .connection_disconnected_effect(ConnectionAttemptId::from_raw(id.value), now_ms)
            .into()
    }

    /// Begins or replaces an expired player-state request.
    #[must_use]
    pub fn begin_player_state_request(
        &self,
        now_ms: u64,
    ) -> Option<MobileMusicPlayerStateRequestId> {
        self.lock_inner()
            .begin_player_state_request(now_ms)
            .map(Into::into)
    }

    /// Completes only the matching player-state request.
    #[must_use]
    pub fn complete_player_state_request(
        &self,
        id: MobileMusicPlayerStateRequestId,
    ) -> MobileMusicRequestCompletion {
        self.lock_inner()
            .complete_player_state_request(PlayerStateRequestId::from_raw(id.value))
            .into()
    }

    /// Completes a poll only when no newer push observation superseded it.
    #[must_use]
    pub fn complete_player_state_request_if_current(
        &self,
        id: MobileMusicPlayerStateRequestId,
        observation_revision: MobileMusicObservationRevision,
    ) -> MobileMusicRequestCompletion {
        self.lock_inner()
            .complete_player_state_request_if_current(
                PlayerStateRequestId::from_raw(id.value),
                ObservationRevision::from_raw(observation_revision.value),
            )
            .into()
    }

    /// Returns the latest authoritative player observation revision.
    #[must_use]
    pub fn player_state_observation_revision(&self) -> MobileMusicObservationRevision {
        self.lock_inner().player_state_observation_revision().into()
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
    pub fn begin_artwork_effect(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        now_ms: u64,
    ) -> Option<MobileMusicArtworkEffect> {
        self.lock_inner()
            .begin_artwork_effect(
                ProviderSessionId::from_raw(provider_generation.value),
                now_ms,
            )
            .map(|effect| MobileMusicArtworkEffect {
                id: effect.id.into(),
                deadline_ms: effect.deadline_ms,
            })
    }

    /// Completes only the matching artwork request.
    #[must_use]
    pub fn complete_artwork_request(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        id: MobileMusicArtworkRequestId,
    ) -> MobileMusicRequestCompletion {
        self.lock_inner()
            .complete_artwork_request(
                ProviderSessionId::from_raw(provider_generation.value),
                ArtworkRequestId::from_raw(id.value),
            )
            .into()
    }

    /// Starts a fresh artwork budget for a new item.
    pub fn reset_artwork(&self) {
        self.lock_inner().reset_artwork();
    }

    /// Schedules another artwork attempt using a distinct Rust identity.
    #[must_use]
    pub fn begin_artwork_retry_effect(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        now_ms: u64,
    ) -> Option<MobileMusicArtworkRetryEffect> {
        self.lock_inner()
            .begin_artwork_retry_effect(
                ProviderSessionId::from_raw(provider_generation.value),
                now_ms,
            )
            .map(|effect| MobileMusicArtworkRetryEffect {
                id: effect.id.into(),
                deadline_ms: effect.deadline_ms,
            })
    }

    /// Completes only the matching provider's current artwork retry delay.
    #[must_use]
    pub fn complete_artwork_retry(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        id: MobileMusicArtworkRetryId,
    ) -> MobileMusicProviderCallbackMatch {
        self.lock_inner()
            .complete_artwork_retry(
                ProviderSessionId::from_raw(provider_generation.value),
                ArtworkRetryId::from_raw(id.value),
            )
            .into()
    }

    /// Begins one transport request.
    #[must_use]
    pub fn begin_transport_effect(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        now_ms: u64,
    ) -> Option<MobileMusicTransportEffect> {
        self.lock_inner()
            .begin_transport_effect(
                ProviderSessionId::from_raw(provider_generation.value),
                now_ms,
            )
            .map(|effect| MobileMusicTransportEffect {
                id: effect.id.into(),
                deadline_ms: effect.deadline_ms,
            })
    }

    /// Begins a transport command owned by one connection attempt.
    #[must_use]
    pub fn begin_transport_effect_for_connection(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        connection_attempt_id: MobileMusicConnectionAttemptId,
        now_ms: u64,
    ) -> Option<MobileMusicTransportEffect> {
        self.lock_inner()
            .begin_transport_effect_for_connection(
                ProviderSessionId::from_raw(provider_generation.value),
                Some(ConnectionAttemptId::from_raw(connection_attempt_id.value)),
                now_ms,
            )
            .map(|effect| MobileMusicTransportEffect {
                id: effect.id.into(),
                deadline_ms: effect.deadline_ms,
            })
    }

    /// Finishes the matching transport request exactly once.
    #[must_use]
    pub fn finish_transport(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        request_id: MobileMusicTransportRequestId,
        outcome: MobileMusicTransportOutcome,
        now_ms: u64,
    ) -> MobileMusicTransportCompletion {
        self.lock_inner()
            .finish_transport(
                ProviderSessionId::from_raw(provider_generation.value),
                TransportRequestId::from_raw(request_id.value),
                outcome.into(),
                now_ms,
            )
            .into()
    }

    /// Applies the portable transport timeout deadline.
    #[must_use]
    pub fn expire_transport(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        now_ms: u64,
    ) -> MobileMusicTransportCompletion {
        self.lock_inner()
            .expire_transport(
                ProviderSessionId::from_raw(provider_generation.value),
                now_ms,
            )
            .into()
    }

    /// Cancels the pending transport request, if any.
    #[must_use]
    pub fn cancel_transport(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        request_id: MobileMusicTransportRequestId,
    ) -> MobileMusicTransportCompletion {
        self.lock_inner()
            .cancel_transport(
                ProviderSessionId::from_raw(provider_generation.value),
                TransportRequestId::from_raw(request_id.value),
            )
            .into()
    }

    /// Cancels a command owned by a connection attempt that just ended.
    #[must_use]
    pub fn cancel_transport_for_connection(
        &self,
        provider_generation: MobileMusicProviderSessionId,
        connection_attempt_id: MobileMusicConnectionAttemptId,
    ) -> MobileMusicTransportCompletion {
        self.lock_inner()
            .cancel_transport_for_connection(
                ProviderSessionId::from_raw(provider_generation.value),
                ConnectionAttemptId::from_raw(connection_attempt_id.value),
            )
            .into()
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

impl From<MobileMusicProviderWorkState> for MusicProviderWorkState {
    fn from(value: MobileMusicProviderWorkState) -> Self {
        match value {
            MobileMusicProviderWorkState::Active => Self::Active,
            MobileMusicProviderWorkState::AuthorizationPending => Self::AuthorizationPending,
            MobileMusicProviderWorkState::CredentialsAvailable => Self::CredentialsAvailable,
            MobileMusicProviderWorkState::RequiresUserAction => Self::RequiresUserAction,
            MobileMusicProviderWorkState::Unavailable => Self::Unavailable,
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

impl From<MusicConnectionEffect> for MobileMusicProviderConnectionEffect {
    fn from(value: MusicConnectionEffect) -> Self {
        Self {
            callback: value.callback.into(),
            transport: value.transport.into(),
        }
    }
}

impl From<MusicConnectionAttemptEffect> for MobileMusicProviderConnectionAttemptEffect {
    fn from(value: MusicConnectionAttemptEffect) -> Self {
        Self {
            attempt_id: value.attempt_id.into(),
            transport: value.transport.into(),
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
                request_id: Some(request_id.into()),
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
        MobileMusicProviderCallbackMatch, MobileMusicProviderConnectionCallback,
        MobileMusicProviderLifecycle, MobileMusicProviderMonitorRequest,
        MobileMusicProviderMonitorResume, MobileMusicProviderMonitorStart,
        MobileMusicTransportCompletionState, MobileMusicTransportOutcome,
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
        let authorization = lifecycle
            .begin_authorization_effect(MobileMusicProviderAuthorizationKind::Authorizing, 0)
            .expect("authorization effect");
        let provider = lifecycle
            .begin_provider_session()
            .expect("provider session");
        let transport = lifecycle
            .begin_transport_effect(provider, 100)
            .expect("transport");

        let suspension = lifecycle.suspend();
        assert!(suspension.observation_gap);
        assert_eq!(
            suspension.cancelled_transport_request_id,
            Some(transport.id)
        );
        assert_eq!(
            lifecycle.classify_provider_session(provider),
            MobileMusicProviderCallbackMatch::Stale
        );
        assert_eq!(
            lifecycle.finish_authorization(authorization.id),
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
                .finish_transport(
                    provider,
                    transport.id,
                    MobileMusicTransportOutcome::Accepted,
                    100,
                )
                .state,
            MobileMusicTransportCompletionState::Stale
        );
    }

    #[test]
    fn binding_projects_connection_end_with_owned_transport_cancellation() {
        let lifecycle = MobileMusicProviderLifecycle::new();
        let provider = lifecycle
            .begin_provider_session()
            .expect("provider session");
        let attempt = lifecycle
            .begin_connection_attempt(0)
            .expect("attempt")
            .attempt_id;
        assert_eq!(
            lifecycle.connection_established(attempt, 0),
            MobileMusicProviderConnectionCallback::Accepted
        );
        let transport = lifecycle
            .begin_transport_effect_for_connection(provider, attempt, 0)
            .expect("transport");

        let ended = lifecycle.connection_failed_effect(attempt, 100);
        assert_eq!(
            ended.callback,
            MobileMusicProviderConnectionCallback::Accepted
        );
        assert_eq!(
            ended.transport.state,
            MobileMusicTransportCompletionState::Finished
        );
        assert_eq!(ended.transport.request_id, Some(transport.id));
        assert_eq!(
            ended.transport.outcome,
            Some(MobileMusicTransportOutcome::Cancelled)
        );
    }
}
