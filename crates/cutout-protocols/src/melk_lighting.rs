//! Candidate ELK-BLEDOM/MELK protocol support for `LotusLamp X` controllers.

use cutout_core::{GattChannel, RgbLightingCommand, TransportAction, WriteMode, WritePayload};

/// Candidate ELK-BLEDOM/MELK command encoder.
///
/// The frame templates are derived from the public `elkbledom` Lotus Lamp X
/// profile and still require physical validation against `MELK-OC21`.
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

impl MelkLightingProfile {
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
        TransportAction::Write {
            channel: MELK_WRITE_CHANNEL,
            bytes,
            mode: WriteMode::WithoutResponse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MELK_FRAME_LEN, MELK_WRITE_CHANNEL, MelkLightingProfile};
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
}
