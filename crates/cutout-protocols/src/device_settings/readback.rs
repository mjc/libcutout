//! Shared evidence wrapping for protocol-owned settings readback adapters.

use cutout_core::{DeviceSettingValue, Measured, SettingId, SettingsEntry, SettingsReadback};

use super::DeviceControlProfile;

/// A field present in a protocol response, with its semantic meaning and evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingObservation {
    /// Stable setting identity.
    pub id: SettingId,
    /// Original evidence, or explicit unknown when the present field is unusable.
    /// A field absent from the response produces no observation at all.
    pub value: Option<Measured<DeviceSettingValue>>,
}

impl DeviceControlProfile {
    /// Converts decoded protocol fields without granting writes or fabricating evidence.
    #[must_use]
    pub fn normalize_readback(self, readback: SettingsReadback) -> Vec<SettingObservation> {
        let mut observations = Vec::new();
        for entry in readback.entries().into_iter().flatten() {
            self.settings_adapter
                .normalize_readback(entry, &mut observations);
        }
        let descriptors = self.descriptors(false);
        observations.retain(|entry| descriptors.iter().any(|item| item.id == entry.id));
        observations
    }
}

pub(super) fn push(
    observations: &mut Vec<SettingObservation>,
    entry: SettingsEntry,
    id: SettingId,
    value: Option<DeviceSettingValue>,
) {
    observations.push(SettingObservation {
        id,
        value: value.map(|value| Measured {
            value,
            source: entry.source,
            quality: entry.quality,
            verification: entry.verification,
        }),
    });
}
