//! Application operations for a verified MELK lighting session.

use cutout_core::{TransportAction, WriteMode};
use cutout_protocols::MelkLightingProfile;
use uuid::Uuid;

pub mod session;

/// Transport mode selected by the protocol profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightingWriteMode {
    /// Submit the write without waiting for a transport response.
    WithoutResponse,
}

/// One bounded protocol write and its confirmation policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LightingWrite {
    /// Characteristic receiving the command frame.
    pub characteristic: Uuid,
    /// Protocol frame emitted by the selected profile.
    pub payload: Vec<u8>,
    /// Required transport mode.
    pub mode: LightingWriteMode,
    /// Characteristic whose notifications may confirm the command.
    pub confirmation_characteristic: Uuid,
    /// Minimum command interval when capture evidence supplies one.
    pub minimum_interval_ms: Option<u16>,
}

/// Converts one typed protocol effect to the portable application write contract.
#[must_use]
pub fn transport_action(action: TransportAction) -> LightingWrite {
    let TransportAction::Write {
        channel,
        bytes,
        mode,
    } = action
    else {
        unreachable!("MELK lighting commands always produce writes")
    };
    let policy = MelkLightingProfile::write_policy();
    LightingWrite {
        characteristic: channel.as_uuid(),
        payload: bytes.as_slice().to_vec(),
        mode: match mode {
            WriteMode::WithoutResponse => LightingWriteMode::WithoutResponse,
            WriteMode::WithResponse => unreachable!("MELK policy uses write without response"),
        },
        confirmation_characteristic: policy.confirmation_channel.as_uuid(),
        minimum_interval_ms: policy.minimum_interval_ms,
    }
}
