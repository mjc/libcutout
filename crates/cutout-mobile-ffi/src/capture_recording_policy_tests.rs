//! Integration invariants for the native capture origin and decoder-evidence boundary.

use super::*;
use cutout_core::{PevcapCapture, PevcapEncoding};
use std::fs;

fn ms(milliseconds: u64) -> MobileMonotonicMillisDto {
    MobileMonotonicMillisDto { milliseconds }
}

fn stationary_snapshot(at: u64) -> MobileTelemetrySnapshotDto {
    let mut snapshot = MobileTelemetrySnapshotDto::from(TelemetrySnapshotDto::from(
        cutout_core::TelemetrySnapshot::default(),
    ));
    snapshot.at_ms = Some(ms(at));
    snapshot.speed_observed_at_ms = Some(ms(at));
    snapshot.speed = Some(SpeedReading {
        value: Speed { value: 0 },
        source: MobileValueSourceDto::Reported,
        quality: MobileValueQualityDto::Known,
        verification: MobileVerificationStatusDto::HardwareVerified,
    });
    snapshot
}

fn decoded_notification(
    builder: &MobilePevcapCaptureBuilder,
    at: u64,
    snapshot: MobileTelemetrySnapshotDto,
) {
    assert_eq!(
        builder.record_decoded_notification(
            ms(at),
            vec![0x11; 16],
            vec![0x22; 16],
            vec![0xaa, 0xbb],
            None,
            Some(snapshot),
            MobileCaptureNotificationEvidenceDto::StationaryTelemetry
        ),
        MobileCaptureWriteOutcomeDto::Accepted,
    );
}

fn capture(
    origin: MobileCaptureOriginDto,
    run: impl FnOnce(&MobilePevcapCaptureBuilder),
) -> PevcapCapture {
    capture_with_setup(|builder| assert!(builder.set_recording_origin(origin)), run)
}

fn capture_with_setup(
    setup: impl FnOnce(&MobilePevcapCaptureBuilder),
    run: impl FnOnce(&MobilePevcapCaptureBuilder),
) -> PevcapCapture {
    let directory = std::env::temp_dir().join(format!("cutout-capture-policy-{}", Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let database =
        open_ride_database(directory.join("ride.sqlite").to_string_lossy().into_owned()).unwrap();
    let path = directory.join("capture.jsonl");
    let builder = MobilePevcapCaptureBuilder::new(
        MobileWallClockUnixMillisDto { milliseconds: 1000 },
        "test".into(),
        None,
    );
    assert!(builder.set_database(Arc::clone(&database)));
    assert!(builder.set_capture_start_monotonic_ms(100));
    assert!(builder.set_music_history_policy(MobileMusicHistoryPolicyDto::HumanReadable));
    setup(&builder);
    assert!(builder.start_writer(path.to_string_lossy().into_owned()));
    run(&builder);
    assert!(builder.finish_writer());
    assert!(!builder.set_recording_origin(MobileCaptureOriginDto::Manual));
    let status = builder.writer_status();
    assert_eq!(status.dropped_messages, 0);
    assert!(!status.failed);
    let capture = PevcapCapture::decode(&fs::read(path).unwrap(), PevcapEncoding::Jsonl).unwrap();
    database.shutdown().unwrap();
    fs::remove_dir_all(directory).unwrap();
    capture
}

#[test]
fn automatic_capture_origin_freezes_and_filters_actual_mobile_clock_wrappers() {
    let capture = capture(MobileCaptureOriginDto::Automatic, |builder| {
        assert!(!builder.set_recording_origin(MobileCaptureOriginDto::Manual));
        for at in 110..=112 {
            decoded_notification(builder, at, stationary_snapshot(at));
        }
    });
    assert_eq!(capture.records.len(), 1);
    assert_eq!(capture.records[0].monotonic_ms.get(), 10);
    let semantic = capture.records[0].semantic_telemetry.as_ref().unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(&semantic.snapshot_json).unwrap();
    assert_eq!(snapshot["at_ms"]["milliseconds"], 10);
    assert_eq!(snapshot["speed_observed_at_ms"]["milliseconds"], 10);
    assert!(
        capture
            .header
            .annotations
            .iter()
            .any(|value| value == "capture_recording_policy=material_changes")
    );
}

#[test]
fn manual_capture_origin_retains_each_decoded_observation() {
    let capture = capture(MobileCaptureOriginDto::Manual, |builder| {
        assert!(!builder.set_recording_origin(MobileCaptureOriginDto::Automatic));
        for at in 110..=112 {
            decoded_notification(builder, at, stationary_snapshot(at));
        }
    });
    assert_eq!(capture.records.len(), 3);
    assert!(
        !capture
            .header
            .annotations
            .iter()
            .any(|value| value.starts_with("capture_recording_policy="))
    );
}

fn music(at: u64) -> MobilePevcapMusicEventDto {
    MobilePevcapMusicEventDto {
        provider: MobileMusicProviderDto::Spotify,
        track_id: "spotify:track:test".into(),
        monotonic_at_ms: at,
        wall_clock_unix_ms: 900 + at,
        clock_uncertainty_ms: 0,
        ride_sequence: None,
    }
}

#[test]
fn automatic_capture_music_occurrences_and_link_transitions_reset_the_baseline() {
    let capture = capture(MobileCaptureOriginDto::Automatic, |builder| {
        for at in 110..=111 {
            decoded_notification(builder, at, stationary_snapshot(at));
        }
        assert_eq!(
            builder.record_music_event(music(112)),
            MobileCaptureWriteOutcomeDto::Accepted
        );
        for at in 113..=114 {
            decoded_notification(builder, at, stationary_snapshot(at));
        }
        assert_eq!(
            builder.record_link_down(ms(115)),
            MobileCaptureWriteOutcomeDto::Accepted
        );
        assert_eq!(
            builder.record_link_up(ms(116), None),
            MobileCaptureWriteOutcomeDto::Accepted
        );
        for at in 117..=118 {
            decoded_notification(builder, at, stationary_snapshot(at));
        }
        for _ in 0..2 {
            assert_eq!(
                builder.record_music_event(music(119)),
                MobileCaptureWriteOutcomeDto::Accepted
            );
        }
        for at in 120..=121 {
            decoded_notification(builder, at, stationary_snapshot(at));
        }
    });
    assert_eq!(capture.records.len(), 6);
    assert_eq!(capture.music_events.len(), 3);
    assert_eq!(
        capture
            .records
            .iter()
            .map(|record| record.monotonic_ms.get())
            .collect::<Vec<_>>(),
        [10, 13, 15, 16, 17, 20]
    );
}

#[test]
fn constructible_stationary_evidence_cannot_hide_missing_moving_or_stale_speed() {
    let capture = capture(MobileCaptureOriginDto::Automatic, |builder| {
        for at in 110..=111 {
            let mut snapshot = stationary_snapshot(at);
            snapshot.speed = None;
            decoded_notification(builder, at, snapshot);
        }
        for at in 112..=113 {
            let mut snapshot = stationary_snapshot(at);
            snapshot.speed.as_mut().unwrap().value.value = 501;
            decoded_notification(builder, at, snapshot);
        }
        for at in 114..=115 {
            let mut snapshot = stationary_snapshot(at);
            snapshot.speed_observed_at_ms = Some(ms(109));
            decoded_notification(builder, at, snapshot);
        }
    });
    assert_eq!(capture.records.len(), 6);
}

#[test]
fn automatic_origin_reservation_failure_preserves_full_default_capture() {
    let capture = capture_with_setup(
        |builder| {
            for index in 0..cutout_core::PEVCAP_MAX_ANNOTATIONS {
                assert_eq!(
                    builder.add_annotation(format!("note={index}")),
                    MobileCaptureWriteOutcomeDto::Accepted
                );
            }
            assert!(!builder.set_recording_origin(MobileCaptureOriginDto::Automatic));
        },
        |builder| {
            for at in 110..=111 {
                decoded_notification(builder, at, stationary_snapshot(at));
            }
        },
    );
    assert_eq!(capture.records.len(), 2);
    assert_eq!(
        capture.header.annotations.len(),
        cutout_core::PEVCAP_MAX_ANNOTATIONS
    );
    assert!(
        !capture
            .header
            .annotations
            .iter()
            .any(|value| value.starts_with("capture_recording_policy="))
    );
}

#[test]
fn automatic_policy_reserves_a_slot_before_ordinary_annotation_admission() {
    let capture = capture_with_setup(
        |builder| {
            assert!(builder.set_recording_origin(MobileCaptureOriginDto::Automatic));
            for index in 0..cutout_core::PEVCAP_MAX_ANNOTATIONS - 1 {
                assert_eq!(
                    builder.add_annotation(format!("note={index}")),
                    MobileCaptureWriteOutcomeDto::Accepted
                );
            }
            assert_eq!(
                builder.add_annotation("note=overflow".into()),
                MobileCaptureWriteOutcomeDto::Rejected
            );
        },
        |builder| decoded_notification(builder, 110, stationary_snapshot(110)),
    );
    assert_eq!(capture.records.len(), 1);
    assert_eq!(
        capture.header.annotations.len(),
        cutout_core::PEVCAP_MAX_ANNOTATIONS
    );
    assert!(
        !capture
            .header
            .annotations
            .iter()
            .any(|value| value == "note=overflow")
    );
}

#[test]
fn active_label_closure_reservation_includes_the_automatic_policy_slot() {
    let capture = capture_with_setup(
        |builder| {
            assert!(builder.set_recording_origin(MobileCaptureOriginDto::Automatic));
            for index in 0..cutout_core::PEVCAP_MAX_ANNOTATIONS - 3 {
                assert_eq!(
                    builder.add_annotation(format!("note={index}")),
                    MobileCaptureWriteOutcomeDto::Accepted
                );
            }
        },
        |builder| {
            assert_eq!(
                builder
                    .change_label(MobileCaptureLabelActionDto::Start {
                        label: MobileCaptureLabelDto::Ride
                    })
                    .unwrap(),
                [MobileCaptureLabelDto::Ride]
            );
            assert_eq!(
                builder.add_annotation("note=overflow".into()),
                MobileCaptureWriteOutcomeDto::Rejected
            );
            decoded_notification(builder, 110, stationary_snapshot(110));
        },
    );
    assert_eq!(
        capture.header.annotations.len(),
        cutout_core::PEVCAP_MAX_ANNOTATIONS
    );
    assert!(
        capture
            .header
            .annotations
            .iter()
            .any(|value| value == "capture_label=ride_start")
    );
    assert!(
        capture
            .header
            .annotations
            .iter()
            .any(|value| value == "capture_label=ride_stop")
    );
    assert!(
        !capture
            .header
            .annotations
            .iter()
            .any(|value| value == "note=overflow")
    );
}
