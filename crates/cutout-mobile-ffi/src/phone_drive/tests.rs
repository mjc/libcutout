use super::*;

fn database() -> std::sync::Arc<crate::RideDatabaseHandle> {
    let path = std::env::temp_dir().join(format!(
        "cutout-phone-drive-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    crate::open_ride_database(path.to_string_lossy().into_owned()).unwrap()
}

#[test]
fn phone_drive_ffi_preferences_roundtrip_without_restoring_pressed_input() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = CutoutSessionStateHandle::with_database(database.clone());
    assert_eq!(
        handle.load_phone_control_mode().unwrap().mode,
        MobilePhoneControlModeDto::Disabled
    );
    for mode in [
        MobilePhoneControlModeDto::Buttons,
        MobilePhoneControlModeDto::Area,
        MobilePhoneControlModeDto::Disabled,
    ] {
        let saved = handle.save_phone_control_mode(mode).unwrap();
        assert_eq!(saved.mode, mode);
        assert_eq!(
            saved.unavailability,
            MobilePhoneDriveUnavailableReasonDto::Disconnected
        );
        let restored = CutoutSessionStateHandle::with_database(database.clone())
            .load_phone_control_mode()
            .unwrap();
        assert_eq!(restored.mode, mode);
        assert!(!matches!(
            restored.phase,
            MobilePhoneDriveInputPhaseDto::Pressed { .. }
        ));
    }
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_ffi_failed_save_keeps_mode_but_retires_input() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = CutoutSessionStateHandle::with_database(database.clone());
    handle
        .save_phone_control_mode(MobilePhoneControlModeDto::Buttons)
        .unwrap();
    {
        let mut owner = handle.lock_inner();
        let input = &mut owner.session_state_mut().phone_drive_input;
        let generation = input.snapshot().generation.unwrap();
        input.observe(generation, core::PhoneDriveContacts::Released);
        input.observe(
            generation,
            core::PhoneDriveContacts::Single {
                contact: core::PhoneDriveContactId::new(1).unwrap(),
                region: Some(core::PhoneDriveAction::Throttle),
            },
        );
    }
    let before = handle.phone_drive_snapshot();
    assert_eq!(
        before.phase,
        MobilePhoneDriveInputPhaseDto::Pressed {
            action: MobilePhoneDriveActionDto::Throttle
        }
    );
    database.shutdown().unwrap();
    assert!(
        handle
            .save_phone_control_mode(MobilePhoneControlModeDto::Area)
            .is_err()
    );
    let failed = handle.phone_drive_snapshot();
    assert_eq!(failed.mode, MobilePhoneControlModeDto::Buttons);
    assert_ne!(failed.generation, before.generation);
    assert_eq!(
        failed.phase,
        MobilePhoneDriveInputPhaseDto::Cancelled {
            reason: MobilePhoneDriveCancelReasonDto::ModeChanged
        }
    );
}

#[test]
fn phone_drive_ffi_concurrent_preference_operations_keep_active_and_durable_mode_together() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = CutoutSessionStateHandle::with_database(database.clone());
    std::thread::scope(|scope| {
        scope.spawn(|| {
            handle
                .save_phone_control_mode(MobilePhoneControlModeDto::Area)
                .unwrap()
        });
        scope.spawn(|| {
            handle
                .save_phone_control_mode(MobilePhoneControlModeDto::Buttons)
                .unwrap()
        });
        scope.spawn(|| handle.load_phone_control_mode().unwrap());
    });
    assert_eq!(
        core::PhoneControlMode::from(handle.phone_drive_snapshot().mode),
        database.inner.phone_control_mode().unwrap()
    );
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_ffi_invalid_native_contact_cannot_become_a_valid_id() {
    let contacts: core::PhoneDriveContacts = MobilePhoneDriveContactsDto::Single {
        contact_id: 0,
        region: Some(MobilePhoneDriveActionDto::Throttle),
    }
    .into();
    assert_eq!(contacts, core::PhoneDriveContacts::Invalid);
    let handle = CutoutSessionStateHandle::new();
    assert!(matches!(
        handle.load_phone_control_mode(),
        Err(MobilePhoneControlError::PersistenceUnavailable)
    ));
    assert!(matches!(
        handle.save_phone_control_mode(MobilePhoneControlModeDto::Area),
        Err(MobilePhoneControlError::PersistenceUnavailable)
    ));
    assert_eq!(
        handle.phone_drive_snapshot().mode,
        MobilePhoneControlModeDto::Disabled
    );
}
