//! Bounded platform presentation ownership; recording lifecycle is reduced independently.

use cutout_application::ride_activity as application;

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
    if application::pending_start(&observation(snapshot.clone())) {
        snapshot
            .identity
            .map_or(MobileRideSessionEffectDto::None, |identity| {
                MobileRideSessionEffectDto::StartActivity { identity }
            })
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
    match application::background_projection(&observation(snapshot.clone())) {
        application::BackgroundProjection::Update => snapshot
            .identity
            .map_or(MobileRideSessionEffectDto::None, |identity| {
                MobileRideSessionEffectDto::UpdateActivity { identity }
            }),
        application::BackgroundProjection::Stale => snapshot
            .identity
            .map_or(MobileRideSessionEffectDto::None, |identity| {
                MobileRideSessionEffectDto::MarkActivityStale { identity }
            }),
        application::BackgroundProjection::None => MobileRideSessionEffectDto::None,
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
    application::admits_telemetry(&observation(snapshot), telemetry_at_ms).then_some(
        MobileRideSessionInputDto::TelemetryObserved {
            at_ms: telemetry_at_ms,
        },
    )
}

/// Keeps a startup marker until the lifecycle has explicitly ended.
#[uniffi::export]
#[must_use]
#[allow(clippy::needless_pass_by_value)] // UniFFI exports owned boundary records.
pub fn ride_activity_may_clear_marker(snapshot: MobileRideSessionSnapshotDto) -> bool {
    application::may_clear_marker(&observation(snapshot))
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

type ApplicationQueue = application::ActivityWorkQueue<
    MobileRideSessionIdentityDto,
    crate::MobileRideSessionEndReasonDto,
>;

impl application::ActivityIdentity for MobileRideSessionIdentityDto {
    fn platform_identifier(&self) -> &str {
        &self.platform_identifier
    }
}

fn observation(
    snapshot: MobileRideSessionSnapshotDto,
) -> application::ActivityObservation<MobileRideSessionIdentityDto> {
    application::ActivityObservation {
        identity: snapshot.identity,
        phase: match snapshot.phase {
            MobileRideSessionPhaseDto::Idle => application::RidePhase::Idle,
            MobileRideSessionPhaseDto::Starting => application::RidePhase::Starting,
            MobileRideSessionPhaseDto::Active => application::RidePhase::Active,
            MobileRideSessionPhaseDto::Reconnecting => application::RidePhase::Reconnecting,
            MobileRideSessionPhaseDto::Stale => application::RidePhase::Stale,
            MobileRideSessionPhaseDto::Ending { .. } => application::RidePhase::Ending,
            MobileRideSessionPhaseDto::Ended { .. } => application::RidePhase::Ended,
        },
        projection: match snapshot.activity {
            MobileActivityProjectionStateDto::Starting => application::ProjectionState::Starting,
            MobileActivityProjectionStateDto::Active { .. } => application::ProjectionState::Active,
            MobileActivityProjectionStateDto::Stale { .. } => application::ProjectionState::Stale,
            _ => application::ProjectionState::Other,
        },
        last_telemetry_at_ms: snapshot.last_telemetry_at_ms,
    }
}

impl
    From<
        application::ActivityWork<
            MobileRideSessionIdentityDto,
            crate::MobileRideSessionEndReasonDto,
        >,
    > for MobileRideActivityWorkDto
{
    fn from(
        work: application::ActivityWork<
            MobileRideSessionIdentityDto,
            crate::MobileRideSessionEndReasonDto,
        >,
    ) -> Self {
        match work {
            application::ActivityWork::Presentation { request_id } => {
                Self::Presentation { request_id }
            }
            application::ActivityWork::Terminal {
                request_id,
                effect,
                stale_after_ms,
            } => Self::Terminal {
                request_id,
                effect: MobileRideSessionEffectDto::EndActivity {
                    identity: effect.identity,
                    reason: effect.reason,
                },
                stale_after_ms,
            },
        }
    }
}

impl
    From<
        application::ActivityWorkAdmission<
            MobileRideSessionIdentityDto,
            crate::MobileRideSessionEndReasonDto,
        >,
    > for MobileRideActivityWorkAdmissionDto
{
    fn from(
        value: application::ActivityWorkAdmission<
            MobileRideSessionIdentityDto,
            crate::MobileRideSessionEndReasonDto,
        >,
    ) -> Self {
        match value {
            application::ActivityWorkAdmission::Run { work } => Self::Run { work: work.into() },
            application::ActivityWorkAdmission::Queued {
                replaced_request_id,
            } => Self::Queued {
                replaced_request_id,
            },
            application::ActivityWorkAdmission::Rejected => Self::Rejected,
        }
    }
}

/// Bounded application work, adapted to native request and effect records.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileRideActivityWorkQueue {
    inner: ApplicationQueue,
}

#[uniffi::export]
impl MobileRideActivityWorkQueue {
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn accept_request(&self, request_id: u64) -> bool {
        self.inner.accept_request(request_id)
    }

    pub fn accept_presentation_request(
        &self,
        request_id: u64,
        platform_identifier: String,
        snapshot: MobileRideSessionSnapshotDto,
        telemetry_at_ms: u64,
    ) -> bool {
        self.inner.accept_presentation_request(
            request_id,
            platform_identifier,
            observation(snapshot),
            telemetry_at_ms,
        )
    }

    pub fn note_session_start(&self, request_id: u64, identity: MobileRideSessionIdentityDto) {
        self.inner.note_session_start(request_id, identity);
    }

    #[must_use]
    pub fn accept_lifecycle_request(
        &self,
        request_id: u64,
        kind: MobileRideActivityLifecycleRequestDto,
        snapshot: MobileRideSessionSnapshotDto,
    ) -> MobileRideActivityLifecycleAdmissionDto {
        let kind = match kind {
            MobileRideActivityLifecycleRequestDto::Background => {
                application::LifecycleRequest::Background
            }
            MobileRideActivityLifecycleRequestDto::Foreground => {
                application::LifecycleRequest::Foreground
            }
            MobileRideActivityLifecycleRequestDto::Terminal => {
                application::LifecycleRequest::Terminal
            }
            MobileRideActivityLifecycleRequestDto::TransportDisconnected { at_ms } => {
                application::LifecycleRequest::TransportDisconnected { at_ms }
            }
        };
        match self
            .inner
            .accept_lifecycle_request(request_id, kind, observation(snapshot))
        {
            application::LifecycleAdmission::Reduce => {
                MobileRideActivityLifecycleAdmissionDto::Reduce
            }
            application::LifecycleAdmission::ReduceAndFlush => {
                MobileRideActivityLifecycleAdmissionDto::ReduceAndFlush
            }
            application::LifecycleAdmission::FlushOnly => {
                MobileRideActivityLifecycleAdmissionDto::FlushOnly
            }
            application::LifecycleAdmission::Rejected => {
                MobileRideActivityLifecycleAdmissionDto::Rejected
            }
        }
    }

    #[must_use]
    pub fn enqueue_lifecycle_projection(
        &self,
        request_id: u64,
    ) -> MobileRideActivityWorkAdmissionDto {
        self.inner.enqueue_lifecycle_projection(request_id).into()
    }

    #[must_use]
    pub fn presentation_requires_renewal(&self, telemetry_at_ms: u64, stale_after_ms: u64) -> bool {
        self.inner
            .presentation_requires_renewal(telemetry_at_ms, stale_after_ms)
    }

    pub fn presentation_updated(&self, telemetry_at_ms: u64) {
        self.inner.presentation_updated(telemetry_at_ms);
    }

    #[must_use]
    pub fn enqueue_presentation(&self, request_id: u64) -> MobileRideActivityWorkAdmissionDto {
        self.inner.enqueue_presentation(request_id).into()
    }

    #[must_use]
    pub fn enqueue_terminal(
        &self,
        request_id: u64,
        effect: MobileRideSessionEffectDto,
        stale_after_ms: u64,
    ) -> MobileRideActivityWorkAdmissionDto {
        let MobileRideSessionEffectDto::EndActivity { identity, reason } = effect else {
            return MobileRideActivityWorkAdmissionDto::Rejected;
        };
        self.inner
            .enqueue_terminal(
                request_id,
                application::TerminalEffect { identity, reason },
                stale_after_ms,
            )
            .into()
    }

    #[must_use]
    pub fn finish_work(&self, request_id: u64) -> Option<MobileRideActivityWorkDto> {
        self.inner.finish_work(request_id).map(Into::into)
    }

    #[must_use]
    pub fn snapshot(&self) -> MobileRideActivityWorkSnapshotDto {
        let snapshot = self.inner.snapshot();
        MobileRideActivityWorkSnapshotDto {
            in_flight_request_id: snapshot.in_flight_request_id,
            pending_presentation_request_id: snapshot.pending_presentation_request_id,
            pending_terminal_request_id: snapshot.pending_terminal_request_id,
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
