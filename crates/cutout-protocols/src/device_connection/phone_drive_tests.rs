use super::*;
use crate::device_connection::tests::{begin, drivetrain_tick, identify_vesc};
use cutout_core::{PhoneDriveContactId, RawFieldValue, RawTelemetryReadback};

fn at(value: u64) -> MonotonicTimestamp {
    MonotonicTimestamp::new(value)
}

fn active() -> (DeviceConnectionSession, ConnectionAttemptToken, u64) {
    let mut owner = DeviceConnectionSession::default();
    let token = begin(&mut owner, "phone-board");
    assert!(owner.configure_vehicle_kind(&token, VehicleKind::Eskate));
    identify_vesc(&mut owner, &token);
    let _ = drivetrain_tick(&mut owner, &token, 3_000);
    owner.set_phone_control_mode(PhoneControlMode::Buttons);
    let unarmed_generation = owner.phone_drive_input_snapshot().generation.unwrap();
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        unarmed_generation,
        PhoneDriveContacts::Released,
        at(3_000),
        at(3_000),
    );
    owner
        .activate_phone_drive(
            &token,
            PhoneDriveLimits::new(3_000, 3_000).unwrap(),
            at(3_000),
        )
        .unwrap();
    let generation = owner.phone_drive_input_snapshot().generation.unwrap();
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        generation,
        PhoneDriveContacts::Released,
        at(3_000),
        at(3_000),
    );
    (owner, token, generation)
}

fn held(action: PhoneDriveAction) -> PhoneDriveContacts {
    PhoneDriveContacts::Single {
        contact: PhoneDriveContactId::new(1).unwrap(),
        region: Some(action),
    }
}

fn drives(step: &DeviceConnectionStep) -> Vec<(PhoneDriveWrite, Vec<u8>)> {
    step.result
        .outputs
        .iter()
        .filter_map(|output| match output {
            SessionOutput::Transport(TransportAction::Write { bytes, .. }) => bytes
                .phone_drive()
                .map(|write| (write, bytes.as_slice().to_vec())),
            _ => None,
        })
        .collect()
}

fn press(
    owner: &mut DeviceConnectionSession,
    token: &ConnectionAttemptToken,
    generation: u64,
) -> PhoneDriveWrite {
    let step = owner
        .observe_phone_drive_contacts_at(
            token,
            generation,
            held(PhoneDriveAction::Throttle),
            at(3_001),
            at(3_001),
        )
        .unwrap();
    drives(&step)[0].0
}

#[test]
fn phone_drive_platform_cancellation_requires_explicit_reactivation_in_a_fresh_input_epoch() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    owner.cancel_phone_drive_at(PhoneDriveCancelReason::TouchCancelled, at(3_002));
    assert_eq!(
        owner.phone_drive_activation(),
        None,
        "system cancellation must retire the old grant"
    );
    let release = drives(&owner.tick_phone_drive(&token, at(3_002), true).unwrap())[0].0;
    assert_eq!(release.kind, PhoneDriveWriteKind::Release);
    assert!(owner.mark_phone_drive_transport(
        &token,
        release,
        ApplicationQueryTransportStatus::Submitted,
        at(3_002)
    ));
    let current_generation = owner.phone_drive_input_snapshot().generation.unwrap();
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        current_generation,
        PhoneDriveContacts::Released,
        at(3_003),
        at(3_003),
    );
    let fresh_contact = owner
        .observe_phone_drive_contacts_at(
            &token,
            current_generation,
            held(PhoneDriveAction::Throttle),
            at(3_004),
            at(3_004),
        )
        .unwrap();
    assert_eq!(
        drives(&fresh_contact),
        Vec::new(),
        "a new UIKit event alone must not restore drive authority"
    );
    assert_eq!(
        owner.phone_drive_availability(),
        PhoneDriveAvailability::Ready
    );
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        current_generation,
        PhoneDriveContacts::Released,
        at(3_005),
        at(3_005),
    );
    let new_activation = owner
        .activate_phone_drive(&token, PhoneDriveLimits::default(), at(3_005))
        .unwrap();
    assert_ne!(new_activation, first.activation_id);
    let new_generation = owner.phone_drive_input_snapshot().generation.unwrap();
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        new_generation,
        PhoneDriveContacts::Released,
        at(3_005),
        at(3_005),
    );
    let reactivated = owner
        .observe_phone_drive_contacts_at(
            &token,
            new_generation,
            held(PhoneDriveAction::Throttle),
            at(3_006),
            at(3_006),
        )
        .unwrap();
    assert_eq!(drives(&reactivated)[0].0.activation_id, new_activation);
    assert_eq!(
        drives(&reactivated)[0].0.kind,
        PhoneDriveWriteKind::Setpoint
    );
}

#[test]
fn phone_drive_cadence_has_no_catch_up_and_input_age_expires_without_poll_capacity() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    assert_eq!(first.submit_before_ms, 3_051);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    assert_eq!(
        drives(&owner.tick_phone_drive(&token, at(3_050), true).unwrap()),
        Vec::new()
    );
    let heartbeat = owner
        .observe_phone_drive_contacts_at(
            &token,
            generation,
            held(PhoneDriveAction::Throttle),
            at(3_050),
            at(3_050),
        )
        .unwrap();
    assert_eq!(drives(&heartbeat), Vec::new());
    let next = drives(&owner.tick_phone_drive(&token, at(3_051), true).unwrap());
    assert_eq!(next.len(), 1);
    assert!(owner.mark_phone_drive_transport(
        &token,
        next[0].0,
        ApplicationQueryTransportStatus::Submitted,
        at(3_051)
    ));
    let expired = owner.tick_phone_drive(&token, at(3_200), false).unwrap();
    assert_eq!(drives(&expired), Vec::new());
    assert_eq!(owner.phone_drive_activation(), None);
    assert_eq!(
        owner.phone_drive_input_snapshot().phase,
        PhoneDriveInputPhase::Cancelled(PhoneDriveCancelReason::InputExpired)
    );
    let release = drives(&owner.tick_phone_drive(&token, at(3_200), true).unwrap());
    assert_eq!(release.len(), 1);
    assert_eq!(release[0].0.kind, PhoneDriveWriteKind::Release);
    assert_eq!(release[0].1, [2, 5, 6, 0, 0, 0, 0, 205, 133, 3]);
    assert_eq!(
        owner.phone_drive_availability(),
        PhoneDriveAvailability::Ready
    );
}

#[test]
fn phone_drive_late_future_and_regressing_contact_times_cancel_the_current_activation() {
    for (observed, received) in [(3_001, 3_151), (3_003, 3_002), (3_000, 3_002)] {
        let (mut owner, token, generation) = active();
        let first = press(&mut owner, &token, generation);
        assert!(owner.mark_phone_drive_transport(
            &token,
            first,
            ApplicationQueryTransportStatus::Submitted,
            at(3_001)
        ));
        let step = owner
            .observe_phone_drive_contacts_at(
                &token,
                generation,
                held(PhoneDriveAction::Throttle),
                at(observed),
                at(received),
            )
            .unwrap();
        assert_eq!(owner.phone_drive_activation(), None);
        assert!(
            drives(&step)
                .iter()
                .all(|(write, _)| write.kind == PhoneDriveWriteKind::Release)
        );
        assert!(!owner.phone_drive_transport_is_current(&token, first, at(received)));
    }
}

#[test]
fn phone_drive_lifecycle_release_survives_authority_retirement_and_has_a_current_deadline() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    owner.cancel_phone_drive_at(PhoneDriveCancelReason::SceneInactive, at(5_000));
    assert_eq!(owner.phone_drive_activation(), None);
    let release = drives(&owner.tick_phone_drive(&token, at(5_000), true).unwrap())[0].0;
    assert_eq!(release.submit_before_ms, 5_050);
    assert!(owner.phone_drive_transport_is_current(&token, release, at(5_049)));
    assert!(!owner.phone_drive_transport_is_current(&token, release, at(5_050)));
    assert_eq!(
        drives(&owner.tick_phone_drive(&token, at(5_050), true).unwrap()),
        Vec::new()
    );
    assert!(!owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(5_050)
    ));
}

#[test]
fn phone_drive_backpressure_expires_queued_propulsion_and_never_retries_its_bytes() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Queued,
        at(3_001)
    ));
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        generation,
        held(PhoneDriveAction::Throttle),
        at(3_050),
        at(3_050),
    );
    let blocked = owner.tick_phone_drive(&token, at(3_051), false).unwrap();
    assert_eq!(drives(&blocked), Vec::new());
    assert_eq!(owner.phone_drive_activation(), None);
    assert!(!owner.phone_drive_transport_is_current(&token, first, at(3_051)));
    let next = drives(&owner.tick_phone_drive(&token, at(3_052), true).unwrap());
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].0.kind, PhoneDriveWriteKind::Release);
    assert!(owner.mark_phone_drive_transport(
        &token,
        next[0].0,
        ApplicationQueryTransportStatus::SubmissionUnknown,
        at(3_052)
    ));
    assert_eq!(
        drives(&owner.tick_phone_drive(&token, at(3_100), true).unwrap()),
        Vec::new()
    );
}

#[test]
fn phone_drive_brake_uses_distinct_positive_brake_current_and_release_does_not_disarm() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    let release = owner
        .observe_phone_drive_contacts_at(
            &token,
            generation,
            PhoneDriveContacts::Released,
            at(3_002),
            at(3_002),
        )
        .unwrap();
    assert!(owner.mark_phone_drive_transport(
        &token,
        drives(&release)[0].0,
        ApplicationQueryTransportStatus::Submitted,
        at(3_002)
    ));
    assert!(owner.phone_drive_activation().is_some());
    let brake = owner
        .observe_phone_drive_contacts_at(
            &token,
            generation,
            held(PhoneDriveAction::Brake),
            at(3_003),
            at(3_003),
        )
        .unwrap();
    assert_eq!(drives(&brake)[0].1, [2, 5, 7, 0, 0, 11, 184, 157, 253, 3]);
}

#[test]
fn phone_drive_reconnect_and_stale_generations_cannot_restore_activation_or_cancel_a_new_one() {
    let (mut owner, token, generation) = active();
    let old = press(&mut owner, &token, generation);
    owner.cancel_phone_drive_at(PhoneDriveCancelReason::Navigation, at(3_002));
    let release = drives(&owner.tick_phone_drive(&token, at(3_002), true).unwrap())[0].0;
    assert!(owner.mark_phone_drive_transport(
        &token,
        release,
        ApplicationQueryTransportStatus::Submitted,
        at(3_002)
    ));
    let unarmed_generation = owner.phone_drive_input_snapshot().generation.unwrap();
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        unarmed_generation,
        PhoneDriveContacts::Released,
        at(3_003),
        at(3_003),
    );
    let next = owner
        .activate_phone_drive(
            &token,
            PhoneDriveLimits::new(3_000, 3_000).unwrap(),
            at(3_003),
        )
        .unwrap();
    assert_ne!(next, old.activation_id);
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        generation,
        held(PhoneDriveAction::Throttle),
        at(9_000),
        at(3_004),
    );
    assert_eq!(owner.phone_drive_activation(), Some(next));
    assert!(!owner.mark_phone_drive_transport(
        &token,
        old,
        ApplicationQueryTransportStatus::SubmissionUnknown,
        at(3_004)
    ));
    assert_eq!(owner.phone_drive_activation(), Some(next));
    let replacement = begin(&mut owner, "phone-board");
    assert_eq!(owner.phone_drive_activation(), None);
    assert!(!owner.phone_drive_transport_is_current(&replacement, old, at(3_005)));
    assert!(owner.tick_phone_drive(&token, at(3_005), true).is_none());
}

#[test]
fn phone_drive_controller_fault_retires_activation_and_refuses_rearming_until_clear() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    let mut fault = RawTelemetryReadback::default();
    fault
        .fields
        .try_push(RawFieldValue::new(
            crate::VESC_RAW_CURRENT_FAULT_CODE_FIELD_ID,
            1,
        ))
        .unwrap();
    owner.observe_phone_drive_fault(&[SessionOutput::Event(
        cutout_core::DeviceEvent::ReadOnlyResponse(cutout_core::ReadOnlyResponse::RawTelemetry(
            fault,
        )),
    )]);
    assert_eq!(owner.phone_drive_activation(), None);
    assert!(!owner.phone_drive_transport_is_current(&token, first, at(3_002)));
    assert_eq!(
        owner.activate_phone_drive(
            &token,
            PhoneDriveLimits::new(3_000, 3_000).unwrap(),
            at(3_002)
        ),
        Err(PhoneDriveActivationError::ControllerFault)
    );
}

#[test]
fn phone_drive_live_geometry_change_retains_neutral_for_the_same_connection() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    let profile = cutout_core::VescDrivetrainProfile::user_configured(
        cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap(),
        cutout_core::VescDrivetrainRevision::new(1).unwrap(),
    );
    let selection = owner
        .prepare_vesc_drivetrain_selection(&token, profile.value())
        .unwrap();
    assert!(owner.apply_vesc_drivetrain_selection(selection, profile));
    assert_eq!(owner.phone_drive_activation(), None);
    let release = drives(&owner.tick_phone_drive(&token, at(3_002), true).unwrap());
    assert_eq!(
        release.len(),
        1,
        "changing live geometry retains neutral intent"
    );
    assert_eq!(release[0].0.kind, PhoneDriveWriteKind::Release);
    assert!(owner.phone_drive_transport_is_current(&token, release[0].0, at(3_002)));
}

#[test]
fn phone_drive_reactivation_cannot_discard_a_pending_neutral_release() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    owner.cancel_phone_drive_at(PhoneDriveCancelReason::Navigation, at(3_002));
    assert_eq!(
        owner.activate_phone_drive(
            &token,
            PhoneDriveLimits::new(3_000, 3_000).unwrap(),
            at(3_003)
        ),
        Err(PhoneDriveActivationError::Busy)
    );
    let release = drives(&owner.tick_phone_drive(&token, at(3_003), true).unwrap())[0].0;
    assert!(owner.mark_phone_drive_transport(
        &token,
        release,
        ApplicationQueryTransportStatus::Submitted,
        at(3_003)
    ));
    let unarmed_generation = owner.phone_drive_input_snapshot().generation.unwrap();
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        unarmed_generation,
        PhoneDriveContacts::Released,
        at(3_004),
        at(3_004),
    );
    let next = owner
        .activate_phone_drive(
            &token,
            PhoneDriveLimits::new(3_000, 3_000).unwrap(),
            at(3_004),
        )
        .unwrap();
    assert_ne!(next, first.activation_id);
    let stale = owner
        .observe_phone_drive_contacts_at(
            &token,
            generation,
            held(PhoneDriveAction::Throttle),
            at(3_005),
            at(3_005),
        )
        .unwrap();
    assert_eq!(drives(&stale), Vec::new());
}

#[test]
fn phone_drive_untimed_preview_input_cannot_mutate_an_active_hold() {
    let (mut owner, token, generation) = active();
    let first = press(&mut owner, &token, generation);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    owner.observe_phone_drive_contacts(&token, generation, PhoneDriveContacts::Released);
    assert_eq!(
        owner.phone_drive_input_snapshot().phase,
        PhoneDriveInputPhase::Pressed(PhoneDriveAction::Throttle),
        "untimed preview cannot consume the active release without neutral emission"
    );
    let expired = owner.tick_phone_drive(&token, at(3_151), true).unwrap();
    assert_eq!(owner.phone_drive_activation(), None);
    assert_eq!(drives(&expired)[0].0.kind, PhoneDriveWriteKind::Release);
}

#[test]
fn phone_drive_gesture_cancel_retains_activation_and_pending_neutral_until_submission() {
    let (mut owner, token, generation) = active();
    let activation = owner.phone_drive_activation();
    let first = press(&mut owner, &token, generation);
    assert!(owner.mark_phone_drive_transport(
        &token,
        first,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    let cancelled = owner
        .observe_phone_drive_contacts_at(
            &token,
            generation,
            PhoneDriveContacts::Multiple,
            at(3_002),
            at(3_002),
        )
        .unwrap();
    assert_eq!(owner.phone_drive_activation(), activation);
    let release = drives(&cancelled)[0].0;
    assert_eq!(release.kind, PhoneDriveWriteKind::Release);
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        generation,
        PhoneDriveContacts::Released,
        at(3_003),
        at(3_003),
    );
    let before_neutral = owner
        .observe_phone_drive_contacts_at(
            &token,
            generation,
            held(PhoneDriveAction::Throttle),
            at(3_004),
            at(3_004),
        )
        .unwrap();
    assert_eq!(drives(&before_neutral), Vec::new());
    assert!(owner.phone_drive_transport_is_current(&token, release, at(3_004)));
    assert!(owner.mark_phone_drive_transport(
        &token,
        release,
        ApplicationQueryTransportStatus::Submitted,
        at(3_004)
    ));
    let still_held = owner
        .observe_phone_drive_contacts_at(
            &token,
            generation,
            held(PhoneDriveAction::Throttle),
            at(3_005),
            at(3_005),
        )
        .unwrap();
    assert!(
        drives(&still_held).is_empty(),
        "a hold rejected before neutral must not replay after it"
    );
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        generation,
        PhoneDriveContacts::Released,
        at(3_006),
        at(3_006),
    );
    let fresh = owner
        .observe_phone_drive_contacts_at(
            &token,
            generation,
            held(PhoneDriveAction::Throttle),
            at(3_007),
            at(3_007),
        )
        .unwrap();
    assert_eq!(drives(&fresh)[0].0.kind, PhoneDriveWriteKind::Setpoint);
}

#[test]
fn phone_drive_backpressure_tick_emits_no_ordinary_polling_writes() {
    let (mut owner, token, _) = active();
    let step = owner.tick_phone_drive(&token, at(6_000), false).unwrap();
    assert!(
        step.result.outputs.iter().all(|output| !matches!(
            output,
            SessionOutput::Transport(TransportAction::Write { .. })
        )),
        "backpressure deadline processing must not schedule ordinary telemetry writes"
    );
}

#[test]
fn phone_drive_activation_requires_an_observed_complete_release_after_cancellation() {
    let (mut owner, token, _) = active();
    owner.cancel_phone_drive_at(PhoneDriveCancelReason::SceneInactive, at(3_001));
    let release = drives(&owner.tick_phone_drive(&token, at(3_001), true).unwrap())[0].0;
    assert!(owner.mark_phone_drive_transport(
        &token,
        release,
        ApplicationQueryTransportStatus::Submitted,
        at(3_001)
    ));
    assert_eq!(
        owner.activate_phone_drive(
            &token,
            PhoneDriveLimits::new(3_000, 3_000).unwrap(),
            at(3_002)
        ),
        Err(PhoneDriveActivationError::InputHeld),
        "cancelled input needs actual all-up evidence before a new activation"
    );
    let generation = owner.phone_drive_input_snapshot().generation.unwrap();
    let _ = owner.observe_phone_drive_contacts_at(
        &token,
        generation,
        PhoneDriveContacts::Released,
        at(3_003),
        at(3_003),
    );
    assert!(
        owner
            .activate_phone_drive(
                &token,
                PhoneDriveLimits::new(3_000, 3_000).unwrap(),
                at(3_003)
            )
            .is_ok()
    );
}
