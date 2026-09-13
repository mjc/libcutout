use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_core::{
    Duration, DutyCycle, MonotonicTimestamp, PhoneAlarmDecision, PhoneAlarmEvaluator,
    PhoneAlarmEvent, PhoneAlarmObservation, PhoneAlarmPolicy, PwmDutyAlarmThreshold,
    RideStopReason, RideWarning,
};

use crate::{MobileVescRideStopReasonDto, MobileVescRideWarningDto};

/// Per-device phone alarm preferences supplied by a native client.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneAlarmPolicyDto {
    /// Whether phone-generated ride alarms are enabled.
    pub enabled: bool,
    /// Consumed PWM duty percent that triggers the phone alarm.
    pub pwm_duty_percent: u8,
    /// Minimum interval before the same active alarm may repeat.
    pub repeat_after_milliseconds: u64,
}

/// Freshness-qualified ride evidence supplied to the Rust alarm owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneAlarmObservationDto {
    /// No alarm condition is active.
    Nominal,
    /// Signed controller duty in permille.
    PwmDuty {
        /// Signed duty magnitude and direction in permille.
        duty_permille: i16,
    },
    /// Typed warning decoded from a VESC protocol.
    ControllerWarning {
        /// Current controller warning.
        warning: MobileVescRideWarningDto,
    },
    /// Typed reason that a VESC controller stopped balancing.
    ControllerStop {
        /// Current stop reason.
        reason: MobileVescRideStopReasonDto,
    },
    /// The latest ride evidence is stale.
    Stale,
    /// The active protocol does not supply the required evidence.
    Unavailable,
}

/// Normalized alarm payload returned for native delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneAlarmEventDto {
    /// PWM duty reached the configured threshold.
    PwmDuty {
        /// Consumed duty in percent.
        duty_percent: u8,
        /// Complementary unused headroom in percent.
        headroom_percent: u8,
    },
    /// A typed VESC controller warning is active.
    ControllerWarning {
        /// Active warning.
        warning: MobileVescRideWarningDto,
    },
    /// A typed VESC stop reason is active.
    ControllerStop {
        /// Active stop reason.
        reason: MobileVescRideStopReasonDto,
    },
}

/// Result of one Rust-owned phone alarm evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneAlarmEvaluationDto {
    /// No native delivery is due.
    Silent,
    /// Deliver one event through the enabled native channels.
    Deliver {
        /// Normalized event to deliver.
        event: MobilePhoneAlarmEventDto,
    },
    /// The supplied duty threshold exceeded one hundred percent.
    InvalidPwmDutyThreshold {
        /// Rejected duty percent.
        value: u8,
    },
}

/// Thin mobile facade over the stateful Rust phone alarm policy.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobilePhoneAlarmEvaluator {
    inner: Mutex<PhoneAlarmEvaluator>,
}

#[uniffi::export]
impl MobilePhoneAlarmEvaluator {
    /// Creates an evaluator with no active alarm condition.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Evaluates one observation without owning native notification delivery.
    pub fn evaluate(
        &self,
        policy: MobilePhoneAlarmPolicyDto,
        observation: MobilePhoneAlarmObservationDto,
        monotonic_milliseconds: u64,
    ) -> MobilePhoneAlarmEvaluationDto {
        let Ok(threshold) = PwmDutyAlarmThreshold::new(policy.pwm_duty_percent) else {
            return MobilePhoneAlarmEvaluationDto::InvalidPwmDutyThreshold {
                value: policy.pwm_duty_percent,
            };
        };
        let policy = if policy.enabled {
            PhoneAlarmPolicy::enabled(
                threshold,
                Duration::from_milliseconds(policy.repeat_after_milliseconds),
            )
        } else {
            PhoneAlarmPolicy::disabled()
        };
        self.lock_inner()
            .evaluate(
                policy,
                observation.into(),
                MonotonicTimestamp::from_milliseconds(monotonic_milliseconds),
            )
            .into()
    }
}

impl MobilePhoneAlarmEvaluator {
    fn lock_inner(&self) -> MutexGuard<'_, PhoneAlarmEvaluator> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl From<MobilePhoneAlarmObservationDto> for PhoneAlarmObservation {
    fn from(observation: MobilePhoneAlarmObservationDto) -> Self {
        match observation {
            MobilePhoneAlarmObservationDto::Nominal => Self::Nominal,
            MobilePhoneAlarmObservationDto::PwmDuty { duty_permille } => {
                Self::PwmDuty(DutyCycle::from_permille(duty_permille))
            }
            MobilePhoneAlarmObservationDto::ControllerWarning { warning } => {
                Self::ControllerWarning(warning.into())
            }
            MobilePhoneAlarmObservationDto::ControllerStop { reason } => {
                Self::ControllerStop(reason.into())
            }
            MobilePhoneAlarmObservationDto::Stale => Self::Stale,
            MobilePhoneAlarmObservationDto::Unavailable => Self::Unavailable,
        }
    }
}

impl From<PhoneAlarmDecision> for MobilePhoneAlarmEvaluationDto {
    fn from(decision: PhoneAlarmDecision) -> Self {
        match decision {
            PhoneAlarmDecision::Silent => Self::Silent,
            PhoneAlarmDecision::Deliver(event) => Self::Deliver {
                event: event.into(),
            },
        }
    }
}

impl From<PhoneAlarmEvent> for MobilePhoneAlarmEventDto {
    fn from(event: PhoneAlarmEvent) -> Self {
        match event {
            PhoneAlarmEvent::PwmDuty {
                duty_percent,
                headroom_percent,
            } => Self::PwmDuty {
                duty_percent,
                headroom_percent,
            },
            PhoneAlarmEvent::ControllerWarning(warning) => Self::ControllerWarning {
                warning: warning.into(),
            },
            PhoneAlarmEvent::ControllerStop(reason) => Self::ControllerStop {
                reason: reason.into(),
            },
        }
    }
}

impl From<MobileVescRideWarningDto> for RideWarning {
    fn from(warning: MobileVescRideWarningDto) -> Self {
        match warning {
            MobileVescRideWarningDto::None => Self::None,
            MobileVescRideWarningDto::LowVoltage => Self::LowVoltage,
            MobileVescRideWarningDto::HighVoltage => Self::HighVoltage,
            MobileVescRideWarningDto::MosfetTemperature => Self::MosfetTemperature,
            MobileVescRideWarningDto::MotorTemperature => Self::MotorTemperature,
            MobileVescRideWarningDto::Current => Self::Current,
            MobileVescRideWarningDto::DutyPushback => Self::DutyPushback,
            MobileVescRideWarningDto::SpeedPushback => Self::SpeedPushback,
            MobileVescRideWarningDto::TemperaturePushback => Self::TemperaturePushback,
            MobileVescRideWarningDto::Wheelslip => Self::Wheelslip,
            MobileVescRideWarningDto::Sensors => Self::Sensors,
            MobileVescRideWarningDto::LowBattery => Self::LowBattery,
            MobileVescRideWarningDto::Error => Self::Error,
            MobileVescRideWarningDto::BmsConnection => Self::BmsConnection,
            MobileVescRideWarningDto::Unknown => Self::Unknown,
        }
    }
}

impl From<RideWarning> for MobileVescRideWarningDto {
    fn from(warning: RideWarning) -> Self {
        match warning {
            RideWarning::None => Self::None,
            RideWarning::LowVoltage => Self::LowVoltage,
            RideWarning::HighVoltage => Self::HighVoltage,
            RideWarning::MosfetTemperature => Self::MosfetTemperature,
            RideWarning::MotorTemperature => Self::MotorTemperature,
            RideWarning::Current => Self::Current,
            RideWarning::DutyPushback => Self::DutyPushback,
            RideWarning::SpeedPushback => Self::SpeedPushback,
            RideWarning::TemperaturePushback => Self::TemperaturePushback,
            RideWarning::Wheelslip => Self::Wheelslip,
            RideWarning::Sensors => Self::Sensors,
            RideWarning::LowBattery => Self::LowBattery,
            RideWarning::Error => Self::Error,
            RideWarning::BmsConnection => Self::BmsConnection,
            RideWarning::Unknown => Self::Unknown,
        }
    }
}

impl From<MobileVescRideStopReasonDto> for RideStopReason {
    fn from(reason: MobileVescRideStopReasonDto) -> Self {
        match reason {
            MobileVescRideStopReasonDto::None => Self::None,
            MobileVescRideStopReasonDto::Pitch => Self::Pitch,
            MobileVescRideStopReasonDto::Roll => Self::Roll,
            MobileVescRideStopReasonDto::SwitchHalf => Self::SwitchHalf,
            MobileVescRideStopReasonDto::SwitchFull => Self::SwitchFull,
            MobileVescRideStopReasonDto::Reverse => Self::Reverse,
            MobileVescRideStopReasonDto::QuickStop => Self::QuickStop,
        }
    }
}

impl From<RideStopReason> for MobileVescRideStopReasonDto {
    fn from(reason: RideStopReason) -> Self {
        match reason {
            RideStopReason::None => Self::None,
            RideStopReason::Pitch => Self::Pitch,
            RideStopReason::Roll => Self::Roll,
            RideStopReason::SwitchHalf => Self::SwitchHalf,
            RideStopReason::SwitchFull => Self::SwitchFull,
            RideStopReason::Reverse => Self::Reverse,
            RideStopReason::QuickStop => Self::QuickStop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_facade_preserves_pwm_duty_and_headroom() {
        let evaluator = MobilePhoneAlarmEvaluator::new();
        let policy = MobilePhoneAlarmPolicyDto {
            enabled: true,
            pwm_duty_percent: 80,
            repeat_after_milliseconds: 10_000,
        };

        assert_eq!(
            evaluator.evaluate(
                policy,
                MobilePhoneAlarmObservationDto::PwmDuty { duty_permille: 850 },
                1_000,
            ),
            MobilePhoneAlarmEvaluationDto::Deliver {
                event: MobilePhoneAlarmEventDto::PwmDuty {
                    duty_percent: 85,
                    headroom_percent: 15,
                },
            }
        );
        assert_eq!(
            evaluator.evaluate(
                policy,
                MobilePhoneAlarmObservationDto::PwmDuty { duty_permille: 860 },
                2_000,
            ),
            MobilePhoneAlarmEvaluationDto::Silent
        );
    }

    #[test]
    fn facade_rejects_invalid_thresholds_and_preserves_typed_warnings() {
        let evaluator = MobilePhoneAlarmEvaluator::new();
        let invalid = MobilePhoneAlarmPolicyDto {
            enabled: true,
            pwm_duty_percent: 101,
            repeat_after_milliseconds: 10_000,
        };
        assert_eq!(
            evaluator.evaluate(invalid, MobilePhoneAlarmObservationDto::Nominal, 1),
            MobilePhoneAlarmEvaluationDto::InvalidPwmDutyThreshold { value: 101 }
        );

        let policy = MobilePhoneAlarmPolicyDto {
            pwm_duty_percent: 80,
            ..invalid
        };
        assert_eq!(
            evaluator.evaluate(
                policy,
                MobilePhoneAlarmObservationDto::ControllerWarning {
                    warning: MobileVescRideWarningDto::MotorTemperature,
                },
                2,
            ),
            MobilePhoneAlarmEvaluationDto::Deliver {
                event: MobilePhoneAlarmEventDto::ControllerWarning {
                    warning: MobileVescRideWarningDto::MotorTemperature,
                },
            }
        );
    }
}
