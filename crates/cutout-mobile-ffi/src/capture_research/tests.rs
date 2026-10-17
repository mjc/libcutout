//! Builder-level evidence admission and persisted native readback.
use super::*;
use crate::{
    MobileCaptureFinishOutcomeDto, MobileMonotonicMillisDto, MobilePevcapCaptureBuilder,
    MobileWallClockUnixMillisDto, open_ride_database,
};
use std::{fs, path::PathBuf, sync::Arc};

fn recording() -> (
    PathBuf,
    Arc<RideDatabaseHandle>,
    Arc<MobilePevcapCaptureBuilder>,
) {
    let directory = std::env::temp_dir().join(format!("cutout-research-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let database =
        open_ride_database(directory.join("ride.sqlite").to_string_lossy().into_owned()).unwrap();
    let builder = MobilePevcapCaptureBuilder::new(
        MobileWallClockUnixMillisDto {
            milliseconds: 1_700_000_000_000,
        },
        "unsupported-platform".into(),
        None,
    );
    assert!(builder.set_database(Arc::clone(&database)));
    assert!(builder.set_capture_start_monotonic_ms(100));
    assert!(
        builder.start_writer(
            directory
                .join("capture.jsonl")
                .to_string_lossy()
                .into_owned()
        )
    );
    (directory, database, builder)
}

fn draft() -> MobileCaptureSettingsDraftDto {
    MobileCaptureSettingsDraftDto {
        profile: MobileCaptureResearchProfileDto {
            profile_id: "nosfet_aero".into(),
            pev_category: Some("euc".into()),
            model: Some("NOSFET Aero".into()),
            firmware: Some("reported firmware".into()),
            selection: MobileCaptureResearchProfileSelectionDto::Manual,
        },
        observed_at_ms: Some(250),
        source: MobileCaptureObservationSourceDto::ManufacturerApp,
        values: vec![
            MobileCaptureSettingObservationValueDto {
                setting_id: "display_brightness".into(),
                value: MobileCaptureObservedValueDto::Entered {
                    value: "145.25".into(),
                },
                unit: Some("%".into()),
                note: None,
            },
            MobileCaptureSettingObservationValueDto {
                setting_id: "pwm_tiltback".into(),
                value: MobileCaptureObservedValueDto::Off,
                unit: Some("%".into()),
                note: None,
            },
            MobileCaptureSettingObservationValueDto {
                setting_id: "high_beam".into(),
                value: MobileCaptureObservedValueDto::Boolean { value: false },
                unit: None,
                note: None,
            },
        ],
        note: Some("Brightness read before changing it".into()),
    }
}

#[test]
fn builder_settings_snapshot_is_durable_and_survives_database_reopen() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (directory, database, builder) = recording();
    let first = builder.record_settings_observation(draft(), 600).unwrap();
    assert_eq!(first.observed_at_ms, 250);
    assert_eq!(first.recorded_at_ms, 500);
    assert_eq!(
        first.values[0].value,
        MobileCaptureObservedValueDto::Entered {
            value: "145.25".into()
        }
    );
    assert_eq!(first.values[1].value, MobileCaptureObservedValueDto::Off);
    assert_eq!(
        first.values[2].value,
        MobileCaptureObservedValueDto::Boolean { value: false }
    );
    assert_eq!(
        capture_research_unusual_fields(first.clone()),
        ["display_brightness"]
    );
    let source = MobileCaptureSubmissionSourceDto::LiveCapture {
        id: first.capture_id.clone(),
    };
    assert_eq!(
        database
            .capture_settings_observations(source.clone())
            .unwrap(),
        [first.clone()]
    );

    let mut after = draft();
    after.observed_at_ms = None;
    after.values[0].value = MobileCaptureObservedValueDto::Entered { value: "30".into() };
    let second = builder.record_settings_observation(after, 900).unwrap();
    assert_eq!(second.observed_at_ms, 800);
    assert_eq!(second.recorded_at_ms, 800);
    assert_eq!(second.capture_id, first.capture_id);
    assert_eq!(
        builder.settings_observations().unwrap(),
        [first.clone(), second.clone()]
    );
    assert!(matches!(
        builder.finish_writer_outcome(),
        MobileCaptureFinishOutcomeDto::DatabaseFinished { .. }
    ));
    drop(builder);
    database.shutdown().unwrap();
    drop(database);

    let reopened =
        open_ride_database(directory.join("ride.sqlite").to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        reopened.capture_settings_observations(source).unwrap(),
        [first, second]
    );
    reopened.shutdown().unwrap();
    drop(reopened);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn builder_rejects_invalid_clock_profile_and_numeric_text_without_accepting_snapshots() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (directory, database, builder) = recording();
    assert!(matches!(builder.record_settings_observation(draft(), 99),
        Err(MobileCaptureResearchError::InvalidObservation { message }) if message == "Capture time is before recording started"));
    let mut future = draft();
    future.observed_at_ms = Some(501);
    assert!(builder.record_settings_observation(future, 600).is_err());
    let mut unknown = draft();
    unknown.profile.profile_id = "unsupported manufacturer".into();
    assert!(builder.record_settings_observation(unknown, 600).is_err());
    let mut invalid = draft();
    invalid.values[0].value = MobileCaptureObservedValueDto::Entered {
        value: "bright".into(),
    };
    assert!(builder.record_settings_observation(invalid, 600).is_err());
    assert!(builder.settings_observations().unwrap().is_empty());
    let MobileCaptureFinishOutcomeDto::DatabaseFinished {
        live_capture_id, ..
    } = builder.finish_writer_outcome()
    else {
        panic!("database writer should remain finishable after rejected drafts");
    };
    assert!(
        database
            .capture_settings_observations(MobileCaptureSubmissionSourceDto::LiveCapture {
                id: live_capture_id
            })
            .unwrap()
            .is_empty()
    );
    drop(builder);
    database.shutdown().unwrap();
    drop(database);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn builder_requires_an_active_writer_before_and_after_recording() {
    let idle = MobilePevcapCaptureBuilder::new(
        MobileWallClockUnixMillisDto {
            milliseconds: 1_700_000_000_000,
        },
        "unknown".into(),
        None,
    );
    assert!(matches!(
        idle.record_settings_observation(draft(), 600),
        Err(MobileCaptureResearchError::NotRecording)
    ));
    assert!(idle.settings_observations().unwrap().is_empty());
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (directory, database, builder) = recording();
    assert!(matches!(
        builder.finish_writer_outcome(),
        MobileCaptureFinishOutcomeDto::DatabaseFinished { .. }
    ));
    assert!(matches!(
        builder.record_settings_observation(draft(), 600),
        Err(MobileCaptureResearchError::NotRecording)
    ));
    assert!(builder.settings_observations().unwrap().is_empty());
    drop(builder);
    database.shutdown().unwrap();
    drop(database);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn builder_forces_manual_profile_selection_without_promoting_device_identity() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (directory, database, builder) = recording();
    assert!(builder.metadata().resolved_identity.is_none());
    let mut input = draft();
    input.profile.selection = MobileCaptureResearchProfileSelectionDto::Resolved;
    let saved = builder.record_settings_observation(input, 600).unwrap();
    assert_eq!(
        saved.profile.selection,
        MobileCaptureResearchProfileSelectionDto::Manual
    );
    assert_eq!(saved.profile.profile_id, "nosfet_aero");
    assert_eq!(saved.profile.pev_category.as_deref(), Some("euc"));
    assert_eq!(saved.profile.firmware.as_deref(), Some("reported firmware"));
    assert!(builder.metadata().resolved_identity.is_none());
    assert_eq!(
        builder.record_link_down(MobileMonotonicMillisDto { milliseconds: 700 }),
        crate::MobileCaptureWriteOutcomeDto::Accepted
    );
    assert!(matches!(
        builder.finish_writer_outcome(),
        MobileCaptureFinishOutcomeDto::DatabaseFinished { .. }
    ));
    drop(builder);
    database.shutdown().unwrap();
    drop(database);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn pending_settings_acknowledgement_keeps_cache_and_ble_admission_available() {
    use std::sync::mpsc::sync_channel;
    use std::time::Duration;
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (directory, database, builder) = recording();
    let (entered_sender, entered_receiver) = sync_channel(1);
    let (release_sender, release_receiver) = sync_channel(1);
    *builder
        .settings_observation_acknowledgement_gate
        .lock()
        .unwrap() = Some((entered_sender, release_receiver));
    let saving = Arc::clone(&builder);
    let save = std::thread::spawn(move || saving.record_settings_observation(draft(), 600));
    // Hold the actual save after its flush barrier was admitted, before the
    // caller consumes the receipt and publishes it as acknowledged. Storage may
    // already be durable; this gate tests acknowledgement, not a SQLite lock.
    entered_receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let (snapshot_sender, snapshot_receiver) = sync_channel(1);
    let reading = Arc::clone(&builder);
    let snapshot = std::thread::spawn(move || {
        let result = reading.settings_observations();
        snapshot_sender.send(result).unwrap();
    });
    let acknowledged = snapshot_receiver.recv_timeout(Duration::from_secs(1));
    let (admission_sender, admission_receiver) = sync_channel(1);
    let recording = Arc::clone(&builder);
    let admission = std::thread::spawn(move || {
        admission_sender
            .send(recording.record_link_down(MobileMonotonicMillisDto { milliseconds: 700 }))
            .unwrap();
    });
    let outcome = admission_receiver.recv_timeout(Duration::from_secs(1));
    // Release the held save before assertions or joins, including regressions
    // where an incorrectly held cache/writer mutex times out either observer.
    release_sender.send(()).unwrap();
    let saved = save.join().unwrap().unwrap();
    snapshot.join().unwrap();
    admission.join().unwrap();
    assert!(
        acknowledged.unwrap().unwrap().is_empty(),
        "pending values must not be acknowledged"
    );
    assert_eq!(
        outcome.unwrap(),
        crate::MobileCaptureWriteOutcomeDto::Accepted
    );
    assert_eq!(builder.settings_observations().unwrap(), [saved]);
    assert!(matches!(
        builder.finish_writer_outcome(),
        MobileCaptureFinishOutcomeDto::DatabaseFinished { .. }
    ));
    drop(builder);
    database.shutdown().unwrap();
    drop(database);
    fs::remove_dir_all(directory).unwrap();
}
