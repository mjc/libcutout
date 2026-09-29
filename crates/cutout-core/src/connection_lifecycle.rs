//! Attempt identity and terminal detection state shared by mobile transports.

use crate::MonotonicTimestamp;
use std::marker::PhantomData;

/// Identity of one connection attempt, including retries of the same device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionAttemptToken {
    /// Monotonic attempt generation within this session owner.
    generation: u64,
    /// Platform identifier selected for this attempt.
    platform_identifier: String,
}

/// Identity of one Rust-approved reconnect timer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionRetryToken(u64);

impl ConnectionRetryToken {
    /// Constructs a token at an FFI boundary. Callers should retain, not mint, tokens.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the retry identity for native timer correlation.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// One retry timer decision made by the connection lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionRetry {
    token: ConnectionRetryToken,
    attempt: u8,
    deadline: MonotonicTimestamp,
    platform_identifier: String,
}

impl ConnectionRetry {
    /// Returns the identity required to admit this timer when it fires.
    #[must_use]
    pub const fn token(&self) -> ConnectionRetryToken {
        self.token
    }

    /// Returns the one-based retry number.
    #[must_use]
    pub const fn attempt(&self) -> u8 {
        self.attempt
    }

    /// Returns the earliest monotonic time at which this retry may start.
    #[must_use]
    pub const fn deadline(&self) -> MonotonicTimestamp {
        self.deadline
    }

    /// Returns the platform identity selected for the failed connection.
    #[must_use]
    pub fn platform_identifier(&self) -> &str {
        &self.platform_identifier
    }
}

/// Result of asking Rust whether another reconnect should be scheduled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionRetryDecision {
    /// Retry is permitted after the returned monotonic deadline.
    Scheduled(ConnectionRetry),
    /// The retry budget is exhausted; the attempt is the next number that would be used.
    Exhausted {
        /// The next retry number, beyond the configured limit.
        attempt: u8,
    },
    /// The supplied connection no longer owns this retry sequence.
    Rejected,
}

/// Borrowed proof that an attempt is current, connected, and protocol-verified.
///
/// The proof is tied to the lifecycle borrow that produced it. Callers must obtain a new proof
/// after any lifecycle mutation instead of carrying a Boolean verification result across a
/// connection transition.
#[derive(Debug)]
pub struct VerifiedConnectionAttempt<'a> {
    token: &'a ConnectionAttemptToken,
    _lifecycle: PhantomData<&'a ConnectionAttemptLifecycle>,
}

impl VerifiedConnectionAttempt<'_> {
    /// Returns the verified platform identity.
    #[must_use]
    pub fn platform_identifier(&self) -> &str {
        self.token.platform_identifier()
    }

    /// Returns the connection generation captured by this proof.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.token.generation()
    }
}

impl ConnectionAttemptToken {
    /// Constructs a token at an FFI boundary. Callers should retain, not mint, tokens.
    #[must_use]
    pub fn new(generation: u64, platform_identifier: String) -> Self {
        Self {
            generation,
            platform_identifier,
        }
    }

    /// Returns the attempt generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the selected platform identifier.
    #[must_use]
    pub fn platform_identifier(&self) -> &str {
        &self.platform_identifier
    }
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
    /// A previously verified transport or session failed.
    Failed,
    /// A verified session observed protocol evidence that conflicts with its decoder.
    Conflicted,
}

/// Native link availability, independent of protocol admission.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionTransportState {
    /// No active native link can deliver data.
    #[default]
    Disconnected,
    /// A native connection request is outstanding.
    Connecting,
    /// Native link established; subscriptions may still be pending or unavailable.
    Connected,
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
    /// Native link availability; record-only never promises a working connection.
    pub transport: ConnectionTransportState,
    /// Deadline covering connection, GATT discovery and identification together.
    pub deadline: Option<MonotonicTimestamp>,
}

/// Owns connection attempt identity alongside the session's protocol detector.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionAttemptLifecycle {
    snapshot: ConnectionAttemptSnapshot,
    retry_attempt: u8,
    retry_token: u64,
    failed_attempt: Option<ConnectionAttemptToken>,
    pending_retry: Option<ConnectionRetry>,
}

impl ConnectionAttemptLifecycle {
    /// Starts an attempt and retires the previous attempt, even for the same device.
    pub fn begin(
        &mut self,
        platform_identifier: String,
        at: MonotonicTimestamp,
    ) -> ConnectionAttemptToken {
        self.retry_attempt = 0;
        self.failed_attempt = None;
        self.pending_retry = None;
        self.begin_attempt(platform_identifier, at)
    }

    fn begin_attempt(
        &mut self,
        platform_identifier: String,
        at: MonotonicTimestamp,
    ) -> ConnectionAttemptToken {
        self.snapshot.generation = self.snapshot.generation.wrapping_add(1);
        let token = ConnectionAttemptToken::new(self.snapshot.generation, platform_identifier);
        self.snapshot.token = Some(token.clone());
        self.snapshot.readiness = ConnectionReadiness::Pending;
        self.snapshot.transport = ConnectionTransportState::Connecting;
        self.snapshot.deadline = Some(MonotonicTimestamp::new(at.get().saturating_add(15_000)));
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        token
    }

    /// Requests the next reconnect using Rust-owned attempt limits and backoff.
    ///
    /// `jitter_permille` is platform-provided entropy in the inclusive range 0..=1000; policy,
    /// retry count, and deadline remain owned here.
    pub fn request_retry(
        &mut self,
        failed: &ConnectionAttemptToken,
        at: MonotonicTimestamp,
        jitter_permille: u16,
    ) -> ConnectionRetryDecision {
        if self.pending_retry.is_some() {
            return ConnectionRetryDecision::Rejected;
        }

        let can_retry = match self.snapshot.readiness {
            ConnectionReadiness::Pending
            | ConnectionReadiness::Verified
            | ConnectionReadiness::RecordOnly
            | ConnectionReadiness::Failed => true,
            ConnectionReadiness::Disconnected | ConnectionReadiness::Conflicted => false,
        };
        let is_active_failure = self.is_current(failed) && can_retry;
        if !is_active_failure && self.failed_attempt.as_ref() != Some(failed) {
            return ConnectionRetryDecision::Rejected;
        }

        let attempt = self.retry_attempt.saturating_add(1);
        if attempt > 3 {
            if self.is_current(failed) {
                self.snapshot.generation = self.snapshot.generation.wrapping_add(1);
                self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
                self.snapshot.token = None;
                self.snapshot.deadline = None;
                self.snapshot.readiness = ConnectionReadiness::Failed;
                self.snapshot.transport = ConnectionTransportState::Disconnected;
            }
            self.failed_attempt = Some(failed.clone());
            self.retry_attempt = attempt;
            return ConnectionRetryDecision::Exhausted { attempt };
        }

        let identifier = failed.platform_identifier().to_owned();
        if self.is_current(failed) {
            self.snapshot.deadline = None;
            self.snapshot.readiness = ConnectionReadiness::Failed;
            self.snapshot.transport = ConnectionTransportState::Disconnected;
            self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        }
        self.failed_attempt = Some(failed.clone());
        self.retry_attempt = attempt;
        self.retry_token = self.retry_token.wrapping_add(1);

        let jitter = u64::from(jitter_permille.min(1_000));
        let base_ms = 250_u64.saturating_mul(1_u64 << (attempt - 1));
        let delay_ms = base_ms
            .saturating_mul(800_000 + 400 * jitter)
            .saturating_add(500_000)
            / 1_000_000;
        let retry = ConnectionRetry {
            token: ConnectionRetryToken::new(self.retry_token),
            attempt,
            deadline: MonotonicTimestamp::new(at.get().saturating_add(delay_ms)),
            platform_identifier: identifier,
        };
        self.pending_retry = Some(retry.clone());
        ConnectionRetryDecision::Scheduled(retry)
    }

    /// Starts an approved retry only when its current timer has reached the Rust deadline.
    pub fn admit_retry(
        &mut self,
        token: ConnectionRetryToken,
        at: MonotonicTimestamp,
    ) -> Option<ConnectionAttemptToken> {
        let retry = self.pending_retry.as_ref()?;
        if retry.token != token || at < retry.deadline {
            return None;
        }
        let retry = self.pending_retry.take()?;
        self.failed_attempt = None;
        Some(self.begin_attempt(retry.platform_identifier, at))
    }

    /// Cancels the matching pending retry and resets its consecutive-failure budget.
    pub fn cancel_retry(&mut self, token: ConnectionRetryToken) -> bool {
        if self.pending_retry.as_ref().map(|retry| retry.token) != Some(token) {
            return false;
        }
        self.pending_retry = None;
        self.retry_attempt = 0;
        self.failed_attempt = None;
        true
    }

    /// Returns the pending timer's deadline only to its owner.
    #[must_use]
    pub fn retry_deadline(&self, token: ConnectionRetryToken) -> Option<MonotonicTimestamp> {
        self.pending_retry
            .as_ref()
            .filter(|retry| retry.token == token)
            .map(|retry| retry.deadline)
    }

    /// Checks ownership without modifying the attempt or detector.
    #[must_use]
    pub fn is_current(&self, token: &ConnectionAttemptToken) -> bool {
        self.snapshot.token.as_ref() == Some(token)
    }

    /// Atomically checks identity and permission to admit decoded ride telemetry.
    #[must_use]
    pub fn is_verified(&self, token: &ConnectionAttemptToken) -> bool {
        self.verified_attempt(token).is_some()
    }

    /// Borrows the current attempt only while it is connected and protocol-verified.
    #[must_use]
    pub fn verified_attempt<'a>(
        &'a self,
        token: &'a ConnectionAttemptToken,
    ) -> Option<VerifiedConnectionAttempt<'a>> {
        (self.is_current(token)
            && self.snapshot.readiness == ConnectionReadiness::Verified
            && self.snapshot.transport == ConnectionTransportState::Connected)
            .then_some(VerifiedConnectionAttempt {
                token,
                _lifecycle: PhantomData,
            })
    }

    /// Completes pending identification exactly once.
    pub fn finish_detection(&mut self, token: &ConnectionAttemptToken, verified: bool) -> bool {
        if !self.is_current(token) || self.snapshot.readiness != ConnectionReadiness::Pending {
            return false;
        }
        if verified && self.snapshot.transport != ConnectionTransportState::Connected {
            return false;
        }
        self.snapshot.readiness = if verified {
            ConnectionReadiness::Verified
        } else {
            ConnectionReadiness::RecordOnly
        };
        if verified {
            self.retry_attempt = 0;
            self.failed_attempt = None;
            self.pending_retry = None;
        }
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
        self.retry_attempt = 0;
        self.failed_attempt = None;
        self.pending_retry = None;
        self.snapshot.generation = self.snapshot.generation.wrapping_add(1);
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        self.snapshot.token = None;
        self.snapshot.deadline = None;
        self.snapshot.readiness = ConnectionReadiness::Disconnected;
        self.snapshot.transport = ConnectionTransportState::Disconnected;
    }

    /// Accepts native link establishment only for the outstanding active attempt.
    pub fn connected(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.is_current(token)
            || self.snapshot.transport != ConnectionTransportState::Connecting
        {
            return false;
        }
        self.snapshot.transport = ConnectionTransportState::Connected;
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        true
    }

    /// Records loss of native transport without claiming capture packets remain available.
    pub fn link_down(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.is_current(token) {
            return false;
        }
        self.snapshot.transport = ConnectionTransportState::Disconnected;
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        self.transport_failed(token);
        true
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
            let failed = token.clone();
            self.disconnect();
            self.failed_attempt = Some(failed);
            self.snapshot.readiness = ConnectionReadiness::Failed;
            return true;
        }
        false
    }

    /// Invalidates a verified attempt whose live protocol evidence became contradictory.
    pub fn conflict(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.is_current(token) || self.snapshot.readiness != ConnectionReadiness::Verified {
            return false;
        }
        self.snapshot.generation = self.snapshot.generation.wrapping_add(1);
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        self.snapshot.token = None;
        self.snapshot.deadline = None;
        self.snapshot.readiness = ConnectionReadiness::Conflicted;
        self.snapshot.transport = ConnectionTransportState::Disconnected;
        true
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
        assert!(lifecycle.connected(&token));
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
    fn verified_attempt_guard_exposes_only_current_connected_identity() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let token = lifecycle.begin("A".into(), MonotonicTimestamp::new(100));

        assert!(lifecycle.verified_attempt(&token).is_none());
        assert!(lifecycle.connected(&token));
        assert!(lifecycle.finish_detection(&token, true));

        let verified = lifecycle
            .verified_attempt(&token)
            .expect("connected verified attempt");
        assert_eq!(verified.platform_identifier(), "A");
        assert_eq!(verified.generation(), token.generation());

        lifecycle.disconnect();
        assert!(lifecycle.verified_attempt(&token).is_none());
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
        assert!(lifecycle.connected(&live));
        assert!(lifecycle.finish_detection(&live, true));
        assert!(lifecycle.transport_failed(&live));
        assert_eq!(lifecycle.snapshot().readiness, ConnectionReadiness::Failed);
        assert!(!lifecycle.is_current(&live));
    }

    #[test]
    fn record_only_distinguishes_discovery_error_from_missing_transport() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let token = lifecycle.begin("A".into(), MonotonicTimestamp::new(0));
        assert!(lifecycle.connected(&token));
        assert!(lifecycle.transport_failed(&token));
        assert_eq!(
            lifecycle.snapshot().readiness,
            ConnectionReadiness::RecordOnly
        );
        assert_eq!(
            lifecycle.snapshot().transport,
            ConnectionTransportState::Connected
        );
        assert!(lifecycle.link_down(&token));
        assert_eq!(
            lifecycle.snapshot().readiness,
            ConnectionReadiness::RecordOnly
        );
        assert_eq!(
            lifecycle.snapshot().transport,
            ConnectionTransportState::Disconnected
        );
        assert!(!lifecycle.connected(&token));
    }

    #[test]
    fn protocol_conflict_is_terminal_and_rejects_the_old_token() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let token = lifecycle.begin("A".into(), MonotonicTimestamp::new(0));
        assert!(lifecycle.connected(&token));
        assert!(lifecycle.finish_detection(&token, true));
        assert!(lifecycle.conflict(&token));
        assert_eq!(
            lifecycle.snapshot().readiness,
            ConnectionReadiness::Conflicted
        );
        assert!(!lifecycle.is_current(&token));
        assert!(!lifecycle.conflict(&token));
    }

    #[test]
    fn reconnect_retry_deadline_and_admission_are_owned_by_rust() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let first = lifecycle.begin("A".into(), MonotonicTimestamp::new(100));
        assert!(lifecycle.connected(&first));
        assert!(lifecycle.finish_detection(&first, true));

        let retry = lifecycle.request_retry(&first, MonotonicTimestamp::new(1_000), 500);
        let ConnectionRetryDecision::Scheduled(retry) = retry else {
            panic!("verified connection should schedule its first retry");
        };
        assert_eq!(retry.attempt(), 1);
        assert_eq!(retry.deadline(), MonotonicTimestamp::new(1_250));
        assert_eq!(retry.platform_identifier(), "A");
        assert!(lifecycle.is_current(&first));
        assert_eq!(
            lifecycle.snapshot().transport,
            ConnectionTransportState::Disconnected
        );

        assert!(
            lifecycle
                .admit_retry(retry.token(), MonotonicTimestamp::new(1_249))
                .is_none()
        );
        let second = lifecycle
            .admit_retry(retry.token(), MonotonicTimestamp::new(1_250))
            .expect("retry is admitted at its deadline");
        assert_eq!(second.platform_identifier(), "A");
        assert!(!lifecycle.is_current(&first));
        assert!(lifecycle.is_current(&second));
        assert_eq!(
            lifecycle.snapshot().transport,
            ConnectionTransportState::Connecting
        );
    }

    #[test]
    fn retry_exhaustion_and_stale_timer_cannot_start_a_connection() {
        let mut lifecycle = ConnectionAttemptLifecycle::default();
        let mut attempt = lifecycle.begin("A".into(), MonotonicTimestamp::new(0));

        for expected in 1..=3 {
            let decision = lifecycle.request_retry(
                &attempt,
                MonotonicTimestamp::new(u64::from(expected) * 1_000),
                500,
            );
            let ConnectionRetryDecision::Scheduled(retry) = decision else {
                panic!("retry {expected} should be scheduled");
            };
            assert_eq!(retry.attempt(), expected);
            let stale = retry.token();
            attempt = lifecycle
                .admit_retry(stale, retry.deadline())
                .expect("scheduled retry should be admitted");
            assert!(lifecycle.connected(&attempt));
            assert!(lifecycle.finish_detection(&attempt, false));
        }

        let fourth = lifecycle.request_retry(&attempt, MonotonicTimestamp::new(10_000), 500);
        assert_eq!(fourth, ConnectionRetryDecision::Exhausted { attempt: 4 });
        assert!(
            lifecycle
                .admit_retry(
                    ConnectionRetryToken::new(u64::MAX),
                    MonotonicTimestamp::new(u64::MAX)
                )
                .is_none()
        );
        assert_eq!(
            lifecycle.snapshot().transport,
            ConnectionTransportState::Disconnected
        );
    }
}
