//! Complete, schema-checked configuration objects used by one VESC write procedure.

use cutout_core::{DeviceSettingValue, SettingId};

use crate::refloat_settings::RefloatSettings;
use crate::vesc_configuration::{VescConfiguration, VescConfigurationKind};

/// Distinct reply domains; an answer for one object cannot settle another read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigurationKind {
    Refloat,
    Motor,
    App,
}

impl ConfigurationKind {
    pub(crate) const ALL: [Self; 3] = [Self::Refloat, Self::Motor, Self::App];

    pub(crate) fn for_setting(id: SettingId) -> Option<Self> {
        if crate::refloat_settings::field_id(id).is_some() {
            Some(Self::Refloat)
        } else {
            crate::vesc_configuration::kind_for_setting(id).map(|kind| match kind {
                VescConfigurationKind::Motor => Self::Motor,
                VescConfigurationKind::App => Self::App,
            })
        }
    }

    pub(crate) const fn read_command(self) -> &'static [u8] {
        match self {
            Self::Refloat => &[93, 0],
            Self::Motor => VescConfigurationKind::Motor.read_command_payload(),
            Self::App => VescConfigurationKind::App.read_command_payload(),
        }
    }
}

#[derive(Clone, Debug)]
// Refloat's bounded fixed image avoids heap allocation through the write path.
#[allow(clippy::large_enum_variant)]
pub(crate) enum SettingsImage {
    Refloat(RefloatSettings),
    Firmware(VescConfiguration),
}

impl SettingsImage {
    pub(crate) fn kind(&self) -> ConfigurationKind {
        match self {
            Self::Refloat(_) => ConfigurationKind::Refloat,
            Self::Firmware(image) => match image.kind() {
                VescConfigurationKind::Motor => ConfigurationKind::Motor,
                VescConfigurationKind::App => ConfigurationKind::App,
            },
        }
    }

    pub(crate) fn ids(&self) -> &'static [SettingId] {
        match self {
            Self::Refloat(_) => crate::refloat_settings::supported_setting_ids(),
            Self::Firmware(image) => crate::vesc_configuration::supported_setting_ids(image.kind()),
        }
    }

    pub(crate) fn field_id(&self, id: SettingId) -> Option<u16> {
        match self {
            Self::Refloat(_) => crate::refloat_settings::field_id(id),
            Self::Firmware(_) => crate::vesc_configuration::field_id(id),
        }
    }

    pub(crate) fn value(&self, id: SettingId) -> Option<DeviceSettingValue> {
        match self {
            Self::Refloat(image) => image.value(id),
            Self::Firmware(image) => image.value(id),
        }
    }

    pub(crate) fn changed(&self, id: SettingId, value: DeviceSettingValue) -> Option<Self> {
        match self {
            Self::Refloat(image) => image.changed(id, value).ok().map(Self::Refloat),
            Self::Firmware(image) => image.changed(id, value).ok().map(Self::Firmware),
        }
    }

    pub(crate) fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Refloat(expected), Self::Refloat(observed)) => {
                expected.is_same_except_default(observed)
            }
            (Self::Firmware(expected), Self::Firmware(observed)) => {
                expected.is_same_image(observed)
            }
            _ => false,
        }
    }

    pub(crate) fn read_command_payload(&self) -> &'static [u8] {
        match self {
            Self::Refloat(_) => ConfigurationKind::Refloat.read_command(),
            Self::Firmware(image) => image.read_command_payload(),
        }
    }

    pub(crate) fn write_command_payload(&self) -> Vec<u8> {
        match self {
            Self::Refloat(image) => image.write_command_payload().to_vec(),
            Self::Firmware(image) => image.write_command_payload(),
        }
    }
}
