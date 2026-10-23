use crate::{ElapsedTimeMilliseconds, WallClockUnixMilliseconds};

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
        EXPLICIT_PAUSE_PERSISTENCE_WINDOW, INTERRUPTED_RIDE_RESUME_WINDOW, RideRecoveryDecision,
        RideRecoveryKind, ride_recovery_decision,
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
}
