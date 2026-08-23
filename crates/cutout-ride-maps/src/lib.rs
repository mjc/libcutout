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
pub use location::{LocationAdmission, LocationSample, LocationSource};
mod summary;
pub use summary::{DistanceMillimetres, RideSummary, distance_between_millimetres};
mod recording;
pub use recording::{
    MAX_LIVE_ROUTE_POINTS, RideMapMetadata, RideMapPoint, RideMapRecorder, RouteTelemetryState,
    TELEMETRY_FRESHNESS_MILLISECONDS, TelemetryObservation, VehicleAssociation,
};

#[cfg(test)]
mod tests {
    use super::{
        Coordinate, LatitudeE7, LocationAdmission, LocationSample, LocationSource, LongitudeE7,
        RideEvent, RideLifecycleState, RideSummary, TransitionError,
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
            1_000,
            1_700_000_000_000,
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
            1_000,
            1_700_000_000_000,
            None,
            LocationSource::Live,
        );
        let second = LocationSample::new(
            Coordinate::from_degrees(40.0, -104.999).unwrap(),
            2_000,
            1_700_000_001_000,
            None,
            LocationSource::Live,
        );
        let summary = RideSummary::from_samples(&[first, second]);
        assert_eq!(summary.point_count(), 2);
        assert!(summary.distance_millimetres() > 80_000);
        assert!(summary.distance_millimetres() < 100_000);
    }
}
