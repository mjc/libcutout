use cutout_core::{
    DeviceActionId, DeviceActionProgress, DeviceCommand, RawFieldValue, SettingsEntry,
    SettingsReadback, ValueQuality, ValueSource, VerificationStatus,
};
use cutout_protocols::{
    AERO_FIELD_GYRO_CALIBRATION_STATE, ActionAccess, ActionConfirmation, ActionRole,
    DeviceControlProfile, aero_control_profile, falcon_control_profile,
};

fn gyro_readback(raw: i64) -> SettingsReadback {
    SettingsReadback::available([Some(SettingsEntry {
        field: RawFieldValue {
            id: AERO_FIELD_GYRO_CALIBRATION_STATE,
            value: raw,
        },
        source: ValueSource::Reported,
        quality: ValueQuality::Known,
        verification: VerificationStatus::SourceVerified,
    })])
}

#[test]
fn aero_actions_are_typed_and_remain_unverified_outside_validation_mode() {
    let profile = aero_control_profile();
    let actions = profile.action_descriptors(false);
    assert_eq!(actions.len(), 2);
    let reset = actions
        .iter()
        .find(|action| action.id == DeviceActionId::ResetTripMeter)
        .unwrap();
    assert_eq!(reset.label_key, "actions.trip_meter_reset.label");
    assert_eq!(reset.help_key, "actions.trip_meter_reset.help");
    assert_eq!(reset.role, ActionRole::Destructive);
    assert_eq!(reset.confirmation, ActionConfirmation::None);
    assert_eq!(reset.access, ActionAccess::Unverified);

    let gyro = actions
        .iter()
        .find(|action| action.id == DeviceActionId::GyroCalibration)
        .unwrap();
    assert_eq!(gyro.label_key, "actions.gyro_calibration.label");
    assert_eq!(gyro.help_key, "actions.gyro_calibration.help");
    assert_eq!(gyro.role, ActionRole::Procedure);
    assert_eq!(gyro.confirmation, ActionConfirmation::ProgressReadback);
    assert_eq!(gyro.access, ActionAccess::Unverified);

    assert_eq!(
        profile.action_command(DeviceActionId::ResetTripMeter, false),
        Err(cutout_protocols::ActionRequestError::Unverified)
    );
    assert_eq!(
        profile.action_command(DeviceActionId::GyroCalibration, true),
        Ok(DeviceCommand::SetAeroGyroCalibration)
    );
    assert!(falcon_control_profile().action_descriptors(true).is_empty());
    assert!(
        DeviceControlProfile::default()
            .action_descriptors(true)
            .is_empty()
    );
}

#[test]
fn gyro_progress_preserves_evidence_and_explicit_unknown() {
    let profile = aero_control_profile();
    for (raw, progress) in [
        (0, None),
        (1, Some(DeviceActionProgress::AdjustingAttitude)),
        (2, Some(DeviceActionProgress::ReadyToCalibrate)),
        (3, None),
        (128, None),
    ] {
        let observations = profile.normalize_action_readback(gyro_readback(raw));
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].id, DeviceActionId::GyroCalibration);
        assert_eq!(observations[0].progress.map(|entry| entry.value), progress);
        if let Some(measured) = observations[0].progress {
            assert_eq!(measured.source, ValueSource::Reported);
            assert_eq!(measured.quality, ValueQuality::Known);
            assert_eq!(measured.verification, VerificationStatus::SourceVerified);
        }
    }
    assert!(
        profile
            .normalize_action_readback(SettingsReadback::unavailable())
            .is_empty()
    );
}
