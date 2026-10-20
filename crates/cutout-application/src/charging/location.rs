use cutout_ride_maps::{ChargingLocationFix, select_charging_location_fix};

/// Selects the candidate that the charging map may use for its current-location query.
#[must_use]
pub fn select_charging_location_candidate(
    current: Option<ChargingLocationFix>,
    candidate: ChargingLocationFix,
    now_unix_seconds: f64,
) -> Option<ChargingLocationFix> {
    select_charging_location_fix(current, candidate, now_unix_seconds)
}

/// Whether a charging-map fix remains within the freshness window.
#[must_use]
pub fn charging_location_fix_is_fresh(fix: ChargingLocationFix, now_unix_seconds: f64) -> bool {
    select_charging_location_fix(None, fix, now_unix_seconds).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(latitude: f64, accuracy: f64, timestamp: f64) -> ChargingLocationFix {
        ChargingLocationFix::new(latitude, 0.0, timestamp, accuracy).expect("valid fix")
    }

    #[test]
    fn candidate_selection_prefers_the_domain_selected_candidate_and_checks_freshness() {
        let current = fix(0.0, 20.0, 100.0);
        let candidate = fix(1.0, 5.0, 101.0);
        assert_eq!(
            select_charging_location_candidate(Some(current), candidate, 101.0),
            Some(candidate)
        );
        assert!(charging_location_fix_is_fresh(candidate, 221.0));
        assert!(!charging_location_fix_is_fresh(candidate, 222.0));
    }

    #[test]
    fn candidate_selection_uses_the_domain_choice_for_a_newer_candidate() {
        let current = fix(0.0, 5.0, 100.0);
        let candidate = fix(1.0, 20.0, 101.0);
        assert_eq!(
            select_charging_location_candidate(Some(current), candidate, 101.0),
            Some(candidate)
        );
    }
}
