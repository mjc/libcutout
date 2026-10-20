//! Bounded activity work, semantic fences and ordered terminal obligations.

use std::sync::{Mutex, PoisonError};

/// Identity used to fence activity work independently from payload presentation.
pub trait ActivityIdentity: Clone + Eq {
    fn platform_identifier(&self) -> &str;
}

/// Lifecycle phase relevant to activity admission, independent of terminal reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RidePhase {
    Idle,
    Starting,
    Active,
    Reconnecting,
    Stale,
    Ending,
    Ended,
}

/// Projection observation used by activity policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionState {
    Starting,
    Active,
    Stale,
    Other,
}

/// Snapshot needed by activity policy; unrelated presentation fields stay with the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityObservation<I> {
    pub identity: Option<I>,
    pub phase: RidePhase,
    pub projection: ProjectionState,
    pub last_telemetry_at_ms: Option<u64>,
}

/// A terminal obligation carries its original identity and reason until acknowledged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalEffect<I, R> {
    pub identity: I,
    pub reason: R,
}

/// Semantic lifecycle requests have separate fences from presentation replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleRequest {
    Background,
    Foreground,
    Terminal,
    TransportDisconnected { at_ms: u64 },
}

/// Effects admitted by the semantic request fence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleAdmission {
    Reduce,
    ReduceAndFlush,
    FlushOnly,
    Rejected,
}

/// Projection selected for background observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackgroundProjection {
    Update,
    Stale,
    None,
}

#[must_use]
pub fn pending_start<I>(snapshot: &ActivityObservation<I>) -> bool {
    snapshot.phase == RidePhase::Starting
        && snapshot.projection == ProjectionState::Starting
        && snapshot.identity.is_some()
}

#[must_use]
pub fn background_projection<I>(snapshot: &ActivityObservation<I>) -> BackgroundProjection {
    if snapshot.identity.is_none()
        || match snapshot.projection {
            ProjectionState::Active | ProjectionState::Stale => false,
            ProjectionState::Starting | ProjectionState::Other => true,
        }
    {
        return BackgroundProjection::None;
    }
    match snapshot.phase {
        RidePhase::Active => BackgroundProjection::Update,
        RidePhase::Reconnecting | RidePhase::Stale => BackgroundProjection::Stale,
        _ => BackgroundProjection::None,
    }
}

#[must_use]
pub fn admits_telemetry<I>(snapshot: &ActivityObservation<I>, telemetry_at_ms: u64) -> bool {
    snapshot.identity.is_some()
        && snapshot
            .last_telemetry_at_ms
            .is_none_or(|previous| telemetry_at_ms > previous)
        && match snapshot.phase {
            RidePhase::Starting | RidePhase::Active | RidePhase::Stale => true,
            RidePhase::Idle | RidePhase::Reconnecting | RidePhase::Ending | RidePhase::Ended => {
                false
            }
        }
}

#[must_use]
pub fn may_clear_marker<I>(snapshot: &ActivityObservation<I>) -> bool {
    snapshot.phase == RidePhase::Ended
}

/// One platform operation admitted by Rust. platform snapshot payloads remain host-owned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityWork<I, R> {
    /// Replaceable presentation of the latest requested ride snapshot.
    Presentation { request_id: u64 },
    /// Required cleanup of the exact Rust-issued ride identity.
    Terminal {
        request_id: u64,
        effect: TerminalEffect<I, R>,
        stale_after_ms: u64,
    },
}

impl<I, R> ActivityWork<I, R> {
    const fn request_id(&self) -> u64 {
        match self {
            Self::Presentation { request_id } | Self::Terminal { request_id, .. } => *request_id,
        }
    }
}

/// Native dispatch effect for an admitted request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityWorkAdmission<I, R> {
    /// This caller owns the sole platform worker until matching completion.
    Run { work: ActivityWork<I, R> },
    /// No caller waits; retain only the latest host presentation payload.
    Queued { replaced_request_id: Option<u64> },
    /// An old request or nonterminal effect cannot enter this queue.
    Rejected,
}

/// Constant-size queue readback for dispatch and behavioral diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityWorkSnapshot {
    /// Request currently owning the platform worker.
    pub in_flight_request_id: Option<u64>,
    /// Latest host snapshot awaiting the worker.
    pub pending_presentation_request_id: Option<u64>,
    /// Fixed identity cleanup awaiting the worker.
    pub pending_terminal_request_id: Option<u64>,
}

#[derive(Debug)]
struct WorkState<I, R> {
    latest_request_id: u64,
    latest_presentation_request_id: u64,
    latest_telemetry_at_ms: Option<u64>,
    latest_transport_request_id: u64,
    disconnected_at_ms: Option<u64>,
    latest_session_request_id: u64,
    latest_session_identity: Option<I>,
    pending_start_platform_identifier: Option<String>,
    latest_scene_request_id: u64,
    latest_background_request_id: u64,
    latest_terminal_request_id: u64,
    in_flight_request_id: Option<u64>,
    pending_presentation_request_id: Option<u64>,
    terminal: Option<ActivityWork<I, R>>,
    last_projection_telemetry_at_ms: Option<u64>,
}

/// One in-flight platform effect, one latest presentation and one fixed terminal obligation.
///
/// No waiters or snapshots are stored here. Required capture flushes never enter this queue.
#[derive(Debug)]
pub struct ActivityWorkQueue<I, R> {
    state: Mutex<WorkState<I, R>>,
}

impl<I: ActivityIdentity, R: Clone + Eq> ActivityWorkQueue<I, R> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Orders observations before host code reduces the shared Rust lifecycle.
    pub fn accept_request(&self, request_id: u64) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if request_id <= state.latest_presentation_request_id
            || request_id <= state.latest_terminal_request_id
        {
            return false;
        }
        state.latest_presentation_request_id = request_id;
        state.latest_request_id = request_id;
        true
    }

    /// Separates new ride intent from same-ride presentation ordering.
    #[allow(clippy::needless_pass_by_value)] // UniFFI exports owned boundary records.
    pub fn accept_presentation_request(
        &self,
        request_id: u64,
        platform_identifier: String,
        snapshot: ActivityObservation<I>,
        telemetry_at_ms: u64,
    ) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if request_id <= state.latest_presentation_request_id
            || request_id <= state.latest_terminal_request_id
        {
            return false;
        }
        let starts_new_session = match snapshot.phase {
            RidePhase::Idle | RidePhase::Ending | RidePhase::Ended => true,
            _ => snapshot
                .identity
                .as_ref()
                .is_none_or(|identity| identity.platform_identifier() != platform_identifier),
        } && state.pending_start_platform_identifier.as_ref()
            != Some(&platform_identifier);
        if !starts_new_session
            && state
                .disconnected_at_ms
                .is_some_and(|disconnected| telemetry_at_ms <= disconnected)
        {
            return false;
        }
        state.latest_presentation_request_id = request_id;
        state.latest_request_id = request_id;
        if starts_new_session {
            state.latest_session_request_id = request_id;
            state.pending_start_platform_identifier = Some(platform_identifier);
            state.disconnected_at_ms = None;
            state.latest_telemetry_at_ms = Some(telemetry_at_ms);
        } else {
            state.latest_telemetry_at_ms = Some(
                state
                    .latest_telemetry_at_ms
                    .unwrap_or(telemetry_at_ms)
                    .max(telemetry_at_ms),
            );
        }
        true
    }

    /// Records the request that owns a Rust-issued session identity.
    pub fn note_session_start(&self, request_id: u64, identity: I) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.latest_session_identity.as_ref() != Some(&identity) {
            if request_id >= state.latest_session_request_id
                && state.pending_start_platform_identifier.as_ref()
                    == Some(&identity.platform_identifier().to_owned())
            {
                // A newer payload fulfils the original intent; it is not a new ride.
                state.pending_start_platform_identifier = None;
            } else {
                state.latest_session_request_id = state.latest_session_request_id.max(request_id);
            }
            state.latest_session_identity = Some(identity);
        }
    }

    /// Required lifecycle effects have separate fences from replaceable presentation.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // UniFFI exports owned boundary records.
    pub fn accept_lifecycle_request(
        &self,
        request_id: u64,
        kind: LifecycleRequest,
        snapshot: ActivityObservation<I>,
    ) -> LifecycleAdmission {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if request_id == 0 {
            return LifecycleAdmission::Rejected;
        }
        match kind {
            LifecycleRequest::Terminal => {
                if request_id <= state.latest_session_request_id
                    || request_id <= state.latest_terminal_request_id
                {
                    return LifecycleAdmission::Rejected;
                }
                state.latest_terminal_request_id = request_id;
                state.pending_start_platform_identifier = None;
                state.latest_request_id = state.latest_request_id.max(request_id);
            }
            LifecycleRequest::Background => {
                if request_id <= state.latest_background_request_id {
                    return LifecycleAdmission::Rejected;
                }
                state.latest_background_request_id = request_id;
                if request_id <= state.latest_scene_request_id {
                    return LifecycleAdmission::FlushOnly;
                }
                state.latest_scene_request_id = request_id;
                match snapshot.phase {
                    RidePhase::Idle | RidePhase::Ended => {
                        return LifecycleAdmission::ReduceAndFlush;
                    }
                    _ => {}
                }
            }
            LifecycleRequest::TransportDisconnected { at_ms } => {
                if request_id <= state.latest_transport_request_id
                    || request_id <= state.latest_session_request_id
                    || state
                        .latest_telemetry_at_ms
                        .is_some_and(|received| received > at_ms)
                    || state
                        .disconnected_at_ms
                        .is_some_and(|previous| at_ms < previous)
                {
                    return LifecycleAdmission::Rejected;
                }
                state.latest_transport_request_id = request_id;
                state.disconnected_at_ms = Some(at_ms);
                state.pending_presentation_request_id = None;
            }
            LifecycleRequest::Foreground => {
                if request_id <= state.latest_scene_request_id {
                    return LifecycleAdmission::Rejected;
                }
                state.latest_scene_request_id = request_id;
            }
        }
        LifecycleAdmission::Reduce
    }

    /// Queues only the current semantic scene or transport projection.
    #[must_use]
    pub fn enqueue_lifecycle_projection(&self, request_id: u64) -> ActivityWorkAdmission<I, R> {
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if request_id == 0
                || (request_id != state.latest_scene_request_id
                    && request_id != state.latest_transport_request_id)
                || (request_id < state.latest_request_id
                    && request_id != state.latest_transport_request_id)
            {
                return ActivityWorkAdmission::Rejected;
            }
            state.latest_request_id = request_id;
        }
        self.enqueue_presentation(request_id)
    }

    /// Renews equal content only when received telemetry advanced by half the stale window.
    #[must_use]
    pub fn presentation_requires_renewal(&self, telemetry_at_ms: u64, stale_after_ms: u64) -> bool {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .last_projection_telemetry_at_ms
            .map_or(telemetry_at_ms > 0, |previous| {
                telemetry_at_ms > previous
                    && telemetry_at_ms - previous >= (stale_after_ms / 2).max(1)
            })
    }

    /// Records successful platform presentation, rather than admission or a failed platform await.
    pub fn presentation_updated(&self, telemetry_at_ms: u64) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.last_projection_telemetry_at_ms = Some(
            state
                .last_projection_telemetry_at_ms
                .unwrap_or(telemetry_at_ms)
                .max(telemetry_at_ms),
        );
    }

    /// Admits presentation without retaining a caller when the platform worker is occupied.
    #[must_use]
    pub fn enqueue_presentation(&self, request_id: u64) -> ActivityWorkAdmission<I, R> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if request_id != state.latest_request_id || request_id == 0 {
            return ActivityWorkAdmission::Rejected;
        }
        if state.in_flight_request_id.is_none() {
            state.in_flight_request_id = Some(request_id);
            return ActivityWorkAdmission::Run {
                work: ActivityWork::Presentation { request_id },
            };
        }
        ActivityWorkAdmission::Queued {
            replaced_request_id: state.pending_presentation_request_id.replace(request_id),
        }
    }

    /// Retains terminal cleanup ahead of presentation. Only Rust end effects are admissible.
    #[must_use]
    pub fn enqueue_terminal(
        &self,
        request_id: u64,
        effect: TerminalEffect<I, R>,
        stale_after_ms: u64,
    ) -> ActivityWorkAdmission<I, R> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if request_id == 0
            || (request_id != state.latest_request_id
                && request_id != state.latest_terminal_request_id)
        {
            return ActivityWorkAdmission::Rejected;
        }
        let work = ActivityWork::Terminal {
            request_id,
            effect,
            stale_after_ms,
        };
        let replaced_request_id = state.pending_presentation_request_id.take();
        if state.in_flight_request_id.is_none() {
            state.in_flight_request_id = Some(request_id);
            return ActivityWorkAdmission::Run { work };
        }
        // Cleanup cannot be superseded by a newer observation, including another end.
        // The lifecycle owns one activity identity until that cleanup is acknowledged.
        if state.terminal.is_none() {
            state.terminal = Some(work);
        }
        ActivityWorkAdmission::Queued {
            replaced_request_id,
        }
    }

    /// Consumes one matching completion and transfers the worker to required cleanup first.
    #[must_use]
    pub fn finish_work(&self, request_id: u64) -> Option<ActivityWork<I, R>> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.in_flight_request_id != Some(request_id) {
            return None;
        }
        let next = state.terminal.take().or_else(|| {
            state
                .pending_presentation_request_id
                .take()
                .filter(|request_id| *request_id == state.latest_request_id)
                .map(|request_id| ActivityWork::Presentation { request_id })
        });
        state.in_flight_request_id = next.as_ref().map(ActivityWork::request_id);
        next
    }

    #[must_use]
    pub fn snapshot(&self) -> ActivityWorkSnapshot {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        ActivityWorkSnapshot {
            in_flight_request_id: state.in_flight_request_id,
            pending_presentation_request_id: state.pending_presentation_request_id,
            pending_terminal_request_id: state.terminal.as_ref().map(ActivityWork::request_id),
        }
    }
}

impl<I, R> Default for WorkState<I, R> {
    fn default() -> Self {
        Self {
            latest_request_id: 0,
            latest_presentation_request_id: 0,
            latest_telemetry_at_ms: None,
            latest_transport_request_id: 0,
            disconnected_at_ms: None,
            latest_session_request_id: 0,
            latest_session_identity: None,
            pending_start_platform_identifier: None,
            latest_scene_request_id: 0,
            latest_background_request_id: 0,
            latest_terminal_request_id: 0,
            in_flight_request_id: None,
            pending_presentation_request_id: None,
            terminal: None,
            last_projection_telemetry_at_ms: None,
        }
    }
}

impl<I, R> Default for ActivityWorkQueue<I, R> {
    fn default() -> Self {
        Self {
            state: Mutex::new(WorkState::default()),
        }
    }
}
