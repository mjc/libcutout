use thiserror::Error;

use crate::{Duration, DutyCycle, MonotonicTimestamp, RideStopReason, RideWarning};

const PWM_REARM_HYSTERESIS_PERCENT: u8 = 5;

/// A phone-generated PWM alarm threshold expressed as consumed duty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmDutyAlarmThreshold(u8);

impl PwmDutyAlarmThreshold {
    /// Creates a threshold from consumed PWM duty percent.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPwmDutyAlarmThreshold`] when `duty_percent` exceeds 100.
    pub const fn new(duty_percent: u8) -> Result<Self, InvalidPwmDutyAlarmThreshold> {
        if duty_percent <= 100 {
            Ok(Self(duty_percent))
        } else {
            Err(InvalidPwmDutyAlarmThreshold {
                value: duty_percent,
            })
        }
    }

    /// Returns consumed PWM duty percent.
    #[must_use]
    pub const fn duty_percent(self) -> u8 {
        self.0
    }

    /// Returns the complementary unused PWM headroom percent.
    #[must_use]
    pub const fn headroom_percent(self) -> u8 {
        100 - self.0
    }

    fn permille(self) -> u16 {
        u16::from(self.0) * 10
    }

    fn rearm_permille(self) -> u16 {
        u16::from(self.0.saturating_sub(PWM_REARM_HYSTERESIS_PERCENT)) * 10
    }
}

/// A PWM duty threshold outside zero through one hundred percent.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("PWM duty alarm threshold {value}% is outside 0% through 100%")]
pub struct InvalidPwmDutyAlarmThreshold {
    value: u8,
}

/// One fresh piece of ride evidence considered by the phone alarm policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneAlarmObservation {
    /// No alarm condition is active in the latest fresh ride state.
    Nominal,
    /// Fresh, signed controller duty; the policy evaluates its magnitude.
    PwmDuty(DutyCycle),
    /// Fresh controller warning decoded by the vehicle protocol.
    ControllerWarning(RideWarning),
    /// Fresh reason that the controller stopped balancing.
    ControllerStop(RideStopReason),
    /// Ride telemetry is too old to justify a phone alarm.
    Stale,
    /// The connected protocol does not supply the required evidence.
    Unavailable,
}

/// A normalized phone alarm event ready for native delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneAlarmEvent {
    /// Controller duty reached the configured phone threshold.
    PwmDuty {
        /// Consumed duty in percent.
        duty_percent: u8,
        /// Complementary unused headroom in percent.
        headroom_percent: u8,
    },
    /// A vehicle protocol reported a typed controller warning.
    ControllerWarning(RideWarning),
    /// A vehicle protocol reported why balancing stopped.
    ControllerStop(RideStopReason),
}

impl PhoneAlarmEvent {
    const fn condition(self) -> ActivePhoneAlarmCondition {
        match self {
            Self::PwmDuty { .. } => ActivePhoneAlarmCondition::PwmDuty,
            Self::ControllerWarning(warning) => {
                ActivePhoneAlarmCondition::ControllerWarning(warning)
            }
            Self::ControllerStop(reason) => ActivePhoneAlarmCondition::ControllerStop(reason),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivePhoneAlarmCondition {
    PwmDuty,
    ControllerWarning(RideWarning),
    ControllerStop(RideStopReason),
}

/// Whether native code should deliver a phone-generated alarm now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneAlarmDecision {
    /// No delivery is due.
    Silent,
    /// Deliver the normalized event through the enabled native channels.
    Deliver(PhoneAlarmEvent),
}

/// User-selected phone alarm policy, independent of wheel firmware alarms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneAlarmPolicy {
    enabled: bool,
    pwm_duty_threshold: PwmDutyAlarmThreshold,
    repeat_after: Duration,
}

impl PhoneAlarmPolicy {
    /// Creates a disabled policy. Disabled policies never deliver.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            pwm_duty_threshold: PwmDutyAlarmThreshold(80),
            repeat_after: Duration::from_seconds(10),
        }
    }

    /// Creates an enabled policy with an explicit PWM duty basis and repeat interval.
    #[must_use]
    pub const fn enabled(
        pwm_duty_threshold: PwmDutyAlarmThreshold,
        repeat_after: Duration,
    ) -> Self {
        Self {
            enabled: true,
            pwm_duty_threshold,
            repeat_after,
        }
    }
}

/// Stateful transition and repeat gate for phone-generated ride alarms.
#[derive(Debug, Default)]
pub struct PhoneAlarmEvaluator {
    active: Option<ActivePhoneAlarmCondition>,
    delivered_at: Option<MonotonicTimestamp>,
}

impl PhoneAlarmEvaluator {
    /// Evaluates one observation and returns a bounded native-delivery decision.
    pub fn evaluate(
        &mut self,
        policy: PhoneAlarmPolicy,
        observation: PhoneAlarmObservation,
        now: MonotonicTimestamp,
    ) -> PhoneAlarmDecision {
        if !policy.enabled {
            self.clear();
            return PhoneAlarmDecision::Silent;
        }

        let next = Self::event(policy, observation);
        let Some(event) = next else {
            if self.should_rearm(policy, observation) {
                self.clear();
            }
            return PhoneAlarmDecision::Silent;
        };

        let condition = event.condition();
        let is_transition = self.active != Some(condition);
        let repeat_due = self.delivered_at.is_some_and(|delivered_at| {
            now.saturating_duration_since(delivered_at) >= policy.repeat_after
        });
        if !is_transition && !repeat_due {
            return PhoneAlarmDecision::Silent;
        }

        self.active = Some(condition);
        self.delivered_at = Some(now);
        PhoneAlarmDecision::Deliver(event)
    }

    fn event(
        policy: PhoneAlarmPolicy,
        observation: PhoneAlarmObservation,
    ) -> Option<PhoneAlarmEvent> {
        match observation {
            PhoneAlarmObservation::PwmDuty(duty) => {
                let duty_permille = duty.as_permille().unsigned_abs().min(1_000);
                (duty_permille >= policy.pwm_duty_threshold.permille()).then(|| {
                    let duty_percent = u8::try_from(duty_permille / 10).unwrap_or(100);
                    PhoneAlarmEvent::PwmDuty {
                        duty_percent,
                        headroom_percent: 100 - duty_percent,
                    }
                })
            }
            PhoneAlarmObservation::ControllerWarning(RideWarning::None | RideWarning::Unknown)
            | PhoneAlarmObservation::ControllerStop(RideStopReason::None)
            | PhoneAlarmObservation::Nominal
            | PhoneAlarmObservation::Stale
            | PhoneAlarmObservation::Unavailable => None,
            PhoneAlarmObservation::ControllerWarning(warning) => {
                Some(PhoneAlarmEvent::ControllerWarning(warning))
            }
            PhoneAlarmObservation::ControllerStop(reason) => {
                Some(PhoneAlarmEvent::ControllerStop(reason))
            }
        }
    }

    fn should_rearm(&self, policy: PhoneAlarmPolicy, observation: PhoneAlarmObservation) -> bool {
        match (self.active, observation) {
            (Some(ActivePhoneAlarmCondition::PwmDuty), PhoneAlarmObservation::PwmDuty(duty)) => {
                duty.as_permille().unsigned_abs() <= policy.pwm_duty_threshold.rearm_permille()
            }
            (
                _,
                PhoneAlarmObservation::Stale
                | PhoneAlarmObservation::Unavailable
                | PhoneAlarmObservation::Nominal
                | PhoneAlarmObservation::ControllerWarning(RideWarning::None | RideWarning::Unknown)
                | PhoneAlarmObservation::ControllerStop(RideStopReason::None),
            ) => true,
            _ => false,
        }
    }

    fn clear(&mut self) {
        self.active = None;
        self.delivered_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Duration, DutyCycle, MonotonicTimestamp, RideWarning};

    fn at(milliseconds: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::from_milliseconds(milliseconds)
    }

    #[test]
    fn pwm_threshold_names_duty_and_headroom_as_complements() {
        let threshold = PwmDutyAlarmThreshold::new(80).unwrap();

        assert_eq!(threshold.duty_percent(), 80);
        assert_eq!(threshold.headroom_percent(), 20);
        assert!(PwmDutyAlarmThreshold::new(101).is_err());
    }

    #[test]
    fn evaluator_delivers_fresh_transitions_and_bounded_repeats() {
        let policy = PhoneAlarmPolicy::enabled(
            PwmDutyAlarmThreshold::new(80).unwrap(),
            Duration::from_seconds(10),
        );
        let mut evaluator = PhoneAlarmEvaluator::default();
        let pwm = PhoneAlarmObservation::PwmDuty(DutyCycle::from_permille(850));

        assert_eq!(
            evaluator.evaluate(policy, pwm, at(1_000)),
            PhoneAlarmDecision::Deliver(PhoneAlarmEvent::PwmDuty {
                duty_percent: 85,
                headroom_percent: 15,
            })
        );
        assert_eq!(
            evaluator.evaluate(policy, pwm, at(5_000)),
            PhoneAlarmDecision::Silent
        );
        assert_eq!(
            evaluator.evaluate(
                policy,
                PhoneAlarmObservation::PwmDuty(DutyCycle::from_permille(860)),
                at(6_000),
            ),
            PhoneAlarmDecision::Silent
        );
        assert_eq!(
            evaluator.evaluate(policy, pwm, at(11_000)),
            PhoneAlarmDecision::Deliver(PhoneAlarmEvent::PwmDuty {
                duty_percent: 85,
                headroom_percent: 15,
            })
        );

        assert_eq!(
            evaluator.evaluate(
                policy,
                PhoneAlarmObservation::ControllerWarning(RideWarning::LowVoltage),
                at(11_001),
            ),
            PhoneAlarmDecision::Deliver(PhoneAlarmEvent::ControllerWarning(
                RideWarning::LowVoltage
            ))
        );
    }

    #[test]
    fn evaluator_suppresses_disabled_stale_unknown_and_pwm_chatter() {
        let policy = PhoneAlarmPolicy::enabled(
            PwmDutyAlarmThreshold::new(80).unwrap(),
            Duration::from_seconds(10),
        );
        let mut evaluator = PhoneAlarmEvaluator::default();

        assert_eq!(
            evaluator.evaluate(
                PhoneAlarmPolicy::disabled(),
                PhoneAlarmObservation::PwmDuty(DutyCycle::from_permille(900)),
                at(1),
            ),
            PhoneAlarmDecision::Silent
        );
        assert_eq!(
            evaluator.evaluate(policy, PhoneAlarmObservation::Stale, at(2)),
            PhoneAlarmDecision::Silent
        );
        assert_eq!(
            evaluator.evaluate(
                policy,
                PhoneAlarmObservation::ControllerWarning(RideWarning::Unknown),
                at(3),
            ),
            PhoneAlarmDecision::Silent
        );

        let high = PhoneAlarmObservation::PwmDuty(DutyCycle::from_permille(810));
        assert!(matches!(
            evaluator.evaluate(policy, high, at(4)),
            PhoneAlarmDecision::Deliver(_)
        ));
        assert_eq!(
            evaluator.evaluate(
                policy,
                PhoneAlarmObservation::PwmDuty(DutyCycle::from_permille(790)),
                at(5),
            ),
            PhoneAlarmDecision::Silent
        );
        assert_eq!(
            evaluator.evaluate(policy, high, at(6)),
            PhoneAlarmDecision::Silent
        );
        assert_eq!(
            evaluator.evaluate(
                policy,
                PhoneAlarmObservation::PwmDuty(DutyCycle::from_permille(740)),
                at(7),
            ),
            PhoneAlarmDecision::Silent
        );
        assert!(matches!(
            evaluator.evaluate(policy, high, at(8)),
            PhoneAlarmDecision::Deliver(_)
        ));
    }
}
