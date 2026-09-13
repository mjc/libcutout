//! Immutable connection admission through the existing session-state mutex.

use cutout_core::{ConnectionAttemptSnapshot, ConnectionAttemptToken, ConnectionReadiness};

use crate::{CutoutSessionStateHandle, DeviceDetectionSession, MonotonicTimestamp};

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
    /// Monotonic whole-attempt deadline in milliseconds.
    pub deadline_ms: Option<u64>,
}

impl From<MobileConnectionAttemptTokenDto> for ConnectionAttemptToken {
    fn from(value: MobileConnectionAttemptTokenDto) -> Self {
        Self {
            generation: value.generation,
            platform_identifier: value.platform_identifier,
        }
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
                    generation: token.generation,
                    platform_identifier: token.platform_identifier.clone(),
                }),
            readiness: match value.readiness {
                ConnectionReadiness::Disconnected => MobileConnectionReadinessDto::Disconnected,
                ConnectionReadiness::Pending => MobileConnectionReadinessDto::Pending,
                ConnectionReadiness::Verified => MobileConnectionReadinessDto::Verified,
                ConnectionReadiness::RecordOnly => MobileConnectionReadinessDto::RecordOnly,
                ConnectionReadiness::Failed => MobileConnectionReadinessDto::Failed,
            },
            deadline_ms: value.deadline.map(MonotonicTimestamp::get),
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Replaces attempt and detector together, preserving discovery observations.
    pub fn begin_connection_attempt(
        &self,
        platform_identifier: String,
        now_ms: u64,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner.state.reset_device_identity();
        inner.detector = DeviceDetectionSession::default();
        inner
            .state
            .connection
            .begin(platform_identifier, MonotonicTimestamp::new(now_ms));
        inner.state.connection.snapshot().into()
    }

    /// Returns readiness and identity under one lock.
    #[must_use]
    pub fn connection_attempt_snapshot(&self) -> MobileConnectionAttemptSnapshotDto {
        self.lock_inner().state.connection.snapshot().into()
    }

    /// Admits raw transport work only for the active attempt.
    #[must_use]
    pub fn connection_attempt_is_current(&self, token: MobileConnectionAttemptTokenDto) -> bool {
        self.lock_inner().state.connection.is_current(&token.into())
    }

    /// Admits queued ride work only while the captured attempt remains verified.
    #[must_use]
    pub fn verified_connection_attempt_is_current(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> bool {
        self.lock_inner()
            .state
            .connection
            .is_verified(&token.into())
    }

    /// Expires pending detection without allowing a late response to promote it.
    pub fn expire_connection_attempt(
        &self,
        token: MobileConnectionAttemptTokenDto,
        now_ms: u64,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner
            .state
            .connection
            .expire(&token.into(), MonotonicTimestamp::new(now_ms));
        inner.state.connection.snapshot().into()
    }

    /// Invalidates before native cancellation so queued consumers reject old work.
    pub fn disconnect_connection_attempt(&self) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner.state.connection.disconnect();
        inner.state.connection.snapshot().into()
    }

    /// Keeps detection errors record-only and invalidates errors after verification.
    pub fn connection_transport_failed(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> MobileConnectionAttemptSnapshotDto {
        let mut inner = self.lock_inner();
        inner.state.connection.transport_failed(&token.into());
        inner.state.connection.snapshot().into()
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
        assert!(!handle.verified_connection_attempt_is_current(token.clone()));
        {
            let mut inner = handle.lock_inner();
            assert!(
                inner
                    .state
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
