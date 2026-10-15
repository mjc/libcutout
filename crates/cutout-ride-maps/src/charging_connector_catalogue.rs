use crate::{ChargingConnector, ChargingCountry};

const CONNECTORS: &[ChargingConnector] = &[
    ChargingConnector::TypeA,
    ChargingConnector::TypeC,
    ChargingConnector::TypeD,
    ChargingConnector::TypeE,
    ChargingConnector::TypeF,
    ChargingConnector::TypeG,
    ChargingConnector::TypeH,
    ChargingConnector::TypeI,
    ChargingConnector::TypeJ,
    ChargingConnector::TypeK,
    ChargingConnector::TypeL,
    ChargingConnector::TypeM,
    ChargingConnector::TypeN,
    ChargingConnector::TypeO,
    ChargingConnector::Nema5_15,
    ChargingConnector::Nema5_20,
    ChargingConnector::Nema14_50,
    ChargingConnector::Nema14_30,
    ChargingConnector::Nema10_30,
    ChargingConnector::Nema6_50,
    ChargingConnector::Nema10_50,
    ChargingConnector::J1772,
    ChargingConnector::Type2,
    ChargingConnector::Ccs1,
    ChargingConnector::Ccs2,
    ChargingConnector::Chademo,
    ChargingConnector::Nacs,
    ChargingConnector::Unknown,
];

/// Returns the connector picker catalogue in stable order for a GPS-derived country.
/// This presentation policy neither selects nor confirms a connector.
#[must_use]
pub fn charging_connector_choices(
    location_country: Option<&ChargingCountry>,
) -> Vec<ChargingConnector> {
    let mut choices = CONNECTORS.to_vec();
    choices.sort_by_key(|connector| {
        (
            prevalence(connector, location_country),
            country_priority(connector, location_country),
        )
    });
    choices
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prevalence {
    LocalOutlet,
    RegionalEv,
    Other,
}

fn country_priority(connector: &ChargingConnector, country: Option<&ChargingCountry>) -> bool {
    // Ascending sort puts the preferred US household face (false) first.
    !(country.is_some_and(|country| country.as_str() == "US")
        && connector == &ChargingConnector::Nema5_15)
}

fn prevalence(connector: &ChargingConnector, country: Option<&ChargingCountry>) -> Prevalence {
    let Some(country) = country else {
        return Prevalence::Other;
    };
    if local_outlets(country).contains(connector) {
        Prevalence::LocalOutlet
    } else if regional_ev_connectors(country).contains(connector) {
        Prevalence::RegionalEv
    } else {
        Prevalence::Other
    }
}

// Coarse regional groups, not market-share estimates. Ties retain catalogue order.
// Sources and the distinction from national-default suggestions: docs/charging-connector-ordering.md.
fn local_outlets(country: &ChargingCountry) -> &'static [ChargingConnector] {
    use ChargingConnector::{
        Nema5_15, TypeA, TypeC, TypeD, TypeE, TypeF, TypeG, TypeI, TypeM, TypeN,
    };
    match country.as_str() {
        "US" | "CA" => &[
            TypeA,
            Nema5_15,
            ChargingConnector::Nema14_50,
            ChargingConnector::Nema14_30,
            ChargingConnector::Nema10_30,
            ChargingConnector::Nema6_50,
            ChargingConnector::Nema10_50,
        ],
        "MX" | "JP" => &[TypeA, Nema5_15],
        "GB" | "IE" => &[TypeG],
        "DE" | "AT" => &[TypeC, TypeF],
        "FR" | "BE" => &[TypeC, TypeE],
        "AU" | "NZ" => &[TypeI],
        "BR" => &[TypeC, TypeN],
        "IN" => &[TypeC, TypeD, TypeM],
        _ => &[],
    }
}

fn regional_ev_connectors(country: &ChargingCountry) -> &'static [ChargingConnector] {
    use ChargingConnector::{Ccs1, Ccs2, J1772, Nacs, Type2};
    match country.as_str() {
        "US" | "CA" => &[J1772, Ccs1, Nacs],
        "GB" | "IE" | "DE" | "AT" | "FR" | "BE" => &[Type2, Ccs2],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_outlets_precede_other_connectors_in_representative_regions() {
        for (code, expected) in [
            (
                "US",
                vec![
                    ChargingConnector::Nema5_15,
                    ChargingConnector::TypeA,
                    ChargingConnector::Nema14_50,
                    ChargingConnector::Nema14_30,
                    ChargingConnector::Nema10_30,
                    ChargingConnector::Nema6_50,
                    ChargingConnector::Nema10_50,
                ],
            ),
            ("GB", vec![ChargingConnector::TypeG]),
            (
                "DE",
                vec![ChargingConnector::TypeC, ChargingConnector::TypeF],
            ),
            ("AU", vec![ChargingConnector::TypeI]),
            (
                "FR",
                vec![ChargingConnector::TypeC, ChargingConnector::TypeE],
            ),
        ] {
            let country = ChargingCountry::new(code).unwrap();
            let choices = charging_connector_choices(Some(&country));
            assert_eq!(
                choices
                    .iter()
                    .take(expected.len())
                    .cloned()
                    .collect::<Vec<_>>(),
                expected,
                "{code}"
            );
        }
    }

    #[test]
    fn regional_ev_connectors_precede_foreign_household_connectors() {
        for (code, local_ev, foreign_outlet) in [
            ("US", ChargingConnector::Nacs, ChargingConnector::TypeG),
            ("CA", ChargingConnector::J1772, ChargingConnector::TypeI),
            ("GB", ChargingConnector::Type2, ChargingConnector::TypeA),
            ("DE", ChargingConnector::Ccs2, ChargingConnector::TypeA),
        ] {
            let country = ChargingCountry::new(code).unwrap();
            let choices = charging_connector_choices(Some(&country));
            assert!(
                choices.iter().position(|c| c == &local_ev).unwrap()
                    < choices.iter().position(|c| c == &foreign_outlet).unwrap(),
                "{code}"
            );
        }
    }

    #[test]
    fn every_region_preserves_all_distinct_picker_choices_without_duplicates() {
        for code in ["US", "GB", "DE", "AU", "JP", "FR", "ZZ"] {
            let country = ChargingCountry::new(code).unwrap();
            let choices = charging_connector_choices(Some(&country));
            assert_eq!(choices.len(), CONNECTORS.len());
            for connector in CONNECTORS {
                assert_eq!(
                    choices.iter().filter(|value| *value == connector).count(),
                    1,
                    "{code}: {connector:?}"
                );
            }
            assert_eq!(choices.last(), Some(&ChargingConnector::Unknown));
        }
    }

    #[test]
    fn unsupported_and_missing_location_share_the_stable_fallback_order() {
        let unsupported = ChargingCountry::new("ZZ").unwrap();
        assert_eq!(charging_connector_choices(None), CONNECTORS);
        assert_eq!(charging_connector_choices(Some(&unsupported)), CONNECTORS);
    }

    #[test]
    fn equal_prevalence_keeps_catalogue_order() {
        let france = ChargingCountry::new("FR").unwrap();
        let choices = charging_connector_choices(Some(&france));
        let tied = choices
            .into_iter()
            .filter(|connector| {
                connector != &ChargingConnector::TypeC
                    && connector != &ChargingConnector::TypeE
                    && connector != &ChargingConnector::Type2
                    && connector != &ChargingConnector::Ccs2
            })
            .collect::<Vec<_>>();
        let fallback = CONNECTORS
            .iter()
            .filter(|connector| {
                *connector != &ChargingConnector::TypeC
                    && *connector != &ChargingConnector::TypeE
                    && *connector != &ChargingConnector::Type2
                    && *connector != &ChargingConnector::Ccs2
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(tied, fallback);
    }
}
