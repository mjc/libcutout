#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Rust-owned ride recording and geospatial domain primitives.

mod coordinate;
pub use coordinate::{CoordinateError, LatitudeE7, LongitudeE7, Wgs84Coordinate};
mod lifecycle;
pub use lifecycle::{RideEvent, RideLifecycleState, TransitionError};
mod location;
pub use location::{
    LocationAdmission, LocationSample, LocationSource, MonotonicMilliseconds,
    WallClockUnixMilliseconds,
};
mod summary;
pub use summary::{
    AverageSpeedMillimetresPerSecond, DistanceMillimetres, RidePointCount, RideSummary,
    distance_between, distance_between_millimetres,
};
mod recording;
pub use recording::{
    BackgroundGapCount, MAX_GAP_MILLISECONDS, MAX_LIVE_ROUTE_POINTS, RideDurationMilliseconds,
    RideLifecycleTiming, RideMapMetadata, RideMapPoint, RideMapRecorder, RideMapSegmentId,
    RidePointSequence, RideSegmentCount, RideSegmentStartReason, RouteTelemetryState,
    TELEMETRY_FRESHNESS_MILLISECONDS, TelemetryObservation, VehicleAssociation, VehicleIdentity,
    VehicleIdentityError,
};
mod projection;
pub use projection::{
    MAX_ROUTE_DISPLAY_POINTS, RouteDisplayBudget, RouteDisplayPoint, RouteEndpointMetadata,
    RoutePrivacyClass, RoutePrivacyGridE7, RoutePrivacyPolicy, RouteProjectionAccumulator,
    RouteProjectionError, RouteSegmentDisplayMetadata, RouteViewport, count_segment_runs,
    project_route_points, project_route_points_cancellable, project_route_points_from_iter,
    route_endpoint_metadata, route_segment_display_metadata,
};

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{
        AverageSpeedMillimetresPerSecond, BackgroundGapCount, CoordinateError, DistanceMillimetres,
        LatitudeE7, LocationAdmission, LocationSample, LocationSource, LongitudeE7,
        MAX_ROUTE_DISPLAY_POINTS, MonotonicMilliseconds, RideEvent, RideLifecycleState,
        RideMapPoint, RideMapRecorder, RideMapSegmentId, RidePointCount, RidePointSequence,
        RideSegmentStartReason, RideSummary, RouteDisplayBudget, RoutePrivacyClass,
        RoutePrivacyGridE7, RoutePrivacyPolicy, RouteProjectionError, RouteTelemetryState,
        RouteViewport, TransitionError, VehicleIdentity, WallClockUnixMilliseconds,
        Wgs84Coordinate, project_route_points, project_route_points_cancellable,
        project_route_points_from_iter, route_endpoint_metadata, route_segment_display_metadata,
    };

    #[test]
    fn coordinate_rejects_non_finite_and_out_of_range_values() {
        assert!(Wgs84Coordinate::from_degrees(40.0, -105.0).is_ok());
        assert!(Wgs84Coordinate::from_degrees(f64::NAN, -105.0).is_err());
        assert!(Wgs84Coordinate::from_degrees(91.0, -105.0).is_err());
        assert!(Wgs84Coordinate::from_degrees(40.0, 181.0).is_err());
        assert_eq!(
            LatitudeE7::try_from(900_000_001),
            Err(CoordinateError::LatitudeOutOfRange)
        );
        assert_eq!(
            LongitudeE7::try_from(-1_800_000_001),
            Err(CoordinateError::LongitudeOutOfRange)
        );
    }

    #[test]
    fn coordinate_uses_fixed_point_degree_values() {
        let coordinate = Wgs84Coordinate::from_degrees(40.123_456_7, -105.765_432_1).unwrap();
        assert_eq!(
            coordinate.latitude(),
            LatitudeE7::try_from(401_234_567).unwrap()
        );
        assert_eq!(
            coordinate.longitude(),
            LongitudeE7::try_from(-1_057_654_321).unwrap()
        );
    }

    #[test]
    fn background_gap_count_is_typed_and_saturating() {
        let count = BackgroundGapCount::new(u64::MAX).saturating_add(BackgroundGapCount::new(1));
        assert_eq!(count.as_u64(), u64::MAX);
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
        let coordinate = Wgs84Coordinate::from_degrees(40.0, -105.0).unwrap();
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
            Wgs84Coordinate::from_degrees(40.0, -105.0).unwrap(),
            MonotonicMilliseconds::new(1_000),
            WallClockUnixMilliseconds::new(1_700_000_000_000),
            None,
            LocationSource::Live,
        );
        let second = LocationSample::new(
            Wgs84Coordinate::from_degrees(40.0, -104.999).unwrap(),
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
    fn ride_summary_derives_average_speed_from_persisted_distance_and_duration() {
        let summary = RideSummary::from_stored(RidePointCount::new(2), 10_000);

        assert_eq!(
            summary
                .average_speed_millimetres_per_second(2_000)
                .map(AverageSpeedMillimetresPerSecond::as_u64),
            Some(5_000)
        );
        assert_eq!(summary.average_speed_millimetres_per_second(0), None);
        assert_eq!(
            RideSummary::from_stored(RidePointCount::new(2), 0)
                .average_speed_millimetres_per_second(2_000),
            None
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
                Wgs84Coordinate::from_degrees(latitude, -105.0).unwrap(),
                MonotonicMilliseconds::new(1_000 + offset),
                WallClockUnixMilliseconds::new(1_700_000_000_000 + offset),
                None,
                LocationSource::Live,
            );
            assert_eq!(recorder.check_sample(&sample), LocationAdmission::Accepted);
            recorder.record_sample(sample);
        }

        let viewport = RouteViewport::new(
            LatitudeE7::try_from(400_000_000).unwrap(),
            LatitudeE7::try_from(400_002_000).unwrap(),
            LongitudeE7::try_from(-1_050_000_000).unwrap(),
            LongitudeE7::try_from(-1_049_999_000).unwrap(),
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
                LatitudeE7::try_from(400_001_000).unwrap(),
                LatitudeE7::try_from(400_000_000).unwrap(),
                LongitudeE7::try_from(-1_050_000_000).unwrap(),
                LongitudeE7::try_from(-1_049_999_000).unwrap(),
            )
            .is_none()
        );
        assert!(
            RouteViewport::new(
                LatitudeE7::try_from(400_000_000).unwrap(),
                LatitudeE7::try_from(400_001_000).unwrap(),
                LongitudeE7::try_from(1_790_000_000).unwrap(),
                LongitudeE7::try_from(-1_790_000_000).unwrap(),
            )
            .unwrap()
            .crosses_antimeridian()
        );
        assert!(RoutePrivacyGridE7::new(0).is_none());
    }

    #[test]
    fn route_projection_keeps_routes_over_the_cap_bounded() {
        let sample = LocationSample::new(
            Wgs84Coordinate::from_degrees(40.0, -105.0).unwrap(),
            MonotonicMilliseconds::new(1_000),
            WallClockUnixMilliseconds::new(1_700_000_000_000),
            None,
            LocationSource::Live,
        );
        let point = RideMapPoint::new(
            sample,
            RideMapSegmentId::new(0),
            RouteTelemetryState::GpsOnly,
        );
        let candidate_count = MAX_ROUTE_DISPLAY_POINTS + 1;
        let projection = project_route_points_from_iter(
            (0..candidate_count).map(|sequence| (sequence as u64, point)),
            candidate_count,
            RouteDisplayBudget::new(MAX_ROUTE_DISPLAY_POINTS).unwrap(),
            RoutePrivacyPolicy::Precise,
        );

        assert_eq!(projection.len(), MAX_ROUTE_DISPLAY_POINTS);
        assert_eq!(projection.first().unwrap().sequence().as_u64(), 0);
        assert_eq!(
            projection.last().unwrap().sequence().as_u64(),
            MAX_ROUTE_DISPLAY_POINTS as u64
        );
    }

    #[test]
    fn projected_segments_preserve_reason_and_bounded_render_metadata() {
        let sample = |offset| {
            LocationSample::new(
                Wgs84Coordinate::from_degrees(40.0 + offset / 10_000.0, -105.0).unwrap(),
                MonotonicMilliseconds::new(1_000),
                WallClockUnixMilliseconds::new(1_700_000_000_000),
                None,
                LocationSource::Live,
            )
        };
        let points = [
            RideMapPoint::new_with_start_reason(
                sample(0.0),
                RideMapSegmentId::new(0),
                RouteTelemetryState::GpsOnly,
                RideSegmentStartReason::Initial,
            ),
            RideMapPoint::new_with_start_reason(
                sample(1.0),
                RideMapSegmentId::new(1),
                RouteTelemetryState::GpsOnly,
                RideSegmentStartReason::Resume,
            ),
            RideMapPoint::new_with_start_reason(
                sample(2.0),
                RideMapSegmentId::new(2),
                RouteTelemetryState::GpsOnly,
                RideSegmentStartReason::BackgroundGap,
            ),
        ];
        let projected = project_route_points_from_iter(
            points
                .into_iter()
                .enumerate()
                .map(|(sequence, point)| (u64::try_from(sequence).unwrap(), point)),
            3,
            RouteDisplayBudget::new(3).unwrap(),
            RoutePrivacyPolicy::Precise,
        );

        let segments = route_segment_display_metadata(projected);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].segment_id(), RideMapSegmentId::new(0));
        assert_eq!(segments[0].start_reason(), RideSegmentStartReason::Initial);
        assert_eq!(segments[0].visible_point_count(), 1);
        assert_eq!(
            segments[0].first_visible_sequence(),
            Some(RidePointSequence::new(0))
        );
        assert_eq!(
            segments[0].last_visible_sequence(),
            Some(RidePointSequence::new(0))
        );
        assert!(segments[0].is_retained_singleton());
        assert_eq!(segments[1].start_reason(), RideSegmentStartReason::Resume);
        assert_eq!(
            segments[2].start_reason(),
            RideSegmentStartReason::BackgroundGap
        );
    }

    #[test]
    fn privacy_grid_does_not_leak_exact_coordinates_at_world_boundaries() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(MonotonicMilliseconds::new(1_000), None)
            .unwrap();
        let coordinate = Wgs84Coordinate::from_fixed_parts(-900_000_000, -1_800_000_000).unwrap();
        let sample = LocationSample::new(
            coordinate,
            MonotonicMilliseconds::new(1_000),
            WallClockUnixMilliseconds::new(1_700_000_000_000),
            None,
            LocationSource::Live,
        );
        assert_eq!(recorder.check_sample(&sample), LocationAdmission::Accepted);
        recorder.record_sample(sample);

        let projection = project_route_points(
            recorder.points(),
            recorder.first_point_sequence(),
            None,
            RouteDisplayBudget::new(1).unwrap(),
            RoutePrivacyPolicy::grid(RoutePrivacyGridE7::new(7).unwrap()),
        );

        assert_eq!(
            projection[0].privacy_class(),
            RoutePrivacyClass::GridRedacted
        );
        assert_ne!(projection[0].coordinate(), coordinate);
        assert!(projection[0].coordinate().latitude().as_i32() > -900_000_000);
        assert!(projection[0].coordinate().longitude().as_i32() > -1_800_000_000);
    }

    #[test]
    fn streamed_route_projection_preserves_sequence_without_retaining_input() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(MonotonicMilliseconds::new(1_000), None)
            .unwrap();
        for offset in 0..4_u32 {
            let sample = LocationSample::new(
                Wgs84Coordinate::from_degrees(40.0 + f64::from(offset) / 10_000.0, -105.0).unwrap(),
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

    #[test]
    fn route_endpoint_metadata_does_not_promote_viewport_interior_points() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(MonotonicMilliseconds::new(1_000), None)
            .unwrap();
        for (offset, latitude) in [(0, 40.0), (1_000, 40.0001), (2_000, 40.0002)] {
            let sample = LocationSample::new(
                Wgs84Coordinate::from_degrees(latitude, -105.0).unwrap(),
                MonotonicMilliseconds::new(1_000 + offset),
                WallClockUnixMilliseconds::new(1_700_000_000_000 + offset),
                None,
                LocationSource::Live,
            );
            assert_eq!(recorder.check_sample(&sample), LocationAdmission::Accepted);
            recorder.record_sample(sample);
        }

        let viewport = RouteViewport::new(
            LatitudeE7::try_from(400_001_000).unwrap(),
            LatitudeE7::try_from(400_001_000).unwrap(),
            LongitudeE7::try_from(-1_050_000_000).unwrap(),
            LongitudeE7::try_from(-1_049_999_000).unwrap(),
        )
        .unwrap();
        let endpoints = route_endpoint_metadata(
            recorder
                .points()
                .iter()
                .copied()
                .enumerate()
                .map(|(offset, point)| (RidePointSequence::new(offset as u64), point)),
            recorder.point_count(),
            Some(viewport),
        );

        assert_eq!(
            endpoints.start_sequence().map(RidePointSequence::as_u64),
            Some(0)
        );
        assert_eq!(
            endpoints.end_sequence().map(RidePointSequence::as_u64),
            Some(2)
        );
        assert!(!endpoints.start_visible());
        assert!(!endpoints.end_visible());
    }

    #[test]
    fn route_endpoint_metadata_keeps_canonical_sequence_for_a_budgeted_route() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(MonotonicMilliseconds::new(1_000), None)
            .unwrap();
        for (offset, latitude) in [(0, 40.0), (1_000, 40.0001), (2_000, 40.0002)] {
            let sample = LocationSample::new(
                Wgs84Coordinate::from_degrees(latitude, -105.0).unwrap(),
                MonotonicMilliseconds::new(1_000 + offset),
                WallClockUnixMilliseconds::new(1_700_000_000_000 + offset),
                None,
                LocationSource::Live,
            );
            assert_eq!(recorder.check_sample(&sample), LocationAdmission::Accepted);
            recorder.record_sample(sample);
        }

        let endpoints = route_endpoint_metadata(
            recorder
                .points()
                .iter()
                .copied()
                .enumerate()
                .map(|(offset, point)| (RidePointSequence::new(offset as u64), point)),
            recorder.point_count(),
            None,
        );
        let projection = project_route_points(
            recorder.points(),
            recorder.first_point_sequence(),
            None,
            RouteDisplayBudget::new(1).unwrap(),
            RoutePrivacyPolicy::Precise,
        );

        assert_eq!(
            projection
                .iter()
                .map(|point| point.sequence().as_u64())
                .collect::<Vec<_>>(),
            [0]
        );
        assert_eq!(
            endpoints.start_sequence().map(RidePointSequence::as_u64),
            Some(0)
        );
        assert_eq!(
            endpoints.end_sequence().map(RidePointSequence::as_u64),
            Some(2)
        );
        assert!(endpoints.start_visible());
        assert!(endpoints.end_visible());
    }

    #[test]
    fn route_endpoint_metadata_uses_supplied_sequences_for_noncontiguous_points() {
        let sample = |latitude| {
            LocationSample::new(
                Wgs84Coordinate::from_degrees(latitude, -105.0).unwrap(),
                MonotonicMilliseconds::new(1_000),
                WallClockUnixMilliseconds::new(1_700_000_000_000),
                None,
                LocationSource::Live,
            )
        };
        let points = [
            (
                RidePointSequence::new(4),
                RideMapPoint::new(
                    sample(40.0),
                    RideMapSegmentId::new(0),
                    RouteTelemetryState::GpsOnly,
                ),
            ),
            (
                RidePointSequence::new(9),
                RideMapPoint::new(
                    sample(40.0001),
                    RideMapSegmentId::new(0),
                    RouteTelemetryState::GpsOnly,
                ),
            ),
        ];
        let endpoints = route_endpoint_metadata(points, 10, None);

        assert!(!endpoints.start_visible());
        assert!(endpoints.end_visible());
        assert_eq!(endpoints.start_sequence(), Some(RidePointSequence::new(0)));
        assert_eq!(endpoints.end_sequence(), Some(RidePointSequence::new(9)));
    }

    #[test]
    fn cancellable_route_projection_returns_typed_cancellation_without_sleeping() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(MonotonicMilliseconds::new(1_000), None)
            .unwrap();
        for offset in 0..4_u32 {
            let sample = LocationSample::new(
                Wgs84Coordinate::from_degrees(40.0 + f64::from(offset) / 10_000.0, -105.0).unwrap(),
                MonotonicMilliseconds::new(1_000 + u64::from(offset) * 1_000),
                WallClockUnixMilliseconds::new(1_700_000_000_000 + u64::from(offset) * 1_000),
                None,
                LocationSource::Live,
            );
            assert_eq!(recorder.check_sample(&sample), LocationAdmission::Accepted);
            recorder.record_sample(sample);
        }

        let checks = Cell::new(0);
        let error = project_route_points_cancellable(
            recorder.points(),
            recorder.first_point_sequence(),
            None,
            RouteDisplayBudget::new(2).unwrap(),
            RoutePrivacyPolicy::Precise,
            || {
                let check = checks.get();
                checks.set(check + 1);
                check >= 5
            },
        )
        .expect_err("the deterministic cancellation predicate must stop projection");

        assert_eq!(error, RouteProjectionError::Cancelled);
        assert!(checks.get() >= 5);
    }
}
