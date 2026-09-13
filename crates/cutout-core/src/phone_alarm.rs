use thiserror::Error;

use crate::{Duration, DutyCycle, MonotonicTimestamp, RideStopReason, RideWarning};

const PWM_REARM_HYSTERESIS_PERCENT: u8 = 5;
const FAILED_DELIVERY_RETRY_AFTER: Duration = Duration::from_seconds(1);

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

/// Freshness of the complete evidence set considered by phone alarms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneAlarmEvidenceFreshness {
    /// Every supplied value is current enough for an alarm decision.
    Fresh,
    /// The most recent ride evidence is too old for an alarm decision.
    Stale,
    /// The active session has no usable ride evidence.
    Unavailable,
}

/// One freshness-qualified ride state containing every independent alarm channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneAlarmEvidence {
    freshness: PhoneAlarmEvidenceFreshness,
    pwm_duty: Option<DutyCycle>,
    controller_warning: Option<RideWarning>,
    controller_stop: Option<RideStopReason>,
}

impl PhoneAlarmEvidence {
    /// Creates a fresh evidence set. Independent simultaneous conditions remain present.
    #[must_use]
    pub const fn fresh(
        pwm_duty: Option<DutyCycle>,
        controller_warning: Option<RideWarning>,
        controller_stop: Option<RideStopReason>,
    ) -> Self {
        Self {
            freshness: PhoneAlarmEvidenceFreshness::Fresh,
            pwm_duty,
            controller_warning,
            controller_stop,
        }
    }

    /// Creates explicitly stale evidence with no alarm-eligible values.
    #[must_use]
    pub const fn stale() -> Self {
        Self {
            freshness: PhoneAlarmEvidenceFreshness::Stale,
            pwm_duty: None,
            controller_warning: None,
            controller_stop: None,
        }
    }

    /// Creates explicitly unavailable evidence with no alarm-eligible values.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            freshness: PhoneAlarmEvidenceFreshness::Unavailable,
            pwm_duty: None,
            controller_warning: None,
            controller_stop: None,
        }
    }
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

/// One reserved delivery whose success or failure must be acknowledged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneAlarmDeliveryRequest {
    id: u64,
    event: PhoneAlarmEvent,
}

impl PhoneAlarmDeliveryRequest {
    /// Returns the evaluator-owned request identity.
    #[must_use]
    pub const fn id(self) -> u64 {
        self.id
    }

    /// Returns the native alarm payload reserved by this request.
    #[must_use]
    pub const fn event(self) -> PhoneAlarmEvent {
        self.event
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivePhoneAlarmCondition {
    PwmDuty,
    ControllerWarning(RideWarning),
    ControllerStop(RideStopReason),
}

#[derive(Debug, Default)]
struct PhoneAlarmChannelState {
    active: Option<ActivePhoneAlarmCondition>,
    delivered_at: Option<MonotonicTimestamp>,
    failed_at: Option<MonotonicTimestamp>,
    pending_request_id: Option<u64>,
}

impl PhoneAlarmChannelState {
    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Stateful transition, acknowledgement, and repeat gate for phone-generated alarms.
#[derive(Debug)]
pub struct PhoneAlarmEvaluator {
    pwm: PhoneAlarmChannelState,
    warning: PhoneAlarmChannelState,
    stop: PhoneAlarmChannelState,
    next_request_id: u64,
}

impl Default for PhoneAlarmEvaluator {
    fn default() -> Self {
        Self {
            pwm: PhoneAlarmChannelState::default(),
            warning: PhoneAlarmChannelState::default(),
            stop: PhoneAlarmChannelState::default(),
            next_request_id: 1,
        }
    }
}

impl PhoneAlarmEvaluator {
    /// Reserves every independent alarm delivery due for one complete ride state.
    ///
    /// Each returned request remains in flight until [`Self::complete_delivery`] is called.
    pub fn evaluate(
        &mut self,
        policy: PhoneAlarmPolicy,
        evidence: PhoneAlarmEvidence,
        now: MonotonicTimestamp,
    ) -> Vec<PhoneAlarmDeliveryRequest> {
        if !policy.enabled || evidence.freshness != PhoneAlarmEvidenceFreshness::Fresh {
            self.clear();
            return Vec::new();
        }

        let mut requests = Vec::with_capacity(3);
        let pwm_event = evidence
            .pwm_duty
            .and_then(|duty| Self::pwm_event(policy.pwm_duty_threshold, duty));
        Self::update_pwm_rearm(
            &mut self.pwm,
            policy.pwm_duty_threshold,
            evidence.pwm_duty,
            pwm_event,
        );
        Self::reserve_if_due(
            &mut self.pwm,
            pwm_event,
            policy.repeat_after,
            now,
            &mut self.next_request_id,
            &mut requests,
        );

        let warning_event = evidence
            .controller_warning
            .and_then(|warning| match warning {
                RideWarning::None | RideWarning::Unknown => None,
                warning => Some(PhoneAlarmEvent::ControllerWarning(warning)),
            });
        Self::clear_if_absent(&mut self.warning, warning_event);
        Self::reserve_if_due(
            &mut self.warning,
            warning_event,
            policy.repeat_after,
            now,
            &mut self.next_request_id,
            &mut requests,
        );

        let stop_event = evidence.controller_stop.and_then(|reason| match reason {
            RideStopReason::None => None,
            reason => Some(PhoneAlarmEvent::ControllerStop(reason)),
        });
        Self::clear_if_absent(&mut self.stop, stop_event);
        Self::reserve_if_due(
            &mut self.stop,
            stop_event,
            policy.repeat_after,
            now,
            &mut self.next_request_id,
            &mut requests,
        );
        requests
    }

    /// Completes one reserved native delivery.
    ///
    /// Returns `false` when the request was invalidated by newer evidence or policy state.
    pub fn complete_delivery(
        &mut self,
        request_id: u64,
        delivered: bool,
        now: MonotonicTimestamp,
    ) -> bool {
        for state in [&mut self.pwm, &mut self.warning, &mut self.stop] {
            if state.pending_request_id == Some(request_id) {
                state.pending_request_id = None;
                if delivered {
                    state.delivered_at = Some(now);
                    state.failed_at = None;
                } else {
                    state.failed_at = Some(now);
                }
                return true;
            }
        }
        false
    }

    fn reserve_if_due(
        state: &mut PhoneAlarmChannelState,
        event: Option<PhoneAlarmEvent>,
        repeat_after: Duration,
        now: MonotonicTimestamp,
        next_request_id: &mut u64,
        requests: &mut Vec<PhoneAlarmDeliveryRequest>,
    ) {
        let Some(event) = event else { return };
        let condition = event.condition();
        if state.active != Some(condition) {
            state.active = Some(condition);
            state.delivered_at = None;
            state.failed_at = None;
            state.pending_request_id = None;
        }
        if state.pending_request_id.is_some() {
            return;
        }
        let repeat_due = state
            .delivered_at
            .is_none_or(|delivered_at| now.saturating_duration_since(delivered_at) >= repeat_after);
        let retry_due = state.failed_at.is_none_or(|failed_at| {
            now.saturating_duration_since(failed_at) >= FAILED_DELIVERY_RETRY_AFTER
        });
        if !repeat_due || !retry_due {
            return;
        }

        let id = *next_request_id;
        *next_request_id = next_request_id.wrapping_add(1).max(1);
        state.pending_request_id = Some(id);
        requests.push(PhoneAlarmDeliveryRequest { id, event });
    }

    fn pwm_event(threshold: PwmDutyAlarmThreshold, duty: DutyCycle) -> Option<PhoneAlarmEvent> {
        let duty_permille = duty.as_permille().unsigned_abs().min(1_000);
        (duty_permille >= threshold.permille()).then(|| {
            let duty_percent = u8::try_from(duty_permille / 10).unwrap_or(100);
            PhoneAlarmEvent::PwmDuty {
                duty_percent,
                headroom_percent: 100 - duty_percent,
            }
        })
    }

    fn update_pwm_rearm(
        state: &mut PhoneAlarmChannelState,
        threshold: PwmDutyAlarmThreshold,
        duty: Option<DutyCycle>,
        event: Option<PhoneAlarmEvent>,
    ) {
        if event.is_some() {
            return;
        }
        state.pending_request_id = None;
        let should_clear =
            duty.is_none_or(|duty| duty.as_permille().unsigned_abs() <= threshold.rearm_permille());
        if should_clear {
            state.clear();
        }
    }

    fn clear_if_absent(state: &mut PhoneAlarmChannelState, event: Option<PhoneAlarmEvent>) {
        if event.is_none() {
            state.clear();
        }
    }

    fn clear(&mut self) {
        self.pwm.clear();
        self.warning.clear();
        self.stop.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(milliseconds: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::from_milliseconds(milliseconds)
    }

    fn policy() -> PhoneAlarmPolicy {
        PhoneAlarmPolicy::enabled(
            PwmDutyAlarmThreshold::new(80).unwrap(),
            Duration::from_seconds(10),
        )
    }

    fn pwm_evidence(permille: i16) -> PhoneAlarmEvidence {
        PhoneAlarmEvidence::fresh(Some(DutyCycle::from_permille(permille)), None, None)
    }

    #[test]
    fn pwm_threshold_names_duty_and_headroom_as_complements() {
        let threshold = PwmDutyAlarmThreshold::new(80).unwrap();

        assert_eq!(threshold.duty_percent(), 80);
        assert_eq!(threshold.headroom_percent(), 20);
        assert!(PwmDutyAlarmThreshold::new(101).is_err());
    }

    #[test]
    fn successful_delivery_starts_repeat_gate_without_pwm_chatter() {
        let mut evaluator = PhoneAlarmEvaluator::default();
        let first = evaluator.evaluate(policy(), pwm_evidence(850), at(1_000))[0];
        assert!(evaluator.complete_delivery(first.id, true, at(1_100)));

        assert!(
            evaluator
                .evaluate(policy(), pwm_evidence(860), at(2_000))
                .is_empty()
        );
        assert!(
            evaluator
                .evaluate(policy(), pwm_evidence(790), at(3_000))
                .is_empty()
        );
        assert!(
            evaluator
                .evaluate(policy(), pwm_evidence(810), at(4_000))
                .is_empty()
        );
        assert_eq!(
            evaluator
                .evaluate(policy(), pwm_evidence(810), at(11_100))
                .len(),
            1
        );
    }

    #[test]
    fn pwm_rearms_only_below_hysteresis() {
        let mut evaluator = PhoneAlarmEvaluator::default();
        let first = evaluator.evaluate(policy(), pwm_evidence(810), at(1))[0];
        assert!(evaluator.complete_delivery(first.id, true, at(2)));
        assert!(
            evaluator
                .evaluate(policy(), pwm_evidence(740), at(3))
                .is_empty()
        );
        assert_eq!(
            evaluator.evaluate(policy(), pwm_evidence(810), at(4)).len(),
            1
        );
    }

    #[test]
    fn disabled_stale_and_unavailable_evidence_invalidate_requests() {
        let mut evaluator = PhoneAlarmEvaluator::default();
        let request = evaluator.evaluate(policy(), pwm_evidence(900), at(1))[0];

        assert!(
            evaluator
                .evaluate(policy(), PhoneAlarmEvidence::stale(), at(2))
                .is_empty()
        );
        assert!(!evaluator.complete_delivery(request.id, true, at(3)));
        assert!(
            evaluator
                .evaluate(PhoneAlarmPolicy::disabled(), pwm_evidence(900), at(4))
                .is_empty()
        );
        assert!(
            evaluator
                .evaluate(policy(), PhoneAlarmEvidence::unavailable(), at(5))
                .is_empty()
        );
    }

    #[test]
    fn simultaneous_conditions_are_independent_delivery_requests() {
        let mut evaluator = PhoneAlarmEvaluator::default();
        let evidence = PhoneAlarmEvidence::fresh(
            Some(DutyCycle::from_permille(850)),
            Some(RideWarning::MotorTemperature),
            Some(RideStopReason::Pitch),
        );

        let requests = evaluator.evaluate(policy(), evidence, at(1_000));

        assert_eq!(requests.len(), 3);
        assert!(requests.iter().any(|request| matches!(
            request.event,
            PhoneAlarmEvent::PwmDuty {
                duty_percent: 85,
                headroom_percent: 15
            }
        )));
        assert!(requests.iter().any(|request| {
            request.event == PhoneAlarmEvent::ControllerWarning(RideWarning::MotorTemperature)
        }));
        assert!(requests.iter().any(|request| {
            request.event == PhoneAlarmEvent::ControllerStop(RideStopReason::Pitch)
        }));
    }

    #[test]
    fn delivery_failure_retries_without_committing_repeat_gate() {
        let evidence = pwm_evidence(850);
        let mut evaluator = PhoneAlarmEvaluator::default();
        let first = evaluator.evaluate(policy(), evidence, at(1_000))[0];

        assert!(evaluator.complete_delivery(first.id, false, at(1_100)));
        assert!(evaluator.evaluate(policy(), evidence, at(1_500)).is_empty());
        let retry = evaluator.evaluate(policy(), evidence, at(2_100))[0];
        assert_ne!(retry.id, first.id);
        assert!(evaluator.complete_delivery(retry.id, true, at(2_200)));
        assert!(
            evaluator
                .evaluate(policy(), evidence, at(11_000))
                .is_empty()
        );
        assert_eq!(evaluator.evaluate(policy(), evidence, at(12_200)).len(), 1);
    }
}
