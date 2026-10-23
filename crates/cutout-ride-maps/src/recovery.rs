use crate::{ElapsedTimeMilliseconds, WallClockUnixMilliseconds};
use std::num::NonZeroU64;

/// App presence used to anchor recovery from an interrupted live ride.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideAppPresence {
    /// The app is available to the rider in the foreground.
    Foreground,
    /// The app has left the foreground.
    Background,
}

/// Monotonic receipt order for app-presence observations within one Rust ride-map owner.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RideAppPresenceOrder(NonZeroU64);

impl RideAppPresenceOrder {
    /// Creates an observation order from a nonzero monotonically increasing owner sequence.
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the underlying owner sequence.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Whether authoritative wheel evidence permits live inactivity recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideInactivityEligibility {
    /// The canonical wheel-motion owner confirms that the wheel is parked.
    VerifiedParked,
    /// Motion is riding, standing, unknown, or only inferred from GPS.
    NotCanonicallyParked,
}

/// Persisted, ordered evidence of ride inactivity.
///
/// A background transition establishes the inactivity anchor. Repeated newer background
/// observations advance the event watermark without extending that anchor. A verified device
/// observation can move the effective recovery anchor forward while the app remains inactive.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RideInactivityEvidence {
    presence_order: Option<RideAppPresenceOrder>,
    presence_observed_at: Option<WallClockUnixMilliseconds>,
    app_inactive_since: Option<WallClockUnixMilliseconds>,
    last_verified_device_seen: Option<WallClockUnixMilliseconds>,
}

/// Persisted inactivity evidence violates its ordering invariants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideInactivityEvidenceError {
    /// Persisted presence order, event timestamp, and inactive anchor are inconsistent.
    InvalidState,
    /// There is no room to rebase pending presence events above the persisted watermark.
    OrderExhausted,
}

impl RideInactivityEvidence {
    /// Restores persisted evidence without inferring missing timestamps.
    ///
    /// # Errors
    /// Returns [`RideInactivityEvidenceError::InvalidState`] when the presence order and
    /// timestamp are inconsistent or an inactive anchor has no accepted presence event.
    #[must_use]
    pub const fn from_state_parts(
        presence_order: Option<RideAppPresenceOrder>,
        presence_observed_at: Option<WallClockUnixMilliseconds>,
        app_inactive_since: Option<WallClockUnixMilliseconds>,
        last_verified_device_seen: Option<WallClockUnixMilliseconds>,
    ) -> Result<Self, RideInactivityEvidenceError> {
        if presence_order.is_some() != presence_observed_at.is_some()
            || (app_inactive_since.is_some() && presence_order.is_none())
        {
            return Err(RideInactivityEvidenceError::InvalidState);
        }
        Ok(Self {
            presence_order,
            presence_observed_at,
            app_inactive_since,
            last_verified_device_seen,
        })
    }

    /// Returns the latest accepted app-presence receipt order.
    #[must_use]
    pub const fn presence_order(&self) -> Option<RideAppPresenceOrder> {
        self.presence_order
    }

    /// Returns the latest accepted app-presence event timestamp.
    #[must_use]
    pub const fn presence_observed_at(&self) -> Option<WallClockUnixMilliseconds> {
        self.presence_observed_at
    }

    /// Returns when the app most recently entered its current inactive interval.
    #[must_use]
    pub const fn app_inactive_since(&self) -> Option<WallClockUnixMilliseconds> {
        self.app_inactive_since
    }

    /// Returns the latest accepted verified-device observation timestamp.
    #[must_use]
    pub const fn last_verified_device_seen(&self) -> Option<WallClockUnixMilliseconds> {
        self.last_verified_device_seen
    }

    /// Merges a possibly stale snapshot without rolling accepted evidence backward.
    ///
    /// App-presence fields are one ordered fact and therefore move together. Verified-device
    /// time is independent and keeps its newest wall-clock observation. When both snapshots
    /// have the same app-presence order, `self` wins because it is the already accepted state.
    #[must_use]
    pub fn merge(&self, incoming: &Self) -> Self {
        let (presence_order, presence_observed_at, app_inactive_since) =
            if incoming.presence_order > self.presence_order {
                (
                    incoming.presence_order,
                    incoming.presence_observed_at,
                    incoming.app_inactive_since,
                )
            } else {
                (
                    self.presence_order,
                    self.presence_observed_at,
                    self.app_inactive_since,
                )
            };

        Self {
            presence_order,
            presence_observed_at,
            app_inactive_since,
            last_verified_device_seen: match (
                self.last_verified_device_seen,
                incoming.last_verified_device_seen,
            ) {
                (Some(current), Some(candidate)) => Some(current.max(candidate)),
                (Some(current), None) => Some(current),
                (None, Some(candidate)) => Some(candidate),
                (None, None) => None,
            },
        }
    }

    /// Applies a newer app-presence event; stale and equal events are ignored.
    ///
    /// Returns whether the event advanced the stored watermark. Equal conflicting events use
    /// first-write-wins semantics. Repeated background events preserve the original inactive
    /// anchor, while a foreground event clears it.
    pub fn observe_app_presence(
        &mut self,
        order: RideAppPresenceOrder,
        presence: RideAppPresence,
        observed_at: WallClockUnixMilliseconds,
    ) -> bool {
        if self
            .presence_order
            .is_some_and(|previous| order <= previous)
        {
            return false;
        }

        self.presence_order = Some(order);
        self.presence_observed_at = Some(observed_at);
        match presence {
            RideAppPresence::Foreground => self.app_inactive_since = None,
            RideAppPresence::Background => {
                self.app_inactive_since.get_or_insert(observed_at);
            }
        }
        true
    }

    /// Applies a newer verified-device observation without accepting clock regressions.
    ///
    /// Returns whether the timestamp advanced the stored watermark.
    pub fn observe_verified_device_seen(&mut self, observed_at: WallClockUnixMilliseconds) -> bool {
        if self
            .last_verified_device_seen
            .is_some_and(|previous| observed_at <= previous)
        {
            return false;
        }
        self.last_verified_device_seen = Some(observed_at);
        true
    }

    /// Selects a recovery action for an involuntarily interrupted ride while the app remains in
    /// a known inactive interval.
    ///
    /// The latest verified-device observation can extend the effective anchor, but a device
    /// timestamp alone never invents an app-inactive interval.
    #[must_use]
    pub fn inactivity_recovery_decision(
        &self,
        now: WallClockUnixMilliseconds,
        eligibility: RideInactivityEligibility,
    ) -> Option<RideRecoveryDecision> {
        if eligibility != RideInactivityEligibility::VerifiedParked {
            return None;
        }
        let inactive_since = self.app_inactive_since?;
        let anchor = self
            .last_verified_device_seen
            .map_or(inactive_since, |last_seen| inactive_since.max(last_seen));
        Some(ride_recovery_decision(
            RideRecoveryKind::InvoluntaryInterruption,
            now,
            anchor,
        ))
    }
}

/// Maximum age for automatically resuming an involuntarily interrupted ride.
pub const INTERRUPTED_RIDE_RESUME_WINDOW: ElapsedTimeMilliseconds =
    ElapsedTimeMilliseconds::new(3 * 60 * 60 * 1_000);

/// Maximum age for retaining an explicitly paused ride through recovery.
pub const EXPLICIT_PAUSE_PERSISTENCE_WINDOW: ElapsedTimeMilliseconds =
    ElapsedTimeMilliseconds::new(24 * 60 * 60 * 1_000);

/// Why a persisted recording needs recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideRecoveryKind {
    /// The process stopped without an explicit pause.
    InvoluntaryInterruption,
    /// The rider explicitly paused the recording.
    ExplicitPause,
}

/// Action selected for a persisted ride during recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideRecoveryDecision {
    /// Resume the interrupted ride.
    Resume,
    /// Preserve an explicit pause.
    KeepPaused,
    /// Expire recovery and settle the persisted ride.
    Finalize,
}

/// Selects the recovery action from persisted timing and the current wall clock.
#[must_use]
pub const fn ride_recovery_decision(
    kind: RideRecoveryKind,
    now: WallClockUnixMilliseconds,
    updated_at: WallClockUnixMilliseconds,
) -> RideRecoveryDecision {
    let Some(age_milliseconds) = now.as_u64().checked_sub(updated_at.as_u64()) else {
        return RideRecoveryDecision::Finalize;
    };

    match kind {
        RideRecoveryKind::InvoluntaryInterruption
            if age_milliseconds <= INTERRUPTED_RIDE_RESUME_WINDOW.as_u64() =>
        {
            RideRecoveryDecision::Resume
        }
        RideRecoveryKind::ExplicitPause
            if age_milliseconds <= EXPLICIT_PAUSE_PERSISTENCE_WINDOW.as_u64() =>
        {
            RideRecoveryDecision::KeepPaused
        }
        RideRecoveryKind::InvoluntaryInterruption | RideRecoveryKind::ExplicitPause => {
            RideRecoveryDecision::Finalize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EXPLICIT_PAUSE_PERSISTENCE_WINDOW, INTERRUPTED_RIDE_RESUME_WINDOW, RideAppPresence,
        RideAppPresenceOrder, RideInactivityEvidence, RideRecoveryDecision, RideRecoveryKind,
        ride_recovery_decision,
    };
    use crate::WallClockUnixMilliseconds;

    #[test]
    fn interrupted_recovery_uses_the_inclusive_three_hour_boundary() {
        let now = WallClockUnixMilliseconds::new(100_000_000);
        let window = INTERRUPTED_RIDE_RESUME_WINDOW.as_u64();
        let cases = [
            (window - 1, RideRecoveryDecision::Resume),
            (window, RideRecoveryDecision::Resume),
            (window + 1, RideRecoveryDecision::Finalize),
        ];

        for (age, expected) in cases {
            let updated_at = WallClockUnixMilliseconds::new(now.as_u64() - age);
            assert_eq!(
                ride_recovery_decision(RideRecoveryKind::InvoluntaryInterruption, now, updated_at),
                expected,
                "unexpected recovery decision at age {age}"
            );
        }
    }

    #[test]
    fn explicit_pause_uses_the_inclusive_twenty_four_hour_boundary() {
        let now = WallClockUnixMilliseconds::new(100_000_000);
        let window = EXPLICIT_PAUSE_PERSISTENCE_WINDOW.as_u64();
        let cases = [
            (window - 1, RideRecoveryDecision::KeepPaused),
            (window, RideRecoveryDecision::KeepPaused),
            (window + 1, RideRecoveryDecision::Finalize),
        ];

        for (age, expected) in cases {
            let updated_at = WallClockUnixMilliseconds::new(now.as_u64() - age);
            assert_eq!(
                ride_recovery_decision(RideRecoveryKind::ExplicitPause, now, updated_at),
                expected,
                "unexpected explicit-pause decision at age {age}"
            );
        }
    }

    #[test]
    fn a_future_persisted_timestamp_expires_both_recovery_kinds() {
        let now = WallClockUnixMilliseconds::new(10_000);
        let future = WallClockUnixMilliseconds::new(now.as_u64() + 1);

        for kind in [
            RideRecoveryKind::InvoluntaryInterruption,
            RideRecoveryKind::ExplicitPause,
        ] {
            assert_eq!(
                ride_recovery_decision(kind, now, future),
                RideRecoveryDecision::Finalize
            );
        }
    }

    #[test]
    fn inactivity_evidence_orders_presence_and_does_not_renew_background_anchor() {
        let mut evidence = RideInactivityEvidence::default();
        assert!(evidence.observe_app_presence(
            order(1),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(100),
        ));
        assert!(evidence.observe_app_presence(
            order(2),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(200),
        ));
        assert!(!evidence.observe_app_presence(
            order(2),
            RideAppPresence::Foreground,
            WallClockUnixMilliseconds::new(200),
        ));
        assert!(!evidence.observe_app_presence(
            order(1),
            RideAppPresence::Foreground,
            WallClockUnixMilliseconds::new(150),
        ));

        assert_eq!(
            evidence.presence_observed_at(),
            Some(WallClockUnixMilliseconds::new(200))
        );
        assert_eq!(
            evidence.app_inactive_since(),
            Some(WallClockUnixMilliseconds::new(100))
        );
        assert_eq!(
            evidence.inactivity_recovery_decision(
                WallClockUnixMilliseconds::new(100 + INTERRUPTED_RIDE_RESUME_WINDOW.as_u64()),
                super::RideInactivityEligibility::VerifiedParked,
            ),
            Some(RideRecoveryDecision::Resume)
        );
    }

    #[test]
    fn foreground_clears_inactivity_but_keeps_verified_device_evidence() {
        let mut evidence = RideInactivityEvidence::default();
        assert!(evidence.observe_app_presence(
            order(1),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(100),
        ));
        assert!(evidence.observe_verified_device_seen(WallClockUnixMilliseconds::new(150)));
        assert!(evidence.observe_app_presence(
            order(2),
            RideAppPresence::Foreground,
            WallClockUnixMilliseconds::new(200),
        ));
        assert_eq!(evidence.app_inactive_since(), None);
        assert_eq!(
            evidence.last_verified_device_seen(),
            Some(WallClockUnixMilliseconds::new(150))
        );
        assert_eq!(
            evidence.inactivity_recovery_decision(
                WallClockUnixMilliseconds::new(1_000),
                super::RideInactivityEligibility::VerifiedParked,
            ),
            None
        );
    }

    #[test]
    fn inactivity_uses_newest_verified_device_time_and_preserves_pause_window() {
        let mut evidence = RideInactivityEvidence::default();
        assert!(evidence.observe_app_presence(
            order(1),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(100),
        ));
        assert!(evidence.observe_verified_device_seen(WallClockUnixMilliseconds::new(500)));
        assert!(!evidence.observe_verified_device_seen(WallClockUnixMilliseconds::new(499)));

        assert_eq!(
            evidence.inactivity_recovery_decision(
                WallClockUnixMilliseconds::new(500 + INTERRUPTED_RIDE_RESUME_WINDOW.as_u64()),
                super::RideInactivityEligibility::VerifiedParked,
            ),
            Some(RideRecoveryDecision::Resume)
        );
        assert_eq!(
            evidence.inactivity_recovery_decision(
                WallClockUnixMilliseconds::new(501 + INTERRUPTED_RIDE_RESUME_WINDOW.as_u64()),
                super::RideInactivityEligibility::VerifiedParked,
            ),
            Some(RideRecoveryDecision::Finalize)
        );
        assert_eq!(
            ride_recovery_decision(
                RideRecoveryKind::ExplicitPause,
                WallClockUnixMilliseconds::new(500 + EXPLICIT_PAUSE_PERSISTENCE_WINDOW.as_u64()),
                WallClockUnixMilliseconds::new(500),
            ),
            RideRecoveryDecision::KeepPaused
        );
    }

    #[test]
    fn persisted_evidence_parts_round_trip_without_inventing_presence() {
        let restored = RideInactivityEvidence::from_state_parts(
            Some(order(2)),
            Some(WallClockUnixMilliseconds::new(30)),
            Some(WallClockUnixMilliseconds::new(20)),
            Some(WallClockUnixMilliseconds::new(40)),
        )
        .expect("ordered persisted evidence restores");
        assert_eq!(restored.presence_order(), Some(order(2)));
        assert_eq!(
            restored.presence_observed_at(),
            Some(WallClockUnixMilliseconds::new(30))
        );
        assert_eq!(
            restored.app_inactive_since(),
            Some(WallClockUnixMilliseconds::new(20))
        );
        assert_eq!(
            restored.last_verified_device_seen(),
            Some(WallClockUnixMilliseconds::new(40))
        );
        assert_eq!(
            RideInactivityEvidence::default().inactivity_recovery_decision(
                WallClockUnixMilliseconds::new(100),
                super::RideInactivityEligibility::VerifiedParked,
            ),
            None
        );
    }

    #[test]
    fn persisted_evidence_requires_presence_order_for_presence_timestamps() {
        assert_eq!(
            RideInactivityEvidence::from_state_parts(
                None,
                None,
                Some(WallClockUnixMilliseconds::new(10)),
                None,
            ),
            Err(super::RideInactivityEvidenceError::InvalidState)
        );
        assert_eq!(
            RideInactivityEvidence::from_state_parts(
                None,
                Some(WallClockUnixMilliseconds::new(9)),
                None,
                None,
            ),
            Err(super::RideInactivityEvidenceError::InvalidState)
        );
        assert!(
            RideInactivityEvidence::from_state_parts(
                Some(order(1)),
                Some(WallClockUnixMilliseconds::new(9)),
                Some(WallClockUnixMilliseconds::new(10)),
                None,
            )
            .is_ok(),
            "wall-clock correction does not invalidate source ordering"
        );
    }

    #[test]
    fn evidence_merge_keeps_newer_presence_and_device_watermarks() {
        let mut current = RideInactivityEvidence::default();
        current.observe_app_presence(
            order(4),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(400),
        );
        current.observe_verified_device_seen(WallClockUnixMilliseconds::new(600));

        let mut stale = RideInactivityEvidence::default();
        stale.observe_app_presence(
            order(3),
            RideAppPresence::Foreground,
            WallClockUnixMilliseconds::new(500),
        );
        stale.observe_verified_device_seen(WallClockUnixMilliseconds::new(700));

        let merged = current.merge(&stale);
        assert_eq!(merged.presence_order(), Some(order(4)));
        assert_eq!(
            merged.presence_observed_at(),
            Some(WallClockUnixMilliseconds::new(400))
        );
        assert_eq!(
            merged.app_inactive_since(),
            Some(WallClockUnixMilliseconds::new(400))
        );
        assert_eq!(
            merged.last_verified_device_seen(),
            Some(WallClockUnixMilliseconds::new(700))
        );
    }

    #[test]
    fn evidence_merge_keeps_current_presence_for_equal_order() {
        let mut current = RideInactivityEvidence::default();
        current.observe_app_presence(
            order(7),
            RideAppPresence::Foreground,
            WallClockUnixMilliseconds::new(900),
        );
        let mut duplicate = RideInactivityEvidence::default();
        duplicate.observe_app_presence(
            order(7),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(1),
        );

        let merged = current.merge(&duplicate);
        assert_eq!(merged, current);
    }

    #[test]
    fn live_inactivity_requires_canonical_parked_wheel_evidence() {
        let mut evidence = RideInactivityEvidence::default();
        evidence.observe_app_presence(
            order(1),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(100),
        );
        let now = WallClockUnixMilliseconds::new(100 + INTERRUPTED_RIDE_RESUME_WINDOW.as_u64() + 1);

        assert_eq!(
            evidence.inactivity_recovery_decision(
                now,
                super::RideInactivityEligibility::NotCanonicallyParked,
            ),
            None
        );
        assert_eq!(
            evidence.inactivity_recovery_decision(
                now,
                super::RideInactivityEligibility::VerifiedParked,
            ),
            Some(RideRecoveryDecision::Finalize)
        );
    }

    #[test]
    fn presence_receipt_order_is_independent_of_corrected_wall_clock() {
        let mut evidence = RideInactivityEvidence::default();
        assert!(evidence.observe_app_presence(
            order(1),
            RideAppPresence::Foreground,
            WallClockUnixMilliseconds::new(1_000),
        ));
        assert!(evidence.observe_app_presence(
            order(2),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(1_000),
        ));
        assert!(evidence.observe_app_presence(
            order(3),
            RideAppPresence::Background,
            WallClockUnixMilliseconds::new(900),
        ));
        assert!(!evidence.observe_app_presence(
            order(2),
            RideAppPresence::Foreground,
            WallClockUnixMilliseconds::new(2_000),
        ));
        assert_eq!(evidence.presence_order(), Some(order(3)));
        assert_eq!(
            evidence.app_inactive_since(),
            Some(WallClockUnixMilliseconds::new(1_000))
        );
    }

    fn order(value: u64) -> RideAppPresenceOrder {
        RideAppPresenceOrder::new(value).expect("test receipt order is nonzero")
    }
}
