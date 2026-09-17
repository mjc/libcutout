//! Immutable connection admission through the existing session-state mutex.

use cutout_core::{
    ConnectionAttemptSnapshot, ConnectionAttemptToken, ConnectionReadiness,
    ConnectionTransportState,
};
use std::sync::Arc;

use crate::{
    CutoutSessionStateHandle, MobileRideMapCore, MobileRideMapCoreErrorDto,
    MobileRideMapCoreSnapshotDto, MonotonicTimestamp,
};

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
    /// Capture storage or a previously verified connection failed.
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

    /// Atomically admits one verified connection to the Rust-owned ride-map core.
    ///
    /// The session-state lock remains held while the map core consumes the token, so connection
    /// invalidation cannot race between verification and ride admission. Swift supplies only the
    /// Rust-issued attempt token; it cannot choose a separate identity or automatic policy.
    ///
    /// # Errors
    ///
    /// Returns `StaleConnection` when the token is no longer the current verified attempt, or a
    /// typed ride-map error when the map core cannot admit the connection.
    pub fn ensure_ride_recording_for_verified_connection(
        &self,
        ride_map: Arc<MobileRideMapCore>,
        token: MobileConnectionAttemptTokenDto,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        let token = token.into();
        let state = self.lock_inner();
        let verified = state
            .session_state()
            .connection
            .verified_attempt(&token)
            .ok_or(MobileRideMapCoreErrorDto::StaleConnection)?;
        ride_map.ensure_recording_for_vehicle_on_connection(
            verified.platform_identifier().to_owned(),
            at_ms,
            verified.generation(),
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

    /// Invalidates a capture that can no longer preserve incoming evidence.
    pub fn fail_connection_capture(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner.fail_capture(&token.into());
        inner.session_state().connection.snapshot().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn stale_verified_connection_is_rejected_before_ride_admission() {
        let handle = CutoutSessionStateHandle::new();
        let snapshot = handle.begin_connection_attempt("A".into(), 10);
        let token = snapshot.token.unwrap();

        assert_eq!(
            handle
                .ensure_ride_recording_for_verified_connection(MobileRideMapCore::new(), token, 20)
                .expect_err("pending connection cannot enter the ride path"),
            MobileRideMapCoreErrorDto::StaleConnection
        );

        let replacement = handle.begin_connection_attempt("B".into(), 30);
        let replacement_token = replacement.token.unwrap();
        assert_eq!(
            handle
                .ensure_ride_recording_for_verified_connection(
                    MobileRideMapCore::new(),
                    replacement_token,
                    40
                )
                .expect_err("replacement must still be verified before admission"),
            MobileRideMapCoreErrorDto::StaleConnection
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
