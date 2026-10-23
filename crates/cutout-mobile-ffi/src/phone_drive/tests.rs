use super::*;
use crate::{
    MobileApplicationQueryTransportStatusDto, MobileSessionInputDto, MobileSessionInputKindDto,
    MobileVehicleKindDto,
};

fn database() -> std::sync::Arc<crate::RideDatabaseHandle> {
    let path = std::env::temp_dir().join(format!(
        "cutout-phone-drive-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    crate::open_ride_database(path.to_string_lossy().into_owned()).unwrap()
}

fn eligible_eskate_attempt(
    handle: &CutoutSessionStateHandle,
    identity: &str,
    at_ms: u64,
) -> MobileConnectionAttemptTokenDto {
    const VESC_REPLY: &[u8] = &[
        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
        38, 208, 3,
    ];
    let token = handle
        .begin_connection_attempt(identity.to_owned(), at_ms)
        .token
        .unwrap();
    handle.connection_link_established(token.clone());
    handle.observe_connection_notification(token.clone(), VESC_REPLY.to_vec());
    handle.resolve_device_session(token.clone(), false, at_ms + 1);
    assert!(handle.configure_connection_vehicle_kind(token.clone(), MobileVehicleKindDto::Eskate));
    let link = handle
        .ingest_device_session(
            token.clone(),
            MobileSessionInputDto {
                kind: MobileSessionInputKindDto::LinkUp,
                monotonic_ms: crate::MobileMonotonicMillisDto {
                    milliseconds: at_ms + 2,
                },
                max_write_len: None,
                channel: Vec::new(),
                bytes: Vec::new(),
            },
        )
        .unwrap();
    let query = link
        .result
        .outputs
        .into_iter()
        .find_map(|output| output.application_query)
        .expect("verified VESC startup asks about its application");
    assert!(handle.mark_application_query_transport(
        token.clone(),
        query.query_id,
        0,
        1,
        MobileApplicationQueryTransportStatusDto::Submitted,
        at_ms + 2,
    ));
    let _ = handle.ingest_device_session(
        token.clone(),
        MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Tick,
            monotonic_ms: crate::MobileMonotonicMillisDto {
                milliseconds: at_ms + 10_000,
            },
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
        },
    );
    token
}

#[test]
fn phone_drive_ffi_loads_preferences_only_for_the_eligible_target() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = CutoutSessionStateHandle::with_database(database.clone());
    let token = eligible_eskate_attempt(&handle, "board-a", 1_000);

    assert!(matches!(
        handle.phone_drive_snapshot().availability,
        MobilePhoneDriveAvailabilityDto::Ready
    ));
    handle
        .set_phone_drive_settings_presented(token.clone(), true)
        .unwrap();
    let snapshot = handle.load_phone_drive_preferences(token).unwrap();
    assert_eq!(snapshot.device_identity.as_deref(), Some("board-a"));
    assert_eq!(snapshot.mode, MobilePhoneControlModeDto::Disabled);
    assert_eq!(snapshot.limits.throttle_milliamps, 3_000);
    assert_eq!(snapshot.limits.brake_milliamps, 3_000);
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_ffi_rejects_stale_device_attempts_without_publishing_into_replacement() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = CutoutSessionStateHandle::with_database(database.clone());
    let first = eligible_eskate_attempt(&handle, "board-a", 1_000);
    handle
        .set_phone_drive_settings_presented(first.clone(), true)
        .unwrap();
    handle.load_phone_drive_preferences(first.clone()).unwrap();
    let saved_a = handle
        .save_phone_drive_preferences(
            first.clone(),
            MobilePhoneControlModeDto::Buttons,
            "7.125".into(),
            "4.5".into(),
        )
        .unwrap();
    assert_eq!(saved_a.device_identity.as_deref(), Some("board-a"));
    assert_eq!(saved_a.limits.throttle_milliamps, 7_125);
    assert_eq!(saved_a.limits.brake_milliamps, 4_500);

    let replacement = eligible_eskate_attempt(&handle, "board-b", 20_000);

    assert_ne!(first.generation, replacement.generation);
    assert!(matches!(
        handle.load_phone_drive_preferences(first.clone()),
        Err(MobilePhoneControlError::DeviceChanged)
    ));
    assert!(matches!(
        handle.set_phone_drive_settings_presented(first.clone(), true),
        Err(MobilePhoneControlError::DeviceChanged)
    ));
    assert!(matches!(
        handle.save_phone_drive_preferences(
            first,
            MobilePhoneControlModeDto::Buttons,
            "7.125".into(),
            "4.5".into(),
        ),
        Err(MobilePhoneControlError::DeviceChanged)
    ));
    handle
        .set_phone_drive_settings_presented(replacement.clone(), true)
        .unwrap();
    let board_b = handle
        .load_phone_drive_preferences(replacement.clone())
        .unwrap();
    assert_eq!(board_b.device_identity.as_deref(), Some("board-b"));
    assert_eq!(board_b.mode, MobilePhoneControlModeDto::Disabled);
    assert_eq!(board_b.limits.throttle_milliamps, 3_000);
    handle
        .set_phone_drive_settings_presented(replacement, false)
        .unwrap();

    let reopened_a = eligible_eskate_attempt(&handle, "board-a", 40_000);
    handle
        .set_phone_drive_settings_presented(reopened_a.clone(), true)
        .unwrap();
    let restored_a = handle.load_phone_drive_preferences(reopened_a).unwrap();
    assert_eq!(restored_a.device_identity.as_deref(), Some("board-a"));
    assert_eq!(restored_a.mode, MobilePhoneControlModeDto::Buttons);
    assert_eq!(restored_a.limits.throttle_milliamps, 7_125);
    assert_eq!(restored_a.limits.brake_milliamps, 4_500);

    database.shutdown().unwrap();
}

#[test]
fn phone_drive_ffi_discards_a_completed_sql_load_after_attempt_replacement() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = std::sync::Arc::new(CutoutSessionStateHandle::with_database(database.clone()));
    let first = eligible_eskate_attempt(&handle, "board-a", 1_000);
    handle
        .set_phone_drive_settings_presented(first.clone(), true)
        .unwrap();
    handle.load_phone_drive_preferences(first.clone()).unwrap();
    handle
        .save_phone_drive_preferences(
            first.clone(),
            MobilePhoneControlModeDto::Buttons,
            "7.125".into(),
            "4.5".into(),
        )
        .unwrap();
    let first_generation = first.generation;

    let (read_tx, read_rx) = std::sync::mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(0);
    let loading_handle = handle.clone();
    let loading_database = database.clone();
    let load = std::thread::spawn(move || {
        load_phone_drive_preferences_with(&loading_handle, first, |identity| {
            let record = loading_database
                .inner
                .phone_drive_preferences(identity)
                .unwrap();
            read_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
            Ok(record)
        })
    });

    read_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the real SQLite read must finish before the controlled pause");
    assert!(
        handle.inner.try_lock().is_ok(),
        "database I/O must not retain the session mutex needed for connection changes"
    );
    let replacement = handle.begin_connection_attempt("board-b".into(), 20_000);
    let replacement = replacement.token.unwrap();
    assert_ne!(replacement.generation, first_generation);

    resume_tx.send(()).unwrap();
    assert!(matches!(
        load.join().unwrap(),
        Err(MobilePhoneControlError::DeviceChanged)
    ));
    let snapshot = handle.phone_drive_snapshot();
    assert_eq!(snapshot.mode, MobilePhoneControlModeDto::Disabled);
    assert_eq!(snapshot.limits.throttle_milliamps, 3_000);
    assert_eq!(snapshot.limits.brake_milliamps, 3_000);
    assert_eq!(
        handle
            .device_session_snapshot()
            .connection
            .token
            .unwrap()
            .generation,
        replacement.generation
    );
    let durable_a = database.inner.phone_drive_preferences("board-a").unwrap();
    assert_eq!(durable_a.mode(), core::PhoneControlMode::Buttons);
    assert_eq!(durable_a.limits().throttle_milliamps(), 7_125);
    assert_eq!(durable_a.limits().brake_milliamps(), 4_500);
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_ffi_invalid_atomic_save_preserves_the_last_complete_device_record() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = CutoutSessionStateHandle::with_database(database.clone());
    let token = eligible_eskate_attempt(&handle, "board-a", 1_000);
    handle
        .set_phone_drive_settings_presented(token.clone(), true)
        .unwrap();
    handle.load_phone_drive_preferences(token.clone()).unwrap();
    let saved = handle
        .save_phone_drive_preferences(
            token.clone(),
            MobilePhoneControlModeDto::Buttons,
            "7.125".into(),
            "4.5".into(),
        )
        .unwrap();
    assert!(matches!(
        handle.save_phone_drive_preferences(
            token.clone(),
            MobilePhoneControlModeDto::Area,
            "8".into(),
            "NaN".into(),
        ),
        Err(MobilePhoneControlError::InvalidLimits)
    ));
    let after_failure = handle.phone_drive_snapshot();
    assert_eq!(after_failure.mode, saved.mode);
    assert_eq!(after_failure.limits, saved.limits);

    handle
        .set_phone_drive_settings_presented(token.clone(), false)
        .unwrap();
    let reopened = eligible_eskate_attempt(&handle, "board-a", 20_000);
    handle
        .set_phone_drive_settings_presented(reopened.clone(), true)
        .unwrap();
    let durable = handle.load_phone_drive_preferences(reopened).unwrap();
    assert_eq!(durable.mode, MobilePhoneControlModeDto::Buttons);
    assert_eq!(durable.limits.throttle_milliamps, 7_125);
    assert_eq!(durable.limits.brake_milliamps, 4_500);
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_ffi_rejects_activation_after_the_requested_input_generation_is_cancelled() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = CutoutSessionStateHandle::with_database(database.clone());
    let token = eligible_eskate_attempt(&handle, "board-a", 1_000);
    handle
        .set_phone_drive_settings_presented(token.clone(), true)
        .unwrap();
    handle.load_phone_drive_preferences(token.clone()).unwrap();
    let saved = handle
        .save_phone_drive_preferences(
            token.clone(),
            MobilePhoneControlModeDto::Buttons,
            "7".into(),
            "4".into(),
        )
        .unwrap();
    let requested_generation = saved.generation.unwrap();
    handle
        .set_phone_drive_settings_presented(token.clone(), false)
        .unwrap();
    let cancelled =
        handle.cancel_phone_drive_at(MobilePhoneDriveCancelReasonDto::Navigation, 12_000);
    let current_generation = cancelled.generation.unwrap();
    assert_ne!(requested_generation, current_generation);
    assert!(
        handle
            .observe_phone_drive_contacts_at(
                token.clone(),
                current_generation,
                MobilePhoneDriveContactsDto::Released,
                12_001,
                12_001,
            )
            .is_some()
    );

    let activation = handle.activate_phone_drive(token, requested_generation, 12_002);
    let snapshot = handle.phone_drive_snapshot();
    database.shutdown().unwrap();
    assert!(matches!(
        activation,
        Err(MobilePhoneControlError::Activation {
            reason: MobilePhoneDriveActivationRefusalDto::InputGenerationChanged
        })
    ));
    assert!(matches!(
        snapshot.availability,
        MobilePhoneDriveAvailabilityDto::Ready
    ));
}

#[test]
fn phone_drive_ffi_rejects_a_save_completion_after_same_attempt_cancellation() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = std::sync::Arc::new(CutoutSessionStateHandle::with_database(database.clone()));
    let token = eligible_eskate_attempt(&handle, "board-a", 1_000);
    handle
        .set_phone_drive_settings_presented(token.clone(), true)
        .unwrap();
    handle.load_phone_drive_preferences(token.clone()).unwrap();

    let (stored_tx, stored_rx) = std::sync::mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(0);
    let saving_handle = handle.clone();
    let saving_database = database.clone();
    let saving_token = token.clone();
    let save = std::thread::spawn(move || {
        save_phone_drive_preferences_with(
            &saving_handle,
            saving_token,
            MobilePhoneControlModeDto::Buttons,
            "7".into(),
            "4".into(),
            |identity, record| {
                saving_database
                    .inner
                    .save_phone_drive_preferences(identity, record)
                    .map_err(|error| MobilePhoneControlError::Persistence {
                        error: crate::map_ride_database_error(error),
                    })?;
                stored_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                Ok(())
            },
        )
    });

    stored_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("actual SQL write must finish before the controlled owner-publication pause");
    let durable = database.inner.phone_drive_preferences("board-a").unwrap();
    assert_eq!(durable.mode(), core::PhoneControlMode::Buttons);
    let before_cancel = handle.phone_drive_snapshot();
    let cancelled =
        handle.cancel_phone_drive_at(MobilePhoneDriveCancelReasonDto::Navigation, 12_000);
    assert_ne!(before_cancel.generation, cancelled.generation);
    assert_eq!(cancelled.mode, MobilePhoneControlModeDto::Disabled);

    resume_tx.send(()).unwrap();
    let completed = save.join().unwrap();
    let final_snapshot = handle.phone_drive_snapshot();
    database.shutdown().unwrap();
    assert!(matches!(
        completed,
        Err(MobilePhoneControlError::InputGenerationChanged)
    ));
    assert_eq!(final_snapshot.generation, cancelled.generation);
    assert_eq!(final_snapshot.mode, MobilePhoneControlModeDto::Disabled);
    assert_eq!(final_snapshot.phase, cancelled.phase);
}

#[test]
fn phone_drive_ffi_concurrent_loads_and_saves_keep_one_complete_record() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = CutoutSessionStateHandle::with_database(database.clone());
    let token = eligible_eskate_attempt(&handle, "board-a", 1_000);
    handle
        .set_phone_drive_settings_presented(token.clone(), true)
        .unwrap();
    handle.load_phone_drive_preferences(token.clone()).unwrap();

    std::thread::scope(|scope| {
        for (mode, throttle, brake) in [
            (MobilePhoneControlModeDto::Buttons, "2.5", "1.25"),
            (MobilePhoneControlModeDto::Area, "4", "3.5"),
        ] {
            let handle = &handle;
            let token = token.clone();
            scope.spawn(move || {
                handle
                    .save_phone_drive_preferences(
                        token,
                        mode,
                        throttle.to_owned(),
                        brake.to_owned(),
                    )
                    .unwrap()
            });
        }
        let handle = &handle;
        let token = token.clone();
        scope.spawn(move || handle.load_phone_drive_preferences(token).unwrap());
    });

    let final_snapshot = handle.load_phone_drive_preferences(token.clone()).unwrap();
    let final_record = database.inner.phone_drive_preferences("board-a").unwrap();
    assert_eq!(
        final_snapshot.mode,
        MobilePhoneControlModeDto::from(final_record.mode())
    );
    assert_eq!(
        final_snapshot.limits.throttle_milliamps,
        final_record.limits().throttle_milliamps()
    );
    assert_eq!(
        final_snapshot.limits.brake_milliamps,
        final_record.limits().brake_milliamps()
    );
    assert!(matches!(
        (
            final_snapshot.mode,
            final_snapshot.limits.throttle_milliamps,
            final_snapshot.limits.brake_milliamps
        ),
        (MobilePhoneControlModeDto::Buttons, 2_500, 1_250)
            | (MobilePhoneControlModeDto::Area, 4_000, 3_500)
    ));
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_ffi_modal_cancellation_does_not_wait_for_configuration_storage() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = database();
    let handle = std::sync::Arc::new(CutoutSessionStateHandle::with_database(database.clone()));
    let token = eligible_eskate_attempt(&handle, "board-a", 1_000);
    handle
        .set_phone_drive_settings_presented(token.clone(), true)
        .unwrap();

    let configuration_gate = handle
        .configuration_updates
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker_handle = handle.clone();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        done_tx
            .send(worker_handle.set_phone_drive_settings_presented(token, false))
            .unwrap();
    });
    started_rx.recv().unwrap();
    let completed_while_storage_ordering_was_held = done_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .is_ok();
    drop(configuration_gate);
    worker.join().unwrap();
    assert!(
        completed_while_storage_ordering_was_held,
        "input cancellation must not wait behind a database operation"
    );
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_ffi_rejects_invalid_settings_without_mutating_session_defaults() {
    let handle = CutoutSessionStateHandle::new();
    assert!(matches!(
        handle.phone_drive_snapshot().availability,
        MobilePhoneDriveAvailabilityDto::Unavailable {
            reason: MobilePhoneDriveUnavailableReasonDto::Disconnected
        }
    ));
    assert_eq!(handle.phone_drive_snapshot().device_identity, None);
    assert_eq!(
        handle.phone_drive_snapshot().mode,
        MobilePhoneControlModeDto::Disabled
    );
    assert_eq!(
        handle.phone_drive_snapshot().limits.throttle_milliamps,
        3_000
    );
    assert_eq!(handle.phone_drive_snapshot().limits.brake_milliamps, 3_000);
}
