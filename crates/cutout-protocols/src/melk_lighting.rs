//! Candidate ELK-BLEDOM/MELK protocol support for `LotusLamp X` controllers.

use cutout_core::RgbLightingCommand;

/// Candidate ELK-BLEDOM/MELK command encoder.
///
/// The frame templates are derived from the public `elkbledom` Lotus Lamp X
/// profile and still require physical validation against `MELK-OC21`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MelkLightingProfile;

/// Length of a candidate MELK command frame.
pub const MELK_FRAME_LEN: usize = 9;

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
}

#[cfg(test)]
mod tests {
    use super::{MELK_FRAME_LEN, MelkLightingProfile};
    use cutout_core::{LightingBrightness, LightingPowerState, RgbColor, RgbLightingCommand};

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
}
