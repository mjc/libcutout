use super::{
    MobileBluetoothUuid, MobileMelkLightingError, MobileMelkLightingGattEvidence,
    MobileMelkLightingProfile, MobileMelkLightingRestoreDecisionKindDto,
    MobileMelkLightingRestoreMarker, MobileMelkLightingRestoreStateDto,
    MobileMelkLightingWriteModeDto, MobileRgbLightingAccessoryRecord,
    MobileRgbLightingConfirmationStateDto, MobileRgbLightingConnectionStateDto,
    MobileRgbLightingProfileKindDto, MobileRgbLightingRecordError,
    mobile_melk_lighting_capabilities, mobile_melk_lighting_capabilities_fingerprint,
    mobile_melk_lighting_pattern_catalog, mobile_melk_lighting_profile_version,
};
use cutout_protocols::{MELK_NOTIFY_CHANNEL, MELK_WRITE_CHANNEL};

#[test]
fn mobile_bluetooth_uuid_round_trips_the_core_uuid_without_bytes_or_text() {
    let mobile_uuid = MobileBluetoothUuid::from(MELK_WRITE_CHANNEL.as_uuid());

    assert_eq!(mobile_uuid.most_significant_bits, 0x0000_fff3_0000_1000);
    assert_eq!(mobile_uuid.least_significant_bits, 0x8000_0080_5f9b_34fb);
    assert_eq!(uuid::Uuid::from(mobile_uuid), MELK_WRITE_CHANNEL.as_uuid());
}

fn observed_profile() -> std::sync::Arc<MobileMelkLightingProfile> {
    MobileMelkLightingProfile::new(
        "MELK-OC21  6A".to_owned(),
        MobileMelkLightingGattEvidence {
            service_present: true,
            write_without_response: true,
            notify_or_indicate: true,
        },
    )
    .expect("observed MELK evidence should select the profile")
}

#[test]
fn mobile_capabilities_match_the_melk_evidence_record() {
    let capabilities = mobile_melk_lighting_capabilities();
    assert_eq!(
        capabilities.verified_effect_ids,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 16, 22, 75]
    );
    assert!(!capabilities.controller_microphone);
    assert!(!capabilities.schedules);
    assert!(!capabilities.addressable_zones);
    assert!(!capabilities.scenes);
}

#[test]
fn profile_version_is_owned_by_rust() {
    assert_eq!(mobile_melk_lighting_profile_version(), 1);
}

#[test]
fn profile_catalog_and_fingerprint_are_owned_by_rust() {
    let catalog = mobile_melk_lighting_pattern_catalog();
    assert_eq!(catalog.len(), 228);
    assert_eq!(catalog[1].name, "Magic Forward");
    assert!(catalog[1].verified);
    assert!(!catalog[11].verified);
    assert_eq!(
        mobile_melk_lighting_capabilities_fingerprint(),
        "1,2,3,4,5,6,7,8,9,10,16,22,75|0|0|0|0"
    );
}

#[test]
fn profile_requires_name_and_complete_gatt_evidence() {
    let result = MobileMelkLightingProfile::new(
        "MELK-OC21  6A".to_owned(),
        MobileMelkLightingGattEvidence {
            service_present: true,
            write_without_response: true,
            notify_or_indicate: false,
        },
    );
    assert!(matches!(
        result,
        Err(MobileMelkLightingError::InvalidGattEvidence)
    ));

    let result = MobileMelkLightingProfile::new(
        "Govee_H607C_D635".to_owned(),
        MobileMelkLightingGattEvidence {
            service_present: true,
            write_without_response: true,
            notify_or_indicate: true,
        },
    );
    assert!(matches!(
        result,
        Err(MobileMelkLightingError::InvalidGattEvidence)
    ));
}

#[test]
fn writes_include_transport_and_confirmation_policy() {
    let profile = observed_profile();
    let write = profile.set_power(true);

    assert_eq!(write.characteristic, MELK_WRITE_CHANNEL.as_uuid().into());
    assert_eq!(
        write.confirmation_characteristic,
        MELK_NOTIFY_CHANNEL.as_uuid().into()
    );
    assert_eq!(write.mode, MobileMelkLightingWriteModeDto::WithoutResponse);
    assert_eq!(write.minimum_interval_ms, Some(50));
    assert_eq!(write.payload, [0x7e, 0x00, 0x04, 0x01, 0, 0, 0, 0, 0xef]);
}
#[test]
fn initialization_writes_use_the_melk_write_channel() {
    let writes = observed_profile().initialization();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].payload, [0x7e, 0x07, 0x83]);
    assert_eq!(writes[1].payload, [0x7e, 0x04, 0x04]);
    assert!(
        writes
            .iter()
            .all(|write| write.characteristic == MELK_WRITE_CHANNEL.as_uuid().into())
    );
    assert!(
        writes
            .iter()
            .all(|write| write.mode == MobileMelkLightingWriteModeDto::WithoutResponse)
    );
}

#[test]
fn brightness_rejects_values_outside_the_protocol_range() {
    let profile = observed_profile();
    assert_eq!(
        profile.set_brightness(101),
        Err(MobileMelkLightingError::InvalidBrightness)
    );
}

#[test]
fn restore_marker_requires_opt_in_and_same_platform_identity() {
    let marker = MobileMelkLightingRestoreMarker::new(
        "melk-1".to_owned(),
        MobileMelkLightingRestoreStateDto {
            power_on: true,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 42,
            playback: None,
        },
    )
    .expect("bounded restore state should be accepted");

    assert_eq!(
        marker.recover("melk-1".to_owned(), false).kind,
        MobileMelkLightingRestoreDecisionKindDto::Disabled
    );
    assert_eq!(
        marker.recover("melk-2".to_owned(), true).kind,
        MobileMelkLightingRestoreDecisionKindDto::DifferentAccessory
    );
    let decision = marker.recover("melk-1".to_owned(), true);
    assert_eq!(
        decision.kind,
        MobileMelkLightingRestoreDecisionKindDto::Restore
    );
    assert_eq!(
        decision.requested,
        Some(MobileMelkLightingRestoreStateDto {
            power_on: true,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 42,
            playback: None,
        })
    );
}

#[test]
fn persisted_rgb_record_round_trips_through_mobile_boundary() {
    let record = MobileRgbLightingAccessoryRecord::new(
        "melk-1".to_owned(),
        MobileRgbLightingProfileKindDto::MelkOc21,
        1,
    )
    .expect("record should be constructible");
    assert_eq!(record.profile(), MobileRgbLightingProfileKindDto::MelkOc21);
    assert_eq!(record.profile_version(), 1);
    record
        .set_alias(Some("Aero LEDs".to_owned()))
        .expect("alias should be valid");
    record
        .set_vehicle_identifier(Some("euc-1".to_owned()))
        .expect("association should be valid");
    let state = MobileMelkLightingRestoreStateDto {
        power_on: true,
        red: 1,
        green: 2,
        blue: 3,
        brightness: 42,
        playback: None,
    };
    record
        .set_requested_state(Some(state))
        .expect("requested state should be valid");
    record
        .set_confirmed_state(Some(state))
        .expect("confirmed state should be valid");
    record.set_confirmation(MobileRgbLightingConfirmationStateDto::Confirmed);
    assert_eq!(record.last_schedule_confirmation(), None);
    record.set_last_schedule_confirmation(MobileRgbLightingConfirmationStateDto::Unconfirmed);
    record.set_connection(MobileRgbLightingConnectionStateDto::Ready);
    record.set_restore_enabled(true);
    record
        .add_preset("Cruise".to_owned(), state)
        .expect("preset should be valid");

    let restored =
        MobileRgbLightingAccessoryRecord::decode(record.encode().expect("record should encode"))
            .expect("record should decode");
    assert_eq!(restored.platform_identifier(), "melk-1");
    assert_eq!(restored.alias().as_deref(), Some("Aero LEDs"));
    assert_eq!(restored.vehicle_identifier().as_deref(), Some("euc-1"));
    assert_eq!(restored.requested_state(), Some(state));
    assert_eq!(restored.confirmed_state(), Some(state));
    assert_eq!(
        restored.confirmation(),
        MobileRgbLightingConfirmationStateDto::Confirmed
    );
    assert_eq!(
        restored.last_schedule_confirmation(),
        Some(MobileRgbLightingConfirmationStateDto::Unconfirmed)
    );
    assert_eq!(
        restored.connection(),
        MobileRgbLightingConnectionStateDto::Ready
    );
    assert!(restored.restore_enabled());
    assert_eq!(restored.presets().len(), 1);
    assert!(
        restored
            .replace_preset("Cruise".to_owned(), state)
            .expect("preset replacement should validate")
    );
    assert!(restored.remove_preset("Cruise".to_owned()));
    assert!(!restored.remove_preset("Cruise".to_owned()));
}

#[test]
fn persisted_rgb_record_refuses_invalid_brightness_without_mutation() {
    let record = MobileRgbLightingAccessoryRecord::new(
        "melk-1".to_owned(),
        MobileRgbLightingProfileKindDto::MelkOc21,
        1,
    )
    .expect("record should be constructible");
    let invalid = MobileMelkLightingRestoreStateDto {
        power_on: true,
        red: 1,
        green: 2,
        blue: 3,
        brightness: 101,
        playback: None,
    };
    assert_eq!(
        record.set_requested_state(Some(invalid)),
        Err(MobileRgbLightingRecordError::InvalidState)
    );
    assert_eq!(record.requested_state(), None);
}
