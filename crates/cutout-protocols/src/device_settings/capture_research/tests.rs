use super::*;
use cutout_core::{
    CaptureObservationSource, CaptureResearchProfile, CaptureResearchProfileSelection,
    CaptureSettingObservationValue, GattChannel, MonotonicTimestamp, WriteMode,
};

// Existing hardware-backed selector-eight fixture from veteran_telemetry's packet tests.
const AERO_SETTINGS: [u8; 75] = hex_literal::hex!(
    "dc5a5c4729f2000000170000ab6c001700000be9\
     045a00000226021ca8f607801b25000080c80000\
     808080808080080000803200364f371e00000100\
     808028062e7964800080801540e23a"
);
// Existing hardware-backed selector-two fixture from veteran_telemetry's tests.
const AERO_LATERAL_SETTINGS: [u8; 87] = hex_literal::hex!(
    "dc5a5c532a7c000000000000ab41001700000cff\
     000000000226021ca8f607801afa000080c80000\
     808080808080022880803080800e310e310e2f0e\
     2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
     310e2e9e05e3ad"
);
// Existing source-backed Falcon Live A/B fixtures from begode_telemetry's tests.
const FALCON_LIVE_A: [u8; 24] =
    hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
const FALCON_LIVE_B: [u8; 24] =
    hex_literal::hex!("55aa000000320000000f003200030502000004185a5a5a5a");

fn inbound(at: u64, channel: GattChannel, bytes: &[u8]) -> PevcapRecord {
    PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(at),
        channel,
        crate::VETERAN_SERVICE_CHANNEL,
        bytes.to_vec(),
    )
}

fn reported(
    id: &str,
    value: CaptureObservedValue,
    unit: Option<&str>,
) -> CaptureSettingObservationValue {
    CaptureSettingObservationValue {
        setting_id: id.into(),
        value,
        unit: unit.map(str::to_owned),
        note: None,
    }
}

fn observation(value: CaptureSettingObservationValue) -> CaptureSettingsObservation {
    CaptureSettingsObservation {
        capture_id: "6b20c1f8-c544-49e5-bc20-247647de00b5".into(),
        profile: CaptureResearchProfile {
            profile_id: "nosfet_aero".into(),
            pev_category: Some("euc".into()),
            model: Some("manually selected for unknown hardware".into()),
            firmware: None,
            selection: CaptureResearchProfileSelection::Manual,
        },
        observed_at_ms: 10,
        recorded_at_ms: 20,
        source: CaptureObservationSource::ManufacturerApp,
        values: vec![value],
        note: None,
    }
}

fn decoded<'a>(readback: &'a CaptureResearchReadback, id: &str) -> &'a CaptureResearchDecodedValue {
    readback
        .values()
        .iter()
        .find(|entry| entry.setting_id == id)
        .unwrap_or_else(|| {
            panic!(
                "missing decoded field {id}; observed values: {:?}",
                readback.values()
            )
        })
}

fn aero_brightness(value: u8) -> Vec<u8> {
    let mut bytes = AERO_SETTINGS.to_vec();
    bytes[55] = value;
    let crc_start = bytes.len() - 4;
    let crc = crc32fast::hash(&bytes[..crc_start]);
    bytes[crc_start..].copy_from_slice(&crc.to_be_bytes());
    bytes
}

#[test]
fn research_catalog_names_read_only_and_model_specific_fields_without_write_permission() {
    let aero = capture_research_catalog("nosfet_aero").unwrap();
    assert_eq!(aero.pev_category, "euc");
    assert_eq!(aero.protocol, ProtocolFamily::VeteranLeaperkimNosfet);
    for id in [
        "display_units",
        "pwm_tiltback",
        "display_brightness",
        "voltage_correction",
        "auto_shutdown_remaining",
        "charge_mode",
        "displayed_pack_voltage",
    ] {
        assert!(aero.fields.iter().any(|field| field.id == id), "{id}");
    }
    for id in [
        "charge_limit_diagnostic",
        "high_speed_mode",
        "low_battery_mode",
        "transport_mode",
    ] {
        assert!(
            aero.fields
                .iter()
                .any(|field| field.id == id && field.research_only),
            "{id}"
        );
    }
    assert!(matches!(
        aero.fields
            .iter()
            .find(|field| field.id == "charge_limit_diagnostic")
            .unwrap()
            .control,
        SettingControl::ReadOnly
    ));
    let falcon = capture_research_catalog("begode_falcon").unwrap();
    assert_eq!(falcon.protocol, ProtocolFamily::BegodeGotway);
    for id in [
        "pedal_mode",
        "roll_angle_mode",
        "speed_alarm_mode",
        "lighting_pattern",
        "power_off_delay",
    ] {
        assert!(falcon.fields.iter().any(|field| field.id == id), "{id}");
    }
    assert!(
        !falcon
            .fields
            .iter()
            .any(|field| field.id == "pedal_hardness")
    );
    assert!(capture_research_catalog("unknown advertisement").is_none());
    assert!(CaptureResearchReadback::new("unknown advertisement", 20).is_none());
}

#[test]
fn native_syntax_checks_preserve_unusual_reported_values_and_manual_context() {
    let original = observation(reported(
        "display_brightness",
        CaptureObservedValue::Entered("125.00".into()),
        Some("%"),
    ));
    assert_eq!(
        validate_capture_research_observation(&original),
        Ok(vec!["display_brightness".into()])
    );
    assert_eq!(
        original.values[0].value,
        CaptureObservedValue::Entered("125.00".into())
    );
    assert_eq!(
        original.profile.selection,
        CaptureResearchProfileSelection::Manual
    );
    assert_eq!(
        original.profile.model.as_deref(),
        Some("manually selected for unknown hardware")
    );
    for text in ["NaN", "inf", "not a number"] {
        let invalid = observation(reported(
            "display_brightness",
            CaptureObservedValue::Entered(text.into()),
            Some("%"),
        ));
        assert_eq!(
            validate_capture_research_observation(&invalid),
            Err(CaptureResearchValueError::InvalidNumber)
        );
    }
    let mut unit_unknown = original.clone();
    unit_unknown.values[0].unit = None;
    assert_eq!(
        validate_capture_research_observation(&unit_unknown),
        Ok(vec![]),
        "unknown units cannot establish an out-of-range value"
    );
    let mut unknown = original;
    unknown.profile.profile_id = "future_device".into();
    assert!(
        unknown.validate().is_ok(),
        "imported profile evidence remains structurally retainable"
    );
    assert_eq!(
        validate_capture_research_observation(&unknown),
        Err(CaptureResearchValueError::UnknownProfile)
    );
    unknown.profile.profile_id = "nosfet_aero".into();
    unknown.values[0].setting_id = "future_field".into();
    assert_eq!(
        validate_capture_research_observation(&unknown),
        Err(CaptureResearchValueError::UnknownField)
    );
}

#[test]
fn aero_readback_uses_packet_times_and_never_treats_outbound_bytes_as_observations() {
    let first = inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS);
    let later_bytes = aero_brightness(40);
    let later = inbound(30, crate::VETERAN_DATA_CHANNEL, &later_bytes);
    let outbound = PevcapRecord::outbound_write(
        MonotonicTimestamp::new(15),
        crate::VETERAN_DATA_CHANNEL,
        WriteMode::WithoutResponse,
        later_bytes,
    );
    let mut before = CaptureResearchReadback::new("nosfet_aero", 20).unwrap();
    assert_eq!(before.values(), &[] as &[CaptureResearchDecodedValue]);
    for record in [&first, &outbound, &later] {
        before.observe(record);
    }
    assert_eq!(
        decoded(&before, "display_brightness").value,
        Some(DeviceSettingValue::Number(30))
    );
    assert_eq!(decoded(&before, "display_brightness").observed_at_ms, 10);
    assert_eq!(
        decoded(&before, "display_units").value,
        Some(DeviceSettingValue::Choice(1))
    );
    assert_eq!(
        decoded(&before, "pwm_tiltback").value,
        Some(DeviceSettingValue::Number(79))
    );
    assert_eq!(
        decoded(&before, "charge_limit_diagnostic").value,
        Some(DeviceSettingValue::Number(46))
    );
    assert_eq!(
        decoded(&before, "high_speed_mode").value,
        None,
        "raw sentinel 128 is an observed unknown, never a guessed false"
    );
    assert_eq!(
        decoded(&before, "displayed_pack_voltage").value,
        Some(DeviceSettingValue::Number(107_380))
    );
    let mut after = CaptureResearchReadback::new("nosfet_aero", 50).unwrap();
    for record in [&first, &outbound, &later] {
        after.observe(record);
    }
    assert_eq!(
        decoded(&after, "display_brightness").value,
        Some(DeviceSettingValue::Number(40))
    );
    assert_eq!(decoded(&after, "display_brightness").observed_at_ms, 30);
    assert!(
        !after
            .values()
            .iter()
            .any(|entry| entry.setting_id == "lateral_tilt_limit"),
        "selector eight does not report selector-two lateral cutoff"
    );
}

#[test]
fn aero_readback_retains_newest_packet_time_when_records_arrive_out_of_order() {
    let mut readback = CaptureResearchReadback::new("nosfet_aero", 30).unwrap();
    readback.observe(&inbound(
        30,
        crate::VETERAN_DATA_CHANNEL,
        &aero_brightness(40),
    ));
    readback.observe(&inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
    assert_eq!(
        decoded(&readback, "display_brightness"),
        &CaptureResearchDecodedValue {
            setting_id: "display_brightness",
            value: Some(DeviceSettingValue::Number(40)),
            observed_at_ms: 30,
        }
    );
}

#[test]
fn aero_readback_uses_later_record_when_packet_times_are_equal() {
    let mut readback = CaptureResearchReadback::new("nosfet_aero", 30).unwrap();
    readback.observe(&inbound(30, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
    readback.observe(&inbound(
        30,
        crate::VETERAN_DATA_CHANNEL,
        &aero_brightness(40),
    ));
    assert_eq!(
        decoded(&readback, "display_brightness"),
        &CaptureResearchDecodedValue {
            setting_id: "display_brightness",
            value: Some(DeviceSettingValue::Number(40)),
            observed_at_ms: 30,
        }
    );
}

#[test]
fn stale_link_boundaries_preserve_newer_aero_readback() {
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(10)),
        PevcapRecord::link_up(MonotonicTimestamp::new(10), None),
    ] {
        let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
        readback.observe(&inbound(
            30,
            crate::VETERAN_DATA_CHANNEL,
            &aero_brightness(40),
        ));
        readback.observe(&boundary);
        assert_eq!(
            decoded(&readback, "display_brightness"),
            &CaptureResearchDecodedValue {
                setting_id: "display_brightness",
                value: Some(DeviceSettingValue::Number(40)),
                observed_at_ms: 30,
            }
        );
    }
}

#[test]
fn stale_link_boundaries_do_not_reset_newer_partial_aero_packet() {
    let bytes = aero_brightness(40);
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(10)),
        PevcapRecord::link_up(MonotonicTimestamp::new(10), None),
    ] {
        let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
        readback.observe(&inbound(30, crate::VETERAN_DATA_CHANNEL, &bytes[..30]));
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
        readback.observe(&boundary);
        readback.observe(&inbound(40, crate::VETERAN_DATA_CHANNEL, &bytes[30..]));
        assert_eq!(
            decoded(&readback, "display_brightness").value,
            Some(DeviceSettingValue::Number(40))
        );
        assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 40);
    }
}

#[test]
fn delayed_pre_boundary_packets_cannot_repopulate_a_new_capture_epoch() {
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(40)),
        PevcapRecord::link_up(MonotonicTimestamp::new(40), None),
    ] {
        let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
        readback.observe(&inbound(20, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
        readback.observe(&boundary);
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
        readback.observe(&inbound(
            30,
            crate::VETERAN_DATA_CHANNEL,
            &aero_brightness(40),
        ));
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
        readback.observe(&inbound(
            50,
            crate::VETERAN_DATA_CHANNEL,
            &aero_brightness(50),
        ));
        assert_eq!(
            decoded(&readback, "display_brightness").value,
            Some(DeviceSettingValue::Number(50))
        );
        assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 50);
    }
}

#[test]
fn equal_timestamp_link_boundaries_follow_storage_order() {
    let mut readback = CaptureResearchReadback::new("nosfet_aero", 30).unwrap();
    readback.observe(&inbound(30, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
    readback.observe(&PevcapRecord::link_down(MonotonicTimestamp::new(30)));
    assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    readback.observe(&inbound(
        30,
        crate::VETERAN_DATA_CHANNEL,
        &aero_brightness(40),
    ));
    assert_eq!(
        decoded(&readback, "display_brightness").value,
        Some(DeviceSettingValue::Number(40))
    );
    readback.observe(&PevcapRecord::link_up(MonotonicTimestamp::new(30), None));
    assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    readback.observe(&inbound(29, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
    assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    readback.observe(&inbound(
        30,
        crate::VETERAN_DATA_CHANNEL,
        &aero_brightness(50),
    ));
    assert_eq!(
        decoded(&readback, "display_brightness").value,
        Some(DeviceSettingValue::Number(50))
    );
    assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 30);
}

#[test]
fn delayed_boundary_invalidates_values_when_decoder_input_spans_epochs() {
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        PevcapRecord::link_up(MonotonicTimestamp::new(20), None),
    ] {
        let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
        readback.observe(&inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
        readback.observe(&inbound(
            30,
            crate::VETERAN_DATA_CHANNEL,
            &AERO_LATERAL_SETTINGS,
        ));
        assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 10);
        readback.observe(&boundary);
        assert_eq!(
            readback
                .values()
                .iter()
                .find(|entry| entry.setting_id == "display_brightness"),
            None
        );
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    }
}

#[test]
fn delayed_boundary_reset_keeps_pre_boundary_packets_unavailable() {
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        PevcapRecord::link_up(MonotonicTimestamp::new(20), None),
    ] {
        let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
        readback.observe(&inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
        readback.observe(&inbound(
            30,
            crate::VETERAN_DATA_CHANNEL,
            &AERO_LATERAL_SETTINGS,
        ));
        readback.observe(&boundary);
        readback.observe(&PevcapRecord::link_down(MonotonicTimestamp::new(5)));
        readback.observe(&inbound(
            15,
            crate::VETERAN_DATA_CHANNEL,
            &aero_brightness(50),
        ));
        assert_eq!(
            readback
                .values()
                .iter()
                .find(|entry| entry.setting_id == "display_brightness"),
            None
        );
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    }
}

#[test]
fn falcon_packet_readback_keeps_independent_field_times_and_resets_on_link_change() {
    assert_eq!(
        crate::begode_falcon_target_voltage_profile(),
        crate::BegodePackVoltageProfile::Begode100VFullCharge,
        "90075 mV is the existing Falcon profile's explicitly selected 100.8 V scaling"
    );
    let mut readback = CaptureResearchReadback::new("begode_falcon", 100).unwrap();
    readback.observe(&inbound(10, crate::BEGODE_DATA_CHANNEL, &FALCON_LIVE_A));
    readback.observe(&inbound(20, crate::BEGODE_DATA_CHANNEL, &FALCON_LIVE_B));
    assert_eq!(
        decoded(&readback, "displayed_pack_voltage").value,
        Some(DeviceSettingValue::Number(90_075))
    );
    assert_eq!(
        decoded(&readback, "displayed_pack_voltage").observed_at_ms,
        10
    );
    assert_eq!(
        decoded(&readback, "beeper_volume_level").value,
        Some(DeviceSettingValue::Number(9))
    );
    assert_eq!(
        decoded(&readback, "power_off_delay").value,
        Some(DeviceSettingValue::Number(15))
    );
    assert_eq!(
        decoded(&readback, "maximum_speed").value,
        Some(DeviceSettingValue::Number(500))
    );
    assert_eq!(decoded(&readback, "maximum_speed").observed_at_ms, 20);
    assert_eq!(
        decoded(&readback, "lighting_pattern").value,
        Some(DeviceSettingValue::Choice(3))
    );
    readback.observe(&PevcapRecord::link_down(MonotonicTimestamp::new(30)));
    assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    readback.observe(&inbound(40, crate::BEGODE_DATA_CHANNEL, &FALCON_LIVE_B));
    assert!(
        !readback
            .values()
            .iter()
            .any(|entry| entry.setting_id == "displayed_pack_voltage")
    );
    assert_eq!(decoded(&readback, "maximum_speed").observed_at_ms, 40);
    readback.observe(&PevcapRecord::link_up(MonotonicTimestamp::new(50), None));
    assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
}

#[test]
fn link_reset_discards_partial_packet_before_the_next_connection() {
    let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
    readback.observe(&inbound(
        10,
        crate::VETERAN_DATA_CHANNEL,
        &AERO_SETTINGS[..30],
    ));
    assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    readback.observe(&PevcapRecord::link_down(MonotonicTimestamp::new(20)));
    readback.observe(&inbound(
        30,
        crate::VETERAN_DATA_CHANNEL,
        &AERO_SETTINGS[30..],
    ));
    assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    readback.observe(&inbound(40, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
    assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 40);
}

#[test]
fn independent_value_comparison_respects_original_units_and_precision() {
    let speed = reported(
        "tiltback_speed",
        CaptureObservedValue::Entered("34.8".into()),
        Some("mph"),
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &speed, DeviceSettingValue::Number(560)),
        Some(false)
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &speed, DeviceSettingValue::Number(540)),
        Some(true)
    );
    let unitless = reported("tiltback_speed", speed.value.clone(), None);
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &unitless, DeviceSettingValue::Number(560)),
        None
    );
    let voltage = reported(
        "displayed_pack_voltage",
        CaptureObservedValue::Entered("108.760".into()),
        Some("V"),
    );
    assert_eq!(
        capture_research_values_differ(
            "nosfet_aero",
            &voltage,
            DeviceSettingValue::Number(108_760)
        ),
        Some(false)
    );
}

#[test]
fn independent_value_comparison_distinguishes_off_zero_false_choices_and_unknown() {
    let off = reported("pwm_tiltback", CaptureObservedValue::Off, Some("%"));
    let zero = reported(
        "pwm_tiltback",
        CaptureObservedValue::Entered("0".into()),
        Some("%"),
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &off, DeviceSettingValue::Disabled),
        Some(false)
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &off, DeviceSettingValue::Number(0)),
        Some(true)
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &zero, DeviceSettingValue::Number(0)),
        Some(false)
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &zero, DeviceSettingValue::Disabled),
        Some(true)
    );
    let invalid_import = reported(
        "pwm_tiltback",
        CaptureObservedValue::Entered("unknown".into()),
        Some("%"),
    );
    assert_eq!(
        capture_research_values_differ(
            "nosfet_aero",
            &invalid_import,
            DeviceSettingValue::Disabled
        ),
        None
    );
    let false_value = reported("transport_mode", CaptureObservedValue::Boolean(false), None);
    assert_eq!(
        capture_research_values_differ(
            "nosfet_aero",
            &false_value,
            DeviceSettingValue::Boolean(false)
        ),
        Some(false)
    );
    assert_eq!(
        capture_research_values_differ(
            "nosfet_aero",
            &false_value,
            DeviceSettingValue::Boolean(true)
        ),
        Some(true)
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &false_value, DeviceSettingValue::Number(0)),
        None
    );
    let unknown = reported("pwm_tiltback", CaptureObservedValue::Unobserved, Some("%"));
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &unknown, DeviceSettingValue::Disabled),
        None
    );
    let units = reported(
        "display_units",
        CaptureObservedValue::Entered("1".into()),
        None,
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &units, DeviceSettingValue::Choice(1)),
        Some(false)
    );
    assert_eq!(
        capture_research_values_differ("nosfet_aero", &units, DeviceSettingValue::Choice(0)),
        Some(true)
    );
}

#[test]
fn explicit_link_up_activates_passive_readback_and_uses_notification_time() {
    let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
    readback.observe(&PevcapRecord::link_up(MonotonicTimestamp::new(5), None));
    assert_eq!(
        readback.values(),
        &[] as &[CaptureResearchDecodedValue],
        "a link does not report any setting"
    );
    readback.observe(&inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
    assert_eq!(
        decoded(&readback, "display_brightness").value,
        Some(DeviceSettingValue::Number(30))
    );
    assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 10);
    readback.observe(&PevcapRecord::link_down(MonotonicTimestamp::new(20)));
    assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
    readback.observe(&PevcapRecord::link_up(MonotonicTimestamp::new(30), None));
    readback.observe(&inbound(
        40,
        crate::VETERAN_DATA_CHANNEL,
        &aero_brightness(40),
    ));
    assert_eq!(
        decoded(&readback, "display_brightness").value,
        Some(DeviceSettingValue::Number(40))
    );
    assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 40);
}

fn assert_batch_matches_independent_cutoffs(
    profile_id: &str,
    cutoffs: &[u64],
    records: &[PevcapRecord],
) {
    let mut batch = CaptureResearchReadbackBatch::new(profile_id, cutoffs).unwrap();
    for record in records {
        batch.observe(record);
    }
    for &cutoff in cutoffs {
        let mut reference = CaptureResearchReadback::new(profile_id, cutoff).unwrap();
        for record in records {
            reference.observe(record);
        }
        assert_eq!(
            batch.values_at(cutoff),
            Some(reference.values()),
            "profile {profile_id}, cutoff {cutoff}"
        );
    }
    assert_eq!(batch.values_at(999), None, "only requested cutoffs exist");
}

#[test]
fn batch_prefixes_match_independent_aero_framing_boundaries_and_equal_times() {
    let records = vec![
        inbound(30, crate::VETERAN_DATA_CHANNEL, &AERO_LATERAL_SETTINGS),
        inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS),
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        inbound(15, crate::VETERAN_DATA_CHANNEL, &aero_brightness(50)),
        inbound(40, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS[..30]),
        PevcapRecord::link_up(MonotonicTimestamp::new(25), None),
        inbound(45, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS[30..]),
        inbound(45, crate::VETERAN_DATA_CHANNEL, &aero_brightness(40)),
        PevcapRecord::outbound_write(
            MonotonicTimestamp::new(46),
            crate::VETERAN_DATA_CHANNEL,
            WriteMode::WithoutResponse,
            aero_brightness(60),
        ),
        inbound(45, crate::VETERAN_DATA_CHANNEL, &aero_brightness(35)),
        PevcapRecord::link_down(MonotonicTimestamp::new(45)),
        inbound(45, crate::VETERAN_DATA_CHANNEL, &aero_brightness(30)),
    ];
    assert_batch_matches_independent_cutoffs(
        "nosfet_aero",
        &[50, 5, 12, 25, 35, 42, 45, 35],
        &records,
    );
}

#[test]
fn batch_prefixes_keep_future_partial_frames_out_of_older_cutoffs() {
    let records = vec![
        inbound(30, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS[..30]),
        inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS),
        inbound(40, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS[30..]),
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        inbound(50, crate::VETERAN_DATA_CHANNEL, &aero_brightness(40)),
    ];
    assert_batch_matches_independent_cutoffs("nosfet_aero", &[12, 25, 35, 45, 55], &records);
}

#[test]
fn batch_prefixes_preserve_falcon_units_context_for_delayed_older_packets() {
    let imperial_live_b = hex_literal::hex!("55aa000000320001000f003200030502000004185a5a5a5a");
    let records = vec![
        inbound(30, crate::BEGODE_DATA_CHANNEL, &imperial_live_b),
        inbound(10, crate::BEGODE_DATA_CHANNEL, &FALCON_LIVE_A),
        inbound(15, crate::BEGODE_DATA_CHANNEL, &FALCON_LIVE_B),
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        inbound(35, crate::BEGODE_DATA_CHANNEL, &FALCON_LIVE_A),
    ];
    assert_batch_matches_independent_cutoffs("begode_falcon", &[12, 18, 25, 32, 40], &records);
}

#[test]
fn ordered_packets_update_one_decoder_prefix_for_all_1024_cutoffs() {
    let cutoffs: Vec<_> = (0..1024).collect();
    let mut batch = CaptureResearchReadbackBatch::new("nosfet_aero", &cutoffs).unwrap();
    for packet in 0..16 {
        batch.observe(&inbound(
            packet * 64,
            crate::VETERAN_DATA_CHANNEL,
            &aero_brightness(u8::try_from(packet + 20).unwrap()),
        ));
    }
    assert_eq!(batch.decoder_updates, 16);
    assert_eq!(batch.groups.len(), 16);
    for cutoff in cutoffs {
        let brightness = batch
            .values_at(cutoff)
            .unwrap()
            .iter()
            .find(|value| value.setting_id == "display_brightness")
            .unwrap();
        assert_eq!(brightness.observed_at_ms, cutoff / 64 * 64);
        assert_eq!(
            brightness.value,
            Some(DeviceSettingValue::Number(
                i32::try_from(cutoff / 64 + 20).unwrap()
            ))
        );
    }
}

#[test]
fn duplicate_cutoffs_share_decoder_state_and_existing_capture_bounds() {
    let mut batch = CaptureResearchReadbackBatch::new("nosfet_aero", &[30, 30, 30]).unwrap();
    batch.observe(&inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
    assert_eq!(batch.decoder_updates, 1);
    assert_eq!(batch.groups.len(), 1);
    assert_eq!(batch.cutoffs, [30]);
    assert!(CaptureResearchReadbackBatch::new("future_profile", &[30]).is_none());
    assert!(CaptureResearchReadbackBatch::new("nosfet_aero", &[]).is_none());
    assert!(
        CaptureResearchReadbackBatch::new(
            "nosfet_aero",
            &[cutout_core::MAX_CAPTURE_SETTINGS_OFFSET_MILLISECONDS + 1]
        )
        .is_none()
    );
    assert!(
        CaptureResearchReadbackBatch::new(
            "nosfet_aero",
            &vec![0; cutout_core::MAX_CAPTURE_SETTINGS_OBSERVATIONS + 1]
        )
        .is_none()
    );
}

#[test]
fn unrelated_settings_remain_outside_available_capture_research_profiles() {
    let unrelated = [
        SettingId::PushbackDutyAngle,
        SettingId::HardwareBeeperEnabled,
        SettingId::MotorDriveCurrentLimit,
        SettingId::ImuSampleRate,
    ];
    for id in unrelated {
        assert_eq!(capture_research_setting_id(id), None);
    }
    assert_eq!(
        capture_research_setting_id(SettingId::DisplayUnits),
        Some("display_units")
    );
    assert_eq!(
        capture_research_setting_id(SettingId::PowerOffDelay),
        Some("power_off_delay")
    );
    for profile in ["nosfet_aero", "begode_falcon"] {
        let catalog = capture_research_catalog(profile).unwrap();
        assert!(
            catalog
                .fields
                .iter()
                .filter_map(|field| field.setting_id)
                .all(|id| !unrelated.contains(&id))
        );
    }
    for profile in ["vesc", "refloat"] {
        assert_eq!(capture_research_catalog(profile), None);
        assert!(CaptureResearchReadback::new(profile, 100).is_none());
    }
}

#[test]
fn delayed_boundary_invalidates_aero_value_completed_from_both_connections() {
    let bytes = aero_brightness(40);
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        PevcapRecord::link_up(MonotonicTimestamp::new(20), None),
    ] {
        let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
        readback.observe(&inbound(10, crate::VETERAN_DATA_CHANNEL, &bytes[..30]));
        readback.observe(&inbound(30, crate::VETERAN_DATA_CHANNEL, &bytes[30..]));
        assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 30);
        assert_eq!(
            decoded(&readback, "display_brightness").value,
            Some(DeviceSettingValue::Number(40))
        );
        readback.observe(&boundary);
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
        readback.observe(&inbound(50, crate::VETERAN_DATA_CHANNEL, &bytes));
        assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 50);
        assert_eq!(
            decoded(&readback, "display_brightness").value,
            Some(DeviceSettingValue::Number(40))
        );
    }
}

#[test]
fn delayed_boundary_discards_aero_partial_frame_with_pre_boundary_input() {
    let bytes = aero_brightness(40);
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        PevcapRecord::link_up(MonotonicTimestamp::new(20), None),
    ] {
        let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
        readback.observe(&inbound(10, crate::VETERAN_DATA_CHANNEL, &bytes[..30]));
        readback.observe(&inbound(30, crate::VETERAN_DATA_CHANNEL, &bytes[30..50]));
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
        readback.observe(&boundary);
        readback.observe(&inbound(40, crate::VETERAN_DATA_CHANNEL, &bytes[50..]));
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
        readback.observe(&inbound(50, crate::VETERAN_DATA_CHANNEL, &bytes));
        assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 50);
    }
}

#[test]
fn delayed_boundary_invalidates_falcon_settings_from_a_cross_connection_frame() {
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        PevcapRecord::link_up(MonotonicTimestamp::new(20), None),
    ] {
        let mut readback = CaptureResearchReadback::new("begode_falcon", 100).unwrap();
        readback.observe(&inbound(
            10,
            crate::BEGODE_DATA_CHANNEL,
            &FALCON_LIVE_B[..12],
        ));
        readback.observe(&inbound(
            30,
            crate::BEGODE_DATA_CHANNEL,
            &FALCON_LIVE_B[12..],
        ));
        assert_eq!(decoded(&readback, "maximum_speed").observed_at_ms, 30);
        assert_eq!(
            decoded(&readback, "maximum_speed").value,
            Some(DeviceSettingValue::Number(500))
        );
        readback.observe(&boundary);
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
        readback.observe(&inbound(50, crate::BEGODE_DATA_CHANNEL, &FALCON_LIVE_B));
        assert_eq!(decoded(&readback, "maximum_speed").observed_at_ms, 50);
    }
}

#[test]
fn delayed_boundary_uses_oldest_input_time_even_when_it_arrived_later() {
    for boundary in [
        PevcapRecord::link_down(MonotonicTimestamp::new(20)),
        PevcapRecord::link_up(MonotonicTimestamp::new(20), None),
    ] {
        let mut readback = CaptureResearchReadback::new("nosfet_aero", 100).unwrap();
        readback.observe(&inbound(
            30,
            crate::VETERAN_DATA_CHANNEL,
            &aero_brightness(40),
        ));
        readback.observe(&inbound(10, crate::VETERAN_DATA_CHANNEL, &AERO_SETTINGS));
        assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 30);
        readback.observe(&boundary);
        assert_eq!(readback.values(), &[] as &[CaptureResearchDecodedValue]);
        readback.observe(&inbound(
            50,
            crate::VETERAN_DATA_CHANNEL,
            &aero_brightness(50),
        ));
        assert_eq!(decoded(&readback, "display_brightness").observed_at_ms, 50);
    }
}
