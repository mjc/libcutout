//! Portable phone-alarm admission and effect coordination.

use cutout_core::{
    DutyCycle, MonotonicTimestamp, PhoneAlarmActions, PhoneAlarmDeliveryRequest,
    PhoneAlarmEvidence, PhoneAlarmManager, PhoneAlarmManagerError, RIDE_SESSION_STALE_AFTER,
    RideStopReason, RideWarning,
};

/// One telemetry observation supplied by the session adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneAlarmObservation {
    /// The session input reports a transport link-down event.
    pub link_down: bool,
    /// Current Rust-owned monotonic time.
    pub now: MonotonicTimestamp,
    /// Monotonic time of the telemetry snapshot, when available.
    pub observed_at: Option<MonotonicTimestamp>,
    /// Current PWM duty observation.
    pub pwm_duty: Option<DutyCycle>,
    /// Current controller warning observation.
    pub controller_warning: Option<RideWarning>,
    /// Current controller stop-reason observation.
    pub controller_stop: Option<RideStopReason>,
}

/// Native notification capability observed for the current delivery pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PhoneAlarmDeliveryCapability {
    /// Whether the native platform can schedule a notification.
    pub can_schedule: bool,
    /// Whether the native platform should request sound for this pass.
    pub plays_sound: bool,
}

/// Native effects selected by one alarm evaluation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhoneAlarmEffects {
    /// Reserved alarm requests native code should schedule.
    pub schedule: Vec<PhoneAlarmScheduledDelivery>,
    /// Previously reserved requests native code should cancel.
    pub cancel_request_ids: Vec<u64>,
}

/// One reserved request paired with the native sound capability at admission time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneAlarmScheduledDelivery {
    /// Core-owned delivery request.
    pub request: PhoneAlarmDeliveryRequest,
    /// Whether native scheduling should request sound for this delivery.
    pub plays_sound: bool,
}

/// Queue of not-yet-delivered effects accumulated across session transitions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhoneAlarmEffectQueue {
    schedule: Vec<PhoneAlarmScheduledDelivery>,
    cancel_request_ids: Vec<u64>,
}

impl PhoneAlarmEffectQueue {
    /// Merges effects, removing queued schedules invalidated by a later transition.
    pub fn enqueue(&mut self, mut effects: PhoneAlarmEffects) {
        self.discard_queued_schedules(&effects.cancel_request_ids);
        self.schedule.append(&mut effects.schedule);
        for request_id in effects.cancel_request_ids {
            if !self.cancel_request_ids.contains(&request_id) {
                self.cancel_request_ids.push(request_id);
            }
        }
    }

    /// Drains accumulated effects in their established order.
    #[must_use]
    pub fn drain(&mut self) -> PhoneAlarmEffects {
        let schedule = std::mem::take(&mut self.schedule);
        let cancel_request_ids = std::mem::take(&mut self.cancel_request_ids);
        PhoneAlarmEffects {
            schedule,
            cancel_request_ids,
        }
    }

    /// Removes queued schedules invalidated by their request identities.
    pub fn discard_queued_schedules(&mut self, request_ids: &[u64]) {
        self.schedule
            .retain(|delivery| !request_ids.contains(&delivery.request.id()));
    }
}

/// Builds freshness-qualified evidence using the session's existing stale-age policy.
#[must_use]
pub fn evidence_for_observation(
    observation: PhoneAlarmObservation,
    include_controller_conditions: bool,
) -> PhoneAlarmEvidence {
    if observation.link_down {
        return PhoneAlarmEvidence::unavailable();
    }
    let Some(observed_at) = observation.observed_at else {
        return PhoneAlarmEvidence::unavailable();
    };
    if observation.now.saturating_duration_since(observed_at) > RIDE_SESSION_STALE_AFTER {
        return PhoneAlarmEvidence::stale();
    }
    PhoneAlarmEvidence::fresh(
        observation.pwm_duty,
        include_controller_conditions
            .then_some(observation.controller_warning)
            .flatten(),
        include_controller_conditions
            .then_some(observation.controller_stop)
            .flatten(),
    )
}

/// Evaluates one observation and converts every core decision into native effects.
pub fn evaluate(
    manager: &mut PhoneAlarmManager,
    capability: PhoneAlarmDeliveryCapability,
    observation: PhoneAlarmObservation,
    include_controller_conditions: bool,
) -> PhoneAlarmEffects {
    let actions = if capability.can_schedule {
        manager.evaluate(
            evidence_for_observation(observation, include_controller_conditions),
            observation.now,
        )
    } else {
        PhoneAlarmActions::default()
    };
    let (schedule, mut cancel_request_ids) = actions.into_parts();
    if !capability.can_schedule {
        cancel_request_ids.extend(manager.invalidate_deliveries());
    }
    PhoneAlarmEffects {
        schedule: schedule
            .into_iter()
            .map(|request| PhoneAlarmScheduledDelivery {
                request,
                plays_sound: capability.plays_sound,
            })
            .collect(),
        cancel_request_ids,
    }
}

/// Activates a device while keeping the core manager's validation and bounded store.
///
/// # Errors
///
/// Returns a core validation or capacity error when persisted preferences cannot be restored or
/// the requested device identity cannot be activated.
pub fn activate_device(
    manager: &mut PhoneAlarmManager,
    identity: String,
    persisted_preferences: Option<cutout_core::PhoneAlarmPreferences>,
) -> Result<cutout_core::PhoneAlarmPreferences, PhoneAlarmManagerError> {
    if let Some(preferences) = persisted_preferences {
        manager.restore_preferences(identity.clone(), preferences)?;
    }
    manager.activate_device(identity)
}

/// Replaces preferences only while the requested device remains active.
///
/// # Errors
///
/// Returns a core validation or capacity error when the device is not active or the preferences
/// cannot be stored.
pub fn replace_active_preferences(
    manager: &mut PhoneAlarmManager,
    identity: &str,
    preferences: cutout_core::PhoneAlarmPreferences,
) -> Result<(), PhoneAlarmManagerError> {
    manager.active_preferences_for(identity)?;
    manager.restore_preferences(identity.to_owned(), preferences)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_down_and_missing_time_are_unavailable_and_old_snapshot_is_stale() {
        let now = MonotonicTimestamp::new(20_000);
        let input = PhoneAlarmObservation {
            link_down: true,
            now,
            observed_at: Some(now),
            pwm_duty: None,
            controller_warning: None,
            controller_stop: None,
        };
        assert_eq!(
            evidence_for_observation(input, true),
            PhoneAlarmEvidence::unavailable()
        );
        let without_timestamp = PhoneAlarmObservation {
            link_down: false,
            observed_at: None,
            ..input
        };
        assert_eq!(
            evidence_for_observation(without_timestamp, true),
            PhoneAlarmEvidence::unavailable()
        );
        let stale = PhoneAlarmObservation {
            link_down: false,
            observed_at: Some(MonotonicTimestamp::new(
                20_000 - RIDE_SESSION_STALE_AFTER.as_milliseconds() - 1,
            )),
            ..input
        };
        assert_eq!(
            evidence_for_observation(stale, true),
            PhoneAlarmEvidence::stale()
        );
    }

    #[test]
    fn controller_conditions_can_be_excluded_without_hiding_pwm_evidence() {
        let evidence = PhoneAlarmObservation {
            link_down: false,
            now: MonotonicTimestamp::new(1_000),
            observed_at: Some(MonotonicTimestamp::new(1_000)),
            pwm_duty: Some(DutyCycle::from_permille(900)),
            controller_warning: Some(RideWarning::LowVoltage),
            controller_stop: Some(RideStopReason::Pitch),
        };
        let selected = evidence_for_observation(evidence, false);
        assert_eq!(
            selected,
            PhoneAlarmEvidence::fresh(Some(DutyCycle::from_permille(900)), None, None)
        );
    }

    #[test]
    fn later_cancellation_removes_queued_delivery_and_is_reported_once() {
        let now = MonotonicTimestamp::new(1_000);
        let mut manager = PhoneAlarmManager::default();
        activate_device(&mut manager, "wheel-a".into(), None).unwrap();
        manager.set_enabled(true).unwrap();
        let observation = PhoneAlarmObservation {
            link_down: false,
            now,
            observed_at: Some(now),
            pwm_duty: Some(DutyCycle::from_permille(900)),
            controller_warning: None,
            controller_stop: None,
        };
        let mut queue = PhoneAlarmEffectQueue::default();
        let due = evaluate(
            &mut manager,
            PhoneAlarmDeliveryCapability {
                can_schedule: true,
                plays_sound: true,
            },
            observation,
            true,
        );
        let id = due.schedule[0].request.id();
        queue.enqueue(due);
        let cancelled = evaluate(
            &mut manager,
            PhoneAlarmDeliveryCapability {
                can_schedule: false,
                plays_sound: false,
            },
            observation,
            true,
        );
        queue.enqueue(cancelled);
        let drained = queue.drain();
        assert_eq!(drained.schedule, Vec::<PhoneAlarmScheduledDelivery>::new());
        assert_eq!(drained.cancel_request_ids, vec![id]);
        assert_eq!(queue.drain().cancel_request_ids, Vec::<u64>::new());
    }

    #[test]
    fn activation_restores_persisted_preferences_before_selecting_the_device() {
        let mut manager = PhoneAlarmManager::default();
        let persisted = cutout_core::PhoneAlarmPreferences::new(true, 91).unwrap();
        let activated = activate_device(&mut manager, "wheel-a".into(), Some(persisted)).unwrap();
        assert_eq!(activated, persisted);
        assert_eq!(manager.active_identity(), Some("wheel-a"));
    }
}
