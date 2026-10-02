//! Immutable connection admission through the existing session-state mutex.

use cutout_core::{
    ConnectionAttemptSnapshot, ConnectionAttemptToken, ConnectionReadiness,
    ConnectionRetryDecision, ConnectionTransportState,
};
use std::sync::Arc;

use crate::{
    CutoutSessionStateHandle, MobileRideMapConnectionAdmission, MobileRideMapCore,
    MobileRideMapCoreErrorDto, MobileRideMapSpeedObservationDto,
    MobileRideMapTelemetryObservationDto, MonotonicTimestamp,
};
#[cfg(test)]
use crate::{MobileRideMapAdmissionPollDto, MobileRideMapCoreSnapshotDto};

/// Identity captured with native callbacks and decoded telemetry.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileConnectionAttemptTokenDto {
    /// Distinguishes retries, including retries of the same peripheral.
    pub generation: u64,
    /// Selected platform identity, independent of advertisement names.
    pub platform_identifier: String,
}

/// Rust-owned permission to enter the typed ride path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileConnectionReadinessDto {
    /// No active connection attempt.
    Disconnected,
    /// Transport and identification are pending.
    Pending,
    /// Validated protocol permits compatible read-only ride telemetry.
    Verified,
    /// Identification ended; raw capture alone remains available.
    RecordOnly,
    /// A previously verified transport or session failed.
    Failed,
    /// A verified decoder observed contradictory protocol evidence.
    Conflicted,
}

/// Native connection availability, separate from protocol admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileConnectionTransportStateDto {
    /// Native transport is unavailable; capture may contain only retained evidence.
    Disconnected,
    /// Native connection is outstanding.
    Connecting,
    /// Native connection exists, without promising any particular subscription.
    Connected,
}

/// One atomic publication for UI and queued consumers.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileConnectionAttemptSnapshotDto {
    /// Changes on replacement and invalidation.
    pub generation: u64,
    /// Orders accepted updates within and across attempts.
    pub revision: u64,
    /// Active raw-transport identity, which may be record-only.
    pub token: Option<MobileConnectionAttemptTokenDto>,
    /// Current admission permission.
    pub readiness: MobileConnectionReadinessDto,
    /// Whether native transport can currently deliver data.
    pub transport: MobileConnectionTransportStateDto,
    /// Monotonic whole-attempt deadline in milliseconds.
    pub deadline_ms: Option<u64>,
}

/// Retry approved by Rust, including the timer identity and monotonic deadline.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileConnectionRetryDto {
    /// Fences timer callbacks after cancellation or replacement.
    pub token: u64,
    /// One-based consecutive retry number.
    pub attempt: u32,
    /// Device identity to reconnect through `CoreBluetooth`.
    pub platform_identifier: String,
    /// Earliest monotonic time at which Rust will admit this retry.
    pub deadline_ms: u64,
}

/// Typed result of Rust reconnect policy.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileConnectionRetryDecisionDto {
    /// Schedule a native timer, then ask Rust to admit it when it fires.
    Scheduled { retry: MobileConnectionRetryDto },
    /// The configured number of consecutive retries has been exhausted.
    Exhausted { attempt: u32 },
    /// This attempt no longer owns the reconnect request.
    Rejected,
}

impl From<ConnectionRetryDecision> for MobileConnectionRetryDecisionDto {
    fn from(value: ConnectionRetryDecision) -> Self {
        match value {
            ConnectionRetryDecision::Scheduled(retry) => Self::Scheduled {
                retry: MobileConnectionRetryDto {
                    token: retry.token().value(),
                    attempt: u32::from(retry.attempt()),
                    platform_identifier: retry.platform_identifier().to_owned(),
                    deadline_ms: retry.deadline().get(),
                },
            },
            ConnectionRetryDecision::Exhausted { attempt } => Self::Exhausted {
                attempt: u32::from(attempt),
            },
            ConnectionRetryDecision::Rejected => Self::Rejected,
        }
    }
}

impl From<MobileConnectionAttemptTokenDto> for ConnectionAttemptToken {
    fn from(value: MobileConnectionAttemptTokenDto) -> Self {
        Self::new(value.generation, value.platform_identifier)
    }
}

impl From<&ConnectionAttemptSnapshot> for MobileConnectionAttemptSnapshotDto {
    fn from(value: &ConnectionAttemptSnapshot) -> Self {
        Self {
            generation: value.generation,
            revision: value.revision,
            token: value
                .token
                .as_ref()
                .map(|token| MobileConnectionAttemptTokenDto {
                    generation: token.generation(),
                    platform_identifier: token.platform_identifier().to_owned(),
                }),
            readiness: match value.readiness {
                ConnectionReadiness::Disconnected => MobileConnectionReadinessDto::Disconnected,
                ConnectionReadiness::Pending => MobileConnectionReadinessDto::Pending,
                ConnectionReadiness::Verified => MobileConnectionReadinessDto::Verified,
                ConnectionReadiness::RecordOnly => MobileConnectionReadinessDto::RecordOnly,
                ConnectionReadiness::Failed => MobileConnectionReadinessDto::Failed,
                ConnectionReadiness::Conflicted => MobileConnectionReadinessDto::Conflicted,
            },
            transport: match value.transport {
                ConnectionTransportState::Disconnected => {
                    MobileConnectionTransportStateDto::Disconnected
                }
                ConnectionTransportState::Connecting => {
                    MobileConnectionTransportStateDto::Connecting
                }
                ConnectionTransportState::Connected => MobileConnectionTransportStateDto::Connected,
            },
            deadline_ms: value.deadline.map(MonotonicTimestamp::get),
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Requests a reconnect decision from Rust; `jitter_permille` supplies platform entropy only.
    pub fn request_connection_retry(
        &self,
        token: MobileConnectionAttemptTokenDto,
        now_ms: u64,
        jitter_permille: u16,
    ) -> MobileConnectionRetryDecisionDto {
        let mut inner = self.lock_inner();
        inner
            .session_state_mut()
            .connection
            .request_retry(
                &token.into(),
                MonotonicTimestamp::new(now_ms),
                jitter_permille,
            )
            .into()
    }

    /// Admits a timer only after its Rust-owned monotonic deadline.
    pub fn admit_connection_retry(
        &self,
        token: u64,
        now_ms: u64,
    ) -> Option<MobileConnectionAttemptSnapshotDto> {
        let mut inner = self.lock_inner();
        let admitted = inner.admit_retry(
            cutout_core::ConnectionRetryToken::new(token),
            MonotonicTimestamp::new(now_ms),
        );
        admitted.map(|_| inner.session_state().connection.snapshot().into())
    }

    /// Cancels only the matching timer and clears its consecutive retry budget.
    pub fn cancel_connection_retry(&self, token: u64) -> bool {
        self.lock_inner()
            .session_state_mut()
            .connection
            .cancel_retry(cutout_core::ConnectionRetryToken::new(token))
    }

    /// Returns the Rust deadline for a still-pending retry timer.
    #[must_use]
    pub fn connection_retry_deadline(&self, token: u64) -> Option<u64> {
        self.lock_inner()
            .session_state()
            .connection
            .retry_deadline(cutout_core::ConnectionRetryToken::new(token))
            .map(MonotonicTimestamp::get)
    }

    /// Accepts a native link callback for its captured attempt.
    pub fn connection_link_established(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner
            .session_state_mut()
            .connection
            .connected(&token.into());
        inner.session_state().connection.snapshot().into()
    }

    /// Marks link loss before publishing capture availability or retrying.
    pub fn connection_link_down(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner.link_down(&token.into());
        inner.session_state().connection.snapshot().into()
    }

    /// Replaces attempt and detector together, preserving discovery observations.
    pub fn begin_connection_attempt(
        &self,
        platform_identifier: String,
        now_ms: u64,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner.begin_attempt(platform_identifier, MonotonicTimestamp::new(now_ms));
        inner.session_state().connection.snapshot().into()
    }

    /// Returns readiness and identity under one lock.
    #[must_use]
    pub fn connection_attempt_snapshot(&self) -> MobileConnectionAttemptSnapshotDto {
        self.lock_inner()
            .session_state()
            .connection
            .snapshot()
            .into()
    }

    /// Admits raw transport work only for the active attempt.
    #[must_use]
    pub fn connection_attempt_is_current(&self, token: MobileConnectionAttemptTokenDto) -> bool {
        self.lock_inner()
            .session_state()
            .connection
            .is_current(&token.into())
    }

    /// Admits queued ride work only while the captured attempt remains verified.
    #[must_use]
    pub fn verified_connection_attempt_is_current(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> bool {
        self.lock_inner()
            .session_state()
            .connection
            .is_verified(&token.into())
    }

    /// Atomically enqueues one verified connection admission in the Rust-owned ride-map core.
    ///
    /// The session-state lock remains held through ordered enqueue, so connection invalidation
    /// cannot race between verification and admission. SQLite completion is polled separately,
    /// after this method releases the session lock. Swift supplies only the Rust-issued token.
    ///
    /// # Errors
    ///
    /// Returns `StaleConnection` when the token is no longer the current verified attempt, or a
    /// typed ride-map error when the map core cannot enqueue the connection.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns the Arc argument at the binding boundary."
    )]
    pub fn begin_ride_recording_for_verified_connection(
        &self,
        ride_map: Arc<MobileRideMapCore>,
        token: MobileConnectionAttemptTokenDto,
        at_ms: u64,
    ) -> Result<Arc<MobileRideMapConnectionAdmission>, MobileRideMapCoreErrorDto> {
        let token = token.into();
        let pending = {
            let state = self.lock_inner();
            let verified = state
                .session_state()
                .connection
                .verified_attempt(&token)
                .ok_or(MobileRideMapCoreErrorDto::StaleConnection)?;
            ride_map.begin_verified_connection_admission(
                verified.platform_identifier(),
                at_ms,
                verified.generation(),
            )?
        };
        Ok(MobileRideMapConnectionAdmission::new(ride_map, pending))
    }

    /// Records telemetry only when the current verified vehicle owns the active ride.
    ///
    /// Connection identity and generation are read from the Rust-issued token while the session
    /// lock is held. The ride-map core rejects notifications from a different vehicle without
    /// advancing that ride's telemetry freshness.
    ///
    /// # Errors
    ///
    /// Returns `StaleConnection` when the token is no longer the current verified attempt, or a
    /// typed ride-map error when durable telemetry metadata cannot be updated.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns the Arc argument at the binding boundary."
    )]
    pub fn observe_ride_telemetry_for_verified_connection(
        &self,
        ride_map: Arc<MobileRideMapCore>,
        token: MobileConnectionAttemptTokenDto,
        at_ms: u64,
        speed_observation: Option<MobileRideMapSpeedObservationDto>,
    ) -> Result<MobileRideMapTelemetryObservationDto, MobileRideMapCoreErrorDto> {
        let token = token.into();
        let state = self.lock_inner();
        let verified = state
            .session_state()
            .connection
            .verified_attempt(&token)
            .ok_or(MobileRideMapCoreErrorDto::StaleConnection)?;
        ride_map.observe_telemetry_for_vehicle_on_connection(
            verified.platform_identifier(),
            verified.generation(),
            at_ms,
            speed_observation,
        )
    }

    /// Expires pending detection without allowing a late response to promote it.
    pub fn expire_connection_attempt(
        &self,
        token: MobileConnectionAttemptTokenDto,
        now_ms: u64,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner
            .session_state_mut()
            .connection
            .expire(&token.into(), MonotonicTimestamp::new(now_ms));
        inner.session_state().connection.snapshot().into()
    }

    /// Invalidates before native cancellation so queued consumers reject old work.
    pub fn disconnect_connection_attempt(&self) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner.disconnect();
        inner.session_state().connection.snapshot().into()
    }

    /// Keeps detection errors record-only and invalidates errors after verification.
    pub fn connection_transport_failed(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner.transport_failed(&token.into());
        inner.session_state().connection.snapshot().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MobileRideEventDto, MobileRideMapLifecycleCommand, MobileRideMapLifecyclePollDto,
        MobileRideMapSpeedDto, MobileRideMapSpeedSourceDto,
    };

    fn complete_admission(
        admission: &MobileRideMapConnectionAdmission,
    ) -> Option<MobileRideMapCoreSnapshotDto> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match admission.poll().expect("admission completes") {
                MobileRideMapAdmissionPollDto::Completed { snapshot } => return snapshot,
                MobileRideMapAdmissionPollDto::Pending => {
                    assert!(std::time::Instant::now() < deadline, "admission timed out");
                    std::thread::yield_now();
                }
            }
        }
    }

    fn complete_lifecycle(command: &MobileRideMapLifecycleCommand) {
        loop {
            match command.poll().expect("lifecycle command completes") {
                MobileRideMapLifecyclePollDto::Pending => std::thread::yield_now(),
                MobileRideMapLifecyclePollDto::Completed { .. } => return,
            }
        }
    }

    fn admit_verified_connection(
        handle: &CutoutSessionStateHandle,
        ride_map: Arc<MobileRideMapCore>,
        platform_identifier: &str,
        at_ms: u64,
    ) -> (
        MobileConnectionAttemptTokenDto,
        MobileRideMapCoreSnapshotDto,
    ) {
        let token = handle
            .begin_connection_attempt(platform_identifier.to_owned(), at_ms)
            .token
            .expect("connection attempt gets a token");
        handle.connection_link_established(token.clone());
        assert!(
            handle
                .lock_inner()
                .session_state_mut()
                .connection
                .finish_detection(&token.clone().into(), true)
        );
        let admission = handle
            .begin_ride_recording_for_verified_connection(ride_map, token.clone(), at_ms + 100)
            .expect("verified connection is admitted");
        (
            token,
            complete_admission(&admission).expect("ride is available"),
        )
    }

    #[test]
    fn queued_admission_reads_identity_and_verification_atomically() {
        let handle = CutoutSessionStateHandle::new();
        let snapshot = handle.begin_connection_attempt("A".into(), 10);
        let token = snapshot.token.unwrap();
        handle.connection_link_established(token.clone());
        assert!(!handle.verified_connection_attempt_is_current(token.clone()));
        {
            let mut inner = handle.lock_inner();
            assert!(
                inner
                    .session_state_mut()
                    .connection
                    .finish_detection(&token.clone().into(), true)
            );
        }
        assert!(handle.verified_connection_attempt_is_current(token.clone()));
        handle.begin_connection_attempt("A".into(), 20);
        assert!(!handle.verified_connection_attempt_is_current(token.clone()));
        assert!(!handle.connection_attempt_is_current(token));
    }

    #[test]
    fn retry_ffi_requires_the_rust_deadline_and_returns_a_new_attempt() {
        let handle = CutoutSessionStateHandle::new();
        let initial = handle.begin_connection_attempt("A".into(), 100);
        let token = initial.token.expect("attempt token");
        handle.connection_link_established(token.clone());
        {
            let mut inner = handle.lock_inner();
            assert!(
                inner
                    .session_state_mut()
                    .connection
                    .finish_detection(&token.clone().into(), true)
            );
        }

        let MobileConnectionRetryDecisionDto::Scheduled { retry } =
            handle.request_connection_retry(token.clone(), 1_000, 500)
        else {
            panic!("verified connection should schedule its first retry");
        };
        assert_eq!(retry.platform_identifier, "A");
        assert_eq!(retry.attempt, 1);
        assert_eq!(retry.deadline_ms, 1_250);
        assert!(handle.admit_connection_retry(retry.token, 1_249).is_none());

        let admitted = handle
            .admit_connection_retry(retry.token, retry.deadline_ms)
            .expect("timer admitted at Rust deadline");
        assert_eq!(admitted.readiness, MobileConnectionReadinessDto::Pending);
        assert_eq!(
            admitted.transport,
            MobileConnectionTransportStateDto::Connecting
        );
        assert_ne!(
            admitted.token.expect("new token").generation,
            token.generation
        );
        assert!(!handle.connection_attempt_is_current(token));
    }

    #[test]
    fn stale_verified_connection_is_rejected_before_ride_admission() {
        let handle = CutoutSessionStateHandle::new();
        let snapshot = handle.begin_connection_attempt("A".into(), 10);
        let token = snapshot.token.unwrap();

        assert_eq!(
            handle
                .begin_ride_recording_for_verified_connection(MobileRideMapCore::new(), token, 20)
                .expect_err("pending connection cannot enter the ride path"),
            MobileRideMapCoreErrorDto::StaleConnection
        );

        let replacement = handle.begin_connection_attempt("B".into(), 30);
        let replacement_token = replacement.token.unwrap();
        assert_eq!(
            handle
                .begin_ride_recording_for_verified_connection(
                    MobileRideMapCore::new(),
                    replacement_token,
                    40
                )
                .expect_err("replacement must still be verified before admission"),
            MobileRideMapCoreErrorDto::StaleConnection
        );
    }

    #[test]
    fn ride_telemetry_requires_the_verified_vehicle_to_match_the_ride() {
        let handle = CutoutSessionStateHandle::new();
        let ride_map = MobileRideMapCore::new();
        let started = ride_map.start_gps_only(900).unwrap();
        let (token_a, associated_a) =
            admit_verified_connection(&handle, ride_map.clone(), "pev-a", 1_000);
        assert_eq!(associated_a.ride_id, started.ride_id);
        assert_eq!(
            handle
                .observe_ride_telemetry_for_verified_connection(
                    ride_map.clone(),
                    token_a.clone(),
                    1_200,
                    Some(MobileRideMapSpeedObservationDto {
                        millimetres_per_second: 5_000,
                        observed_at_ms: 1_200,
                    }),
                )
                .unwrap(),
            MobileRideMapTelemetryObservationDto::Observed
        );
        assert_eq!(
            ride_map.current_snapshot(1_200).unwrap().live_speed,
            MobileRideMapSpeedDto {
                millimetres_per_second: 5_000,
                source: MobileRideMapSpeedSourceDto::Vehicle,
            }
        );
        let (token_b, retained_a) =
            admit_verified_connection(&handle, ride_map.clone(), "pev-b", 1_300);
        assert_eq!(retained_a.ride_id, started.ride_id);
        assert_eq!(retained_a.associated_vehicle.as_deref(), Some("pev-a"));
        assert_eq!(
            handle
                .observe_ride_telemetry_for_verified_connection(
                    ride_map.clone(),
                    token_b,
                    1_500,
                    Some(MobileRideMapSpeedObservationDto {
                        millimetres_per_second: 9_000,
                        observed_at_ms: 1_500,
                    }),
                )
                .unwrap(),
            MobileRideMapTelemetryObservationDto::NotAssociated
        );
        assert_eq!(
            ride_map.current_snapshot(1_500).unwrap().live_speed,
            MobileRideMapSpeedDto {
                millimetres_per_second: 5_000,
                source: MobileRideMapSpeedSourceDto::Vehicle,
            }
        );

        let (token_a_again, _) =
            admit_verified_connection(&handle, ride_map.clone(), "pev-a", 1_600);
        assert_eq!(
            handle
                .observe_ride_telemetry_for_verified_connection(
                    ride_map.clone(),
                    token_a_again.clone(),
                    1_800,
                    Some(MobileRideMapSpeedObservationDto {
                        millimetres_per_second: 0,
                        observed_at_ms: 1_800,
                    }),
                )
                .unwrap(),
            MobileRideMapTelemetryObservationDto::Observed
        );
        assert_eq!(
            ride_map.current_snapshot(1_800).unwrap().live_speed,
            MobileRideMapSpeedDto {
                millimetres_per_second: 0,
                source: MobileRideMapSpeedSourceDto::Vehicle,
            }
        );
    }

    #[test]
    fn queued_speed_from_before_async_resume_is_not_reused() {
        let handle = CutoutSessionStateHandle::new();
        let ride_map = MobileRideMapCore::new();
        ride_map.start_gps_only(900).unwrap();
        let token = handle
            .begin_connection_attempt("pev-a".into(), 1_000)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        assert!(
            handle
                .lock_inner()
                .session_state_mut()
                .connection
                .finish_detection(&token.clone().into(), true)
        );
        let admission = handle
            .begin_ride_recording_for_verified_connection(ride_map.clone(), token.clone(), 1_100)
            .unwrap();
        complete_admission(&admission).unwrap();
        handle
            .observe_ride_telemetry_for_verified_connection(
                ride_map.clone(),
                token.clone(),
                1_200,
                Some(MobileRideMapSpeedObservationDto {
                    millimetres_per_second: 5_000,
                    observed_at_ms: 1_200,
                }),
            )
            .unwrap();

        let pause_token = ride_map
            .current_snapshot(1_200)
            .unwrap()
            .command_token
            .unwrap();
        let pause = ride_map
            .begin_lifecycle_command(MobileRideEventDto::Pause, pause_token, 1_250)
            .unwrap();
        complete_lifecycle(&pause);
        let resume_token = ride_map
            .current_snapshot(1_250)
            .unwrap()
            .command_token
            .unwrap();
        let resume = ride_map
            .begin_lifecycle_command(MobileRideEventDto::Resume, resume_token, 1_600)
            .unwrap();
        complete_lifecycle(&resume);

        handle
            .observe_ride_telemetry_for_verified_connection(
                ride_map.clone(),
                token.clone(),
                1_700,
                Some(MobileRideMapSpeedObservationDto {
                    millimetres_per_second: 4_000,
                    observed_at_ms: 1_200,
                }),
            )
            .unwrap();
        assert_eq!(
            ride_map.current_snapshot(1_700).unwrap().live_speed.source,
            MobileRideMapSpeedSourceDto::Unavailable
        );
        handle
            .observe_ride_telemetry_for_verified_connection(
                ride_map.clone(),
                token,
                1_800,
                Some(MobileRideMapSpeedObservationDto {
                    millimetres_per_second: -2_000,
                    observed_at_ms: 1_800,
                }),
            )
            .unwrap();
        assert_eq!(
            ride_map.current_snapshot(1_800).unwrap().live_speed,
            MobileRideMapSpeedDto {
                millimetres_per_second: -2_000,
                source: MobileRideMapSpeedSourceDto::Vehicle,
            }
        );
    }

    #[test]
    fn verified_admission_releases_session_lock_after_ordered_enqueue() {
        let handle = CutoutSessionStateHandle::new();
        let initial = handle.begin_connection_attempt("A".into(), 10);
        let token = initial.token.unwrap();
        handle.connection_link_established(token.clone());
        {
            let mut inner = handle.lock_inner();
            assert!(
                inner
                    .session_state_mut()
                    .connection
                    .finish_detection(&token.clone().into(), true)
            );
        }
        assert!(handle.verified_connection_attempt_is_current(token.clone()));

        let ride_map = MobileRideMapCore::new();
        let (entered_sender, entered_receiver) = std::sync::mpsc::sync_channel(1);
        let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
        MobileRideMapCore::install_verified_connection_admission_test_gate(
            entered_sender,
            release_receiver,
        );

        let task_handle = Arc::clone(&handle);
        let task_map = Arc::clone(&ride_map);
        let admission = task_handle
            .begin_ride_recording_for_verified_connection(task_map, token.clone(), 20)
            .expect("verified connection enqueues admission without waiting for SQLite");
        let task_admission = Arc::clone(&admission);
        let poll = std::thread::spawn(move || task_admission.poll());
        entered_receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("admission polling reaches the held terminal-result boundary");
        assert_eq!(
            ride_map
                .pause(21)
                .expect_err("lifecycle cannot overtake the pending admission"),
            MobileRideMapCoreErrorDto::AdmissionPending
        );

        let disconnect_handle = Arc::clone(&handle);
        let (disconnected_sender, disconnected_receiver) = std::sync::mpsc::sync_channel(1);
        let disconnect = std::thread::spawn(move || {
            let revision = disconnect_handle.disconnect_connection_attempt().revision;
            let _ = disconnected_sender.send(revision);
        });
        let disconnected_revision =
            disconnected_receiver.recv_timeout(std::time::Duration::from_secs(1));
        release_sender.send(()).unwrap();
        let completed = MobileRideMapAdmissionPollDto::Completed { snapshot: None };
        assert_eq!(poll.join().unwrap().unwrap(), completed);
        assert_eq!(admission.poll().unwrap(), completed);
        assert!(disconnect.join().is_ok());
        assert!(
            disconnected_revision.expect("disconnect must not wait for SQLite completion")
                > initial.revision
        );
        assert!(!handle.verified_connection_attempt_is_current(token));
        assert_eq!(
            ride_map
                .inner
                .lock()
                .unwrap()
                .last_connected_vehicle
                .as_ref()
                .map(cutout_ride_maps::VehicleIdentity::as_str),
            Some("A")
        );
    }

    #[test]
    fn deadline_preserves_raw_capture_without_ride_admission() {
        let handle = CutoutSessionStateHandle::new();
        let initial = handle.begin_connection_attempt("A".into(), 10);
        let token = initial.token.unwrap();
        let expired = handle.expire_connection_attempt(token.clone(), 15_010);
        assert_eq!(expired.readiness, MobileConnectionReadinessDto::RecordOnly);
        assert!(expired.revision > initial.revision);
        assert!(handle.connection_attempt_is_current(token.clone()));
        assert!(!handle.verified_connection_attempt_is_current(token.clone()));
        handle.disconnect_connection_attempt();
        assert!(!handle.connection_attempt_is_current(token));
    }
}
