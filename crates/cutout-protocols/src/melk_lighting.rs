//! Candidate ELK-BLEDOM/MELK protocol support for the `MELK-OC21` controller.

use cutout_core::{GattChannel, RgbLightingCommand, TransportAction, WriteMode, WritePayload};

/// Candidate ELK-BLEDOM/MELK command encoder.
///
/// The frame templates are derived from the public `elkbledom` profile. The
/// historical official app name `LotusLamp X` is provenance only, not an
/// identity or compatibility signal, and the frames still require physical
/// validation against `MELK-OC21`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MelkLightingProfile;

/// Length of a candidate MELK command frame.
pub const MELK_FRAME_LEN: usize = 9;

/// Observed MELK primary service (`FFF0`).
pub const MELK_SERVICE_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xff, 0xf0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// Observed MELK write-without-response characteristic (`FFF3`).
pub const MELK_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xff, 0xf3, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// Observed MELK notification characteristic (`FFF4`).
pub const MELK_NOTIFY_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xff, 0xf4, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// GATT evidence required before selecting the candidate MELK profile.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MelkGattEvidence {
    /// Observed primary service.
    pub service: Option<GattChannel>,

    /// Observed write characteristic.
    pub write: Option<GattChannel>,

    /// Observed notification characteristic.
    pub notify: Option<GattChannel>,
}

impl MelkGattEvidence {
    /// Returns the complete GATT evidence observed for `MELK-OC21  6A`.
    #[must_use]
    pub const fn observed() -> Self {
        Self {
            service: Some(MELK_SERVICE_CHANNEL),
            write: Some(MELK_WRITE_CHANNEL),
            notify: Some(MELK_NOTIFY_CHANNEL),
        }
    }
}

/// Transport and confirmation policy for candidate MELK writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MelkWritePolicy {
    /// Characteristic receiving command frames.
    pub channel: GattChannel,

    /// GATT write mode for command frames.
    pub mode: WriteMode,

    /// Notification characteristic where confirmation may arrive.
    pub confirmation_channel: GattChannel,

    /// Minimum command interval in milliseconds, when capture evidence exists.
    pub minimum_interval_ms: Option<u16>,
}

impl MelkLightingProfile {
    /// Selects the candidate profile only when family name and GATT evidence agree.
    #[must_use]
    pub fn identify(name: &str, evidence: MelkGattEvidence) -> Option<Self> {
        let name = name.trim();
        let bytes = name.as_bytes();
        let prefix = bytes.get(..4)?;
        if !prefix.eq_ignore_ascii_case(b"MELK")
            || !matches!(bytes.get(4), None | Some(b'-' | b' ' | b'_'))
        {
            return None;
        }
        (evidence == MelkGattEvidence::observed()).then_some(Self)
    }

    /// Returns the candidate write and confirmation policy.
    #[must_use]
    pub const fn write_policy() -> MelkWritePolicy {
        MelkWritePolicy {
            channel: MELK_WRITE_CHANNEL,
            mode: WriteMode::WithoutResponse,
            confirmation_channel: MELK_NOTIFY_CHANNEL,
            minimum_interval_ms: None,
        }
    }

    /// Encodes a typed lighting command as a candidate MELK frame.
    #[must_use]
    pub const fn encode(command: RgbLightingCommand) -> [u8; MELK_FRAME_LEN] {
        match command {
            RgbLightingCommand::SetPower(power) => match power {
                cutout_core::LightingPowerState::On => {
                    [0x7e, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0xef]
                }
                cutout_core::LightingPowerState::Off => {
                    [0x7e, 0x00, 0x04, 0x00, 0x00, 0x00, 0xff, 0x00, 0xef]
                }
            },
            RgbLightingCommand::SetSolidColor(color) => {
                let [red, green, blue] = color.channels();
                [0x7e, 0x00, 0x05, 0x03, red, green, blue, 0x00, 0xef]
            }
            RgbLightingCommand::SetBrightness(brightness) => [
                0x7e,
                0x04,
                0x01,
                brightness.as_percent(),
                0xff,
                0x00,
                0xff,
                0x00,
                0xef,
            ],
        }
    }

    /// Wraps a candidate frame as a bounded no-response write action.
    #[must_use]
    pub fn write_action(command: RgbLightingCommand) -> TransportAction {
        let frame = Self::encode(command);
        let Ok(bytes) = WritePayload::try_from_slice(&frame) else {
            unreachable!("fixed MELK frames fit the bounded transport payload");
        };
        let policy = Self::write_policy();
        TransportAction::Write {
            channel: policy.channel,
            bytes,
            mode: policy.mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MELK_FRAME_LEN, MELK_NOTIFY_CHANNEL, MELK_WRITE_CHANNEL, MelkGattEvidence,
        MelkLightingProfile,
    };
    use cutout_core::{
        LightingBrightness, LightingPowerState, RgbColor, RgbLightingCommand, TransportAction,
        WriteMode,
    };

    #[test]
    fn encodes_public_melk_power_frames() {
        assert_eq!(
            MelkLightingProfile::encode(RgbLightingCommand::SetPower(LightingPowerState::On)),
            [0x7e, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0xef]
        );
        assert_eq!(
            MelkLightingProfile::encode(RgbLightingCommand::SetPower(LightingPowerState::Off)),
            [0x7e, 0x00, 0x04, 0x00, 0x00, 0x00, 0xff, 0x00, 0xef]
        );
    }

    #[test]
    fn encodes_public_melk_color_and_brightness_frames() {
        assert_eq!(
            MelkLightingProfile::encode(RgbLightingCommand::SetSolidColor(RgbColor::new(
                0x12, 0x34, 0x56,
            ))),
            [0x7e, 0x00, 0x05, 0x03, 0x12, 0x34, 0x56, 0x00, 0xef]
        );
        let brightness = LightingBrightness::try_from_percent(42).expect("42 is valid");
        assert_eq!(
            MelkLightingProfile::encode(RgbLightingCommand::SetBrightness(brightness)),
            [0x7e, 0x04, 0x01, 42, 0xff, 0x00, 0xff, 0x00, 0xef]
        );
        assert_eq!(MELK_FRAME_LEN, 9);
    }

    #[test]
    fn writes_candidate_frame_to_melk_write_characteristic_without_response() {
        let action =
            MelkLightingProfile::write_action(RgbLightingCommand::SetPower(LightingPowerState::On));

        let TransportAction::Write {
            channel,
            bytes,
            mode,
        } = action
        else {
            panic!("MELK commands must produce a write action");
        };

        assert_eq!(channel, MELK_WRITE_CHANNEL);
        assert_eq!(mode, WriteMode::WithoutResponse);
        assert_eq!(
            bytes.as_slice(),
            [0x7e, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0xef]
        );
        assert_eq!(bytes.len(), MELK_FRAME_LEN);
        assert!(bytes.is_inline());
    }

    #[test]
    fn selects_melk_only_with_family_name_and_complete_gatt_evidence() {
        let evidence = MelkGattEvidence::observed();

        assert_eq!(
            MelkLightingProfile::identify("MELK-OC21  6A", evidence),
            Some(MelkLightingProfile)
        );
        assert_eq!(
            MelkLightingProfile::identify("Govee_H607C_D635", evidence),
            None
        );
        assert_eq!(
            MelkLightingProfile::identify(
                "MELK-OC21  6A",
                MelkGattEvidence {
                    notify: None,
                    ..evidence
                }
            ),
            None
        );
    }

    #[test]
    fn write_policy_separates_transport_and_confirmation_channels() {
        let policy = MelkLightingProfile::write_policy();

        assert_eq!(policy.channel, MELK_WRITE_CHANNEL);
        assert_eq!(policy.mode, WriteMode::WithoutResponse);
        assert_eq!(policy.confirmation_channel, MELK_NOTIFY_CHANNEL);
        assert_eq!(policy.minimum_interval_ms, None);
    }
}
