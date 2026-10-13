use cutout_ride_maps::{CHARGING_OUTLET_CATALOGUE_VERSION, charging_outlet_choices};
use cutout_ride_maps::{
    ChargingConnector, ChargingDetails, ChargingInputError, ChargingPrice, ChargingPriceBasis,
    ChargingSupply, SupplyVoltage,
};
use cutout_ride_maps::{
    ChargingCost, ChargingPaidTerms, ChargingSpot, ChargingSpotId, ChargingVenue, ConnectorLabel,
    Coordinate, NewChargingSpot,
};
use cutout_ride_maps::{ChargingCountry, ChargingPowerDraft, ChargingSupplyKind};
use serde_json::json;

#[test]
fn custom_connector_labels_cannot_alias_catalogue_ids() {
    for label in [" type_c ", "TYPE_C", "J1772", "unknown", "nema_5_15", " \t"] {
        assert_eq!(ConnectorLabel::new(label), Err(ChargingInputError::Text));
        assert!(serde_json::from_value::<ChargingConnector>(json!({"other":label})).is_err());
    }
    let label = ConnectorLabel::new(" Socket X ").expect("custom label");
    assert_eq!(label.as_str(), "Socket X");
    assert_eq!(
        serde_json::from_value::<ChargingConnector>(json!({"other":" Socket X "}))
            .expect("connector"),
        ChargingConnector::Other(label)
    );
}

#[test]
fn national_options_keep_documented_alternatives_and_unknown_fallback() {
    assert_eq!(CHARGING_OUTLET_CATALOGUE_VERSION, 1);
    let germany = ChargingCountry::new("DE").expect("country");
    assert_eq!(
        charging_outlet_choices(Some(&germany)),
        [ChargingConnector::TypeF, ChargingConnector::TypeC]
    );
    let us = ChargingCountry::new("US").expect("country");
    assert_eq!(
        charging_outlet_choices(Some(&us)),
        [ChargingConnector::Nema5_15, ChargingConnector::TypeA]
    );
    let unsupported = ChargingCountry::new("ZZ").expect("code shape");
    assert_eq!(
        charging_outlet_choices(Some(&unsupported)),
        [ChargingConnector::Unknown]
    );
}

#[test]
fn known_voltage_is_bounded_in_whole_supply_volts() {
    for value in [1, 120, 240, 2_000] {
        assert_eq!(SupplyVoltage::new(value).expect("voltage").as_u16(), value);
    }
    for value in [0, 2_001, u16::MAX] {
        assert_eq!(
            SupplyVoltage::new(value),
            Err(ChargingInputError::VoltageOutOfRange)
        );
    }
}

#[test]
fn country_changes_replace_only_suggestions_and_never_confirm_new_values() {
    let us = Some(ChargingCountry::new("us").expect("country"));
    let gb = Some(ChargingCountry::new("GB").expect("country"));
    let initial = ChargingPowerDraft::new(us.clone(), ChargingSupplyKind::Outlet).confirm();
    assert_eq!(initial.clone().with_country(us), initial);
    let replaced = initial.with_country(gb.clone());
    assert_eq!(
        replaced.selection().connectors(),
        [ChargingConnector::TypeG]
    );
    assert_eq!(replaced.confirmed_supply(), None);
    let manual = ChargingSupply::single(
        Some(SupplyVoltage::new(240).expect("voltage")),
        ChargingConnector::Nema14_50,
    );
    let draft = replaced.with_selection(manual.clone());
    assert_eq!(draft.confirmed_supply(), None);
    let confirmed = draft.confirm().with_country(None).with_country(gb);
    assert_eq!(confirmed.confirmed_supply(), Some(&manual));
}

#[test]
fn ev_mode_never_inherits_domestic_defaults_and_replacement_needs_confirmation() {
    let us = Some(ChargingCountry::new("US").expect("country"));
    let ev = ChargingPowerDraft::new(us.clone(), ChargingSupplyKind::EvStation);
    assert_eq!(ev.selection().voltage(), None);
    assert_eq!(ev.selection().connectors(), [ChargingConnector::Unknown]);
    assert_eq!(ev.confirmed_supply(), None);
    let outlet = ev.confirm().with_kind(ChargingSupplyKind::Outlet);
    assert_eq!(
        outlet.selection().voltage().map(SupplyVoltage::as_u16),
        Some(120)
    );
    assert_eq!(outlet.confirmed_supply(), None);
    let manual = ChargingSupply::single(None, ChargingConnector::J1772);
    let selected = outlet
        .with_kind(ChargingSupplyKind::EvStation)
        .with_selection(manual.clone())
        .confirm();
    let updated = selected
        .clone()
        .with_kind(ChargingSupplyKind::EvStation)
        .with_country(Some(ChargingCountry::new("AU").expect("country")));
    assert_eq!(updated.confirmed_supply(), Some(&manual));
    assert_eq!(
        selected.with_country(us).selection().connectors(),
        manual.connectors()
    );
}

#[test]
fn optional_text_and_custom_connectors_are_normalized_and_byte_bounded() {
    assert_eq!(
        ConnectorLabel::new(" \tSocket X \n")
            .expect("label")
            .as_str(),
        "Socket X"
    );
    for label in [
        String::new(),
        "a".repeat(81),
        "é".repeat(41),
        "Socket\0X".to_owned(),
    ] {
        assert_eq!(ConnectorLabel::new(&label), Err(ChargingInputError::Text));
    }
    assert!(ConnectorLabel::new(&"é".repeat(40)).is_ok());
    for name in ["a".repeat(121), "é".repeat(61), "Name\0".to_owned()] {
        assert_eq!(
            ChargingDetails::new(Some(&name), None, None, None, None, vec![]),
            Err(ChargingInputError::Text)
        );
    }
    assert!(
        ChargingDetails::new(
            Some(&"é".repeat(60)),
            None,
            Some(&"a".repeat(500)),
            None,
            None,
            vec![]
        )
        .is_ok()
    );
    assert_eq!(
        ChargingDetails::new(None, None, Some(&"a".repeat(501)), None, None, vec![]),
        Err(ChargingInputError::Text)
    );
}

#[test]
fn a_public_submission_keeps_supply_pairings_and_excludes_draft_context() {
    let pin = Coordinate::from_degrees(40.0, -105.0).expect("pin");
    let first = ChargingSupply::single(
        Some(SupplyVoltage::new(120).expect("voltage")),
        ChargingConnector::Nema5_15,
    );
    let second = ChargingSupply::single(
        Some(SupplyVoltage::new(240).expect("voltage")),
        ChargingConnector::Other(ConnectorLabel::new("Socket X").expect("label")),
    );
    let details = ChargingDetails::new(
        Some(" Cafe "),
        Some(ChargingVenue::Cafe),
        Some(" Open hours "),
        Some(ChargingPaidTerms::PurchaseRequired),
        None,
        vec![first.clone(), second.clone()],
    )
    .expect("details");
    let input = NewChargingSpot::new(pin, " Ask staff. ", ChargingCost::Paid)
        .expect("input")
        .with_details(details)
        .expect("spot");
    let record = ChargingSpot::new(input);
    let restored = ChargingSpot::from_stored(
        ChargingSpotId::parse(&record.id().as_uuid().to_string()).expect("existing identity"),
        record.input().clone(),
        record.provenance(),
    );
    assert_eq!(restored, record);
    assert_ne!(ChargingSpot::new(record.input().clone()).id(), record.id());
    assert_eq!(record.input().details().supplies(), [first, second]);
    let public =
        serde_json::to_value(record.input().community_submission()).expect("public projection");
    assert_eq!(
        public,
        json!({"coordinate":{"latitude_e7":400_000_000,"longitude_e7":-1_050_000_000},"access_note":"Ask staff.","cost":"paid","name":"Cafe","venue":"cafe","hours":"Open hours","paid_terms":"purchase_required","price":null,"supplies":[{"voltage_volts":120,"connectors":["nema_5_15"]},{"voltage_volts":240,"connectors":[{"other":"Socket X"}]}]})
    );
}

#[test]
fn minimum_submission_has_no_power_suggestions_or_optional_requirements() {
    let pin = Coordinate::from_degrees(40.0, -105.0).expect("pin");
    let draft = ChargingPowerDraft::new(
        Some(ChargingCountry::new("US").expect("country")),
        ChargingSupplyKind::Outlet,
    );
    let details = ChargingDetails::new(
        None,
        None,
        None,
        None,
        None,
        draft.confirmed_supply().cloned().into_iter().collect(),
    )
    .expect("omitted power");
    for cost in [ChargingCost::Free, ChargingCost::Paid] {
        let input = NewChargingSpot::new(pin, "Outlet", cost)
            .expect("minimum")
            .with_details(details.clone())
            .expect("valid spot");
        let public = serde_json::to_value(input.community_submission()).expect("payload");
        assert_eq!(public["supplies"], json!([]));
        assert_eq!(public["name"], json!(null));
        assert_eq!(public["venue"], json!(null));
    }
    let paid = ChargingDetails::new(
        None,
        None,
        None,
        Some(ChargingPaidTerms::DirectFee),
        None,
        vec![],
    )
    .expect("optional terms");
    assert_eq!(
        NewChargingSpot::new(pin, "Outlet", ChargingCost::Free)
            .expect("minimum")
            .with_details(paid),
        Err(ChargingInputError::PaidDetailsForFree)
    );
}

#[test]
fn wire_decoding_cannot_bypass_supply_price_or_label_validation() {
    for value in [
        json!({"voltage_volts":0}),
        json!({"voltage_volts":2001}),
        json!({"voltage_volts":1.5}),
        json!({"connectors":["unknown","nacs"]}),
        json!({"connectors":["j1772","j1772"]}),
        json!({"connectors":[{"other":" "}]}),
        json!({"connectors":[],"unexpected":true}),
    ] {
        assert!(serde_json::from_value::<ChargingSupply>(value).is_err());
    }
    let normalized: ChargingSupply =
        serde_json::from_value(json!({"voltage_volts":null,"connectors":[{"other":" Socket X "}]}))
            .expect("validated decode");
    assert_eq!(
        serde_json::to_value(normalized).expect("encode"),
        json!({"voltage_volts":null,"connectors":[{"other":"Socket X"}]})
    );
    assert!(
        serde_json::from_value::<ChargingPrice>(
            json!({"amount_minor":0,"currency":"USD","minor_unit_exponent":2,"basis":"per_charge"})
        )
        .is_err()
    );
    assert!(serde_json::from_value::<ChargingCost>(json!("unknown")).is_err());
    assert!(ChargingSpotId::parse("not-a-uuid").is_err());
    assert_eq!(ChargingCountry::new("gb").expect("country").as_str(), "GB");
    for country in ["", "USA", "1X", "é"] {
        assert_eq!(
            ChargingCountry::new(country),
            Err(ChargingInputError::Country)
        );
    }
}

#[test]
fn country_suggestions_are_drafts_until_explicitly_included() {
    for (country, connector, voltage) in [
        (Some("US"), ChargingConnector::Nema5_15, Some(120)),
        (Some("GB"), ChargingConnector::TypeG, None),
        (Some("DE"), ChargingConnector::TypeF, None),
        (Some("AU"), ChargingConnector::TypeI, None),
        (Some("ZZ"), ChargingConnector::Unknown, None),
        (None, ChargingConnector::Unknown, None),
    ] {
        let context = country.map(|v| ChargingCountry::new(v).expect("country"));
        let draft = ChargingPowerDraft::new(context, ChargingSupplyKind::Outlet);
        assert_eq!(draft.selection().connectors(), [connector]);
        assert_eq!(
            draft.selection().voltage().map(SupplyVoltage::as_u16),
            voltage
        );
        assert_eq!(draft.confirmed_supply(), None);
        let confirmed = draft.confirm();
        let included = confirmed.confirmed_supply().expect("explicit inclusion");
        assert_eq!(included.connectors(), confirmed.selection().connectors());
        assert_eq!(included.voltage(), confirmed.selection().voltage());
    }
}

#[test]
fn paired_supplies_reject_contradictory_or_duplicate_connectors() {
    assert!(ChargingSupply::new(None, vec![]).is_ok());
    assert!(ChargingSupply::new(None, vec![ChargingConnector::Unknown]).is_ok());
    assert_eq!(
        ChargingSupply::new(
            None,
            vec![ChargingConnector::Unknown, ChargingConnector::J1772]
        ),
        Err(ChargingInputError::UnknownConnectorCombined)
    );
    assert_eq!(
        ChargingSupply::new(
            None,
            vec![ChargingConnector::J1772, ChargingConnector::J1772]
        ),
        Err(ChargingInputError::DuplicateConnector)
    );
    assert_eq!(
        ChargingSupply::new(None, vec![ChargingConnector::TypeB; 17]),
        Err(ChargingInputError::TooManyConnectors)
    );
}

#[test]
fn exact_prices_validate_amount_currency_shape_and_decimal_scale() {
    let basis = ChargingPriceBasis::PerKwh;
    let valid = ChargingPrice::new(125, "USD", 2, basis).expect("exact price");
    assert_eq!(valid.amount_minor(), 125);
    assert_eq!(valid.currency(), "USD");
    assert_eq!(valid.minor_unit_exponent(), 2);
    assert_eq!(valid.basis(), basis);
    for (amount, currency, scale) in [
        (0, "USD", 2),
        (9_000_000_000_000_001, "USD", 2),
        (125, "usd", 2),
        (125, "US", 2),
        (125, "US1", 2),
        (125, "USD", 5),
    ] {
        assert_eq!(
            ChargingPrice::new(amount, currency, scale, basis),
            Err(ChargingInputError::Price)
        );
    }
}

#[test]
fn optional_metadata_normalizes_blanks_and_bounds_supply_groups() {
    let details = ChargingDetails::new(Some(" \n "), None, Some("\t"), None, None, vec![])
        .expect("blank optional fields");
    assert_eq!(details.name(), None);
    assert_eq!(details.hours(), None);
    let unknown = ChargingSupply::new(None, vec![]).expect("unknown supply");
    assert_eq!(
        ChargingDetails::new(None, None, None, None, None, vec![unknown; 9]),
        Err(ChargingInputError::TooManySupplies)
    );
}
