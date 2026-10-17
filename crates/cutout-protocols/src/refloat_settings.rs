//! Source-pinned Refloat 1.3.0 custom-configuration settings.
//!
//! This codec intentionally supports one complete schema only. It verifies the
//! serialized signature and size before exposing values, then edits a copy of
//! the complete image so every field outside the two-setting catalog survives.
//! The independent default fixture was serialized by Refloat 1.3.0 at source
//! revision `58b3ccb0ba9ab026e86959460e69fb0721cf1f31`; its SHA-256 is
//! `d669664b79b96711c4e1a1ba2550e7b8ac1c60d2eaf7463fbae9f5bf36d5ac19`.

use cutout_core::{DeviceSettingValue, SettingId};

use crate::{SettingControl, SettingUnit};

/// Refloat 1.3.0 serialized custom-config size, including its four-byte signature.
pub(crate) const REFLOAT_CONFIG_LEN: usize = 282;

/// Refloat 1.3.0 generated `REFLOATCONFIG_SIGNATURE`.
pub(crate) const REFLOAT_CONFIG_SIGNATURE: u32 = 421_162_011;

const REFLOAT_CONFIG_SIGNATURE_BYTES: [u8; 4] = REFLOAT_CONFIG_SIGNATURE.to_be_bytes();

// Floaty-style saved/community tunes cover these rider-feel fields. Keep them
// opaque in ordinary settings writes, even though this schema pins offsets.
#[cfg(test)]
const DEFERRED_PITCH_GAIN_OFFSETS: [usize; 2] = [4, 6];
#[cfg(test)]
const DEFERRED_TUNE_OFFSETS: [usize; 2] = [42, 43];
const PWM_TILTBACK_OFFSET: usize = 68;
const TILTBACK_SPEED_OFFSET: usize = 83;

const PWM_TILTBACK_DISABLED: i16 = 1_000;

/// Stable per-protocol field ids for source-pinned Refloat settings.
pub(crate) const fn field_id(id: SettingId) -> Option<u16> {
    match id {
        SettingId::PwmTiltback => Some(0x9005),
        SettingId::TiltbackSpeed => Some(0x9006),
        _ => None,
    }
}

/// Settings whose wire fields and units are pinned by the Refloat 1.3.0 schema.
pub(crate) const fn supported_setting_ids() -> [SettingId; 2] {
    [SettingId::PwmTiltback, SettingId::TiltbackSpeed]
}

/// Returns the bounded control described by Refloat's checked-in XML schema.
pub(crate) fn control(id: SettingId) -> Option<SettingControl> {
    let number = |minimum, maximum, step, precision, unit, can_disable| SettingControl::Number {
        minimum,
        maximum,
        step,
        precision,
        unit,
        can_disable,
    };
    match id {
        SettingId::PwmTiltback => Some(number(0, 99, 1, 0, SettingUnit::PwmDutyPercent, true)),
        SettingId::TiltbackSpeed => Some(number(
            50,
            1_000,
            50,
            1,
            SettingUnit::KilometresPerHour,
            true,
        )),
        _ => None,
    }
}

/// Checks whether a value is a legal source-schema value for this setting.
pub(crate) fn accepts(id: SettingId, value: DeviceSettingValue) -> bool {
    let Some(control) = control(id) else {
        return false;
    };
    match (control, value) {
        (
            SettingControl::Number {
                minimum,
                maximum,
                step,
                ..
            },
            DeviceSettingValue::Number(value),
        ) => (minimum..=maximum).contains(&value) && (value - minimum) % step == 0,
        (
            SettingControl::Number {
                can_disable: true, ..
            },
            DeviceSettingValue::Disabled,
        ) => true,
        _ => false,
    }
}

/// Failure to decode a complete supported Refloat settings image.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum RefloatSettingsError {
    /// Payload length does not match the pinned Refloat 1.3.0 structure.
    #[error("unsupported Refloat custom-config length")]
    UnsupportedLength,
    /// Payload signature does not match the pinned Refloat 1.3.0 schema.
    #[error("unsupported Refloat custom-config signature")]
    UnsupportedSignature,
    /// The setting is outside the source-backed catalog.
    #[error("unsupported Refloat setting")]
    UnsupportedSetting,
    /// The value is outside the source schema's bounds or increment.
    #[error("invalid Refloat setting value")]
    InvalidValue,
}

/// Validated complete Refloat 1.3.0 configuration with opaque bytes retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RefloatSettings {
    bytes: [u8; REFLOAT_CONFIG_LEN],
}

impl RefloatSettings {
    /// Refloat clears only the default marker when persisting a configuration.
    pub(crate) fn is_same_except_default(&self, other: &Self) -> bool {
        self.bytes()
            .iter()
            .zip(other.bytes().iter())
            .enumerate()
            .all(|(index, (expected, observed))| {
                expected == observed
                    || (index == REFLOAT_CONFIG_LEN - 1 && *expected == 1 && *observed == 0)
            })
    }
    /// Validates the exact source-pinned length and schema signature.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, RefloatSettingsError> {
        if bytes.len() != REFLOAT_CONFIG_LEN {
            return Err(RefloatSettingsError::UnsupportedLength);
        }
        let signature = bytes
            .get(..REFLOAT_CONFIG_SIGNATURE_BYTES.len())
            .ok_or(RefloatSettingsError::UnsupportedLength)?;
        if signature != REFLOAT_CONFIG_SIGNATURE_BYTES {
            return Err(RefloatSettingsError::UnsupportedSignature);
        }
        let mut image = [0; REFLOAT_CONFIG_LEN];
        image.copy_from_slice(bytes);
        Ok(Self { bytes: image })
    }

    /// Returns the complete validated config image for a preserving write.
    pub(crate) const fn bytes(&self) -> &[u8; REFLOAT_CONFIG_LEN] {
        &self.bytes
    }

    /// Returns the normalized semantic value for a catalog field.
    pub(crate) fn value(&self, id: SettingId) -> Option<DeviceSettingValue> {
        match id {
            SettingId::PwmTiltback => {
                let raw = read_i16(&self.bytes, PWM_TILTBACK_OFFSET)?;
                if raw == PWM_TILTBACK_DISABLED {
                    Some(DeviceSettingValue::Disabled)
                } else if (0..PWM_TILTBACK_DISABLED).contains(&raw) && raw % 10 == 0 {
                    Some(DeviceSettingValue::Number(i32::from(raw / 10)))
                } else {
                    None
                }
            }
            SettingId::TiltbackSpeed => {
                self.bytes
                    .get(TILTBACK_SPEED_OFFSET)
                    .copied()
                    .and_then(|raw| {
                        if raw == 0 {
                            Some(DeviceSettingValue::Disabled)
                        } else if raw <= 100 {
                            Some(DeviceSettingValue::Number(i32::from(raw) * 10))
                        } else {
                            None
                        }
                    })
            }
            _ => None,
        }
    }

    /// Changes one catalog field while preserving the rest of the complete image.
    pub(crate) fn changed(
        &self,
        id: SettingId,
        value: DeviceSettingValue,
    ) -> Result<Self, RefloatSettingsError> {
        if field_id(id).is_none() {
            return Err(RefloatSettingsError::UnsupportedSetting);
        }
        if !accepts(id, value) {
            return Err(RefloatSettingsError::InvalidValue);
        }
        let mut updated = self.clone();
        match (id, value) {
            (SettingId::PwmTiltback, DeviceSettingValue::Number(value)) => {
                write_i16(&mut updated.bytes, PWM_TILTBACK_OFFSET, value * 10)?;
            }
            (SettingId::PwmTiltback, DeviceSettingValue::Disabled) => {
                write_i16(
                    &mut updated.bytes,
                    PWM_TILTBACK_OFFSET,
                    i32::from(PWM_TILTBACK_DISABLED),
                )?;
            }
            (SettingId::TiltbackSpeed, DeviceSettingValue::Number(value)) => {
                write_u8(&mut updated.bytes, TILTBACK_SPEED_OFFSET, value / 10)?;
            }
            (SettingId::TiltbackSpeed, DeviceSettingValue::Disabled) => {
                write_u8(&mut updated.bytes, TILTBACK_SPEED_OFFSET, 0)?;
            }
            _ => return Err(RefloatSettingsError::InvalidValue),
        }
        Ok(updated)
    }

    /// Builds VESC command payload `[COMM_SET_CUSTOM_CONFIG, 0, config…]`.
    pub(crate) fn write_command_payload(&self) -> [u8; REFLOAT_CONFIG_LEN + 2] {
        let mut payload = [0; REFLOAT_CONFIG_LEN + 2];
        if let Some(command) = payload.get_mut(0) {
            *command = 95;
        }
        if let Some(config_index) = payload.get_mut(1) {
            *config_index = 0;
        }
        if let Some(config) = payload.get_mut(2..) {
            config.copy_from_slice(&self.bytes);
        }
        payload
    }
}

fn read_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    let encoded = bytes.get(offset..offset.checked_add(2)?)?;
    let encoded: [u8; 2] = encoded.try_into().ok()?;
    Some(i16::from_be_bytes(encoded))
}

fn write_i16(bytes: &mut [u8], offset: usize, value: i32) -> Result<(), RefloatSettingsError> {
    let value = i16::try_from(value).map_err(|_| RefloatSettingsError::InvalidValue)?;
    let end = offset
        .checked_add(2)
        .ok_or(RefloatSettingsError::InvalidValue)?;
    let destination = bytes
        .get_mut(offset..end)
        .ok_or(RefloatSettingsError::InvalidValue)?;
    destination.copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn write_u8(bytes: &mut [u8], offset: usize, value: i32) -> Result<(), RefloatSettingsError> {
    let value = u8::try_from(value).map_err(|_| RefloatSettingsError::InvalidValue)?;
    let destination = bytes
        .get_mut(offset)
        .ok_or(RefloatSettingsError::InvalidValue)?;
    *destination = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated by Refloat 1.3.0's pinned C `confparser_set_defaults_refloatconfig`
    // and `confparser_serialize_refloatconfig`, not by this Rust codec.
    const DEFAULT_CONFIG: &[u8; REFLOAT_CONFIG_LEN] =
        include_bytes!("../tests/fixtures/refloat-settings/refloat-1.3.0-default.bin");

    #[test]
    fn generated_default_fixture_decodes_source_backed_settings() {
        assert_eq!(&DEFAULT_CONFIG[..4], &REFLOAT_CONFIG_SIGNATURE_BYTES);
        let settings = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        assert_eq!(
            settings.value(SettingId::PwmTiltback),
            Some(DeviceSettingValue::Number(80))
        );
        assert_eq!(
            settings.value(SettingId::TiltbackSpeed),
            Some(DeviceSettingValue::Disabled)
        );
    }

    #[test]
    fn changed_field_preserves_complete_config_and_encodes_pinned_wire_values() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let changed = original
            .changed(SettingId::PwmTiltback, DeviceSettingValue::Disabled)
            .expect("disabled duty pushback");
        assert_eq!(
            changed.value(SettingId::PwmTiltback),
            Some(DeviceSettingValue::Disabled)
        );
        assert_eq!(read_i16(changed.bytes(), PWM_TILTBACK_OFFSET), Some(1_000));
        for (offset, (before, after)) in original.bytes().iter().zip(changed.bytes()).enumerate() {
            if !(PWM_TILTBACK_OFFSET..PWM_TILTBACK_OFFSET + 2).contains(&offset) {
                assert_eq!(before, after, "untouched config byte at {offset}");
            }
        }

        let changed = original
            .changed(SettingId::TiltbackSpeed, DeviceSettingValue::Number(1_000))
            .expect("100 km/h speed threshold");
        assert_eq!(changed.bytes().get(TILTBACK_SPEED_OFFSET), Some(&100));
        assert_eq!(
            changed.value(SettingId::TiltbackSpeed),
            Some(DeviceSettingValue::Number(1_000))
        );
        let disabled = original
            .changed(SettingId::TiltbackSpeed, DeviceSettingValue::Disabled)
            .expect("zero disables speed pushback");
        assert_eq!(disabled.bytes().get(TILTBACK_SPEED_OFFSET), Some(&0));
        assert_eq!(
            disabled.value(SettingId::TiltbackSpeed),
            Some(DeviceSettingValue::Disabled)
        );

        let changed = original
            .changed(SettingId::PwmTiltback, DeviceSettingValue::Number(81))
            .expect("valid duty pushback");
        for offset in 0..REFLOAT_CONFIG_LEN {
            if !(PWM_TILTBACK_OFFSET..PWM_TILTBACK_OFFSET + 2).contains(&offset) {
                assert_eq!(
                    changed.bytes()[offset],
                    original.bytes()[offset],
                    "all non-duty config bytes, including deferred tune fields, remain unchanged at {offset}"
                );
            }
        }
        for offset in DEFERRED_PITCH_GAIN_OFFSETS {
            assert_eq!(
                &changed.bytes()[offset..offset + 2],
                &original.bytes()[offset..offset + 2]
            );
        }
        for offset in DEFERRED_TUNE_OFFSETS {
            assert_eq!(changed.bytes()[offset], original.bytes()[offset]);
        }
    }

    #[test]
    fn readback_comparison_allows_only_source_defined_default_marker_clear() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let mut saved_bytes = *DEFAULT_CONFIG;
        let marker = REFLOAT_CONFIG_LEN - 1;
        saved_bytes[marker] = 0;
        let saved = RefloatSettings::decode(&saved_bytes).expect("same schema after save");
        assert!(original.is_same_except_default(&saved));

        saved_bytes[PWM_TILTBACK_OFFSET] ^= 1;
        let changed_field = RefloatSettings::decode(&saved_bytes).expect("same schema");
        assert!(!original.is_same_except_default(&changed_field));

        saved_bytes = *DEFAULT_CONFIG;
        saved_bytes[marker] = 2;
        let unexpected_marker = RefloatSettings::decode(&saved_bytes).expect("same schema");
        assert!(!original.is_same_except_default(&unexpected_marker));
    }

    #[test]
    fn validation_rejects_other_schemas_and_out_of_schema_values() {
        let mut bad_signature = *DEFAULT_CONFIG;
        if let Some(first) = bad_signature.get_mut(0) {
            *first ^= 0xff;
        }
        assert_eq!(
            RefloatSettings::decode(&bad_signature),
            Err(RefloatSettingsError::UnsupportedSignature)
        );
        assert_eq!(
            RefloatSettings::decode(&DEFAULT_CONFIG[..REFLOAT_CONFIG_LEN - 1]),
            Err(RefloatSettingsError::UnsupportedLength)
        );

        let settings = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        assert_eq!(
            settings.changed(SettingId::PwmTiltback, DeviceSettingValue::Number(100)),
            Err(RefloatSettingsError::InvalidValue)
        );
        assert_eq!(
            settings.changed(SettingId::TiltbackSpeed, DeviceSettingValue::Number(1_025)),
            Err(RefloatSettingsError::InvalidValue)
        );
        assert_eq!(
            settings.changed(SettingId::TiltbackSpeed, DeviceSettingValue::Number(0)),
            Err(RefloatSettingsError::InvalidValue)
        );
        assert!(!accepts(
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(100)
        ));
        assert!(accepts(
            SettingId::PwmTiltback,
            DeviceSettingValue::Disabled
        ));
        assert!(accepts(
            SettingId::TiltbackSpeed,
            DeviceSettingValue::Disabled
        ));
    }

    #[test]
    fn observations_preserve_exact_in_range_values_off_the_edit_increment() {
        let mut off_step = *DEFAULT_CONFIG;
        if let Some(speed) = off_step.get_mut(TILTBACK_SPEED_OFFSET) {
            *speed = 97;
        }
        let settings = RefloatSettings::decode(&off_step).expect("same recognized schema");
        assert_eq!(
            settings.value(SettingId::TiltbackSpeed),
            Some(DeviceSettingValue::Number(970))
        );

        let mut invalid_duty = *DEFAULT_CONFIG;
        write_i16(&mut invalid_duty, PWM_TILTBACK_OFFSET, 805)
            .expect("fixture field is within the image");
        let settings = RefloatSettings::decode(&invalid_duty)
            .expect("off-step value remains valid opaque config bytes");
        assert_eq!(settings.value(SettingId::PwmTiltback), None);
    }

    #[test]
    fn write_payload_uses_custom_config_command_and_index_zero() {
        let settings = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let payload = settings.write_command_payload();
        assert_eq!(payload.get(..2), Some([95_u8, 0].as_slice()));
        assert_eq!(payload.get(2..), Some(settings.bytes().as_slice()));
    }
}
