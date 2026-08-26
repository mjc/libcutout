#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Rust-owned ride recording and geospatial domain primitives.

mod coordinate;
pub use coordinate::{Coordinate, CoordinateError, LatitudeE7, LongitudeE7};
mod lifecycle;
pub use lifecycle::{RideEvent, RideLifecycleState, TransitionError};
mod location;
pub use location::{
    LocationAdmission, LocationSample, LocationSource, MonotonicMilliseconds,
    WallClockUnixMilliseconds,
};
mod summary;
pub use summary::{
    DistanceMillimetres, RidePointCount, RideSummary, distance_between,
    distance_between_millimetres,
};
mod recording;
pub use recording::{
    MAX_GAP_MILLISECONDS, MAX_LIVE_ROUTE_POINTS, RideDurationMilliseconds, RideLifecycleTiming,
    RideMapMetadata, RideMapPoint, RideMapRecorder, RideMapSegmentId, RidePointSequence,
    RideSegmentCount, RideSegmentStartReason, RouteTelemetryState,
    TELEMETRY_FRESHNESS_MILLISECONDS, TelemetryObservation, VehicleAssociation, VehicleIdentity,
    VehicleIdentityError,
};
mod projection;
pub use projection::{
    MAX_ROUTE_DISPLAY_POINTS, RouteDisplayBudget, RouteDisplayPoint, RoutePrivacyClass,
    RoutePrivacyGridE7, RoutePrivacyPolicy, RouteViewport, project_route_points,
    project_route_points_from_iter,
};

#[cfg(test)]
mod tests {
    use super::{
        Coordinate, DistanceMillimetres, LatitudeE7, LocationAdmission, LocationSample,
        LocationSource, LongitudeE7, MAX_ROUTE_DISPLAY_POINTS, MonotonicMilliseconds, RideEvent,
        RideLifecycleState, RideMapRecorder, RidePointCount, RideSummary, RouteDisplayBudget,
        RoutePrivacyClass, RoutePrivacyGridE7, RoutePrivacyPolicy, RouteViewport, TransitionError,
        VehicleIdentity, WallClockUnixMilliseconds, project_route_points,
        project_route_points_from_iter,
    };

    #[test]
    fn coordinate_rejects_non_finite_and_out_of_range_values() {
        assert!(Coordinate::from_degrees(40.0, -105.0).is_ok());
        assert!(Coordinate::from_degrees(f64::NAN, -105.0).is_err());
        assert!(Coordinate::from_degrees(91.0, -105.0).is_err());
        assert!(Coordinate::from_degrees(40.0, 181.0).is_err());
    }

    #[test]
    fn coordinate_uses_fixed_point_degree_values() {
        let coordinate = Coordinate::from_degrees(40.123_456_7, -105.765_432_1).unwrap();
        assert_eq!(coordinate.latitude(), LatitudeE7::new(401_234_567));
        assert_eq!(coordinate.longitude(), LongitudeE7::new(-1_057_654_321));
    }

    #[test]
    fn ride_lifecycle_accepts_only_valid_transitions() {
        let state = RideLifecycleState::Draft;
        let state = state.apply(RideEvent::Start).unwrap();
        let state = state.apply(RideEvent::Pause).unwrap();
        let state = state.apply(RideEvent::Resume).unwrap();
        let state = state.apply(RideEvent::Stop).unwrap();
        assert_eq!(state.apply(RideEvent::Save), Ok(RideLifecycleState::Saved));
        assert_eq!(state.apply(RideEvent::Pause), Err(TransitionError::Invalid));
    }

    #[test]
    fn location_admission_deduplicates_repeated_samples() {
        let coordinate = Coordinate::from_degrees(40.0, -105.0).unwrap();
        let sample = LocationSample::new(
            coordinate,
            MonotonicMilliseconds::new(1_000),
            WallClockUnixMilliseconds::new(1_700_000_000_000),
            Some(3_000),
            LocationSource::PevcapImport,
        );
        assert_eq!(sample.admission(None), LocationAdmission::Accepted);
        assert_eq!(
            sample.admission(Some(&sample)),
            LocationAdmission::Duplicate
        );
    }

    #[test]
    fn ride_summary_counts_points_and_distance() {
        let first = LocationSample::new(
            Coordinate::from_degrees(40.0, -105.0).unwrap(),
            MonotonicMilliseconds::new(1_000),
            WallClockUnixMilliseconds::new(1_700_000_000_000),
            None,
            LocationSource::Live,
        );
        let second = LocationSample::new(
            Coordinate::from_degrees(40.0, -104.999).unwrap(),
            MonotonicMilliseconds::new(2_000),
            WallClockUnixMilliseconds::new(1_700_000_001_000),
            None,
            LocationSource::Live,
        );
        let summary = RideSummary::from_samples(&[first, second]);
        assert_eq!(summary.point_count().as_u64(), 2);
        assert!(summary.distance_millimetres() > 80_000);
        assert!(summary.distance_millimetres() < 100_000);
    }

    #[test]
    fn ride_point_count_keeps_count_arithmetic_typed_and_saturating() {
        let count = RidePointCount::from_usize(3);
        assert_eq!(count.saturating_add(RidePointCount::new(2)).as_u64(), 5);
        assert_eq!(count.saturating_sub(RidePointCount::new(4)).as_u64(), 0);
        assert!(RidePointCount::default().is_zero());
    }

    #[test]
    fn distance_keeps_arithmetic_typed_and_saturating() {
        let distance = DistanceMillimetres::new(u64::MAX - 1);
        assert_eq!(
            distance
                .saturating_add(DistanceMillimetres::new(2))
                .as_u64(),
            u64::MAX
        );
    }

    #[test]
    fn vehicle_identity_rejects_blank_values_and_trims_boundaries() {
        assert_eq!(
            VehicleIdentity::new("  NF2557 "),
            Some(VehicleIdentity::new("NF2557").expect("test identity is valid"))
        );
        assert_eq!(VehicleIdentity::new("   "), None);
    }

    #[test]
    fn route_projection_is_viewport_bounded_and_privacy_classified() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(MonotonicMilliseconds::new(1_000), None)
            .unwrap();
        for (offset, latitude) in [
            (0, 40.0),
            (1_000, 40.0001),
            (2_000, 40.0002),
            (3_000, 40.0003),
        ] {
            let sample = LocationSample::new(
                Coordinate::from_degrees(latitude, -105.0).unwrap(),
                MonotonicMilliseconds::new(1_000 + offset),
                WallClockUnixMilliseconds::new(1_700_000_000_000 + offset),
                None,
                LocationSource::Live,
            );
            assert_eq!(recorder.check_sample(&sample), LocationAdmission::Accepted);
            recorder.record_sample(sample);
        }

        let viewport = RouteViewport::new(
            LatitudeE7::new(400_000_000),
            LatitudeE7::new(400_002_000),
            LongitudeE7::new(-1_050_000_000),
            LongitudeE7::new(-1_049_999_000),
        )
        .unwrap();
        let projection = project_route_points(
            recorder.points(),
            recorder.first_point_sequence(),
            Some(viewport),
            RouteDisplayBudget::new(2).unwrap(),
            RoutePrivacyPolicy::grid(RoutePrivacyGridE7::new(1_000).unwrap()),
        );

        assert_eq!(projection.len(), 2);
        assert_eq!(projection[0].sequence().as_u64(), 0);
        assert_eq!(projection[1].sequence().as_u64(), 2);
        assert!(
            projection
                .iter()
                .all(|point| point.privacy_class() == RoutePrivacyClass::GridRedacted)
        );
        assert_eq!(projection[0].coordinate().latitude().as_i32(), 400_000_000);
        assert_eq!(projection[1].coordinate().latitude().as_i32(), 400_002_000);
    }

    #[test]
    fn route_projection_rejects_unbounded_configuration() {
        assert!(RouteDisplayBudget::new(0).is_none());
        assert!(RouteDisplayBudget::new(MAX_ROUTE_DISPLAY_POINTS + 1).is_none());
        assert!(
            RouteViewport::new(
                LatitudeE7::new(400_001_000),
                LatitudeE7::new(400_000_000),
                LongitudeE7::new(-1_050_000_000),
                LongitudeE7::new(-1_049_999_000),
            )
            .is_none()
        );
        assert!(
            RouteViewport::new(
                LatitudeE7::new(400_000_000),
                LatitudeE7::new(400_001_000),
                LongitudeE7::new(1_790_000_000),
                LongitudeE7::new(-1_790_000_000),
            )
            .unwrap()
            .crosses_antimeridian()
        );
        assert!(RoutePrivacyGridE7::new(0).is_none());
    }

    #[test]
    fn streamed_route_projection_preserves_sequence_without_retaining_input() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(MonotonicMilliseconds::new(1_000), None)
            .unwrap();
        for offset in 0..4_u32 {
            let sample = LocationSample::new(
                Coordinate::from_degrees(40.0 + f64::from(offset) / 10_000.0, -105.0).unwrap(),
                MonotonicMilliseconds::new(1_000 + u64::from(offset) * 1_000),
                WallClockUnixMilliseconds::new(1_700_000_000_000 + u64::from(offset) * 1_000),
                None,
                LocationSource::Live,
            );
            assert_eq!(recorder.check_sample(&sample), LocationAdmission::Accepted);
            recorder.record_sample(sample);
        }
        let points = recorder
            .points()
            .iter()
            .copied()
            .enumerate()
            .map(|(sequence, point)| (u64::try_from(sequence).unwrap(), point));

        let projection = project_route_points_from_iter(
            points,
            4,
            RouteDisplayBudget::new(2).unwrap(),
            RoutePrivacyPolicy::Precise,
        );

        assert_eq!(
            projection
                .iter()
                .map(|point| point.sequence().as_u64())
                .collect::<Vec<_>>(),
            vec![0, 3]
        );
    }
}
