//! One owner for a provider monitor, callbacks, authorization, and transport.

use cutout_core::{Duration, MonotonicTimestamp};

use crate::callback_epoch::{
    AuthorizationTransaction, AuthorizationTransactionKind, AuthorizationTransactionMatch,
    CallbackEpoch, CallbackEpochMatch,
};
use crate::connection::{MusicConnection, MusicConnectionCallback};
use crate::ids::{
    ArtworkRequestId, ArtworkRetry, ArtworkRetryId, AuthorizationId, CommandFeedback,
    CommandFeedbackId, ConnectionAttemptId, EstablishedConnectionId, MonitorGeneration, MonitorId,
    ObservationRevision, PlayerStateRequestId, ProviderSessionGeneration, ProviderSessionId,
    TransportRequestId,
};
use crate::player_request::{
    MusicArtworkRequest, MusicPlayerRequest, MusicPlayerRequestCompletion,
    MusicPlayerRequestExpiration,
};
use crate::{
    MusicCommand, MusicMonitor, MusicMonitorRequest, MusicMonitorResume, MusicMonitorStart,
    MusicObservationOutcome, MusicSnapshot, observation::MusicObservationTracker,
};

const MONITOR_POLL_INTERVAL_MS: u64 = 1_000;
const AUTHORIZATION_TIMEOUT_MS: u64 = 20_000;
const TRANSPORT_TIMEOUT_MS: u64 = 10_000;
const ARTWORK_TIMEOUT_MS: u64 = 5_000;
const ARTWORK_RETRY_DELAY_MS: u64 = 1_000;

const fn deadline_after(now_ms: u64, delay_ms: u64) -> MonotonicTimestamp {
    MonotonicTimestamp::new(now_ms).saturating_add_duration(Duration::from_milliseconds(delay_ms))
}

/// One Rust-issued effect identity and its monotonic deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicDeadlineEffect<I> {
    /// Identity used to reject replacement callbacks.
    pub id: I,
    /// Absolute monotonic deadline for the platform executor.
    pub deadline: MonotonicTimestamp,
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

/// Scope that owns one provider transport request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicTransportOwner {
    /// Work owned by the logical provider session itself.
    Provider(ProviderSessionId),
    /// Work owned by one established provider connection.
    Connection {
        /// Logical provider session containing the connection.
        provider_generation: ProviderSessionId,
        /// Proof that the connection attempt was established.
        connection_id: EstablishedConnectionId,
    },
}

impl MusicTransportOwner {
    const fn provider_generation(self) -> ProviderSessionId {
        match self {
            Self::Provider(provider_generation)
            | Self::Connection {
                provider_generation,
                ..
            } => provider_generation,
        }
    }
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
    owner: MusicTransportOwner,
    request_id: TransportRequestId,
    started_at: MonotonicTimestamp,
    command: MusicCommand,
}

#[derive(Debug)]
enum MusicTransportState {
    Available { last_request_id: TransportRequestId },
    Pending(PendingTransport),
}

impl Default for MusicTransportState {
    fn default() -> Self {
        Self::Available {
            last_request_id: TransportRequestId::from_raw(0),
        }
    }
}

impl MusicTransportState {
    fn begin(
        &mut self,
        owner: MusicTransportOwner,
        command: MusicCommand,
        now: MonotonicTimestamp,
    ) -> Option<PendingTransport> {
        let request_id = match self {
            Self::Available { last_request_id } => last_request_id.next()?,
            Self::Pending(_) => return None,
        };
        let pending = PendingTransport {
            owner,
            request_id,
            started_at: now,
            command,
        };
        *self = Self::Pending(pending);
        Some(pending)
    }

    const fn pending(&self) -> Option<PendingTransport> {
        match self {
            Self::Pending(pending) => Some(*pending),
            Self::Available { .. } => None,
        }
    }

    fn finish(
        &mut self,
        provider_generation: ProviderSessionId,
        request_id: TransportRequestId,
    ) -> Option<PendingTransport> {
        let pending = self.pending().filter(|pending| {
            pending.owner.provider_generation() == provider_generation
                && pending.request_id == request_id
        })?;
        *self = Self::Available {
            last_request_id: pending.request_id,
        };
        Some(pending)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum MusicObservationOwnership {
    #[default]
    Idle,
    Active,
}

impl MusicObservationOwnership {
    const fn has_gap(self) -> bool {
        matches!(self, Self::Active)
    }
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
    transport: MusicTransportState,
    observations: MusicObservationTracker,
    observation_ownership: MusicObservationOwnership,
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
            transport: MusicTransportState::default(),
            observations: MusicObservationTracker::new(),
            observation_ownership: MusicObservationOwnership::Idle,
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

    /// Orders and classifies a provider observation under this session's correlation state.
    pub fn observe_music(&mut self, snapshot: MusicSnapshot) -> MusicObservationOutcome {
        self.observations.observe(snapshot)
    }

    /// Drops provider observation and command correlation without fabricating a transition.
    pub fn reset_observation_correlation(&mut self) {
        self.observations.reset();
    }

    /// Drops observation baselines so the next accepted value starts a new history association.
    pub fn reset_observation_baselines(&mut self) {
        self.observations.reset_observations();
    }

    /// Drops accepted command correlation while preserving provider observations.
    pub fn clear_pending_command_correlation(&mut self) {
        self.observations.clear_pending_skips();
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
            deadline: deadline_after(now_ms, MONITOR_POLL_INTERVAL_MS),
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
        let observation_gap = self.observation_ownership.has_gap();
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
        let _ = self.retire_all_provider_work();
        self.command_feedback.invalidate();
        let generation = self.provider_session.begin()?;
        self.observation_ownership = MusicObservationOwnership::Active;
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
        self.command_feedback.invalidate();
        self.retire_all_provider_work()
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
        let deadline = match kind {
            // Interactive login is completed by the provider handoff. It must
            // not expire while a human is in the provider app.
            AuthorizationTransactionKind::Authorizing => MonotonicTimestamp::new(u64::MAX),
            AuthorizationTransactionKind::Renewing => {
                deadline_after(now_ms, AUTHORIZATION_TIMEOUT_MS)
            }
        };
        Some(MusicDeadlineEffect {
            id: self.begin_authorization(kind)?,
            deadline,
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
        let previous_id = self.connection.callback_id();
        let attempt_id = self.connection.begin_attempt_id(now_ms)?;
        let transport = previous_id.map_or(MusicTransportCompletion::Stale, |id| {
            self.cancel_transport_for_connection_current(EstablishedConnectionId::from_raw(
                id.raw(),
            ))
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
    ) -> Option<EstablishedConnectionId> {
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
        self.finish_connection(id, callback)
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

    /// Retires only the matching player-state request after its deadline.
    #[must_use]
    pub fn expire_player_state_request(
        &mut self,
        id: PlayerStateRequestId,
        now_ms: u64,
    ) -> MusicPlayerRequestExpiration {
        self.player_state.expire(id, now_ms)
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
            deadline: deadline_after(now_ms, ARTWORK_TIMEOUT_MS),
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
            deadline: deadline_after(now_ms, ARTWORK_RETRY_DELAY_MS),
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

    /// Starts one transport command for a provider or established connection.
    #[must_use]
    pub fn begin_transport_effect(
        &mut self,
        owner: MusicTransportOwner,
        command: MusicCommand,
        now_ms: u64,
    ) -> Option<MusicDeadlineEffect<TransportRequestId>> {
        let owner_is_current = match owner {
            MusicTransportOwner::Provider(provider_generation) => {
                self.provider_session.classify(provider_generation) == CallbackEpochMatch::Current
            }
            MusicTransportOwner::Connection {
                provider_generation,
                connection_id,
            } => {
                self.provider_session.classify(provider_generation) == CallbackEpochMatch::Current
                    && self.connection.is_established(connection_id)
            }
        };
        if !owner_is_current {
            return None;
        }
        let pending = self
            .transport
            .begin(owner, command, MonotonicTimestamp::new(now_ms))?;
        if matches!(command, MusicCommand::Previous | MusicCommand::Next) {
            self.observations
                .issue_skip(pending.request_id, pending.started_at);
        }
        Some(MusicDeadlineEffect {
            id: pending.request_id,
            deadline: deadline_after(now_ms, TRANSPORT_TIMEOUT_MS),
        })
    }

    /// Finishes the current transport request exactly once.
    #[must_use]
    pub fn finish_transport(
        &mut self,
        provider_generation: ProviderSessionId,
        request_id: TransportRequestId,
        outcome: MusicTransportOutcome,
    ) -> MusicTransportCompletion {
        let Some(pending) = self.transport.pending() else {
            return MusicTransportCompletion::Stale;
        };
        if pending.owner.provider_generation() != provider_generation
            || pending.request_id != request_id
        {
            return MusicTransportCompletion::Stale;
        }
        if let MusicTransportOwner::Connection { connection_id, .. } = pending.owner
            && !self.connection.is_established(connection_id)
        {
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
        let Some(pending) = self.transport.finish(provider_generation, request_id) else {
            return MusicTransportCompletion::Stale;
        };
        if outcome != MusicTransportOutcome::Accepted
            && matches!(pending.command, MusicCommand::Previous | MusicCommand::Next)
        {
            self.observations.cancel_skip(request_id);
        }
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
        let Some(pending) = self.transport.pending() else {
            return MusicTransportCompletion::Stale;
        };
        if pending.owner.provider_generation() != provider_generation {
            return MusicTransportCompletion::Stale;
        }
        if MonotonicTimestamp::new(now_ms)
            .saturating_duration_since(pending.started_at)
            .as_milliseconds()
            < TRANSPORT_TIMEOUT_MS
        {
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

    fn cancel_current_transport(&mut self) -> MusicTransportCompletion {
        let Some(pending) = self.transport.pending() else {
            return MusicTransportCompletion::Stale;
        };
        self.finish_transport_unchecked(
            pending.owner.provider_generation(),
            pending.request_id,
            MusicTransportOutcome::Cancelled,
        )
    }

    fn end_connection(&mut self, id: ConnectionAttemptId, now_ms: u64) -> MusicConnectionEffect {
        let callback = self.connection.failed_for(id, now_ms);
        self.finish_connection(id, callback)
    }

    fn finish_connection(
        &mut self,
        id: ConnectionAttemptId,
        callback: MusicConnectionCallback,
    ) -> MusicConnectionEffect {
        if callback == MusicConnectionCallback::Stale {
            return MusicConnectionEffect {
                callback,
                transport: MusicTransportCompletion::Stale,
            };
        }
        self.command_feedback.invalidate();
        self.player_state.reset();
        MusicConnectionEffect {
            callback,
            transport: self.cancel_transport_for_connection_current(
                EstablishedConnectionId::from_raw(id.raw()),
            ),
        }
    }

    fn cancel_transport_for_connection_current(
        &mut self,
        connection_id: EstablishedConnectionId,
    ) -> MusicTransportCompletion {
        let Some(pending) = self.transport.pending() else {
            return MusicTransportCompletion::Stale;
        };
        if !matches!(
            pending.owner,
            MusicTransportOwner::Connection {
                connection_id: current,
                ..
            } if current == connection_id
        ) {
            return MusicTransportCompletion::Stale;
        }
        self.finish_transport_unchecked(
            pending.owner.provider_generation(),
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
        self.observation_ownership = MusicObservationOwnership::Idle;
        self.connection.reset();
        self.player_state.reset();
        self.artwork.reset();
        self.artwork_retry.invalidate();
        self.observations.clear_pending_skips();
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

#[cfg(test)]
mod tests {
    use super::{MusicTransportOwner, MusicTransportState};
    use crate::{
        MusicCommand,
        ids::{ProviderSessionId, TransportRequestId},
    };
    use cutout_core::MonotonicTimestamp;

    #[test]
    fn exhausted_transport_identity_does_not_create_a_pending_state() {
        let mut state = MusicTransportState::Available {
            last_request_id: TransportRequestId::from_raw(u64::MAX),
        };

        assert!(
            state
                .begin(
                    MusicTransportOwner::Provider(ProviderSessionId::from_raw(1)),
                    MusicCommand::Play,
                    MonotonicTimestamp::new(0),
                )
                .is_none()
        );
        assert!(matches!(
            state,
            MusicTransportState::Available {
                last_request_id: id
            } if id == TransportRequestId::from_raw(u64::MAX)
        ));
    }
}
