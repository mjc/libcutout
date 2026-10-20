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
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&first)
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
        Err(MobileCaptureResearchError::InvalidObservation { reason }) if reason == "Capture time is before recording started"));
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
    assert_eq!(
        builder.settings_observations().unwrap(),
        [] as [MobileCaptureSettingsObservationDto; 0]
    );
    let MobileCaptureFinishOutcomeDto::DatabaseFinished {
        live_capture_id, ..
    } = builder.finish_writer_outcome()
    else {
        panic!("database writer should remain finishable after rejected drafts");
    };
    assert_eq!(
        database
            .capture_settings_observations(MobileCaptureSubmissionSourceDto::LiveCapture {
                id: live_capture_id
            })
            .unwrap(),
        [] as [MobileCaptureSettingsObservationDto; 0]
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
    assert_eq!(
        idle.settings_observations().unwrap(),
        [] as [MobileCaptureSettingsObservationDto; 0]
    );
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
    assert_eq!(
        builder.settings_observations().unwrap(),
        [] as [MobileCaptureSettingsObservationDto; 0]
    );
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

fn batch_requests() -> Vec<MobileCaptureResearchReadbackRequestDto> {
    [35, 12, 35, 5, 25]
        .into_iter()
        .map(|at_ms| MobileCaptureResearchReadbackRequestDto {
            profile_id: "nosfet_aero".into(),
            at_ms,
        })
        .collect()
}

fn batch_packet(at_ms: u64) -> cutout_core::PevcapRecord {
    let bytes = hex::decode("dc5a5c4729f2000000170000ab6c001700000be9045a00000226021ca8f607801b25000080c80000808080808080080000803200364f371e00000100808028062e7964800080801540e23a").unwrap();
    cutout_core::PevcapRecord::inbound_notification(
        cutout_core::MonotonicTimestamp::new(at_ms),
        cutout_protocols::VETERAN_DATA_CHANNEL,
        cutout_protocols::VETERAN_SERVICE_CHANNEL,
        bytes,
    )
}

#[test]
fn batch_readback_visits_transport_once_for_multiple_cutoffs_without_time_order_assumptions() {
    let mut scans = 0;
    let mut records_read = 0;
    let requests = batch_requests();
    let result = readback_batch(requests.clone(), |observe| {
        scans += 1;
        for record in [
            batch_packet(10),
            batch_packet(500),
            cutout_core::PevcapRecord::link_down(cutout_core::MonotonicTimestamp::new(20)),
            batch_packet(30),
        ] {
            records_read += 1;
            observe(record);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(scans, 1);
    assert_eq!(records_read, 4);
    assert_eq!(result.len(), requests.len());
    for (response, request) in result.iter().zip(requests) {
        assert_eq!(response.profile_id, request.profile_id);
        assert_eq!(response.at_ms, request.at_ms);
    }
    let packet_times = result
        .iter()
        .map(|response| {
            response
                .values
                .iter()
                .find(|value| value.setting_id == "display_brightness")
                .map(|value| value.observed_at_ms)
        })
        .collect::<Vec<_>>();
    assert_eq!(packet_times, [Some(30), Some(10), Some(30), None, None]);
    assert_eq!(
        result[0], result[2],
        "duplicate cutoff requests retain their result positions"
    );
}

#[test]
fn batch_readback_rejects_unbounded_requests_before_reading_capture_bytes() {
    let invalid = [
        vec![MobileCaptureResearchReadbackRequestDto {
            profile_id: "nosfet_aero".into(),
            at_ms: cutout_core::MAX_CAPTURE_SETTINGS_OFFSET_MILLISECONDS + 1,
        }],
        vec![
            MobileCaptureResearchReadbackRequestDto {
                profile_id: "nosfet_aero".into(),
                at_ms: 0
            };
            cutout_core::MAX_CAPTURE_SETTINGS_OBSERVATIONS + 1
        ],
    ];
    for requests in invalid {
        assert!(
            readback_batch(requests, |_| panic!(
                "invalid requests must not scan the capture"
            ))
            .is_err()
        );
    }
    assert_eq!(
        readback_batch(Vec::new(), |_| panic!(
            "empty review must not scan the capture"
        ))
        .unwrap(),
        [] as [MobileCaptureResearchReadbackDto; 0]
    );
}

#[test]
fn batch_readback_matches_active_builder_and_saved_database_observations() {
    let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (directory, database, builder) = recording();
    let first = builder.record_settings_observation(draft(), 600).unwrap();
    let source = MobileCaptureSubmissionSourceDto::LiveCapture {
        id: first.capture_id,
    };
    for record in [
        batch_packet(10),
        batch_packet(500),
        cutout_core::PevcapRecord::link_down(cutout_core::MonotonicTimestamp::new(20)),
        batch_packet(30),
    ] {
        let timestamp = MobileMonotonicMillisDto {
            milliseconds: record.monotonic_ms.get() + 100,
        };
        let outcome = if record.direction == cutout_core::PevcapDirection::LinkDown {
            builder.record_link_down(timestamp)
        } else {
            builder.record_notification(
                timestamp,
                record.characteristic.as_bytes().to_vec(),
                record
                    .service
                    .unwrap_or(cutout_protocols::VETERAN_SERVICE_CHANNEL)
                    .as_bytes()
                    .to_vec(),
                record.bytes.to_vec(),
            )
        };
        assert_eq!(outcome, crate::MobileCaptureWriteOutcomeDto::Accepted);
    }
    let active = builder
        .capture_research_readback_batch(batch_requests())
        .unwrap();
    assert!(matches!(
        builder.finish_writer_outcome(),
        MobileCaptureFinishOutcomeDto::DatabaseFinished { .. }
    ));
    let saved = database
        .capture_research_readback_batch(source, batch_requests())
        .unwrap();
    assert_eq!(saved, active);
    drop(builder);
    database.shutdown().unwrap();
    drop(database);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn batch_readback_preserves_known_results_and_unknown_profile_positions() {
    let requests = vec![
        MobileCaptureResearchReadbackRequestDto {
            profile_id: "nosfet_aero".into(),
            at_ms: 12,
        },
        MobileCaptureResearchReadbackRequestDto {
            profile_id: "future_profile".into(),
            at_ms: 35,
        },
        MobileCaptureResearchReadbackRequestDto {
            profile_id: "begode_falcon".into(),
            at_ms: 35,
        },
        MobileCaptureResearchReadbackRequestDto {
            profile_id: "nosfet_aero".into(),
            at_ms: 35,
        },
    ];
    let mut scans = 0;
    let results = readback_batch(requests.clone(), |observe| {
        scans += 1;
        observe(batch_packet(10));
        observe(cutout_core::PevcapRecord::inbound_notification(
            cutout_core::MonotonicTimestamp::new(20),
            cutout_protocols::BEGODE_DATA_CHANNEL,
            cutout_protocols::VETERAN_SERVICE_CHANNEL,
            hex::decode("55aa000000320000000f003200030502000004185a5a5a5a").unwrap(),
        ));
        observe(batch_packet(30));
        Ok(())
    })
    .unwrap();
    assert_eq!(scans, 1);
    assert_eq!(results.len(), requests.len());
    for (result, request) in results.iter().zip(requests) {
        assert_eq!(result.profile_id, request.profile_id);
        assert_eq!(result.at_ms, request.at_ms);
    }
    assert_eq!(results[1].values, []);
    let maximum_speed = results[2]
        .values
        .iter()
        .find(|value| value.setting_id == "maximum_speed")
        .unwrap();
    assert_eq!(maximum_speed.observed_at_ms, 20);
    assert_eq!(
        maximum_speed.value,
        Some(crate::MobileSettingValueDto::Number { value: 500 })
    );
    for (index, expected_at) in [(0, 10), (3, 30)] {
        let brightness = results[index]
            .values
            .iter()
            .find(|value| value.setting_id == "display_brightness")
            .unwrap();
        assert_eq!(brightness.observed_at_ms, expected_at);
        assert_eq!(
            brightness.value,
            Some(crate::MobileSettingValueDto::Number { value: 30 })
        );
    }
}

#[test]
fn batch_readback_unknown_profiles_still_validate_the_retained_source() {
    let requests = vec![MobileCaptureResearchReadbackRequestDto {
        profile_id: "future_profile".into(),
        at_ms: 35,
    }];
    let mut scans = 0;
    let results = readback_batch(requests.clone(), |observe| {
        scans += 1;
        observe(batch_packet(10));
        Ok(())
    })
    .unwrap();
    assert_eq!(scans, 1);
    assert_eq!(results[0].profile_id, "future_profile");
    assert_eq!(results[0].at_ms, 35);
    assert_eq!(results[0].values, []);
    let failure = readback_batch(requests, |_| Err(MobileCaptureResearchError::WriterFailed));
    assert!(matches!(
        failure,
        Err(MobileCaptureResearchError::WriterFailed)
    ));
}
