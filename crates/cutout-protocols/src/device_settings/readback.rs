//! Shared evidence wrapping for protocol-owned settings readback adapters.

use cutout_core::{
    DeviceSettingValue, Measured, ProtocolFamily, SettingId, SettingsEntry, SettingsReadback,
};

use super::DeviceControlProfile;

/// A field present in a protocol response, with its semantic meaning and evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingObservation {
    /// Protocol family whose field mapping produced this observation.
    pub protocol: Option<ProtocolFamily>,
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
        let Some(protocol) = self.settings_adapter.protocol() else {
            return Vec::new();
        };
        let mut observations = Vec::new();
        for entry in readback.entries().into_iter().flatten() {
            self.settings_adapter
                .normalize_readback(entry, &mut observations);
        }
        let descriptors = self.descriptors(false);
        observations.retain(|entry| descriptors.iter().any(|item| item.id == entry.id));
        for observation in &mut observations {
            observation.protocol = Some(protocol);
        }
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
        protocol: None,
        id,
        value: value.map(|value| Measured {
            value,
            source: entry.source,
            quality: entry.quality,
            verification: entry.verification,
        }),
    });
}
