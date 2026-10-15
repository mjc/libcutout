use cutout_ride_maps::{
    ChargingConnector, ChargingConnectorPrevalence, ChargingCountry, ChargingSupply,
    charging_connector_choices,
};

#[test]
fn local_spot_counts_override_country_order_without_outlet_count_bias() {
    let country = ChargingCountry::new("US").unwrap();
    let mut prevalence = ChargingConnectorPrevalence::new(Some(&country));
    let many_outlets = ChargingSupply::single(None, ChargingConnector::Nema5_15)
        .with_outlet_count(Some(100))
        .unwrap();
    prevalence.record_spot(&[many_outlets.clone(), many_outlets]);
    for _ in 0..2 {
        prevalence.record_spot(&[ChargingSupply::single(None, ChargingConnector::J1772)]);
    }
    let choices = prevalence.into_choices();
    assert_eq!(
        &choices[..2],
        &[ChargingConnector::J1772, ChargingConnector::Nema5_15]
    );
    assert_eq!(
        choices.len(),
        charging_connector_choices(Some(&country)).len()
    );
}

#[test]
fn unknown_and_custom_connectors_do_not_displace_country_fallback() {
    let country = ChargingCountry::new("US").unwrap();
    let mut prevalence = ChargingConnectorPrevalence::new(Some(&country));
    prevalence.record_spot(&[ChargingSupply::single(None, ChargingConnector::Unknown)]);
    assert_eq!(
        prevalence.into_choices(),
        charging_connector_choices(Some(&country))
    );
}

#[test]
fn ties_retain_country_order_and_missing_gps_retains_the_full_catalogue() {
    let mut prevalence = ChargingConnectorPrevalence::new(None);
    prevalence.record_spot(&[
        ChargingSupply::single(None, ChargingConnector::TypeC),
        ChargingSupply::single(None, ChargingConnector::TypeA),
    ]);
    assert_eq!(prevalence.into_choices(), charging_connector_choices(None));
}
