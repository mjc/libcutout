use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_core::{
    Duration, DutyCycle, MonotonicTimestamp, PhoneAlarmDeliveryRequest, PhoneAlarmEvaluator,
    PhoneAlarmEvent, PhoneAlarmEvidence, PhoneAlarmPolicy, PwmDutyAlarmThreshold, RideStopReason,
    RideWarning,
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

/// Freshness of the complete native ride evidence set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneAlarmEvidenceFreshnessDto {
    /// Every supplied value is current enough for an alarm decision.
    Fresh,
    /// The most recent ride evidence is too old for an alarm decision.
    Stale,
    /// The active session has no usable ride evidence.
    Unavailable,
}

/// Complete native ride evidence supplied to the Rust alarm owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneAlarmEvidenceDto {
    /// Freshness shared by every optional field.
    pub freshness: MobilePhoneAlarmEvidenceFreshnessDto,
    /// Signed controller duty in permille when supplied by the protocol.
    pub pwm_duty_permille: Option<i16>,
    /// Typed VESC warning when supplied by the protocol.
    pub controller_warning: Option<MobileVescRideWarningDto>,
    /// Typed VESC stop reason when supplied by the protocol.
    pub controller_stop: Option<MobileVescRideStopReasonDto>,
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

/// One Rust-reserved native delivery request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneAlarmDeliveryRequestDto {
    /// Evaluator-owned request identity used for completion.
    pub id: u64,
    /// Normalized event to deliver.
    pub event: MobilePhoneAlarmEventDto,
}

/// Result of one Rust-owned phone alarm evaluation.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneAlarmEvaluationDto {
    /// Every independent delivery currently due.
    Ready {
        /// Zero to three reserved native deliveries.
        requests: Vec<MobilePhoneAlarmDeliveryRequestDto>,
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

    /// Reserves every independent delivery due for the complete evidence set.
    pub fn evaluate(
        &self,
        policy: MobilePhoneAlarmPolicyDto,
        evidence: MobilePhoneAlarmEvidenceDto,
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
        let requests = self
            .lock_inner()
            .evaluate(
                policy,
                evidence.into(),
                MonotonicTimestamp::from_milliseconds(monotonic_milliseconds),
            )
            .into_iter()
            .map(Into::into)
            .collect();
        MobilePhoneAlarmEvaluationDto::Ready { requests }
    }

    /// Records whether native scheduling succeeded for one reserved request.
    pub fn complete_delivery(
        &self,
        request_id: u64,
        delivered: bool,
        monotonic_milliseconds: u64,
    ) -> bool {
        self.lock_inner().complete_delivery(
            request_id,
            delivered,
            MonotonicTimestamp::from_milliseconds(monotonic_milliseconds),
        )
    }
}

impl MobilePhoneAlarmEvaluator {
    fn lock_inner(&self) -> MutexGuard<'_, PhoneAlarmEvaluator> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl From<MobilePhoneAlarmEvidenceDto> for PhoneAlarmEvidence {
    fn from(evidence: MobilePhoneAlarmEvidenceDto) -> Self {
        match evidence.freshness {
            MobilePhoneAlarmEvidenceFreshnessDto::Fresh => Self::fresh(
                evidence.pwm_duty_permille.map(DutyCycle::from_permille),
                evidence.controller_warning.map(Into::into),
                evidence.controller_stop.map(Into::into),
            ),
            MobilePhoneAlarmEvidenceFreshnessDto::Stale => Self::stale(),
            MobilePhoneAlarmEvidenceFreshnessDto::Unavailable => Self::unavailable(),
        }
    }
}

impl From<PhoneAlarmDeliveryRequest> for MobilePhoneAlarmDeliveryRequestDto {
    fn from(request: PhoneAlarmDeliveryRequest) -> Self {
        Self {
            id: request.id(),
            event: request.event().into(),
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

    fn policy() -> MobilePhoneAlarmPolicyDto {
        MobilePhoneAlarmPolicyDto {
            enabled: true,
            pwm_duty_percent: 80,
            repeat_after_milliseconds: 10_000,
        }
    }

    #[test]
    fn facade_preserves_simultaneous_typed_events_and_completion() {
        let evaluator = MobilePhoneAlarmEvaluator::new();
        let evidence = MobilePhoneAlarmEvidenceDto {
            freshness: MobilePhoneAlarmEvidenceFreshnessDto::Fresh,
            pwm_duty_permille: Some(850),
            controller_warning: Some(MobileVescRideWarningDto::MotorTemperature),
            controller_stop: Some(MobileVescRideStopReasonDto::Pitch),
        };

        let MobilePhoneAlarmEvaluationDto::Ready { requests } =
            evaluator.evaluate(policy(), evidence, 1_000)
        else {
            panic!("valid policy must return requests");
        };
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().any(|request| {
            request.event
                == MobilePhoneAlarmEventDto::PwmDuty {
                    duty_percent: 85,
                    headroom_percent: 15,
                }
        }));
        for request in requests {
            assert!(evaluator.complete_delivery(request.id, true, 1_100));
        }
        assert_eq!(
            evaluator.evaluate(policy(), evidence, 2_000),
            MobilePhoneAlarmEvaluationDto::Ready {
                requests: Vec::new()
            }
        );
    }

    #[test]
    fn facade_rejects_invalid_threshold_and_retries_failed_delivery() {
        let evaluator = MobilePhoneAlarmEvaluator::new();
        let invalid = MobilePhoneAlarmPolicyDto {
            pwm_duty_percent: 101,
            ..policy()
        };
        let evidence = MobilePhoneAlarmEvidenceDto {
            freshness: MobilePhoneAlarmEvidenceFreshnessDto::Fresh,
            pwm_duty_permille: Some(850),
            controller_warning: None,
            controller_stop: None,
        };
        assert_eq!(
            evaluator.evaluate(invalid, evidence, 1),
            MobilePhoneAlarmEvaluationDto::InvalidPwmDutyThreshold { value: 101 }
        );

        let MobilePhoneAlarmEvaluationDto::Ready { requests } =
            evaluator.evaluate(policy(), evidence, 1_000)
        else {
            panic!("valid policy must return requests");
        };
        assert!(evaluator.complete_delivery(requests[0].id, false, 1_100));
        assert_eq!(
            evaluator.evaluate(policy(), evidence, 1_500),
            MobilePhoneAlarmEvaluationDto::Ready {
                requests: Vec::new()
            }
        );
        let MobilePhoneAlarmEvaluationDto::Ready { requests } =
            evaluator.evaluate(policy(), evidence, 2_100)
        else {
            panic!("valid policy must return requests");
        };
        assert_eq!(requests.len(), 1);
    }
}
