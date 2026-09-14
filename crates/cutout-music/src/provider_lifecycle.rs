//! One owner for a provider monitor, callbacks, authorization, and transport.

use crate::callback_epoch::{
    AuthorizationTransaction, AuthorizationTransactionKind, AuthorizationTransactionMatch,
    CallbackEpoch, CallbackEpochMatch,
};
use crate::connection::{MusicConnection, MusicConnectionCallback};
use crate::ids::{
    ArtworkRequestId, ArtworkRetry, ArtworkRetryId, AuthorizationId, CommandFeedback,
    CommandFeedbackId, ConnectionAttemptId, MonitorGeneration, MonitorId, ObservationRevision,
    PlayerStateRequestId, ProviderSessionGeneration, ProviderSessionId, TransportRequestId,
};
use crate::player_request::{
    MusicArtworkRequest, MusicPlayerRequest, MusicPlayerRequestCompletion,
};
use crate::{MusicMonitor, MusicMonitorRequest, MusicMonitorResume, MusicMonitorStart};

const MONITOR_POLL_INTERVAL_MS: u64 = 1_000;
const AUTHORIZATION_TIMEOUT_MS: u64 = 20_000;
const TRANSPORT_TIMEOUT_MS: u64 = 10_000;
const ARTWORK_TIMEOUT_MS: u64 = 5_000;
const ARTWORK_RETRY_DELAY_MS: u64 = 1_000;

/// One Rust-issued effect identity and its monotonic deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicDeadlineEffect<I> {
    /// Identity used to reject replacement callbacks.
    pub id: I,
    /// Absolute monotonic deadline for the platform executor.
    pub deadline_ms: u64,
}

/// Provider facts used by Rust to decide whether monitoring should continue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicProviderWorkState {
    /// The provider is connected or can produce observations.
    Active,
    /// Provider authorization is still in flight.
    AuthorizationPending,
    /// Credentials remain available for bounded reconnection.
    CredentialsAvailable,
    /// User action is required before more provider work is useful.
    RequiresUserAction,
    /// The provider integration is unavailable.
    Unavailable,
}

/// Terminal result for one provider transport request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicTransportOutcome {
    /// The provider accepted the command.
    Accepted,
    /// The provider rejected or failed the command.
    Failed,
    /// The provider did not call back before the deadline.
    TimedOut,
    /// The owning provider session ended before completion.
    Cancelled,
}

/// Result of applying a callback, deadline, or cancellation to transport state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicTransportCompletion {
    /// The request remains pending.
    Pending,
    /// This transition ended the current request exactly once.
    Finished {
        /// Correlation identity of the completed request.
        request_id: TransportRequestId,
        /// Terminal request outcome.
        outcome: MusicTransportOutcome,
    },
    /// The callback belonged to a completed or replaced request.
    Stale,
}

/// Result of ending a provider connection attempt or session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicConnectionEffect {
    /// Whether the callback belonged to the current connection.
    pub callback: MusicConnectionCallback,
    /// Transport work owned by that connection, if any.
    pub transport: MusicTransportCompletion,
}

/// One connection attempt and any transport retired when it replaced an owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicConnectionAttemptEffect {
    /// Identity for callbacks from this attempt.
    pub attempt_id: ConnectionAttemptId,
    /// Transport owned by the replaced attempt, when present.
    pub transport: MusicTransportCompletion,
}

/// Provider work cancelled by a scene suspension.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MusicProviderSuspension {
    /// Whether active observation stopped and the ride timeline has an unknown interval.
    pub observation_gap: bool,
    /// Transport request that the platform must resume as cancelled, when present.
    pub cancelled_transport_request_id: Option<TransportRequestId>,
}

/// One foreground monitor effect admitted by Rust-owned intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicMonitorEffect {
    /// Correlation identity for the platform task.
    pub generation: MonitorId,
    /// Whether this start may launch authorization UI.
    pub start: MusicMonitorStart,
}

#[derive(Clone, Copy, Debug)]
struct PendingTransport {
    provider_generation: ProviderSessionId,
    connection_attempt_id: Option<ConnectionAttemptId>,
    request_id: TransportRequestId,
    started_at_ms: u64,
}

/// Shared portable lifecycle for one selected music provider.
///
/// The platform executes SDK effects. This owner supplies all replaceable
/// identities and makes background, callback, and timeout transitions atomic.
#[derive(Debug)]
pub struct MusicProviderLifecycle {
    monitor: MusicMonitor,
    monitor_generation: CallbackEpoch<MonitorGeneration>,
    provider_session: CallbackEpoch<ProviderSessionGeneration>,
    authorization: AuthorizationTransaction,
    connection: MusicConnection,
    player_state: MusicPlayerRequest,
    artwork: MusicArtworkRequest,
    artwork_retry: CallbackEpoch<ArtworkRetry>,
    command_feedback: CallbackEpoch<CommandFeedback>,
    last_transport_id: Option<u64>,
    pending_transport: Option<PendingTransport>,
    observing: bool,
}

impl Default for MusicProviderLifecycle {
    fn default() -> Self {
        Self {
            monitor: MusicMonitor::default(),
            monitor_generation: CallbackEpoch::default(),
            provider_session: CallbackEpoch::default(),
            authorization: AuthorizationTransaction::default(),
            connection: MusicConnection::default(),
            player_state: MusicPlayerRequest::default(),
            artwork: MusicArtworkRequest::default(),
            artwork_retry: CallbackEpoch::default(),
            command_feedback: CallbackEpoch::default(),
            last_transport_id: Some(0),
            pending_transport: None,
            observing: false,
        }
    }
}

impl MusicProviderLifecycle {
    /// Begins one replaceable command-feedback presentation.
    #[must_use]
    pub fn begin_command_feedback(&mut self) -> Option<CommandFeedbackId> {
        self.command_feedback.begin()
    }

    /// Classifies a command completion without changing the visible request.
    #[must_use]
    pub fn classify_command_feedback(&self, id: CommandFeedbackId) -> CallbackEpochMatch {
        self.command_feedback.classify(id)
    }

    /// Retires visible command feedback on a provider-session boundary.
    pub fn invalidate_command_feedback(&mut self) {
        self.command_feedback.invalidate();
    }

    /// Dismisses only the matching visible command feedback.
    #[must_use]
    pub fn dismiss_command_feedback(&mut self, id: CommandFeedbackId) -> CallbackEpochMatch {
        self.command_feedback.finish(id)
    }

    /// Records foreground monitoring intent.
    pub fn request_monitor(&mut self, request: MusicMonitorRequest) {
        self.monitor.request(request);
    }

    /// Cancels all monitor and provider work, including authorization.
    pub fn cancel_monitor(&mut self) -> MusicTransportCompletion {
        self.monitor.cancel();
        self.monitor_generation.invalidate();
        self.authorization.invalidate();
        self.retire_all_provider_work()
    }

    /// Admits one foreground effect with a Rust-owned correlation identity.
    #[must_use]
    pub fn begin_monitor(&mut self) -> Option<MusicMonitorEffect> {
        let start = self.monitor.pending_start()?;
        let generation = self.monitor_generation.begin()?;
        self.monitor.take_start()?;
        Some(MusicMonitorEffect { generation, start })
    }

    /// Classifies a foreground monitor task without ending it.
    #[must_use]
    pub fn classify_monitor(&self, generation: MonitorId) -> CallbackEpochMatch {
        self.monitor_generation.classify(generation)
    }

    /// Ends only the matching foreground monitor task.
    #[must_use]
    pub fn finish_monitor(&mut self, generation: MonitorId) -> CallbackEpochMatch {
        self.monitor_generation.finish(generation)
    }

    /// Returns the next poll deadline only while this monitor and provider work remain current.
    #[must_use]
    pub fn next_monitor_poll(
        &self,
        generation: MonitorId,
        work_state: MusicProviderWorkState,
        now_ms: u64,
    ) -> Option<MusicDeadlineEffect<MonitorId>> {
        if self.monitor_generation.classify(generation) == CallbackEpochMatch::Stale
            || matches!(
                work_state,
                MusicProviderWorkState::RequiresUserAction | MusicProviderWorkState::Unavailable
            )
        {
            return None;
        }
        Some(MusicDeadlineEffect {
            id: generation,
            deadline_ms: now_ms.saturating_add(MONITOR_POLL_INTERVAL_MS),
        })
    }

    /// Whether the foreground scene currently permits provider work.
    #[must_use]
    pub const fn is_scene_active(&self) -> bool {
        self.monitor.is_scene_active()
    }

    /// Suspends provider work while retaining passive observation intent and an
    /// authorization callback already handed to the provider application.
    pub fn suspend(&mut self) -> MusicProviderSuspension {
        self.monitor.suspend();
        self.monitor_generation.invalidate();
        let observation_gap = self.observing;
        let cancelled_transport_request_id = self.cancel_current_transport().finished_request_id();
        self.invalidate_provider_work();
        MusicProviderSuspension {
            observation_gap,
            cancelled_transport_request_id,
        }
    }

    /// Restores foreground monitoring intent without granting authorization.
    #[must_use]
    pub fn resume(&mut self) -> MusicMonitorResume {
        self.monitor.resume()
    }

    /// Begins one replaceable provider SDK object generation.
    #[must_use]
    pub fn begin_provider_session(&mut self) -> Option<ProviderSessionId> {
        if !self.provider_session.can_begin() {
            let _ = self.retire_all_provider_work();
            self.command_feedback.invalidate();
            return None;
        }
        let _ = self.retire_all_provider_work();
        self.command_feedback.invalidate();
        let generation = self.provider_session.begin()?;
        self.observing = true;
        Some(generation)
    }

    /// Classifies a provider callback without ending the generation.
    #[must_use]
    pub fn classify_provider_session(&self, id: ProviderSessionId) -> CallbackEpochMatch {
        self.provider_session.classify(id)
    }

    /// Ends only the matching provider generation and cancels its transport.
    #[must_use]
    pub fn retire_provider_session(&mut self, id: ProviderSessionId) -> MusicTransportCompletion {
        if self.provider_session.finish(id) == CallbackEpochMatch::Stale {
            return MusicTransportCompletion::Stale;
        }
        self.observing = false;
        self.command_feedback.invalidate();
        self.connection.reset();
        self.player_state.reset();
        self.artwork.reset();
        self.cancel_current_transport()
    }

    /// Begins a user authorization or silent renewal transaction.
    #[must_use]
    pub fn begin_authorization(
        &mut self,
        kind: AuthorizationTransactionKind,
    ) -> Option<AuthorizationId> {
        self.authorization.begin(kind)
    }

    /// Begins authorization and returns its Rust-owned timeout deadline.
    #[must_use]
    pub fn begin_authorization_effect(
        &mut self,
        kind: AuthorizationTransactionKind,
        now_ms: u64,
    ) -> Option<MusicDeadlineEffect<AuthorizationId>> {
        let deadline_ms = match kind {
            // Interactive login is completed by the provider handoff. It must
            // not expire while a human is in the provider app.
            AuthorizationTransactionKind::Authorizing => u64::MAX,
            AuthorizationTransactionKind::Renewing => {
                now_ms.saturating_add(AUTHORIZATION_TIMEOUT_MS)
            }
        };
        Some(MusicDeadlineEffect {
            id: self.begin_authorization(kind)?,
            deadline_ms,
        })
    }

    /// Classifies an authorization callback without finishing it.
    #[must_use]
    pub fn classify_authorization(&self, id: AuthorizationId) -> AuthorizationTransactionMatch {
        self.authorization.classify(id)
    }

    /// Finishes the matching authorization transaction exactly once.
    #[must_use]
    pub fn finish_authorization(&mut self, id: AuthorizationId) -> AuthorizationTransactionMatch {
        self.authorization.finish(id)
    }

    /// Explicitly retires the current authorization transaction.
    pub fn invalidate_authorization(&mut self) {
        self.authorization.invalidate();
    }

    /// Begins a bounded provider connection attempt.
    #[must_use]
    pub fn begin_connection_attempt(
        &mut self,
        now_ms: u64,
    ) -> Option<MusicConnectionAttemptEffect> {
        let previous_id = self.connection.current_id();
        let attempt_id = self.connection.begin_attempt_id(now_ms)?;
        let transport = previous_id.map_or(MusicTransportCompletion::Stale, |id| {
            self.cancel_transport_for_connection_current(id)
        });
        Some(MusicConnectionAttemptEffect {
            attempt_id,
            transport,
        })
    }

    /// Classifies a connection or player callback without changing retry state.
    #[must_use]
    pub fn classify_connection(
        &self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionCallback {
        self.connection.classify_at(id, now_ms)
    }

    /// Accepts success only for the current connection attempt.
    #[must_use]
    pub fn connection_established(
        &mut self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionCallback {
        self.connection.established_for_at(id, now_ms)
    }

    /// Accepts failure and retires all work owned by the connection.
    #[must_use]
    pub fn connection_failed_effect(
        &mut self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionEffect {
        self.end_connection(id, now_ms)
    }

    /// Retires a timed-out attempt when its SDK reports success too late.
    #[must_use]
    pub fn connection_expired_effect(
        &mut self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionEffect {
        let callback = self.connection.expired_for(id, now_ms);
        if callback == MusicConnectionCallback::Stale {
            return MusicConnectionEffect {
                callback,
                transport: MusicTransportCompletion::Stale,
            };
        }
        self.player_state.reset();
        MusicConnectionEffect {
            callback,
            transport: self.cancel_transport_for_connection_current(id),
        }
    }

    /// Accepts disconnection and retires all work owned by the connection.
    #[must_use]
    pub fn connection_disconnected_effect(
        &mut self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionEffect {
        self.end_connection(id, now_ms)
    }

    /// Begins or replaces an expired player-state request.
    #[must_use]
    pub fn begin_player_state_request(&mut self, now_ms: u64) -> Option<PlayerStateRequestId> {
        self.player_state.begin(now_ms)
    }

    /// Completes only the matching player-state request.
    #[must_use]
    pub fn complete_player_state_request(
        &mut self,
        id: PlayerStateRequestId,
        now_ms: u64,
    ) -> MusicPlayerRequestCompletion {
        self.player_state.complete(id, now_ms)
    }

    /// Completes a poll only when no newer authoritative observation arrived.
    #[must_use]
    pub fn complete_player_state_request_if_current(
        &mut self,
        id: PlayerStateRequestId,
        observation_revision: ObservationRevision,
        now_ms: u64,
    ) -> MusicPlayerRequestCompletion {
        self.player_state
            .complete_if_current(id, observation_revision, now_ms)
    }

    /// Revision of the latest authoritative player observation.
    #[must_use]
    pub fn player_state_observation_revision(&self) -> ObservationRevision {
        self.player_state.observation_revision()
    }

    /// Refreshes the current player-state freshness window.
    pub fn mark_player_state_observed(&mut self, now_ms: u64) {
        self.player_state.mark_observed(now_ms);
    }

    /// Whether the cached player state exceeded the portable freshness window.
    #[must_use]
    pub fn is_player_state_stale(&self, now_ms: u64) -> bool {
        self.player_state.is_stale(now_ms)
    }

    /// Begins one bounded artwork request.
    #[must_use]
    pub fn begin_artwork_effect(
        &mut self,
        provider_generation: ProviderSessionId,
        now_ms: u64,
    ) -> Option<MusicDeadlineEffect<ArtworkRequestId>> {
        if self.provider_session.classify(provider_generation) == CallbackEpochMatch::Stale {
            return None;
        }
        self.artwork.begin().map(|id| MusicDeadlineEffect {
            id,
            deadline_ms: now_ms.saturating_add(ARTWORK_TIMEOUT_MS),
        })
    }

    /// Completes only the matching artwork request.
    #[must_use]
    pub fn complete_artwork_request(
        &mut self,
        provider_generation: ProviderSessionId,
        id: ArtworkRequestId,
    ) -> MusicPlayerRequestCompletion {
        if self.provider_session.classify(provider_generation) == CallbackEpochMatch::Stale {
            return MusicPlayerRequestCompletion::Stale;
        }
        self.artwork.complete(id)
    }

    /// Starts a fresh artwork budget without reusing callback identities.
    pub fn reset_artwork(&mut self) {
        self.artwork.reset();
        self.artwork_retry.invalidate();
    }

    /// Schedules the next bounded artwork attempt with a distinct Rust identity.
    #[must_use]
    pub fn begin_artwork_retry_effect(
        &mut self,
        provider_generation: ProviderSessionId,
        now_ms: u64,
    ) -> Option<MusicDeadlineEffect<ArtworkRetryId>> {
        if self.provider_session.classify(provider_generation) == CallbackEpochMatch::Stale
            || !self.artwork.can_retry()
        {
            return None;
        }
        Some(MusicDeadlineEffect {
            id: self.artwork_retry.begin()?,
            deadline_ms: now_ms.saturating_add(ARTWORK_RETRY_DELAY_MS),
        })
    }

    /// Completes only the current provider's matching artwork retry delay.
    #[must_use]
    pub fn complete_artwork_retry(
        &mut self,
        provider_generation: ProviderSessionId,
        id: ArtworkRetryId,
    ) -> CallbackEpochMatch {
        if self.provider_session.classify(provider_generation) == CallbackEpochMatch::Stale {
            return CallbackEpochMatch::Stale;
        }
        self.artwork_retry.finish(id)
    }

    /// Starts one transport command when another is not pending.
    #[must_use]
    pub fn begin_transport_effect(
        &mut self,
        provider_generation: ProviderSessionId,
        now_ms: u64,
    ) -> Option<MusicDeadlineEffect<TransportRequestId>> {
        self.begin_transport_effect_for_connection(provider_generation, None, now_ms)
    }

    /// Starts one transport command owned by a specific connection attempt.
    #[must_use]
    pub fn begin_transport_effect_for_connection(
        &mut self,
        provider_generation: ProviderSessionId,
        connection_attempt_id: Option<ConnectionAttemptId>,
        now_ms: u64,
    ) -> Option<MusicDeadlineEffect<TransportRequestId>> {
        if self.provider_session.classify(provider_generation) == CallbackEpochMatch::Stale
            || self.pending_transport.is_some()
            || connection_attempt_id.is_some_and(|id| !self.connection.is_connected(id))
        {
            return None;
        }
        let request_id = self
            .last_transport_id
            .map_or(Some(1), |last_id| last_id.checked_add(1))?;
        self.last_transport_id = Some(request_id);
        self.pending_transport = Some(PendingTransport {
            provider_generation,
            connection_attempt_id,
            request_id: TransportRequestId::from_raw(request_id),
            started_at_ms: now_ms,
        });
        Some(MusicDeadlineEffect {
            id: TransportRequestId::from_raw(request_id),
            deadline_ms: now_ms.saturating_add(TRANSPORT_TIMEOUT_MS),
        })
    }

    /// Finishes the current transport request exactly once.
    #[must_use]
    pub fn finish_transport(
        &mut self,
        provider_generation: ProviderSessionId,
        request_id: TransportRequestId,
        outcome: MusicTransportOutcome,
        now_ms: u64,
    ) -> MusicTransportCompletion {
        let Some(pending) = self.pending_transport else {
            return MusicTransportCompletion::Stale;
        };
        if pending.provider_generation != provider_generation || pending.request_id != request_id {
            return MusicTransportCompletion::Stale;
        }
        if pending.connection_attempt_id.is_some_and(|id| {
            self.connection.classify_at(id, now_ms) == MusicConnectionCallback::Stale
        }) {
            return self.finish_transport_unchecked(
                provider_generation,
                request_id,
                MusicTransportOutcome::Cancelled,
            );
        }
        self.finish_transport_unchecked(provider_generation, request_id, outcome)
    }

    fn finish_transport_unchecked(
        &mut self,
        provider_generation: ProviderSessionId,
        request_id: TransportRequestId,
        outcome: MusicTransportOutcome,
    ) -> MusicTransportCompletion {
        if self.pending_transport.is_none_or(|pending| {
            pending.provider_generation != provider_generation || pending.request_id != request_id
        }) {
            return MusicTransportCompletion::Stale;
        }
        self.pending_transport = None;
        MusicTransportCompletion::Finished {
            request_id,
            outcome,
        }
    }

    /// Times out a request only after the portable deadline.
    #[must_use]
    pub fn expire_transport(
        &mut self,
        provider_generation: ProviderSessionId,
        now_ms: u64,
    ) -> MusicTransportCompletion {
        let Some(pending) = self.pending_transport else {
            return MusicTransportCompletion::Stale;
        };
        if pending.provider_generation != provider_generation {
            return MusicTransportCompletion::Stale;
        }
        if now_ms.saturating_sub(pending.started_at_ms) < TRANSPORT_TIMEOUT_MS {
            return MusicTransportCompletion::Pending;
        }
        self.finish_transport_unchecked(
            provider_generation,
            pending.request_id,
            MusicTransportOutcome::TimedOut,
        )
    }

    /// Cancels the pending transport request, if any.
    #[must_use]
    pub fn cancel_transport(
        &mut self,
        provider_generation: ProviderSessionId,
        request_id: TransportRequestId,
    ) -> MusicTransportCompletion {
        self.finish_transport_unchecked(
            provider_generation,
            request_id,
            MusicTransportOutcome::Cancelled,
        )
    }

    /// Cancels the request owned by a connection that just ended.
    #[must_use]
    pub fn cancel_transport_for_connection(
        &mut self,
        provider_generation: ProviderSessionId,
        connection_attempt_id: ConnectionAttemptId,
    ) -> MusicTransportCompletion {
        let Some(pending) = self.pending_transport else {
            return MusicTransportCompletion::Stale;
        };
        if pending.provider_generation != provider_generation
            || pending.connection_attempt_id != Some(connection_attempt_id)
        {
            return MusicTransportCompletion::Stale;
        }
        self.finish_transport_unchecked(
            provider_generation,
            pending.request_id,
            MusicTransportOutcome::Cancelled,
        )
    }

    fn cancel_current_transport(&mut self) -> MusicTransportCompletion {
        let Some(pending) = self.pending_transport else {
            return MusicTransportCompletion::Stale;
        };
        self.finish_transport_unchecked(
            pending.provider_generation,
            pending.request_id,
            MusicTransportOutcome::Cancelled,
        )
    }

    fn end_connection(&mut self, id: ConnectionAttemptId, now_ms: u64) -> MusicConnectionEffect {
        let callback = self.connection.failed_for(id, now_ms);
        if callback == MusicConnectionCallback::Stale {
            return MusicConnectionEffect {
                callback,
                transport: MusicTransportCompletion::Stale,
            };
        }
        self.player_state.reset();
        MusicConnectionEffect {
            callback,
            transport: self.cancel_transport_for_connection_current(id),
        }
    }

    fn cancel_transport_for_connection_current(
        &mut self,
        connection_attempt_id: ConnectionAttemptId,
    ) -> MusicTransportCompletion {
        let Some(pending) = self.pending_transport else {
            return MusicTransportCompletion::Stale;
        };
        if pending.connection_attempt_id != Some(connection_attempt_id) {
            return MusicTransportCompletion::Stale;
        }
        self.finish_transport_unchecked(
            pending.provider_generation,
            pending.request_id,
            MusicTransportOutcome::Cancelled,
        )
    }

    fn retire_all_provider_work(&mut self) -> MusicTransportCompletion {
        let completion = self.cancel_current_transport();
        self.invalidate_provider_work();
        completion
    }

    fn invalidate_provider_work(&mut self) {
        self.provider_session.invalidate();
        self.observing = false;
        self.connection.reset();
        self.player_state.reset();
        self.artwork.reset();
        self.artwork_retry.invalidate();
    }
}

impl MusicTransportCompletion {
    fn finished_request_id(self) -> Option<TransportRequestId> {
        match self {
            Self::Finished { request_id, .. } => Some(request_id),
            Self::Pending | Self::Stale => None,
        }
    }
}
