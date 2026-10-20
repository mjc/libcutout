//! Rust-owned provider lifecycle state shared by native bindings and Rust UIs.

use std::sync::{Mutex, PoisonError};

use cutout_core::{MonotonicTimestamp, WallClockUnixTimestamp};
use cutout_music::callback_epoch::CallbackEpochMatch;
use cutout_music::connection::MusicConnectionCallback;
use cutout_music::ids::{
    ArtworkRequestId, ArtworkRetryId, AuthorizationId, CommandFeedbackId, ConnectionAttemptId,
    EstablishedConnectionId, HistoryTransitionId, MonitorId, ObservationRevision, PlayHandoffId,
    PlayerStateRequestId, ProviderSessionId, TransportRequestId,
};
use cutout_music::player_request::{MusicPlayerRequestCompletion, MusicPlayerRequestExpiration};
use cutout_music::provider_lifecycle::{
    self as lifecycle, MusicConnectionAttemptAdmission, MusicConnectionEffect, MusicMonitorEffect,
    MusicPlaybackTransport, MusicProviderLifecycle, MusicProviderSuspension,
    MusicProviderWorkState, MusicTransportCompletion, MusicTransportOutcome, MusicTransportOwner,
};
use cutout_music::{
    MusicCapabilities, MusicCommand, MusicMonitorResume, MusicObservationOutcome,
    MusicObservationTiming, MusicPlaybackState, MusicProvider, MusicSnapshot,
    MusicUnsettledHistoryTransition,
};

/// Stable provider interface used to select a native SDK.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderInterface {
    AppleMusicSystemPlayer,
    SpotifyAppRemote,
    AppHandoffOnly,
}

/// Provider storage and presentation keys selected by the shared API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderProfile {
    pub interface: ProviderInterface,
    pub storage_key: &'static str,
    pub title_key: &'static str,
    pub unavailable_key: Option<&'static str>,
}

/// Result of admitting the limited `SoundCloud` app-handoff operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SoundCloudCommandAdmission {
    Refused,
    Unavailable,
    Handoff {
        url: String,
        effect: lifecycle::MusicDeadlineEffect<TransportRequestId>,
    },
}

/// Returns the fixed interface and presentation keys for a provider.
#[must_use]
pub const fn provider_profile(provider: MusicProvider) -> ProviderProfile {
    match provider {
        MusicProvider::AppleMusic => ProviderProfile {
            interface: ProviderInterface::AppleMusicSystemPlayer,
            storage_key: "apple_music",
            title_key: "music.provider.apple_music",
            unavailable_key: None,
        },
        MusicProvider::Spotify => ProviderProfile {
            interface: ProviderInterface::SpotifyAppRemote,
            storage_key: "spotify",
            title_key: "music.provider.spotify",
            unavailable_key: None,
        },
        MusicProvider::SoundCloud => ProviderProfile {
            interface: ProviderInterface::AppHandoffOnly,
            storage_key: "soundcloud",
            title_key: "music.provider.soundcloud",
            unavailable_key: Some("music.soundcloud.unavailable"),
        },
    }
}

/// Restores persisted provider selection while preserving Apple Music default.
#[must_use]
pub fn provider_from_storage(value: Option<&str>) -> MusicProvider {
    match value {
        Some("spotify") => MusicProvider::Spotify,
        Some("soundcloud") => MusicProvider::SoundCloud,
        _ => MusicProvider::AppleMusic,
    }
}

/// Returns the supported local `SoundCloud` snapshot at the supplied host clock.
#[must_use]
pub fn soundcloud_unavailable_snapshot(now_ms: u64) -> MusicSnapshot {
    cutout_music::soundcloud::unavailable_snapshot(MonotonicTimestamp::new(now_ms))
}

/// Matches callback paths with only empty/slash root equivalence.
#[must_use]
pub fn callback_path_matches(expected: &str, actual: &str) -> bool {
    cutout_music::music_callback_path_matches(expected, actual)
}

/// Returns the shared localized title key for the playback state.
#[must_use]
pub const fn playback_title_key(state: MusicPlaybackState) -> &'static str {
    state.fallback_title_key()
}

/// Selects local playback before the authorized Web API fallback.
#[must_use]
pub fn playback_transport(
    app_remote_connected: bool,
    web_playback_authorized: bool,
) -> MusicPlaybackTransport {
    cutout_music::provider_lifecycle::select_playback_transport(
        app_remote_connected,
        web_playback_authorized,
    )
}

/// One thread-safe owner for provider lifecycle and cross-call observation state.
#[derive(Debug, Default)]
pub struct ProviderLifecycle {
    lifecycle: Mutex<MusicProviderLifecycle>,
    correlation_ride_id: Mutex<Option<String>>,
    player_observations: Mutex<[Option<MusicSnapshot>; 3]>,
}

/// Retired ride association and its unsettled history payloads.
#[derive(Debug)]
pub struct RetiredHistoryAssociation {
    pub ride_id: String,
    pub transitions: Vec<MusicUnsettledHistoryTransition>,
}

impl ProviderLifecycle {
    /// Creates an idle lifecycle with no observation association.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Begins one replaceable command-feedback presentation.
    #[must_use]
    pub fn begin_command_feedback(&self) -> Option<CommandFeedbackId> {
        self.with_lifecycle(MusicProviderLifecycle::begin_command_feedback)
    }

    /// Classifies a command callback without changing the visible request.
    #[must_use]
    pub fn classify_command_feedback(&self, id: CommandFeedbackId) -> CallbackEpochMatch {
        self.with_lifecycle(|state| state.classify_command_feedback(id))
    }

    /// Dismisses only the matching visible command feedback.
    #[must_use]
    pub fn dismiss_command_feedback(&self, id: CommandFeedbackId) -> CallbackEpochMatch {
        self.with_lifecycle(|state| state.dismiss_command_feedback(id))
    }

    /// Retains foreground monitor intent.
    pub fn request_monitor(&self, request: cutout_music::MusicMonitorRequest) {
        self.with_lifecycle(|state| state.request_monitor(request));
    }

    /// Cancels monitoring and all provider work.
    #[must_use]
    pub fn cancel_monitor(&self) -> MusicTransportCompletion {
        self.with_lifecycle(MusicProviderLifecycle::cancel_monitor)
    }

    /// Admits one foreground monitor task.
    #[must_use]
    pub fn begin_monitor(&self) -> Option<MusicMonitorEffect> {
        self.with_lifecycle(MusicProviderLifecycle::begin_monitor)
    }

    /// Classifies a monitor task without ending it.
    #[must_use]
    pub fn classify_monitor(&self, generation: MonitorId) -> CallbackEpochMatch {
        self.with_lifecycle(|state| state.classify_monitor(generation))
    }

    /// Ends only the matching monitor task.
    #[must_use]
    pub fn finish_monitor(&self, generation: MonitorId) -> CallbackEpochMatch {
        self.with_lifecycle(|state| state.finish_monitor(generation))
    }

    /// Schedules a monitor poll only while provider work remains current.
    #[must_use]
    pub fn next_monitor_poll(
        &self,
        generation: MonitorId,
        work_state: MusicProviderWorkState,
        now_ms: u64,
    ) -> Option<lifecycle::MusicDeadlineEffect<MonitorId>> {
        self.with_lifecycle(|state| state.next_monitor_poll(generation, work_state, now_ms))
    }

    /// Returns whether the foreground scene currently permits provider work.
    #[must_use]
    pub fn is_scene_active(&self) -> bool {
        self.lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_scene_active()
    }

    /// Suspends provider work while retaining passive observation intent.
    pub fn suspend(&self) -> MusicProviderSuspension {
        self.with_lifecycle(MusicProviderLifecycle::suspend)
    }

    /// Restores the foreground scene without granting authorization.
    #[must_use]
    pub fn resume(&self) -> MusicMonitorResume {
        self.with_lifecycle(MusicProviderLifecycle::resume)
    }

    /// Begins a replaceable provider SDK object generation.
    #[must_use]
    pub fn begin_provider_session(&self) -> Option<ProviderSessionId> {
        self.with_lifecycle(MusicProviderLifecycle::begin_provider_session)
    }

    /// Invalidates command feedback when the provider session changes.
    pub fn invalidate_command_feedback(&self) {
        self.with_lifecycle(MusicProviderLifecycle::invalidate_command_feedback);
    }

    /// Removes only the matching history transition after durable handling.
    #[must_use]
    pub fn acknowledge_history_transition(
        &self,
        id: HistoryTransitionId,
    ) -> cutout_music::MusicHistoryTransitionAcknowledgement {
        self.with_lifecycle(|state| state.acknowledge_history_transition(id))
    }

    /// Drops accepted command correlation while preserving provider observations.
    pub fn clear_pending_command_correlation(&self) {
        self.with_lifecycle(MusicProviderLifecycle::clear_pending_command_correlation);
    }

    /// Classifies a provider callback without ending its generation.
    #[must_use]
    pub fn classify_provider_session(&self, id: ProviderSessionId) -> CallbackEpochMatch {
        self.with_lifecycle(|state| state.classify_provider_session(id))
    }

    /// Ends the current provider generation and cancels its transport.
    #[must_use]
    pub fn retire_provider_session(&self, id: ProviderSessionId) -> MusicTransportCompletion {
        self.with_lifecycle(|state| state.retire_provider_session(id))
    }

    /// Begins one authorization transaction and its deadline.
    #[must_use]
    pub fn begin_authorization_effect(
        &self,
        kind: cutout_music::callback_epoch::AuthorizationTransactionKind,
        now_ms: u64,
    ) -> Option<lifecycle::MusicDeadlineEffect<AuthorizationId>> {
        self.with_lifecycle(|state| state.begin_authorization_effect(kind, now_ms))
    }

    /// Classifies an authorization callback.
    #[must_use]
    pub fn classify_authorization(
        &self,
        id: AuthorizationId,
    ) -> cutout_music::callback_epoch::AuthorizationTransactionMatch {
        self.with_lifecycle(|state| state.classify_authorization(id))
    }

    /// Finishes the current authorization callback exactly once.
    #[must_use]
    pub fn finish_authorization(
        &self,
        id: AuthorizationId,
    ) -> cutout_music::callback_epoch::AuthorizationTransactionMatch {
        self.with_lifecycle(|state| state.finish_authorization(id))
    }

    /// Explicitly retires authorization.
    pub fn invalidate_authorization(&self) {
        self.with_lifecycle(MusicProviderLifecycle::invalidate_authorization);
    }

    /// Begins one bounded provider connection attempt.
    #[must_use]
    pub fn begin_connection_attempt(&self, now_ms: u64) -> MusicConnectionAttemptAdmission {
        self.with_lifecycle(|state| state.begin_connection_attempt(now_ms))
    }

    /// Classifies a connection or player callback without changing retry state.
    #[must_use]
    pub fn classify_connection(
        &self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionCallback {
        self.with_lifecycle(|state| state.classify_connection(id, now_ms))
    }

    /// Accepts success only for the current connection attempt.
    #[must_use]
    pub fn connection_established(
        &self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> Option<EstablishedConnectionId> {
        self.with_lifecycle(|state| state.connection_established(id, now_ms))
    }

    /// Accepts failure and retires work owned by the connection.
    #[must_use]
    pub fn connection_failed_effect(
        &self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionEffect {
        self.with_lifecycle(|state| state.connection_failed_effect(id, now_ms))
    }

    /// Retires a timed-out attempt that reported success too late.
    #[must_use]
    pub fn connection_expired_effect(
        &self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionEffect {
        self.with_lifecycle(|state| state.connection_expired_effect(id, now_ms))
    }

    /// Accepts disconnection and retires work owned by the connection.
    #[must_use]
    pub fn connection_disconnected_effect(
        &self,
        id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionEffect {
        self.with_lifecycle(|state| state.connection_disconnected_effect(id, now_ms))
    }

    /// Begins or replaces an expired player-state request.
    #[must_use]
    pub fn begin_player_state_request(&self, now_ms: u64) -> Option<PlayerStateRequestId> {
        self.with_lifecycle(|state| state.begin_player_state_request(now_ms))
    }

    /// Completes only the matching player-state request.
    #[must_use]
    pub fn complete_player_state_request(
        &self,
        id: PlayerStateRequestId,
        now_ms: u64,
    ) -> MusicPlayerRequestCompletion {
        self.with_lifecycle(|state| state.complete_player_state_request(id, now_ms))
    }

    /// Retires only the matching request after its deadline.
    #[must_use]
    pub fn expire_player_state_request(
        &self,
        id: PlayerStateRequestId,
        now_ms: u64,
    ) -> MusicPlayerRequestExpiration {
        self.with_lifecycle(|state| state.expire_player_state_request(id, now_ms))
    }

    /// Completes a poll only when no newer push observation superseded it.
    #[must_use]
    pub fn complete_player_state_request_if_current(
        &self,
        id: PlayerStateRequestId,
        observation_revision: ObservationRevision,
        now_ms: u64,
    ) -> MusicPlayerRequestCompletion {
        self.with_lifecycle(|state| {
            state.complete_player_state_request_if_current(id, observation_revision, now_ms)
        })
    }

    /// Returns the latest authoritative player-state revision.
    #[must_use]
    pub fn player_state_observation_revision(&self) -> ObservationRevision {
        self.lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .player_state_observation_revision()
    }

    /// Refreshes player-state freshness.
    pub fn mark_player_state_observed(&self, now_ms: u64) {
        self.with_lifecycle(|state| state.mark_player_state_observed(now_ms));
    }

    /// Returns whether cached player state is stale.
    #[must_use]
    pub fn is_player_state_stale(&self, now_ms: u64) -> bool {
        self.lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_player_state_stale(now_ms)
    }

    /// Begins one bounded artwork request.
    #[must_use]
    pub fn begin_artwork_effect(
        &self,
        provider_generation: ProviderSessionId,
        now_ms: u64,
    ) -> Option<lifecycle::MusicDeadlineEffect<ArtworkRequestId>> {
        self.with_lifecycle(|state| state.begin_artwork_effect(provider_generation, now_ms))
    }

    /// Completes only the matching provider artwork request.
    #[must_use]
    pub fn complete_artwork_request(
        &self,
        provider_generation: ProviderSessionId,
        id: ArtworkRequestId,
    ) -> MusicPlayerRequestCompletion {
        self.with_lifecycle(|state| state.complete_artwork_request(provider_generation, id))
    }

    /// Starts a fresh artwork request budget.
    pub fn reset_artwork(&self) {
        self.with_lifecycle(MusicProviderLifecycle::reset_artwork);
    }

    /// Schedules another bounded artwork attempt.
    #[must_use]
    pub fn begin_artwork_retry_effect(
        &self,
        provider_generation: ProviderSessionId,
        now_ms: u64,
    ) -> Option<lifecycle::MusicDeadlineEffect<ArtworkRetryId>> {
        self.with_lifecycle(|state| state.begin_artwork_retry_effect(provider_generation, now_ms))
    }

    /// Completes only the current provider's artwork retry delay.
    #[must_use]
    pub fn complete_artwork_retry(
        &self,
        provider_generation: ProviderSessionId,
        id: ArtworkRetryId,
    ) -> CallbackEpochMatch {
        self.with_lifecycle(|state| state.complete_artwork_retry(provider_generation, id))
    }

    /// Begins one provider or connection-owned transport request.
    #[must_use]
    pub fn begin_transport_effect(
        &self,
        owner: MusicTransportOwner,
        command: MusicCommand,
        now_ms: u64,
    ) -> Option<lifecycle::MusicDeadlineEffect<TransportRequestId>> {
        self.with_lifecycle(|state| state.begin_transport_effect(owner, command, now_ms))
    }

    /// Begins playback confirmation for an app handoff.
    #[must_use]
    pub fn begin_play_handoff(
        &self,
        now_ms: u64,
    ) -> Option<lifecycle::MusicDeadlineEffect<PlayHandoffId>> {
        self.with_lifecycle(|state| state.begin_play_handoff(now_ms))
    }

    /// Confirms playback from a fresh observation.
    #[must_use]
    pub fn observe_play_handoff(
        &self,
        id: PlayHandoffId,
        state: MusicPlaybackState,
        observed_at_ms: u64,
    ) -> Option<MusicTransportOutcome> {
        self.with_lifecycle(|lifecycle| lifecycle.observe_play_handoff(id, state, observed_at_ms))
    }

    /// Expires an unconfirmed provider app handoff.
    #[must_use]
    pub fn expire_play_handoff(
        &self,
        id: PlayHandoffId,
        now_ms: u64,
    ) -> Option<MusicTransportOutcome> {
        self.with_lifecycle(|state| state.expire_play_handoff(id, now_ms))
    }

    /// Retires a rejected or cancelled provider app handoff.
    #[must_use]
    pub fn finish_play_handoff(&self, id: PlayHandoffId) -> CallbackEpochMatch {
        self.with_lifecycle(|state| state.finish_play_handoff(id))
    }

    /// Finishes the matching provider transport request once.
    #[must_use]
    pub fn finish_transport(
        &self,
        provider_generation: ProviderSessionId,
        request_id: TransportRequestId,
        outcome: MusicTransportOutcome,
    ) -> MusicTransportCompletion {
        self.with_lifecycle(|state| {
            state.finish_transport(provider_generation, request_id, outcome)
        })
    }

    /// Expires a provider transport request at its Rust-owned deadline.
    #[must_use]
    pub fn expire_transport(
        &self,
        provider_generation: ProviderSessionId,
        now_ms: u64,
    ) -> MusicTransportCompletion {
        self.with_lifecycle(|state| state.expire_transport(provider_generation, now_ms))
    }

    /// Cancels the matching pending provider transport request.
    #[must_use]
    pub fn cancel_transport(
        &self,
        provider_generation: ProviderSessionId,
        request_id: TransportRequestId,
    ) -> MusicTransportCompletion {
        self.with_lifecycle(|state| state.cancel_transport(provider_generation, request_id))
    }

    fn with_lifecycle<R>(&self, operation: impl FnOnce(&mut MusicProviderLifecycle) -> R) -> R {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        operation(&mut lifecycle)
    }

    /// Accepts only a newer player observation for its provider.
    #[must_use]
    pub fn observe_player(&self, snapshot: MusicSnapshot) -> Option<MusicSnapshot> {
        let index = match snapshot.provider() {
            MusicProvider::AppleMusic => 0,
            MusicProvider::Spotify => 1,
            MusicProvider::SoundCloud => 2,
        };
        let mut latest = self
            .player_observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if latest[index]
            .as_ref()
            .is_some_and(|previous| previous.observed_at() >= snapshot.observed_at())
        {
            return None;
        }
        latest[index] = Some(snapshot.clone());
        Some(snapshot)
    }

    /// Associates an accepted observation with one ride and retains its history
    /// transition until the caller reports the durable receipt.
    ///
    /// A ride change is refused while the previous association has unsettled
    /// history. Out-of-order observations return `Ok(None)` without advancing
    /// the association.
    ///
    /// # Errors
    /// Returns [`ObserveRideError::PendingHistory`] when the previous ride has unsettled history,
    /// or [`ObserveRideError::HistoryFull`] when the bounded transition queue is full.
    pub fn observe_for_ride(
        &self,
        ride_id: Option<String>,
        snapshot: MusicSnapshot,
        wall_clock_at_ms: u64,
        clock_uncertainty_ms: u64,
    ) -> Result<Option<cutout_music::MusicObservationDecision>, ObserveRideError> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut correlation = self
            .correlation_ride_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if *correlation != ride_id {
            if lifecycle.has_pending_history() {
                return Err(ObserveRideError::PendingHistory);
            }
            lifecycle.reset_observation_correlation();
            *correlation = ride_id;
        }
        let timing = MusicObservationTiming::new(
            WallClockUnixTimestamp::from_milliseconds(wall_clock_at_ms),
            clock_uncertainty_ms,
        );
        match lifecycle.observe_music(snapshot, timing) {
            MusicObservationOutcome::Accepted(decision) => Ok(Some(*decision)),
            MusicObservationOutcome::OutOfOrder => Ok(None),
            MusicObservationOutcome::Full => Err(ObserveRideError::HistoryFull),
        }
    }

    /// Classifies an observation without changing the current ride association.
    ///
    /// # Errors
    /// Returns [`ObserveRideError::HistoryFull`] when the bounded transition queue is full.
    pub fn observe(
        &self,
        snapshot: MusicSnapshot,
        wall_clock_at_ms: u64,
        clock_uncertainty_ms: u64,
    ) -> Result<Option<cutout_music::MusicObservationDecision>, ObserveRideError> {
        let timing = MusicObservationTiming::new(
            WallClockUnixTimestamp::from_milliseconds(wall_clock_at_ms),
            clock_uncertainty_ms,
        );
        match self.with_lifecycle(|lifecycle| lifecycle.observe_music(snapshot, timing)) {
            MusicObservationOutcome::Accepted(decision) => Ok(Some(*decision)),
            MusicObservationOutcome::OutOfOrder => Ok(None),
            MusicObservationOutcome::Full => Err(ObserveRideError::HistoryFull),
        }
    }

    /// Resets ride correlation after all previously admitted observation work
    /// has settled.
    pub fn reset_observation_correlation(&self) {
        self.with_lifecycle(MusicProviderLifecycle::reset_observation_correlation);
        *self
            .correlation_ride_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Starts a new history association from the next accepted observation.
    pub fn reset_observation_baselines(&self) {
        self.with_lifecycle(MusicProviderLifecycle::reset_observation_baselines);
        *self
            .correlation_ride_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Retires the prior ride association after a ride change or closure.
    #[must_use]
    pub fn retire_closed_history(
        &self,
        current_ride_id: Option<&str>,
        current_closed: bool,
    ) -> Option<RetiredHistoryAssociation> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut correlation = self
            .correlation_ride_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let previous = correlation.as_ref()?;
        let current = current_ride_id?;
        if previous == current && !current_closed {
            return None;
        }
        let ride_id = previous.clone();
        let transitions = lifecycle.retire_history_association();
        *correlation = None;
        (!transitions.is_empty()).then_some(RetiredHistoryAssociation {
            ride_id,
            transitions,
        })
    }

    /// Checks observed capabilities before admitting a provider transport effect.
    #[must_use]
    pub fn begin_supported_transport_effect(
        &self,
        owner: MusicTransportOwner,
        command: MusicCommand,
        capabilities: MusicCapabilities,
        now_ms: u64,
    ) -> Option<lifecycle::MusicDeadlineEffect<TransportRequestId>> {
        if !capabilities.supports(command) {
            return None;
        }
        self.with_lifecycle(|lifecycle| lifecycle.begin_transport_effect(owner, command, now_ms))
    }

    /// Admits only explicit `SoundCloud` app opening through the shared lifecycle.
    #[must_use]
    pub fn begin_soundcloud_command(
        &self,
        provider_generation: ProviderSessionId,
        command: MusicCommand,
        now_ms: u64,
    ) -> SoundCloudCommandAdmission {
        let cutout_music::soundcloud::SoundCloudCommandAdmission::Handoff(handoff) =
            cutout_music::soundcloud::admit_command(command)
        else {
            return SoundCloudCommandAdmission::Refused;
        };
        let effect = self.with_lifecycle(|lifecycle| {
            lifecycle.begin_transport_effect(
                MusicTransportOwner::Provider(provider_generation),
                command,
                now_ms,
            )
        });
        let Some(effect) = effect else {
            return SoundCloudCommandAdmission::Unavailable;
        };
        SoundCloudCommandAdmission::Handoff {
            url: handoff.url().to_owned(),
            effect,
        }
    }
}

/// Why a music observation could not be associated with a ride.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserveRideError {
    PendingHistory,
    HistoryFull,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_requires_an_advertised_action() {
        let lifecycle = ProviderLifecycle::new();
        let provider = lifecycle
            .begin_provider_session()
            .expect("provider session");
        let owner = MusicTransportOwner::Provider(provider);
        let none = MusicCapabilities::new();
        assert!(
            lifecycle
                .begin_supported_transport_effect(owner, MusicCommand::Play, none, 1)
                .is_none()
        );
        let play = none.with(MusicCommand::Play);
        assert!(
            lifecycle
                .begin_supported_transport_effect(owner, MusicCommand::Play, play, 2)
                .is_some()
        );
        assert!(
            lifecycle
                .begin_supported_transport_effect(owner, MusicCommand::Pause, play, 3)
                .is_none()
        );
    }

    #[test]
    fn lifecycle_suspension_cancels_owned_transport_and_stales_prior_work() {
        let lifecycle = ProviderLifecycle::new();
        lifecycle.request_monitor(cutout_music::MusicMonitorRequest::Authorize);
        let monitor = lifecycle.begin_monitor().expect("monitor effect");
        let authorization = lifecycle
            .begin_authorization_effect(
                cutout_music::callback_epoch::AuthorizationTransactionKind::Authorizing,
                0,
            )
            .expect("authorization effect");
        let provider = lifecycle
            .begin_provider_session()
            .expect("provider session");
        let transport = lifecycle
            .begin_transport_effect(
                MusicTransportOwner::Provider(provider),
                MusicCommand::Play,
                100,
            )
            .expect("transport");

        let suspension = lifecycle.suspend();
        assert!(suspension.observation_gap);
        assert_eq!(
            suspension.cancelled_transport_request_id,
            Some(transport.id)
        );
        assert_eq!(
            lifecycle.classify_provider_session(provider),
            CallbackEpochMatch::Stale
        );
        assert_eq!(
            lifecycle.finish_authorization(authorization.id),
            cutout_music::callback_epoch::AuthorizationTransactionMatch::Authorizing
        );
        assert_eq!(lifecycle.resume(), MusicMonitorResume::Restored);
        assert_eq!(
            lifecycle.classify_monitor(monitor.generation),
            CallbackEpochMatch::Stale
        );
        assert_eq!(
            lifecycle.finish_transport(provider, transport.id, MusicTransportOutcome::Accepted),
            MusicTransportCompletion::Stale
        );
    }

    #[test]
    fn connection_end_cancels_only_its_owned_transport() {
        let lifecycle = ProviderLifecycle::new();
        let provider = lifecycle
            .begin_provider_session()
            .expect("provider session");
        let MusicConnectionAttemptAdmission::Started(effect) =
            lifecycle.begin_connection_attempt(0)
        else {
            panic!("expected connection attempt");
        };
        let connection = lifecycle
            .connection_established(effect.attempt_id, 0)
            .expect("established connection");
        let transport = lifecycle
            .begin_transport_effect(
                MusicTransportOwner::Connection {
                    provider_generation: provider,
                    connection_id: connection,
                },
                MusicCommand::Play,
                0,
            )
            .expect("transport");

        let ended = lifecycle.connection_failed_effect(effect.attempt_id, 100);
        assert_eq!(ended.callback, MusicConnectionCallback::Accepted);
        assert_eq!(
            ended.transport,
            MusicTransportCompletion::Finished {
                request_id: transport.id,
                outcome: MusicTransportOutcome::Cancelled,
            }
        );
    }

    #[test]
    fn player_request_expiry_distinguishes_pending_expired_and_stale() {
        let lifecycle = ProviderLifecycle::new();
        let request = lifecycle
            .begin_player_state_request(1_000)
            .expect("player request");
        assert_eq!(
            lifecycle.expire_player_state_request(request, 10_999),
            MusicPlayerRequestExpiration::Pending
        );
        assert_eq!(
            lifecycle.expire_player_state_request(request, 11_000),
            MusicPlayerRequestExpiration::Expired
        );
        assert_eq!(
            lifecycle.expire_player_state_request(request, 11_001),
            MusicPlayerRequestExpiration::Stale
        );
    }

    #[test]
    fn player_observations_keep_the_newest_value_per_provider() {
        let lifecycle = ProviderLifecycle::new();
        let position =
            cutout_music::MusicPlaybackPosition::new(None, None).expect("empty position is valid");
        let older = MusicSnapshot::new(
            MusicProvider::Spotify,
            "spotify:session:1",
            MusicPlaybackState::Playing,
            None,
            position,
            MonotonicTimestamp::new(10),
            MusicCapabilities::new(),
        )
        .expect("valid session observation");
        let newer = MusicSnapshot::new(
            MusicProvider::Spotify,
            "spotify:session:1",
            MusicPlaybackState::Paused,
            None,
            position,
            MonotonicTimestamp::new(11),
            MusicCapabilities::new(),
        )
        .expect("valid session observation");

        assert_eq!(lifecycle.observe_player(older.clone()), Some(older.clone()));
        assert_eq!(lifecycle.observe_player(older), None);
        assert_eq!(lifecycle.observe_player(newer.clone()), Some(newer));
    }
}
