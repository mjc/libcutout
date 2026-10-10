//! Bounded platform presentation ownership; recording lifecycle is reduced independently.

use std::sync::{Mutex, PoisonError};

use crate::{
    MobileActivityProjectionStateDto, MobileRideSessionEffectDto, MobileRideSessionIdentityDto,
    MobileRideSessionInputDto, MobileRideSessionPhaseDto, MobileRideSessionSnapshotDto,
};

/// Retains a recovered start obligation when a newer native snapshot replaces its payload.
#[uniffi::export]
#[must_use]
pub fn ride_activity_pending_start_effect(
    snapshot: MobileRideSessionSnapshotDto,
) -> MobileRideSessionEffectDto {
    if snapshot.phase == MobileRideSessionPhaseDto::Starting
        && snapshot.activity == MobileActivityProjectionStateDto::Starting
        && let Some(identity) = snapshot.identity
    {
        MobileRideSessionEffectDto::StartActivity { identity }
    } else {
        MobileRideSessionEffectDto::None
    }
}

/// Projects current lifecycle state without treating a background snapshot as fresh telemetry.
#[uniffi::export]
#[must_use]
pub fn ride_activity_background_projection_effect(
    snapshot: MobileRideSessionSnapshotDto,
) -> MobileRideSessionEffectDto {
    let Some(identity) = snapshot.identity else {
        return MobileRideSessionEffectDto::None;
    };
    match snapshot.activity {
        MobileActivityProjectionStateDto::Active { .. }
        | MobileActivityProjectionStateDto::Stale { .. } => {}
        _ => return MobileRideSessionEffectDto::None,
    }
    match snapshot.phase {
        MobileRideSessionPhaseDto::Active => {
            MobileRideSessionEffectDto::UpdateActivity { identity }
        }
        MobileRideSessionPhaseDto::Reconnecting | MobileRideSessionPhaseDto::Stale => {
            MobileRideSessionEffectDto::MarkActivityStale { identity }
        }
        _ => MobileRideSessionEffectDto::None,
    }
}

/// Independent semantic effects that cannot be superseded by ordinary presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideActivityLifecycleRequestDto {
    /// Entering the background requires a capture checkpoint.
    Background,
    /// Returning to the foreground updates app presence.
    Foreground,
    /// Ending a logical ride retains its identity cleanup.
    Terminal,
    /// Actual transport loss, independently ordered against received telemetry.
    TransportDisconnected { at_ms: u64 },
}

/// Native effects admitted by the Rust semantic request fence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideActivityLifecycleAdmissionDto {
    /// Reduce the corresponding Rust lifecycle input.
    Reduce,
    /// Record scene presence and checkpoint capture before a ride identity exists.
    ReduceAndFlush,
    /// A newer foreground transition won, but the required checkpoint remains due.
    FlushOnly,
    /// A duplicate or an event from an earlier ride cannot affect this ride.
    Rejected,
}

/// Replays only newer actual telemetry, without reviving disconnected or terminal rides.
#[uniffi::export]
#[must_use]
#[allow(clippy::needless_pass_by_value)] // UniFFI exports owned boundary records.
pub fn ride_activity_admitted_telemetry_input(
    snapshot: MobileRideSessionSnapshotDto,
    telemetry_at_ms: u64,
) -> Option<MobileRideSessionInputDto> {
    if snapshot.identity.is_none()
        || snapshot
            .last_telemetry_at_ms
            .is_some_and(|previous| telemetry_at_ms <= previous)
    {
        return None;
    }
    match snapshot.phase {
        MobileRideSessionPhaseDto::Starting
        | MobileRideSessionPhaseDto::Active
        | MobileRideSessionPhaseDto::Stale => Some(MobileRideSessionInputDto::TelemetryObserved {
            at_ms: telemetry_at_ms,
        }),
        _ => None,
    }
}

/// Keeps a startup marker until the lifecycle has explicitly ended.
#[uniffi::export]
#[must_use]
#[allow(clippy::needless_pass_by_value)] // UniFFI exports owned boundary records.
pub fn ride_activity_may_clear_marker(snapshot: MobileRideSessionSnapshotDto) -> bool {
    match snapshot.phase {
        MobileRideSessionPhaseDto::Ended { .. } => true,
        _ => false,
    }
}

/// One platform operation admitted by Rust. Apple snapshot payloads remain native-owned.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideActivityWorkDto {
    /// Replaceable presentation of the latest requested ride snapshot.
    Presentation { request_id: u64 },
    /// Required cleanup of the exact Rust-issued ride identity.
    Terminal {
        request_id: u64,
        effect: MobileRideSessionEffectDto,
        stale_after_ms: u64,
    },
}

impl MobileRideActivityWorkDto {
    const fn request_id(&self) -> u64 {
        match self {
            Self::Presentation { request_id } | Self::Terminal { request_id, .. } => *request_id,
        }
    }
}

/// Native dispatch effect for an admitted request.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideActivityWorkAdmissionDto {
    /// This caller owns the sole platform worker until matching completion.
    Run { work: MobileRideActivityWorkDto },
    /// No caller waits; retain only the latest native presentation payload.
    Queued { replaced_request_id: Option<u64> },
    /// An old request or nonterminal effect cannot enter this queue.
    Rejected,
}

/// Constant-size queue readback for dispatch and behavioral diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideActivityWorkSnapshotDto {
    /// Request currently owning the platform worker.
    pub in_flight_request_id: Option<u64>,
    /// Latest native snapshot awaiting the worker.
    pub pending_presentation_request_id: Option<u64>,
    /// Fixed identity cleanup awaiting the worker.
    pub pending_terminal_request_id: Option<u64>,
}

#[derive(Debug, Default)]
struct WorkState {
    latest_request_id: u64,
    latest_presentation_request_id: u64,
    latest_telemetry_at_ms: Option<u64>,
    latest_transport_request_id: u64,
    disconnected_at_ms: Option<u64>,
    latest_session_request_id: u64,
    latest_session_identity: Option<MobileRideSessionIdentityDto>,
    pending_start_platform_identifier: Option<String>,
    latest_scene_request_id: u64,
    latest_background_request_id: u64,
    latest_terminal_request_id: u64,
    in_flight_request_id: Option<u64>,
    pending_presentation_request_id: Option<u64>,
    terminal: Option<MobileRideActivityWorkDto>,
    last_projection_telemetry_at_ms: Option<u64>,
}

/// One in-flight Apple effect, one latest presentation and one fixed terminal obligation.
///
/// No waiters or snapshots are stored here. Required capture flushes never enter this queue.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileRideActivityWorkQueue {
    state: Mutex<WorkState>,
}

#[uniffi::export]
impl MobileRideActivityWorkQueue {
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Orders observations before native code reduces the shared Rust lifecycle.
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
        snapshot: MobileRideSessionSnapshotDto,
        telemetry_at_ms: u64,
    ) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if request_id <= state.latest_presentation_request_id
            || request_id <= state.latest_terminal_request_id
        {
            return false;
        }
        let starts_new_session = match snapshot.phase {
            MobileRideSessionPhaseDto::Idle
            | MobileRideSessionPhaseDto::Ending { .. }
            | MobileRideSessionPhaseDto::Ended { .. } => true,
            _ => snapshot
                .identity
                .as_ref()
                .is_none_or(|identity| identity.platform_identifier != platform_identifier),
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
    pub fn note_session_start(&self, request_id: u64, identity: MobileRideSessionIdentityDto) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.latest_session_identity.as_ref() != Some(&identity) {
            if request_id >= state.latest_session_request_id
                && state.pending_start_platform_identifier.as_ref()
                    == Some(&identity.platform_identifier)
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
        kind: MobileRideActivityLifecycleRequestDto,
        snapshot: MobileRideSessionSnapshotDto,
    ) -> MobileRideActivityLifecycleAdmissionDto {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if request_id == 0 {
            return MobileRideActivityLifecycleAdmissionDto::Rejected;
        }
        match kind {
            MobileRideActivityLifecycleRequestDto::Terminal => {
                if request_id <= state.latest_session_request_id
                    || request_id <= state.latest_terminal_request_id
                {
                    return MobileRideActivityLifecycleAdmissionDto::Rejected;
                }
                state.latest_terminal_request_id = request_id;
                state.pending_start_platform_identifier = None;
                state.latest_request_id = state.latest_request_id.max(request_id);
            }
            MobileRideActivityLifecycleRequestDto::Background => {
                if request_id <= state.latest_background_request_id {
                    return MobileRideActivityLifecycleAdmissionDto::Rejected;
                }
                state.latest_background_request_id = request_id;
                if request_id <= state.latest_scene_request_id {
                    return MobileRideActivityLifecycleAdmissionDto::FlushOnly;
                }
                state.latest_scene_request_id = request_id;
                match snapshot.phase {
                    MobileRideSessionPhaseDto::Idle | MobileRideSessionPhaseDto::Ended { .. } => {
                        return MobileRideActivityLifecycleAdmissionDto::ReduceAndFlush;
                    }
                    _ => {}
                }
            }
            MobileRideActivityLifecycleRequestDto::TransportDisconnected { at_ms } => {
                if request_id <= state.latest_transport_request_id
                    || request_id <= state.latest_session_request_id
                    || state
                        .latest_telemetry_at_ms
                        .is_some_and(|received| received > at_ms)
                    || state
                        .disconnected_at_ms
                        .is_some_and(|previous| at_ms < previous)
                {
                    return MobileRideActivityLifecycleAdmissionDto::Rejected;
                }
                state.latest_transport_request_id = request_id;
                state.disconnected_at_ms = Some(at_ms);
                state.pending_presentation_request_id = None;
            }
            MobileRideActivityLifecycleRequestDto::Foreground => {
                if request_id <= state.latest_scene_request_id {
                    return MobileRideActivityLifecycleAdmissionDto::Rejected;
                }
                state.latest_scene_request_id = request_id;
            }
        }
        MobileRideActivityLifecycleAdmissionDto::Reduce
    }

    /// Queues only the current semantic scene or transport projection.
    #[must_use]
    pub fn enqueue_lifecycle_projection(
        &self,
        request_id: u64,
    ) -> MobileRideActivityWorkAdmissionDto {
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if request_id == 0
                || (request_id != state.latest_scene_request_id
                    && request_id != state.latest_transport_request_id)
                || (request_id < state.latest_request_id
                    && request_id != state.latest_transport_request_id)
            {
                return MobileRideActivityWorkAdmissionDto::Rejected;
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

    /// Records successful platform presentation, rather than admission or a failed Apple await.
    pub fn presentation_updated(&self, telemetry_at_ms: u64) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.last_projection_telemetry_at_ms = Some(
            state
                .last_projection_telemetry_at_ms
                .unwrap_or(telemetry_at_ms)
                .max(telemetry_at_ms),
        );
    }

    /// Admits presentation without retaining a caller when the Apple worker is occupied.
    #[must_use]
    pub fn enqueue_presentation(&self, request_id: u64) -> MobileRideActivityWorkAdmissionDto {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if request_id != state.latest_request_id || request_id == 0 {
            return MobileRideActivityWorkAdmissionDto::Rejected;
        }
        if state.in_flight_request_id.is_none() {
            state.in_flight_request_id = Some(request_id);
            return MobileRideActivityWorkAdmissionDto::Run {
                work: MobileRideActivityWorkDto::Presentation { request_id },
            };
        }
        MobileRideActivityWorkAdmissionDto::Queued {
            replaced_request_id: state.pending_presentation_request_id.replace(request_id),
        }
    }

    /// Retains terminal cleanup ahead of presentation. Only Rust end effects are admissible.
    #[must_use]
    pub fn enqueue_terminal(
        &self,
        request_id: u64,
        effect: MobileRideSessionEffectDto,
        stale_after_ms: u64,
    ) -> MobileRideActivityWorkAdmissionDto {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match &effect {
            MobileRideSessionEffectDto::EndActivity { .. } => {}
            _ => return MobileRideActivityWorkAdmissionDto::Rejected,
        }
        if request_id == 0
            || (request_id != state.latest_request_id
                && request_id != state.latest_terminal_request_id)
        {
            return MobileRideActivityWorkAdmissionDto::Rejected;
        }
        let work = MobileRideActivityWorkDto::Terminal {
            request_id,
            effect,
            stale_after_ms,
        };
        let replaced_request_id = state.pending_presentation_request_id.take();
        if state.in_flight_request_id.is_none() {
            state.in_flight_request_id = Some(request_id);
            return MobileRideActivityWorkAdmissionDto::Run { work };
        }
        // Cleanup cannot be superseded by a newer observation, including another end.
        // The lifecycle owns one activity identity until that cleanup is acknowledged.
        if state.terminal.is_none() {
            state.terminal = Some(work);
        }
        MobileRideActivityWorkAdmissionDto::Queued {
            replaced_request_id,
        }
    }

    /// Consumes one matching completion and transfers the worker to required cleanup first.
    #[must_use]
    pub fn finish_work(&self, request_id: u64) -> Option<MobileRideActivityWorkDto> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.in_flight_request_id != Some(request_id) {
            return None;
        }
        let next = state.terminal.take().or_else(|| {
            state
                .pending_presentation_request_id
                .take()
                .filter(|request_id| *request_id == state.latest_request_id)
                .map(|request_id| MobileRideActivityWorkDto::Presentation { request_id })
        });
        state.in_flight_request_id = next.as_ref().map(MobileRideActivityWorkDto::request_id);
        next
    }

    #[must_use]
    pub fn snapshot(&self) -> MobileRideActivityWorkSnapshotDto {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        MobileRideActivityWorkSnapshotDto {
            in_flight_request_id: state.in_flight_request_id,
            pending_presentation_request_id: state.pending_presentation_request_id,
            pending_terminal_request_id: state
                .terminal
                .as_ref()
                .map(MobileRideActivityWorkDto::request_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MobileRideSessionEndReasonDto;

    fn terminal() -> MobileRideSessionEffectDto {
        MobileRideSessionEffectDto::EndActivity {
            identity: MobileRideSessionIdentityDto {
                platform_identifier: "wheel".into(),
                session_id: "00000000-0000-0000-0000-000000000001".into(),
            },
            reason: MobileRideSessionEndReasonDto::UserDisconnect,
        }
    }

    fn started_queue() -> (MobileRideActivityWorkQueue, MobileRideSessionSnapshotDto) {
        let queue = MobileRideActivityWorkQueue::new();
        let source = crate::CutoutSessionStateHandle::new();
        assert!(queue.accept_presentation_request(
            1,
            "wheel".into(),
            source.ride_session_snapshot(),
            0
        ));
        let snapshot = source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "wheel".into(),
            })
            .expect("start")
            .snapshot;
        queue.note_session_start(1, snapshot.identity.clone().expect("identity"));
        (queue, snapshot)
    }

    #[test]
    fn newer_same_ride_presentation_cannot_discard_background_or_terminal() {
        for kind in [
            MobileRideActivityLifecycleRequestDto::Background,
            MobileRideActivityLifecycleRequestDto::Terminal,
        ] {
            let (queue, snapshot) = started_queue();
            assert!(queue.accept_presentation_request(3, "wheel".into(), snapshot.clone(), 0));
            assert_eq!(
                queue.accept_lifecycle_request(2, kind, snapshot),
                MobileRideActivityLifecycleAdmissionDto::Reduce
            );
            if kind == MobileRideActivityLifecycleRequestDto::Terminal {
                assert!(matches!(
                    queue.enqueue_terminal(2, terminal(), 2_000),
                    MobileRideActivityWorkAdmissionDto::Run { .. }
                ));
            }
        }
    }

    #[test]
    fn late_background_checkpoint_survives_foreground_without_reverting_presence() {
        let (queue, snapshot) = started_queue();
        assert_eq!(
            queue.accept_lifecycle_request(
                3,
                MobileRideActivityLifecycleRequestDto::Foreground,
                snapshot.clone()
            ),
            MobileRideActivityLifecycleAdmissionDto::Reduce
        );
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Background,
                snapshot.clone()
            ),
            MobileRideActivityLifecycleAdmissionDto::FlushOnly
        );
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Background,
                snapshot
            ),
            MobileRideActivityLifecycleAdmissionDto::Rejected
        );
    }

    #[test]
    fn old_terminal_cannot_end_distinct_platform_intent_or_new_same_platform_uuid() {
        let (queue, snapshot) = started_queue();
        assert!(queue.accept_presentation_request(3, "another-wheel".into(), snapshot.clone(), 0));
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Terminal,
                snapshot.clone()
            ),
            MobileRideActivityLifecycleAdmissionDto::Rejected
        );
        let (queue, snapshot) = started_queue();
        let mut identity = snapshot.identity.clone().expect("identity");
        identity.session_id = "00000000-0000-0000-0000-000000000002".into();
        queue.note_session_start(3, identity);
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Terminal,
                snapshot
            ),
            MobileRideActivityLifecycleAdmissionDto::Rejected
        );
    }

    #[test]
    fn scene_can_replay_newer_actual_receipt_but_never_old_or_disconnected_data() {
        let (_, mut snapshot) = started_queue();
        snapshot.phase = MobileRideSessionPhaseDto::Stale;
        snapshot.last_telemetry_at_ms = Some(1_000);
        assert!(matches!(
            ride_activity_admitted_telemetry_input(snapshot.clone(), 2_000),
            Some(MobileRideSessionInputDto::TelemetryObserved { at_ms: 2_000 })
        ));
        assert!(ride_activity_admitted_telemetry_input(snapshot.clone(), 1_000).is_none());
        snapshot.phase = MobileRideSessionPhaseDto::Reconnecting;
        assert!(ride_activity_admitted_telemetry_input(snapshot, 2_000).is_none());
    }

    #[test]
    fn background_before_first_presentation_checkpoints_without_discarding_start() {
        let queue = MobileRideActivityWorkQueue::new();
        let source = crate::CutoutSessionStateHandle::new();
        let idle = source.ride_session_snapshot();
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Background,
                idle.clone()
            ),
            MobileRideActivityLifecycleAdmissionDto::ReduceAndFlush
        );
        assert!(queue.accept_presentation_request(1, "wheel".into(), idle, 90));
        assert!(matches!(
            queue.enqueue_presentation(1),
            MobileRideActivityWorkAdmissionDto::Run { .. }
        ));
    }

    #[test]
    fn startup_scene_presence_survives_start_and_recovery_but_explicit_stop_is_terminal() {
        let source = crate::CutoutSessionStateHandle::new();
        let idle = source.ride_session_snapshot();
        assert!(!ride_activity_may_clear_marker(idle));
        let background = source
            .reduce_ride_session(MobileRideSessionInputDto::AppBackgrounded)
            .expect("background");
        assert_eq!(
            background.snapshot.app_presence,
            crate::MobileRideSessionAppPresenceDto::Background
        );
        let started = source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "wheel".into(),
            })
            .expect("start");
        assert_eq!(
            started.snapshot.app_presence,
            crate::MobileRideSessionAppPresenceDto::Background
        );
        let marker = source
            .export_ride_session_marker()
            .expect("export")
            .expect("marker");
        let restored = crate::CutoutSessionStateHandle::new();
        let _ = restored
            .reduce_ride_session(MobileRideSessionInputDto::AppBackgrounded)
            .expect("background");
        let recovery = restored
            .recover_ride_session_marker(marker, Some("wheel".into()))
            .expect("recover");
        assert_eq!(
            recovery.snapshot.app_presence,
            crate::MobileRideSessionAppPresenceDto::Background
        );
        let empty = crate::CutoutSessionStateHandle::new();
        let stopped = empty
            .reduce_ride_session(MobileRideSessionInputDto::UserStopped)
            .expect("stop");
        assert_eq!(
            stopped.snapshot.phase,
            MobileRideSessionPhaseDto::Ended {
                reason: MobileRideSessionEndReasonDto::UserStop
            }
        );
        assert!(ride_activity_may_clear_marker(stopped.snapshot));
        let queue = MobileRideActivityWorkQueue::new();
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Terminal,
                empty.ride_session_snapshot()
            ),
            MobileRideActivityLifecycleAdmissionDto::Reduce
        );
        assert!(!queue.accept_request(1));
    }

    #[test]
    fn actual_disconnect_survives_newer_old_data_and_prunes_queued_presentation() {
        let (queue, snapshot) = started_queue();
        let _ = queue.enqueue_presentation(1);
        assert!(queue.accept_presentation_request(3, "wheel".into(), snapshot.clone(), 90));
        let _ = queue.enqueue_presentation(3);
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::TransportDisconnected { at_ms: 100 },
                snapshot
            ),
            MobileRideActivityLifecycleAdmissionDto::Reduce
        );
        assert_eq!(queue.snapshot().pending_presentation_request_id, None);
        assert!(queue.finish_work(1).is_none());
    }

    #[test]
    fn only_actual_newer_receipt_supersedes_disconnection_and_allows_reconnect() {
        let (queue, snapshot) = started_queue();
        assert!(queue.accept_presentation_request(3, "wheel".into(), snapshot.clone(), 200));
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::TransportDisconnected { at_ms: 100 },
                snapshot.clone()
            ),
            MobileRideActivityLifecycleAdmissionDto::Rejected
        );
        assert_eq!(
            queue.accept_lifecycle_request(
                4,
                MobileRideActivityLifecycleRequestDto::TransportDisconnected { at_ms: 300 },
                snapshot.clone()
            ),
            MobileRideActivityLifecycleAdmissionDto::Reduce
        );
        let mut reconnecting = snapshot;
        reconnecting.phase = MobileRideSessionPhaseDto::Reconnecting;
        assert!(!queue.accept_presentation_request(5, "wheel".into(), reconnecting.clone(), 200));
        assert!(queue.accept_presentation_request(6, "wheel".into(), reconnecting, 400));
    }

    #[test]
    fn uninitialized_terminal_admission_still_owns_orphan_platform_cleanup() {
        let queue = MobileRideActivityWorkQueue::new();
        let source = crate::CutoutSessionStateHandle::new();
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Terminal,
                source.ride_session_snapshot()
            ),
            MobileRideActivityLifecycleAdmissionDto::Reduce
        );
        assert!(matches!(
            queue.enqueue_presentation(2),
            MobileRideActivityWorkAdmissionDto::Run { .. }
        ));
        assert!(!queue.accept_request(1));
    }

    #[test]
    fn late_background_before_ride_initialization_still_checkpoints_without_reverting_scene() {
        let queue = MobileRideActivityWorkQueue::new();
        let source = crate::CutoutSessionStateHandle::new();
        let idle = source.ride_session_snapshot();
        assert_eq!(
            queue.accept_lifecycle_request(
                3,
                MobileRideActivityLifecycleRequestDto::Foreground,
                idle.clone()
            ),
            MobileRideActivityLifecycleAdmissionDto::Reduce
        );
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Background,
                idle
            ),
            MobileRideActivityLifecycleAdmissionDto::FlushOnly
        );
        assert!(queue.accept_request(1));
    }

    #[test]
    fn replacement_of_pending_same_wheel_start_does_not_fence_earlier_lifecycle() {
        for kind in [
            MobileRideActivityLifecycleRequestDto::Terminal,
            MobileRideActivityLifecycleRequestDto::TransportDisconnected { at_ms: 100 },
        ] {
            let queue = MobileRideActivityWorkQueue::new();
            let source = crate::CutoutSessionStateHandle::new();
            let idle = source.ride_session_snapshot();
            assert!(queue.accept_presentation_request(1, "wheel".into(), idle.clone(), 90));
            assert!(queue.accept_presentation_request(3, "wheel".into(), idle.clone(), 90));
            assert_eq!(
                queue.accept_lifecycle_request(2, kind, idle),
                MobileRideActivityLifecycleAdmissionDto::Reduce
            );
        }
        let queue = MobileRideActivityWorkQueue::new();
        let source = crate::CutoutSessionStateHandle::new();
        let idle = source.ride_session_snapshot();
        assert!(queue.accept_presentation_request(1, "wheel".into(), idle.clone(), 90));
        assert!(queue.accept_presentation_request(3, "wheel".into(), idle, 90));
        let started = source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "wheel".into(),
            })
            .expect("start");
        queue.note_session_start(3, started.snapshot.identity.clone().expect("identity"));
        assert_eq!(
            queue.accept_lifecycle_request(
                2,
                MobileRideActivityLifecycleRequestDto::Terminal,
                started.snapshot
            ),
            MobileRideActivityLifecycleAdmissionDto::Reduce
        );
    }

    #[test]
    fn stalled_worker_retains_only_latest_presentation_and_releases_ownership_once() {
        let queue = MobileRideActivityWorkQueue::new();
        assert!(queue.accept_request(1));
        assert!(matches!(
            queue.enqueue_presentation(1),
            MobileRideActivityWorkAdmissionDto::Run { .. }
        ));
        for request_id in 2..=2_000 {
            assert!(queue.accept_request(request_id));
            assert!(matches!(
                queue.enqueue_presentation(request_id),
                MobileRideActivityWorkAdmissionDto::Queued { .. }
            ));
        }
        assert_eq!(queue.snapshot().in_flight_request_id, Some(1));
        assert_eq!(
            queue.snapshot().pending_presentation_request_id,
            Some(2_000)
        );
        assert_eq!(
            queue.finish_work(1),
            Some(MobileRideActivityWorkDto::Presentation { request_id: 2_000 })
        );
        assert!(queue.finish_work(1).is_none());
        assert_eq!(queue.snapshot().in_flight_request_id, Some(2_000));
        assert!(queue.finish_work(2_000).is_none());
        assert_eq!(queue.snapshot().in_flight_request_id, None);
    }

    #[test]
    fn terminal_identity_survives_newer_presentation_and_runs_before_it() {
        let queue = MobileRideActivityWorkQueue::new();
        assert!(queue.accept_request(1));
        let _ = queue.enqueue_presentation(1);
        assert!(queue.accept_request(2));
        let _ = queue.enqueue_presentation(2);
        assert!(queue.accept_request(3));
        let expected = terminal();
        assert_eq!(
            queue.enqueue_terminal(3, expected.clone(), 0),
            MobileRideActivityWorkAdmissionDto::Queued {
                replaced_request_id: Some(2)
            }
        );
        assert!(queue.accept_request(4));
        let _ = queue.enqueue_presentation(4);
        assert_eq!(
            queue.finish_work(1),
            Some(MobileRideActivityWorkDto::Terminal {
                request_id: 3,
                effect: expected,
                stale_after_ms: 0
            })
        );
        assert_eq!(
            queue.finish_work(3),
            Some(MobileRideActivityWorkDto::Presentation { request_id: 4 })
        );
    }

    #[test]
    fn stale_requests_flush_effects_and_old_completions_cannot_replace_terminal() {
        let queue = MobileRideActivityWorkQueue::new();
        assert!(!queue.accept_request(0));
        assert!(queue.accept_request(5));
        assert!(!queue.accept_request(4));
        assert_eq!(
            queue.enqueue_presentation(4),
            MobileRideActivityWorkAdmissionDto::Rejected
        );
        assert_eq!(
            queue.enqueue_terminal(5, MobileRideSessionEffectDto::None, 0),
            MobileRideActivityWorkAdmissionDto::Rejected
        );
        let _ = queue.enqueue_terminal(5, terminal(), 0);
        assert!(queue.finish_work(4).is_none());
        assert_eq!(queue.snapshot().in_flight_request_id, Some(5));
    }

    #[test]
    fn a_later_terminal_cannot_discard_required_identity_cleanup() {
        let queue = MobileRideActivityWorkQueue::new();
        assert!(queue.accept_request(1));
        let _ = queue.enqueue_presentation(1);
        assert!(queue.accept_request(2));
        let expected = terminal();
        let _ = queue.enqueue_terminal(2, expected.clone(), 0);
        assert!(queue.accept_request(3));
        let mut newer = terminal();
        if let MobileRideSessionEffectDto::EndActivity { identity, .. } = &mut newer {
            identity.session_id = "00000000-0000-0000-0000-000000000002".into();
        }
        let _ = queue.enqueue_terminal(3, newer, 0);
        assert_eq!(
            queue.finish_work(1),
            Some(MobileRideActivityWorkDto::Terminal {
                request_id: 2,
                effect: expected,
                stale_after_ms: 0,
            })
        );
    }

    #[test]
    fn recovered_start_remains_owned_by_the_original_identity() {
        let source = crate::CutoutSessionStateHandle::new();
        let started = source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "wheel".into(),
            })
            .expect("valid start");
        assert_eq!(
            ride_activity_pending_start_effect(started.snapshot.clone()),
            started.effect
        );
        let mut ending = started.snapshot;
        ending.phase = MobileRideSessionPhaseDto::Ending {
            reason: MobileRideSessionEndReasonDto::UserDisconnect,
        };
        assert_eq!(
            ride_activity_pending_start_effect(ending),
            MobileRideSessionEffectDto::None
        );
    }

    #[test]
    fn background_projection_does_not_convert_disconnect_into_fresh_telemetry() {
        let source = crate::CutoutSessionStateHandle::new();
        let started = source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "wheel".into(),
            })
            .expect("valid start");
        let identity = started.snapshot.identity.expect("start identity");
        let _ = source
            .reduce_ride_session(MobileRideSessionInputDto::ActivityStarted {
                identity: identity.clone(),
                activity_id: "activity-1".into(),
            })
            .expect("valid acknowledgement");
        let disconnected = source
            .reduce_ride_session(MobileRideSessionInputDto::BluetoothDisconnected { at_ms: 100 })
            .expect("valid disconnect");
        assert_eq!(
            ride_activity_background_projection_effect(disconnected.snapshot),
            MobileRideSessionEffectDto::MarkActivityStale { identity }
        );
        assert_eq!(
            source.ride_session_snapshot().phase,
            MobileRideSessionPhaseDto::Reconnecting
        );
    }

    #[test]
    fn equal_content_renewal_requires_new_received_telemetry_and_successful_presentation() {
        let queue = MobileRideActivityWorkQueue::new();
        queue.presentation_updated(1_000);
        assert!(!queue.presentation_requires_renewal(1_000, 2_000));
        assert!(!queue.presentation_requires_renewal(999, 2_000));
        assert!(!queue.presentation_requires_renewal(1_999, 2_000));
        assert!(queue.presentation_requires_renewal(2_000, 2_000));
        assert!(queue.presentation_requires_renewal(2_000, 2_000));
        queue.presentation_updated(2_000);
        queue.presentation_updated(1_999);
        assert!(!queue.presentation_requires_renewal(2_000, 2_000));
        assert!(queue.presentation_requires_renewal(3_000, 2_000));
    }

    #[test]
    fn observation_without_platform_work_invalidates_stale_presentation_but_not_cleanup() {
        let queue = MobileRideActivityWorkQueue::new();
        assert!(queue.accept_request(1));
        let _ = queue.enqueue_presentation(1);
        assert!(queue.accept_request(2));
        let _ = queue.enqueue_terminal(2, terminal(), 0);
        assert!(queue.accept_request(3));
        let _ = queue.enqueue_presentation(3);
        assert!(queue.accept_request(4));
        assert!(matches!(
            queue.finish_work(1),
            Some(MobileRideActivityWorkDto::Terminal { request_id: 2, .. })
        ));
        assert!(queue.finish_work(2).is_none());
        assert!(queue.snapshot().pending_presentation_request_id.is_none());
    }

    #[test]
    fn marker_recovery_cannot_replace_or_restart_a_lifecycle_that_still_owns_work() {
        use crate::MobileRideSessionInputDto;
        let source = crate::CutoutSessionStateHandle::new();
        let _ = source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "wheel".into(),
            })
            .expect("valid source start");
        let distinct_marker = source
            .export_ride_session_marker()
            .expect("encode marker")
            .expect("live marker");
        for phase in [
            MobileRideSessionPhaseDto::Starting,
            MobileRideSessionPhaseDto::Active,
            MobileRideSessionPhaseDto::Reconnecting,
            MobileRideSessionPhaseDto::Stale,
            MobileRideSessionPhaseDto::Ending {
                reason: MobileRideSessionEndReasonDto::UserDisconnect,
            },
        ] {
            let handle = crate::CutoutSessionStateHandle::new();
            let started = handle
                .reduce_ride_session(MobileRideSessionInputDto::Start {
                    platform_identifier: "wheel".into(),
                })
                .expect("valid start");
            let identity = started.snapshot.identity.expect("start identity");
            let matching_marker = handle
                .export_ride_session_marker()
                .expect("encode marker")
                .expect("live marker");
            if phase != MobileRideSessionPhaseDto::Starting {
                let _ = handle
                    .reduce_ride_session(MobileRideSessionInputDto::ActivityStarted {
                        identity,
                        activity_id: "activity-1".into(),
                    })
                    .expect("valid acknowledgement");
            }
            match phase {
                MobileRideSessionPhaseDto::Reconnecting => {
                    let _ = handle
                        .reduce_ride_session(MobileRideSessionInputDto::BluetoothDisconnected {
                            at_ms: 100,
                        })
                        .expect("disconnect");
                }
                MobileRideSessionPhaseDto::Stale => {
                    let _ = handle
                        .reduce_ride_session(MobileRideSessionInputDto::TelemetryObserved {
                            at_ms: 100,
                        })
                        .expect("telemetry");
                    let _ = handle
                        .reduce_ride_session(MobileRideSessionInputDto::FreshnessChecked {
                            now_ms: 2_100,
                        })
                        .expect("freshness");
                }
                MobileRideSessionPhaseDto::Ending { .. } => {
                    let _ = handle
                        .reduce_ride_session(MobileRideSessionInputDto::UserDisconnected)
                        .expect("disconnect");
                }
                _ => {}
            }
            let before = handle.ride_session_snapshot();
            assert_eq!(before.phase, phase);
            for marker in [&matching_marker, &distinct_marker] {
                for restored in [Some("wheel".into()), None] {
                    let decision = handle
                        .recover_ride_session_marker(marker.clone(), restored)
                        .expect("valid marker");
                    assert_eq!(decision.snapshot, before);
                    assert_eq!(decision.effect, MobileRideSessionEffectDto::None);
                    assert_eq!(
                        handle.export_ride_session_marker().expect("encode marker"),
                        Some(matching_marker.clone())
                    );
                }
            }
        }
    }
}
