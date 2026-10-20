use cutout_application::alarms::{
    PhoneAlarmDeliveryCapability, PhoneAlarmEffectQueue, PhoneAlarmObservation, evaluate,
};
use cutout_core::{DutyCycle, MonotonicTimestamp, PhoneAlarmManager};

#[test]
fn rust_client_evaluates_alarm_evidence_and_consumes_cancellation_effects() {
    let now = MonotonicTimestamp::new(1_000);
    let mut manager = PhoneAlarmManager::default();
    manager.activate_device("wheel-a".into()).unwrap();
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
    let delivery = evaluate(
        &mut manager,
        PhoneAlarmDeliveryCapability {
            can_schedule: true,
            plays_sound: true,
        },
        observation,
        true,
    );
    assert_eq!(delivery.schedule.len(), 1);
    let request_id = delivery.schedule[0].request.id();
    queue.enqueue(delivery);

    let cancelled = evaluate(
        &mut manager,
        PhoneAlarmDeliveryCapability::default(),
        observation,
        true,
    );
    queue.enqueue(cancelled);
    let result = queue.drain();
    assert_eq!(result.schedule, Vec::new());
    assert_eq!(result.cancel_request_ids, vec![request_id]);
}
