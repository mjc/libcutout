/// Portable current-location data used by charging map selection.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingLocationFixDto {
    /// Latitude in WGS84 degrees.
    pub latitude_degrees: f64,
    /// Longitude in WGS84 degrees.
    pub longitude_degrees: f64,
    /// Source timestamp in Unix seconds.
    pub source_timestamp_unix_seconds: f64,
    /// Horizontal accuracy in metres.
    pub horizontal_accuracy_meters: f64,
}

/// Applies the portable charging-map fix-selection policy.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_location_candidate_is_accepted(
    current: Option<MobileChargingLocationFixDto>,
    candidate: MobileChargingLocationFixDto,
    now_unix_seconds: f64,
) -> bool {
    let Some(candidate) = domain_fix(candidate) else {
        return false;
    };
    let current = current.and_then(domain_fix);
    cutout_ride_maps::select_charging_location_fix(current, candidate, now_unix_seconds).is_some()
}

/// Whether a charging-map fix remains within the freshness window.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_location_fix_is_fresh(
    fix: MobileChargingLocationFixDto,
    now_unix_seconds: f64,
) -> bool {
    let Some(fix) = domain_fix(fix) else {
        return false;
    };
    cutout_ride_maps::select_charging_location_fix(None, fix, now_unix_seconds).is_some()
}

fn domain_fix(
    value: MobileChargingLocationFixDto,
) -> Option<cutout_ride_maps::ChargingLocationFix> {
    cutout_ride_maps::ChargingLocationFix::new(
        value.latitude_degrees,
        value.longitude_degrees,
        value.source_timestamp_unix_seconds,
        value.horizontal_accuracy_meters,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(latitude_degrees: f64, accuracy: f64, timestamp: f64) -> MobileChargingLocationFixDto {
        MobileChargingLocationFixDto {
            latitude_degrees,
            longitude_degrees: 0.0,
            source_timestamp_unix_seconds: timestamp,
            horizontal_accuracy_meters: accuracy,
        }
    }

    #[test]
    fn ffi_adapter_accepts_selected_candidate_and_rejects_invalid_input() {
        let current = fix(0.0, 20.0, 100.0);
        let candidate = fix(1.0, 5.0, 101.0);
        assert!(mobile_charging_location_candidate_is_accepted(
            Some(current),
            candidate,
            101.0
        ));
        assert!(!mobile_charging_location_candidate_is_accepted(
            None,
            fix(f64::NAN, 5.0, 101.0),
            101.0
        ));
        assert!(!mobile_charging_location_fix_is_fresh(candidate, 222.0));
        assert!(mobile_charging_location_fix_is_fresh(candidate, 221.0));
    }
}
