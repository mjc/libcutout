use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use crate::{Duration, DutyCycle, MonotonicTimestamp, RideStopReason, RideWarning};

const PWM_REARM_HYSTERESIS_PERCENT: u8 = 5;
const FAILED_DELIVERY_RETRY_AFTER: Duration = Duration::from_seconds(1);
const PHONE_ALARM_REPEAT_AFTER: Duration = Duration::from_seconds(30);
const DEFAULT_PWM_DUTY_PERCENT: u8 = 80;
const MAX_PHONE_ALARM_DEVICES: usize = 256;
const MAX_DEVICE_IDENTITY_BYTES: usize = 1_024;

// Keep native delivery identities distinct when a phone-alarm evaluator is recreated.
static NEXT_PHONE_ALARM_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// A phone-generated PWM alarm threshold expressed as consumed duty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmDutyAlarmThreshold(u8);

impl PwmDutyAlarmThreshold {
    /// Creates a threshold from consumed PWM duty percent.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPwmDutyAlarmThreshold`] unless `duty_percent` is 1 through 100.
    pub const fn new(duty_percent: u8) -> Result<Self, InvalidPwmDutyAlarmThreshold> {
        if duty_percent >= 1 && duty_percent <= 100 {
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

/// A PWM duty threshold outside one through one hundred percent.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("PWM duty alarm threshold {value}% is outside 1% through 100%")]
pub struct InvalidPwmDutyAlarmThreshold {
    value: u8,
}

impl InvalidPwmDutyAlarmThreshold {
    /// Returns the rejected duty percent.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.value
    }
}

/// Persisted phone-generated alarm preferences for one device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneAlarmPreferences {
    enabled: bool,
    pwm_duty_threshold: PwmDutyAlarmThreshold,
}

impl Default for PhoneAlarmPreferences {
    fn default() -> Self {
        Self {
            enabled: false,
            pwm_duty_threshold: PwmDutyAlarmThreshold(DEFAULT_PWM_DUTY_PERCENT),
        }
    }
}

impl PhoneAlarmPreferences {
    /// Creates validated phone alarm preferences.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPwmDutyAlarmThreshold`] unless duty is 1 through 100 percent.
    pub fn new(enabled: bool, duty_percent: u8) -> Result<Self, InvalidPwmDutyAlarmThreshold> {
        Ok(Self {
            enabled,
            pwm_duty_threshold: PwmDutyAlarmThreshold::new(duty_percent)?,
        })
    }

    /// Returns whether this device may produce phone-generated alarms.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Returns the configured consumed PWM duty threshold.
    #[must_use]
    pub const fn duty_percent(self) -> u8 {
        self.pwm_duty_threshold.duty_percent()
    }

    /// Returns the complementary unused PWM headroom threshold.
    #[must_use]
    pub const fn headroom_percent(self) -> u8 {
        self.pwm_duty_threshold.headroom_percent()
    }
}

/// Invalid manager input or persisted phone alarm preferences.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PhoneAlarmManagerError {
    /// An active device is required before reading or changing its preferences.
    #[error("no active device")]
    NoActiveDevice,
    /// Device identities must be non-empty and bounded.
    #[error("invalid device identity")]
    InvalidDeviceIdentity,
    /// The PWM duty threshold was outside one through one hundred percent.
    #[error(transparent)]
    InvalidPwmDutyThreshold(#[from] InvalidPwmDutyAlarmThreshold),
    /// The PWM headroom threshold was outside zero through ninety-nine percent.
    #[error("PWM headroom alarm threshold {0}% is outside 0% through 99%")]
    InvalidPwmHeadroomThreshold(u8),
    /// The preference store exceeded its bounded device count.
    #[error("too many phone alarm preference devices")]
    TooManyDevices,
    /// The expected device is no longer active.
    #[error("active phone alarm device changed")]
    DeviceIdentityChanged,
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

/// Native effects from one Rust-owned alarm transition.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhoneAlarmActions {
    scheduled: Vec<PhoneAlarmDeliveryRequest>,
    cancelled_request_ids: Vec<u64>,
}

impl PhoneAlarmActions {
    /// Returns alarm deliveries that native code should schedule.
    #[must_use]
    pub fn scheduled(&self) -> &[PhoneAlarmDeliveryRequest] {
        &self.scheduled
    }

    /// Returns previously scheduled request identities that native code should cancel.
    #[must_use]
    pub fn cancelled_request_ids(&self) -> &[u64] {
        &self.cancelled_request_ids
    }

    /// Consumes the actions into scheduled deliveries and cancellation identities.
    #[must_use]
    pub fn into_parts(self) -> (Vec<PhoneAlarmDeliveryRequest>, Vec<u64>) {
        (self.scheduled, self.cancelled_request_ids)
    }
}

impl std::ops::Deref for PhoneAlarmActions {
    type Target = [PhoneAlarmDeliveryRequest];

    fn deref(&self) -> &Self::Target {
        &self.scheduled
    }
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
    waiting_for_rearm: bool,
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
}

impl Default for PhoneAlarmEvaluator {
    fn default() -> Self {
        Self {
            pwm: PhoneAlarmChannelState::default(),
            warning: PhoneAlarmChannelState::default(),
            stop: PhoneAlarmChannelState::default(),
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
    ) -> PhoneAlarmActions {
        if !policy.enabled || evidence.freshness != PhoneAlarmEvidenceFreshness::Fresh {
            return PhoneAlarmActions {
                scheduled: Vec::new(),
                cancelled_request_ids: self.clear(),
            };
        }

        let mut requests = Vec::with_capacity(3);
        let mut cancelled_request_ids = Vec::new();
        let pwm_event = evidence
            .pwm_duty
            .and_then(|duty| Self::pwm_event(policy.pwm_duty_threshold, duty));
        Self::update_pwm_rearm(
            &mut self.pwm,
            policy.pwm_duty_threshold,
            evidence.pwm_duty,
            pwm_event,
            &mut cancelled_request_ids,
        );
        Self::reserve_if_due(
            &mut self.pwm,
            pwm_event,
            policy.repeat_after,
            now,
            &mut requests,
            &mut cancelled_request_ids,
        );

        let warning_event = evidence
            .controller_warning
            .and_then(|warning| match warning {
                RideWarning::None | RideWarning::Unknown => None,
                warning => Some(PhoneAlarmEvent::ControllerWarning(warning)),
            });
        Self::clear_if_absent(&mut self.warning, warning_event, &mut cancelled_request_ids);
        Self::reserve_if_due(
            &mut self.warning,
            warning_event,
            policy.repeat_after,
            now,
            &mut requests,
            &mut cancelled_request_ids,
        );

        let stop_event = evidence.controller_stop.and_then(|reason| match reason {
            RideStopReason::None => None,
            reason => Some(PhoneAlarmEvent::ControllerStop(reason)),
        });
        Self::clear_if_absent(&mut self.stop, stop_event, &mut cancelled_request_ids);
        Self::reserve_if_due(
            &mut self.stop,
            stop_event,
            policy.repeat_after,
            now,
            &mut requests,
            &mut cancelled_request_ids,
        );
        PhoneAlarmActions {
            scheduled: requests,
            cancelled_request_ids,
        }
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
        requests: &mut Vec<PhoneAlarmDeliveryRequest>,
        cancelled_request_ids: &mut Vec<u64>,
    ) {
        let Some(event) = event else { return };
        let condition = event.condition();
        if state.active != Some(condition) {
            if let Some(id) = state.pending_request_id.take() {
                cancelled_request_ids.push(id);
            }
            state.active = Some(condition);
            state.delivered_at = None;
            state.failed_at = None;
            state.pending_request_id = None;
        }
        if state.pending_request_id.is_some() {
            return;
        }
        if state.waiting_for_rearm {
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

        let id = NEXT_PHONE_ALARM_REQUEST_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("phone alarm request identity exhausted");
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
        cancelled_request_ids: &mut Vec<u64>,
    ) {
        if event.is_some() {
            return;
        }
        let cancelled_pending = if let Some(id) = state.pending_request_id.take() {
            cancelled_request_ids.push(id);
            true
        } else {
            false
        };
        let should_clear =
            duty.is_none_or(|duty| duty.as_permille().unsigned_abs() <= threshold.rearm_permille());
        if should_clear {
            state.clear();
        } else if cancelled_pending {
            state.waiting_for_rearm = true;
        }
    }

    fn clear_if_absent(
        state: &mut PhoneAlarmChannelState,
        event: Option<PhoneAlarmEvent>,
        cancelled_request_ids: &mut Vec<u64>,
    ) {
        if event.is_none() {
            if let Some(id) = state.pending_request_id {
                cancelled_request_ids.push(id);
            }
            state.clear();
        }
    }

    fn clear(&mut self) -> Vec<u64> {
        let cancelled_request_ids = [&self.pwm, &self.warning, &self.stop]
            .into_iter()
            .filter_map(|state| state.pending_request_id)
            .collect();
        self.pwm.clear();
        self.warning.clear();
        self.stop.clear();
        cancelled_request_ids
    }
}

/// Rust-owned phone alarm preferences and delivery lifecycle for every device.
#[derive(Debug, Default)]
pub struct PhoneAlarmManager {
    active_identity: Option<String>,
    preferences: BTreeMap<String, PhoneAlarmPreferences>,
    evaluator: PhoneAlarmEvaluator,
    cancelled_request_ids: Vec<u64>,
}

impl PhoneAlarmManager {
    /// Makes one platform-scoped device identity active and returns its preferences.
    ///
    /// # Errors
    ///
    /// Returns [`PhoneAlarmManagerError::InvalidDeviceIdentity`] for an empty or oversized
    /// identity, or [`PhoneAlarmManagerError::TooManyDevices`] when the bounded store is full.
    pub fn activate_device(
        &mut self,
        identity: String,
    ) -> Result<PhoneAlarmPreferences, PhoneAlarmManagerError> {
        Self::validate_identity(&identity)?;
        let identity_changed = self.active_identity.as_deref() != Some(identity.as_str());
        if !self.preferences.contains_key(&identity)
            && self.preferences.len() >= MAX_PHONE_ALARM_DEVICES
        {
            return Err(PhoneAlarmManagerError::TooManyDevices);
        }
        let preferences = *self.preferences.entry(identity.clone()).or_default();
        self.active_identity = Some(identity);
        if identity_changed {
            self.invalidate();
        }
        Ok(preferences)
    }

    /// Clears the active session identity and invalidates every in-flight delivery.
    pub fn clear_active_device(&mut self) {
        self.active_identity = None;
        self.invalidate();
    }

    /// Returns the active device's alarm preferences.
    #[must_use]
    pub fn active_preferences(&self) -> Option<PhoneAlarmPreferences> {
        self.active_identity
            .as_ref()
            .and_then(|identity| self.preferences.get(identity))
            .copied()
    }

    /// Returns the active platform-scoped device identity.
    #[must_use]
    pub fn active_identity(&self) -> Option<&str> {
        self.active_identity.as_deref()
    }

    /// Returns preferences only when `identity` is still the active device.
    ///
    /// # Errors
    ///
    /// Returns a typed identity error for invalid input, no active device, or an identity switch.
    pub fn active_preferences_for(
        &self,
        identity: &str,
    ) -> Result<PhoneAlarmPreferences, PhoneAlarmManagerError> {
        Self::validate_identity(identity)?;
        let active_identity = self
            .active_identity()
            .ok_or(PhoneAlarmManagerError::NoActiveDevice)?;
        if active_identity != identity {
            return Err(PhoneAlarmManagerError::DeviceIdentityChanged);
        }
        self.active_preferences()
            .ok_or(PhoneAlarmManagerError::NoActiveDevice)
    }

    /// Enables or disables phone alarms for the active device.
    ///
    /// # Errors
    ///
    /// Returns [`PhoneAlarmManagerError::NoActiveDevice`] without an active identity.
    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), PhoneAlarmManagerError> {
        self.update_active_preferences(|preferences| preferences.enabled = enabled)
    }

    /// Sets the active device's threshold as consumed PWM duty percent.
    ///
    /// # Errors
    ///
    /// Returns [`PhoneAlarmManagerError::NoActiveDevice`] without an active identity, or
    /// [`PhoneAlarmManagerError::InvalidPwmDutyThreshold`] outside 1 through 100 percent.
    pub fn set_duty_percent(&mut self, duty_percent: u8) -> Result<(), PhoneAlarmManagerError> {
        let threshold = PwmDutyAlarmThreshold::new(duty_percent)?;
        self.update_active_preferences(|preferences| {
            preferences.pwm_duty_threshold = threshold;
        })
    }

    /// Sets the active device's threshold as unused PWM headroom percent.
    ///
    /// # Errors
    ///
    /// Returns [`PhoneAlarmManagerError::NoActiveDevice`] without an active identity, or
    /// [`PhoneAlarmManagerError::InvalidPwmHeadroomThreshold`] outside 0 through 99 percent.
    pub fn set_headroom_percent(
        &mut self,
        headroom_percent: u8,
    ) -> Result<(), PhoneAlarmManagerError> {
        let Some(duty_percent) = 100_u8.checked_sub(headroom_percent) else {
            return Err(PhoneAlarmManagerError::InvalidPwmHeadroomThreshold(
                headroom_percent,
            ));
        };
        self.set_duty_percent(duty_percent)
    }

    /// Reserves every independent delivery due for the active device.
    pub fn evaluate(
        &mut self,
        evidence: PhoneAlarmEvidence,
        now: MonotonicTimestamp,
    ) -> PhoneAlarmActions {
        let Some(preferences) = self.active_preferences() else {
            self.invalidate();
            return self.with_pending_cancellations(PhoneAlarmActions::default());
        };
        if !preferences.enabled || evidence.freshness != PhoneAlarmEvidenceFreshness::Fresh {
            self.invalidate();
            return self.with_pending_cancellations(PhoneAlarmActions::default());
        }
        let policy =
            PhoneAlarmPolicy::enabled(preferences.pwm_duty_threshold, PHONE_ALARM_REPEAT_AFTER);
        let actions = self.evaluator.evaluate(policy, evidence, now);
        self.with_pending_cancellations(actions)
    }

    /// Completes one active device delivery reserved by [`Self::evaluate`].
    pub fn complete_delivery(
        &mut self,
        request_id: u64,
        delivered: bool,
        now: MonotonicTimestamp,
    ) -> bool {
        self.active_identity.is_some()
            && self.evaluator.complete_delivery(request_id, delivered, now)
    }

    /// Invalidates all in-flight deliveries while retaining preferences and request sequencing.
    pub fn invalidate_deliveries(&mut self) -> Vec<u64> {
        let mut cancelled = std::mem::take(&mut self.cancelled_request_ids);
        cancelled.extend(self.evaluator.clear());
        cancelled
    }

    /// Drains cancellation identities queued by identity or preference changes.
    pub fn take_cancelled_request_ids(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.cancelled_request_ids)
    }

    /// Restores typed persisted preferences for a device without activating it.
    ///
    /// # Errors
    ///
    /// Returns a typed identity or bounded-store error.
    pub fn restore_preferences(
        &mut self,
        identity: String,
        preferences: PhoneAlarmPreferences,
    ) -> Result<(), PhoneAlarmManagerError> {
        Self::validate_identity(&identity)?;
        if !self.preferences.contains_key(&identity)
            && self.preferences.len() >= MAX_PHONE_ALARM_DEVICES
        {
            return Err(PhoneAlarmManagerError::TooManyDevices);
        }
        let active_changed = self.active_identity.as_deref() == Some(identity.as_str())
            && self.preferences.get(&identity).copied() != Some(preferences);
        self.preferences.insert(identity, preferences);
        if active_changed {
            self.invalidate();
        }
        Ok(())
    }

    fn update_active_preferences(
        &mut self,
        update: impl FnOnce(&mut PhoneAlarmPreferences),
    ) -> Result<(), PhoneAlarmManagerError> {
        let identity = self
            .active_identity
            .as_ref()
            .ok_or(PhoneAlarmManagerError::NoActiveDevice)?;
        let preferences = self
            .preferences
            .get_mut(identity)
            .ok_or(PhoneAlarmManagerError::NoActiveDevice)?;
        let previous = *preferences;
        update(preferences);
        if *preferences != previous {
            self.invalidate();
        }
        Ok(())
    }

    fn validate_identity(identity: &str) -> Result<(), PhoneAlarmManagerError> {
        if identity.trim().is_empty() || identity.len() > MAX_DEVICE_IDENTITY_BYTES {
            Err(PhoneAlarmManagerError::InvalidDeviceIdentity)
        } else {
            Ok(())
        }
    }

    fn invalidate(&mut self) {
        self.cancelled_request_ids.extend(self.evaluator.clear());
    }

    fn with_pending_cancellations(&mut self, mut actions: PhoneAlarmActions) -> PhoneAlarmActions {
        if !self.cancelled_request_ids.is_empty() {
            self.cancelled_request_ids
                .append(&mut actions.cancelled_request_ids);
            actions.cancelled_request_ids = std::mem::take(&mut self.cancelled_request_ids);
        }
        actions
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
        assert!(PwmDutyAlarmThreshold::new(0).is_err());
        assert_eq!(
            PwmDutyAlarmThreshold::new(1).unwrap().headroom_percent(),
            99
        );
        assert_eq!(
            PwmDutyAlarmThreshold::new(100).unwrap().headroom_percent(),
            0
        );
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
    fn pending_pwm_delivery_rearms_only_below_hysteresis() {
        let mut evaluator = PhoneAlarmEvaluator::default();
        let first = evaluator.evaluate(policy(), pwm_evidence(810), at(1));
        assert_eq!(first.len(), 1);

        let cancelled = evaluator.evaluate(policy(), pwm_evidence(790), at(2));
        assert_eq!(cancelled.cancelled_request_ids(), &[first[0].id()]);
        assert!(cancelled.is_empty());
        assert!(
            evaluator
                .evaluate(policy(), pwm_evidence(810), at(3))
                .is_empty()
        );
        assert!(
            evaluator
                .evaluate(policy(), pwm_evidence(740), at(4))
                .is_empty()
        );
        assert_eq!(
            evaluator.evaluate(policy(), pwm_evidence(810), at(5)).len(),
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

    #[test]
    fn manager_owns_per_device_preferences_and_duty_headroom_semantics() {
        let mut manager = PhoneAlarmManager::default();
        manager.activate_device("wheel-a".to_owned()).unwrap();

        assert_eq!(manager.active_preferences().unwrap().duty_percent(), 80);
        assert_eq!(manager.active_preferences().unwrap().headroom_percent(), 20);
        manager.set_enabled(true).unwrap();
        manager.set_headroom_percent(15).unwrap();
        assert_eq!(manager.active_preferences().unwrap().duty_percent(), 85);
        assert_eq!(manager.active_preferences().unwrap().headroom_percent(), 15);

        manager.activate_device("wheel-b".to_owned()).unwrap();
        assert!(!manager.active_preferences().unwrap().enabled());
        assert_eq!(manager.active_preferences().unwrap().duty_percent(), 80);
        assert!(manager.set_duty_percent(0).is_err());
        assert!(manager.set_duty_percent(101).is_err());
        manager.set_headroom_percent(0).unwrap();
        assert_eq!(manager.active_preferences().unwrap().duty_percent(), 100);
        manager.set_headroom_percent(99).unwrap();
        assert_eq!(manager.active_preferences().unwrap().duty_percent(), 1);
        assert!(manager.set_headroom_percent(100).is_err());

        manager.activate_device("wheel-a".to_owned()).unwrap();
        assert!(manager.active_preferences().unwrap().enabled());
        assert_eq!(manager.active_preferences().unwrap().duty_percent(), 85);
    }

    #[test]
    fn manager_evaluates_active_device_with_fixed_repeat_policy() {
        let mut manager = PhoneAlarmManager::default();
        manager.activate_device("wheel-a".to_owned()).unwrap();
        manager.set_enabled(true).unwrap();

        let request = manager.evaluate(pwm_evidence(850), at(1_000)).scheduled()[0];
        assert!(manager.complete_delivery(request.id(), true, at(1_100)));
        assert!(
            manager
                .evaluate(pwm_evidence(850), at(30_999))
                .scheduled()
                .is_empty()
        );
        assert_eq!(
            manager
                .evaluate(pwm_evidence(850), at(31_100))
                .scheduled()
                .len(),
            1
        );
    }

    #[test]
    fn manager_invalidates_in_flight_delivery_on_identity_settings_and_stale_state() {
        let mut manager = PhoneAlarmManager::default();
        manager.activate_device("wheel-a".to_owned()).unwrap();
        manager.set_enabled(true).unwrap();
        let identity_request = manager.evaluate(pwm_evidence(850), at(1)).scheduled()[0];

        manager.activate_device("wheel-b".to_owned()).unwrap();
        assert!(!manager.complete_delivery(identity_request.id(), true, at(2)));
        manager.set_enabled(true).unwrap();
        let identity_actions = manager.evaluate(pwm_evidence(850), at(3));
        assert_eq!(
            identity_actions.cancelled_request_ids(),
            &[identity_request.id()]
        );
        let settings_request = identity_actions.scheduled()[0];
        assert!(settings_request.id() > identity_request.id());
        manager.set_duty_percent(90).unwrap();
        assert!(!manager.complete_delivery(settings_request.id(), true, at(4)));

        manager.set_duty_percent(80).unwrap();
        let settings_actions = manager.evaluate(pwm_evidence(850), at(5));
        assert_eq!(
            settings_actions.cancelled_request_ids(),
            &[settings_request.id()]
        );
        let stale_request = settings_actions.scheduled()[0];
        let stale_actions = manager.evaluate(PhoneAlarmEvidence::stale(), at(6));
        assert!(stale_actions.scheduled().is_empty());
        assert_eq!(stale_actions.cancelled_request_ids(), &[stale_request.id()]);
        assert!(!manager.complete_delivery(stale_request.id(), true, at(7)));
    }

    #[test]
    fn manager_restores_typed_per_device_preferences() {
        let mut manager = PhoneAlarmManager::default();
        manager
            .restore_preferences(
                "wheel-a".to_owned(),
                PhoneAlarmPreferences::new(true, 90).unwrap(),
            )
            .unwrap();

        manager.activate_device("wheel-a".to_owned()).unwrap();
        assert!(manager.active_preferences().unwrap().enabled());
        assert_eq!(manager.active_preferences().unwrap().headroom_percent(), 10);
    }

    #[test]
    fn manager_rejects_invalid_identity() {
        let mut manager = PhoneAlarmManager::default();
        assert!(manager.activate_device(String::new()).is_err());
        assert!(manager.activate_device(" \t".to_owned()).is_err());
        assert!(
            manager
                .restore_preferences(String::new(), PhoneAlarmPreferences::default())
                .is_err()
        );
    }

    #[test]
    fn manager_preserves_nonempty_opaque_identity_bytes() {
        let mut manager = PhoneAlarmManager::default();
        manager.activate_device(" wheel-a ".to_owned()).unwrap();
        manager.set_enabled(true).unwrap();
        manager.activate_device("wheel-a".to_owned()).unwrap();

        assert!(!manager.active_preferences().unwrap().enabled());
    }

    #[test]
    fn manager_rejects_settings_for_a_replaced_identity() {
        let mut manager = PhoneAlarmManager::default();
        manager.activate_device("wheel-a".to_owned()).unwrap();
        assert_eq!(
            manager.active_preferences_for("wheel-a").unwrap(),
            PhoneAlarmPreferences::default()
        );
        manager.activate_device("wheel-b".to_owned()).unwrap();

        assert_eq!(
            manager.active_preferences_for("wheel-a"),
            Err(PhoneAlarmManagerError::DeviceIdentityChanged)
        );
    }
}
