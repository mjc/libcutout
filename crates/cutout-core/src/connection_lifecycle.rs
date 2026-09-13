//! Attempt identity and terminal detection state shared by mobile transports.

use crate::MonotonicTimestamp;

/// Identity of one connection attempt, including retries of the same device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionAttemptToken {
    /// Monotonic attempt generation within this session owner.
    pub generation: u64,
    /// Platform identifier selected for this attempt.
    pub platform_identifier: String,
}

/// Admission state established by the connection and detector owner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionReadiness {
    /// No active attempt exists.
    #[default]
    Disconnected,
    /// Transport setup or protocol identification is still pending.
    Pending,
    /// Protocol evidence permits the typed Ride path.
    Verified,
    /// Detection ended without proof; only capture is permitted.
    RecordOnly,
    /// Capture storage or a previously verified session failed.
    Failed,
}

/// Immutable connection state published to presentation and queued consumers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionAttemptSnapshot {
    /// Changes when an attempt is replaced or invalidated.
    pub generation: u64,
    /// Changes for every accepted state transition.
    pub revision: u64,
    /// Token for the active transport/capture attempt, if any.
    pub token: Option<ConnectionAttemptToken>,
    /// Current admission state.
    pub readiness: ConnectionReadiness,
    /// Deadline covering connection, GATT discovery and identification together.
    pub deadline: Option<MonotonicTimestamp>,
}

/// Owns connection attempt identity alongside the session's protocol detector.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionAttemptLifecycle {
    snapshot: ConnectionAttemptSnapshot,
}

impl ConnectionAttemptLifecycle {
    /// Starts an attempt and retires the previous attempt, even for the same device.
    pub fn begin(
        &mut self,
        platform_identifier: String,
        at: MonotonicTimestamp,
    ) -> ConnectionAttemptToken {
        self.snapshot.generation = self.snapshot.generation.wrapping_add(1);
        let token = ConnectionAttemptToken {
            generation: self.snapshot.generation,
            platform_identifier,
        };
        self.snapshot.token = Some(token.clone());
        self.snapshot.readiness = ConnectionReadiness::Pending;
        self.snapshot.deadline = Some(MonotonicTimestamp::new(at.get().saturating_add(15_000)));
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        token
    }

    /// Checks ownership without modifying the attempt or detector.
    #[must_use]
    pub fn is_current(&self, token: &ConnectionAttemptToken) -> bool {
        self.snapshot.token.as_ref() == Some(token)
    }

    /// Atomically checks identity and permission to admit decoded ride telemetry.
    #[must_use]
    pub fn is_verified(&self, token: &ConnectionAttemptToken) -> bool {
        self.is_current(token) && self.snapshot.readiness == ConnectionReadiness::Verified
    }

    /// Completes pending identification exactly once.
    pub fn finish_detection(&mut self, token: &ConnectionAttemptToken, verified: bool) -> bool {
        if !self.is_current(token) || self.snapshot.readiness != ConnectionReadiness::Pending {
            return false;
        }
        self.snapshot.readiness = if verified {
            ConnectionReadiness::Verified
        } else {
            ConnectionReadiness::RecordOnly
        };
        self.snapshot.deadline = None;
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        true
    }

    /// Ends pending identification when the whole-attempt deadline expires.
    pub fn expire(&mut self, token: &ConnectionAttemptToken, at: MonotonicTimestamp) -> bool {
        if self.snapshot.deadline.is_none_or(|deadline| at < deadline) {
            return false;
        }
        self.finish_detection(token, false)
    }

    /// Invalidates the current attempt before native cancellation or replacement.
    pub fn disconnect(&mut self) {
        self.snapshot.generation = self.snapshot.generation.wrapping_add(1);
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        self.snapshot.token = None;
        self.snapshot.deadline = None;
        self.snapshot.readiness = ConnectionReadiness::Disconnected;
    }

    /// Keeps failed detection recordable; failures after verification are terminal.
    pub fn transport_failed(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.is_current(token) {
            return false;
        }
        if self.snapshot.readiness == ConnectionReadiness::Pending {
            return self.finish_detection(token, false);
        }
        if self.snapshot.readiness == ConnectionReadiness::Verified {
            self.fail_capture();
            return true;
        }
        false
    }

    /// Retires the attempt when capture storage can no longer preserve its evidence.
    pub fn fail_capture(&mut self) {
        self.disconnect();
        self.snapshot.readiness = ConnectionReadiness::Failed;
    }

    /// Returns the current immutable state.
    #[must_use]
    pub const fn snapshot(&self) -> &ConnectionAttemptSnapshot {
        &self.snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_device_or_retrying_same_device_retires_old_callbacks() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let a = lifecycle.begin("A".into(), MonotonicTimestamp::new(1));
        let b = lifecycle.begin("B".into(), MonotonicTimestamp::new(2));
        assert!(!lifecycle.is_current(&a));
        assert!(lifecycle.is_current(&b));
        let retry = lifecycle.begin("B".into(), MonotonicTimestamp::new(3));
        assert!(!lifecycle.is_current(&b));
        assert!(lifecycle.is_current(&retry));
        assert!(retry.generation > b.generation);
    }

    #[test]
    fn missing_gatt_callbacks_expire_to_capture_without_late_promotion() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let token = lifecycle.begin("A".into(), MonotonicTimestamp::new(100));
        assert!(!lifecycle.expire(&token, MonotonicTimestamp::new(15_099)));
        assert!(lifecycle.expire(&token, MonotonicTimestamp::new(15_100)));
        assert_eq!(
            lifecycle.snapshot().readiness,
            ConnectionReadiness::RecordOnly
        );
        assert!(lifecycle.is_current(&token));
        assert!(!lifecycle.is_verified(&token));
        assert!(!lifecycle.finish_detection(&token, true));
        assert!(lifecycle.snapshot().deadline.is_none());
    }

    #[test]
    fn verified_attempt_ignores_late_deadline_and_disconnect_retires_it() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let token = lifecycle.begin("A".into(), MonotonicTimestamp::new(100));
        assert!(lifecycle.finish_detection(&token, true));
        assert!(lifecycle.is_verified(&token));
        assert!(!lifecycle.expire(&token, MonotonicTimestamp::new(99_000)));
        assert!(!lifecycle.finish_detection(&token, false));
        lifecycle.disconnect();
        assert!(!lifecycle.is_current(&token));
        assert!(!lifecycle.is_verified(&token));
        assert_eq!(
            lifecycle.snapshot().readiness,
            ConnectionReadiness::Disconnected
        );
    }

    #[test]
    fn stale_success_and_deadline_cannot_end_replacement_attempt() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let old = lifecycle.begin("A".into(), MonotonicTimestamp::new(100));
        let current = lifecycle.begin("B".into(), MonotonicTimestamp::new(101));
        assert!(!lifecycle.finish_detection(&old, true));
        assert!(!lifecycle.expire(&old, MonotonicTimestamp::new(99_000)));
        assert_eq!(lifecycle.snapshot().readiness, ConnectionReadiness::Pending);
        assert!(lifecycle.finish_detection(&current, false));
        assert!(!lifecycle.finish_detection(&current, true));
    }

    #[test]
    fn transport_failure_preserves_capture_only_before_verification() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let detecting = lifecycle.begin("A".into(), MonotonicTimestamp::new(0));
        assert!(lifecycle.transport_failed(&detecting));
        assert_eq!(
            lifecycle.snapshot().readiness,
            ConnectionReadiness::RecordOnly
        );
        assert!(lifecycle.is_current(&detecting));
        let live = lifecycle.begin("A".into(), MonotonicTimestamp::new(1));
        assert!(lifecycle.finish_detection(&live, true));
        assert!(lifecycle.transport_failed(&live));
        assert_eq!(lifecycle.snapshot().readiness, ConnectionReadiness::Failed);
        assert!(!lifecycle.is_current(&live));
    }
}
