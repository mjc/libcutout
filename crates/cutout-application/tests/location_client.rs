use cutout_application::location::{PhoneLocationSample, PhoneLocationState};
use cutout_core::{ValueSourceDto, VerificationStatusDto};
use cutout_ride_maps::{TELEMETRY_FRESHNESS_MILLISECONDS, TelemetryFreshness};

fn sample() -> PhoneLocationSample {
    PhoneLocationSample {
        wall_clock_unix_ms: 1_700_000_000_000,
        source_timestamp_unix_seconds: None,
        latitude_degrees: 39.7,
        longitude_degrees: -104.9,
        altitude_meters: 1_600.0,
        horizontal_accuracy_meters: Some(5.0),
        vertical_accuracy_meters: None,
        speed_meters_per_second: Some(2.5),
        speed_accuracy_meters_per_second: None,
        course_degrees: None,
        course_accuracy_degrees: None,
    }
}

#[test]
fn receipt_clock_controls_freshness_and_clear_removes_old_capture_context() {
    let location = PhoneLocationState::default();
    let snapshot = location.ingest(sample(), Some(100));
    let speed = snapshot.gps_speed.unwrap();
    assert_eq!(speed.value, 2_500);
    assert_eq!(speed.source, ValueSourceDto::Reported);
    assert_eq!(speed.verification, VerificationStatusDto::SourceVerified);
    assert_eq!(
        location
            .current_readback_at(100 + TELEMETRY_FRESHNESS_MILLISECONDS)
            .freshness,
        TelemetryFreshness::Fresh
    );
    assert_eq!(
        location
            .current_readback_at(101 + TELEMETRY_FRESHNESS_MILLISECONDS)
            .freshness,
        TelemetryFreshness::Stale
    );
    location.clear();
    let readback = location.current_readback_at(102 + TELEMETRY_FRESHNESS_MILLISECONDS);
    assert!(readback.snapshot.latest_sample.is_none());
    assert_eq!(readback.freshness, TelemetryFreshness::Unavailable);
}

#[test]
fn invalid_replacement_erases_prior_usable_speed_instead_of_retaining_it() {
    let location = PhoneLocationState::default();
    location.ingest(sample(), Some(100));
    location.ingest(
        PhoneLocationSample {
            latitude_degrees: f64::NAN,
            ..sample()
        },
        Some(101),
    );
    let current = location.current_readback_at(102);
    assert!(current.snapshot.gps_speed.is_none());
    assert!(current.snapshot.latest_sample.is_none());
    assert_eq!(current.received_at_monotonic_ms, None);
    assert_eq!(current.freshness, TelemetryFreshness::Unavailable);
}

#[test]
fn source_observation_without_receipt_does_not_claim_freshness() {
    let location = PhoneLocationState::default();
    let input = PhoneLocationSample {
        source_timestamp_unix_seconds: Some(1_700_000_002.5),
        ..sample()
    };
    let snapshot = location.ingest(input, None);
    assert_eq!(
        snapshot.latest_sample.unwrap().wall_clock_unix_ms,
        1_700_000_002_500
    );
    assert_eq!(
        location.current_readback_at(100).freshness,
        TelemetryFreshness::Unavailable
    );
}
