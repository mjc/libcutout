use cutout_core::{
    DeviceCommand, DeviceSettingValue, Measured, ProtocolFamily, RawFieldValue, SettingId,
    SettingsEntry, SettingsReadback, ValueQuality, ValueSource, VerificationStatus,
};
use cutout_protocols::{
    AERO_FIELD_PWM_PERCENT, AERO_FIELD_VOLTAGE_CORRECTION_TENTHS_PERCENT,
    BEGODE_FIELD_LED_AND_LIGHT_MODE, BEGODE_FIELD_POWER_OFF_TIMER_MINUTES,
    BEGODE_FIELD_SETTINGS_BITS, BEGODE_FIELD_TILTBACK_SPEED_KMH, DeviceControlProfile,
    SettingAccess, SettingControl, SettingObservation, SettingUnit,
    VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS, VETERAN_FIELD_CHARGE_MODE,
    VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, aero_control_profile, falcon_control_profile,
};

#[test]
fn legacy_pedal_mode_does_not_confirm_the_modern_aero_riding_preset() {
    let profile = aero_control_profile();
    for raw in [-1, 0, 1, 2, 3, 1920, i64::MAX] {
        assert!(read(profile, cutout_protocols::VETERAN_FIELD_PEDALS_MODE, raw).is_empty());
    }
    let preset = profile
        .descriptors(false)
        .into_iter()
        .find(|descriptor| descriptor.id == SettingId::RidingPreset)
        .unwrap();
    assert_eq!(preset.access, SettingAccess::Writable);
    assert!(!preset.confirmation_supported);
    assert_eq!(preset.write_verification, VerificationStatus::Unverified);
}

fn read(profile: DeviceControlProfile, field: u16, raw: i64) -> Vec<SettingObservation> {
    profile.normalize_readback(SettingsReadback::available([Some(SettingsEntry {
        field: RawFieldValue {
            id: field,
            value: raw,
        },
        source: ValueSource::Reported,
        quality: ValueQuality::Known,
        verification: VerificationStatus::SourceVerified,
    })]))
}

fn value(observations: &[SettingObservation], id: SettingId) -> Option<DeviceSettingValue> {
    observations
        .iter()
        .find(|entry| entry.id == id)
        .unwrap()
        .value
        .map(|value| value.value)
}

#[test]
fn pwm_readback_is_duty_and_readability_does_not_grant_a_write() {
    let profile = aero_control_profile();
    assert_eq!(
        read(profile, AERO_FIELD_PWM_PERCENT, 50)
            .first()
            .and_then(|observation| observation.protocol),
        Some(ProtocolFamily::VeteranLeaperkimNosfet)
    );
    for duty in 0..=100 {
        assert_eq!(
            value(
                &read(profile, AERO_FIELD_PWM_PERCENT, duty),
                SettingId::PwmTiltback
            ),
            Some(DeviceSettingValue::Number(duty as i32))
        );
        assert_eq!(
            profile
                .command(
                    SettingId::PwmTiltback,
                    DeviceSettingValue::Number(duty as i32),
                    true
                )
                .is_ok(),
            duty >= 30
        );
    }
    assert_eq!(
        value(
            &read(profile, AERO_FIELD_PWM_PERCENT, 200),
            SettingId::PwmTiltback
        ),
        Some(DeviceSettingValue::Disabled)
    );
    for raw in [-1, 101, 128, 199, 201, i64::MAX] {
        assert_eq!(
            value(
                &read(profile, AERO_FIELD_PWM_PERCENT, raw),
                SettingId::PwmTiltback
            ),
            None
        );
    }
}

#[test]
fn fractional_speed_is_preserved_without_rounding_into_a_confirmable_request() {
    let profile = aero_control_profile();
    let descriptor = profile
        .descriptors(true)
        .into_iter()
        .find(|entry| entry.id == SettingId::TiltbackSpeed)
        .unwrap();
    assert!(matches!(
        descriptor.control,
        SettingControl::Number {
            precision: 1,
            step: 10,
            ..
        }
    ));
    assert_eq!(
        value(
            &read(profile, VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, 116),
            SettingId::TiltbackSpeed
        ),
        Some(DeviceSettingValue::Number(116))
    );
    assert!(
        profile
            .command(
                SettingId::TiltbackSpeed,
                DeviceSettingValue::Number(116),
                true
            )
            .is_err()
    );
    let command = profile
        .command(
            SettingId::TiltbackSpeed,
            DeviceSettingValue::Number(120),
            true,
        )
        .unwrap();
    assert_eq!(
        command,
        DeviceCommand::SetSetting {
            id: SettingId::TiltbackSpeed,
            value: DeviceSettingValue::Number(120),
        }
    );
    assert_eq!(
        value(
            &read(
                falcon_control_profile(),
                BEGODE_FIELD_TILTBACK_SPEED_KMH,
                48
            ),
            SettingId::MaximumSpeed
        ),
        Some(DeviceSettingValue::Number(480))
    );
    for raw in [-1, 100, i64::MAX] {
        assert_eq!(
            value(
                &read(
                    falcon_control_profile(),
                    BEGODE_FIELD_TILTBACK_SPEED_KMH,
                    raw,
                ),
                SettingId::MaximumSpeed,
            ),
            None
        );
    }
    for raw in [-1, 99, 2_001, i64::MAX] {
        assert_eq!(
            value(
                &read(profile, VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, raw,),
                SettingId::TiltbackSpeed,
            ),
            None
        );
    }
}

#[test]
fn packed_falcon_readback_keeps_semantic_choices_and_unknown_modes_distinct() {
    let profile = falcon_control_profile();
    for bits in 0_u16..=3 {
        let observations = read(
            profile,
            BEGODE_FIELD_SETTINGS_BITS,
            i64::from((bits << 13) | (bits << 7) | (bits << 10)),
        );
        assert_eq!(
            value(&observations, SettingId::PedalMode),
            [Some(2), Some(1), Some(0), None][usize::from(bits)].map(DeviceSettingValue::Choice)
        );
        assert_eq!(
            value(&observations, SettingId::RollAngleMode),
            (bits < 3).then_some(DeviceSettingValue::Choice(bits))
        );
        assert_eq!(
            value(&observations, SettingId::SpeedAlarmMode),
            Some(DeviceSettingValue::Choice(bits))
        );
    }
    for light in 0..=3 {
        let observations = read(profile, BEGODE_FIELD_LED_AND_LIGHT_MODE, 0x0300 | light);
        assert_eq!(
            value(&observations, SettingId::LightingPattern),
            Some(DeviceSettingValue::Choice(3))
        );
        assert_eq!(
            value(&observations, SettingId::Headlight),
            match light {
                0 => Some(DeviceSettingValue::Boolean(false)),
                1 => Some(DeviceSettingValue::Boolean(true)),
                _ => None,
            }
        );
    }
    let observations = read(profile, BEGODE_FIELD_LED_AND_LIGHT_MODE, 0xff01);
    assert_eq!(value(&observations, SettingId::LightingPattern), None);
    assert_eq!(
        value(&observations, SettingId::Headlight),
        Some(DeviceSettingValue::Boolean(true))
    );
}

#[test]
fn normalization_preserves_signed_fixed_point_and_original_evidence() {
    let profile = aero_control_profile();
    for raw in -15..=15 {
        assert_eq!(
            value(
                &read(profile, AERO_FIELD_VOLTAGE_CORRECTION_TENTHS_PERCENT, raw),
                SettingId::VoltageCorrection
            ),
            Some(DeviceSettingValue::Number(raw as i32))
        );
    }
    let measured = Measured {
        value: DeviceSettingValue::Number(80),
        source: ValueSource::Estimated,
        quality: ValueQuality::Inferred,
        verification: VerificationStatus::Unverified,
    };
    let entries = profile.normalize_readback(SettingsReadback::available([Some(SettingsEntry {
        field: RawFieldValue {
            id: AERO_FIELD_PWM_PERCENT,
            value: 80,
        },
        source: measured.source,
        quality: measured.quality,
        verification: measured.verification,
    })]));
    assert_eq!(entries[0].value, Some(measured));
}

#[test]
fn absent_and_unrelated_fields_do_not_manufacture_observations() {
    let profile = aero_control_profile();
    assert!(
        profile
            .normalize_readback(SettingsReadback::unavailable())
            .is_empty()
    );
    assert!(
        profile
            .normalize_readback(SettingsReadback::unsupported())
            .is_empty()
    );
    assert!(read(profile, u16::MAX, 80).is_empty());
    assert!(read(falcon_control_profile(), AERO_FIELD_PWM_PERCENT, 80).is_empty());
    assert!(read(DeviceControlProfile::default(), AERO_FIELD_PWM_PERCENT, 80).is_empty());
}

#[test]
fn passive_garage_rows_keep_exact_meaning_and_cannot_be_submitted() {
    let aero = aero_control_profile();
    let descriptors = aero.descriptors(false);
    let auto_shutdown = descriptors
        .iter()
        .find(|entry| entry.id == SettingId::AutoShutdownRemaining)
        .unwrap();
    assert_eq!(auto_shutdown.access, SettingAccess::ReadOnly);
    assert!(matches!(
        auto_shutdown.control,
        SettingControl::Number {
            unit: SettingUnit::Seconds,
            ..
        }
    ));
    let charge_mode = descriptors
        .iter()
        .find(|entry| entry.id == SettingId::ChargeMode)
        .unwrap();
    assert_eq!(charge_mode.access, SettingAccess::ReadOnly);
    assert_eq!(
        value(
            &read(aero, VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS, 90),
            SettingId::AutoShutdownRemaining,
        ),
        Some(DeviceSettingValue::Number(90))
    );
    assert_eq!(
        value(
            &read(aero, VETERAN_FIELD_CHARGE_MODE, 1),
            SettingId::ChargeMode,
        ),
        Some(DeviceSettingValue::Choice(1))
    );
    assert!(
        aero.command(SettingId::ChargeMode, DeviceSettingValue::Choice(1), true,)
            .is_err()
    );

    let falcon = falcon_control_profile();
    let power_off = falcon
        .descriptors(false)
        .into_iter()
        .find(|entry| entry.id == SettingId::PowerOffDelay)
        .unwrap();
    assert_eq!(power_off.access, SettingAccess::ReadOnly);
    assert!(matches!(
        power_off.control,
        SettingControl::Number {
            unit: SettingUnit::Minutes,
            ..
        }
    ));
    assert_eq!(
        value(
            &read(falcon, BEGODE_FIELD_POWER_OFF_TIMER_MINUTES, 15),
            SettingId::PowerOffDelay,
        ),
        Some(DeviceSettingValue::Number(15))
    );
    assert!(read(aero, BEGODE_FIELD_POWER_OFF_TIMER_MINUTES, 15).is_empty());
    assert!(
        read(
            falcon,
            VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS,
            90,
        )
        .is_empty()
    );
}
