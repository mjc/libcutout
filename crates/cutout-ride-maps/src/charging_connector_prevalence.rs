use crate::{ChargingConnector, ChargingCountry, ChargingSupply, charging_connector_choices};

/// Counts distinct nearby spots supporting each fixed connector.
/// Outlet counts and repeated supplies do not amplify a spot's contribution.
#[derive(Clone, Debug)]
pub struct ChargingConnectorPrevalence(Vec<(ChargingConnector, u64)>);

impl ChargingConnectorPrevalence {
    /// Seeds stable ties and unseen connectors from the GPS country fallback.
    #[must_use]
    pub fn new(country: Option<&ChargingCountry>) -> Self {
        Self(
            charging_connector_choices(country)
                .into_iter()
                .map(|connector| (connector, 0))
                .collect(),
        )
    }

    /// Records one visible spot. Unknown and custom labels do not affect ranking.
    pub fn record_spot(&mut self, supplies: &[ChargingSupply]) {
        for (connector, count) in &mut self.0 {
            if *connector != ChargingConnector::Unknown
                && supplies.iter().any(|supply| {
                    supply
                        .connectors()
                        .iter()
                        .any(|observed| same_connector(observed, connector))
                })
            {
                *count = count.saturating_add(1);
            }
        }
    }

    /// Returns the complete catalogue, most commonly observed connectors first.
    #[must_use]
    pub fn into_choices(mut self) -> Vec<ChargingConnector> {
        self.0.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        self.0.into_iter().map(|(connector, _)| connector).collect()
    }
}

fn same_connector(observed: &ChargingConnector, choice: &ChargingConnector) -> bool {
    observed == choice
        || (*observed == ChargingConnector::TypeB && *choice == ChargingConnector::Nema5_15)
}
