//! Candidate ELK-BLEDOM/MELK protocol support for the `MELK-OC21` controller.

use cutout_core::{
    GattChannel, LightingPlayback, MelkControl, RgbLightingCommand, RgbLightingRequestedState,
    TransportAction, WriteMode, WritePayload,
};

/// Candidate ELK-BLEDOM/MELK command encoder.
///
/// The frame templates are derived from the public `elkbledom` profile. The
/// historical official app name `LotusLamp X` is provenance only, not an
/// identity or compatibility signal, and the frames still require physical
/// validation against `MELK-OC21`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MelkLightingProfile;

/// Candidate nine-byte effect command examples retained for every effect currently exposed by
/// the MELK-OC21 UI. These encode the public MELK template; they are not traffic captures and do
/// not replace exact-controller capture evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MelkLightingEffectFixture {
    pub id: u8,
    pub frame: [u8; MELK_FRAME_LEN],
}

/// Reference effect grouping used by clients to present the controller catalog.
///
/// The IDs and grouping are protocol metadata, not SwiftUI state. Names remain reference labels
/// until each controller firmware mapping has physical evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MelkLightingEffectGroup {
    /// Human-readable reference group name.
    pub name: &'static str,
    /// Effect IDs in this group.
    pub ids: &'static [u8],
}

/// User-observed capabilities for the current MELK-OC21 profile.
///
/// The protocol encoder can represent additional reference commands, but only these
/// capabilities have physical evidence for this controller so far.
///
/// IDs 1-10 are enabled from the user's first ten-device trial; names remain
/// reference-catalog labels until each visual mapping is independently matched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "these independent capability flags mirror protocol evidence"
)]
pub struct MelkLightingCapabilities {
    /// Effect IDs observed working on the user's controller.
    pub verified_effect_ids: &'static [u8],
    /// Whether controller-local microphone modes have been physically verified.
    pub controller_microphone: bool,
    /// Whether controller-local schedules have been physically verified.
    pub schedules: bool,
    /// Whether independently addressable zones have been physically verified.
    pub addressable_zones: bool,
    /// Whether named scenes are supported by the controller profile.
    pub scenes: bool,
}

impl MelkLightingCapabilities {
    /// Conservative MELK-OC21 capability evidence.
    #[must_use]
    pub const fn melk_oc21() -> Self {
        Self {
            verified_effect_ids: &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 16, 22, 75],
            controller_microphone: false,
            schedules: false,
            addressable_zones: false,
            scenes: false,
        }
    }

    /// Returns bounded candidate OC21 effect command examples for diagnostics and tests.
    #[must_use]
    pub const fn effect_fixtures() -> &'static [MelkLightingEffectFixture] {
        &MELK_OC21_EFFECT_FIXTURES
    }

    /// Returns whether the effect ID is enabled from the current OC21 evidence record.
    #[must_use]
    pub fn supports_effect(self, pattern: u8) -> bool {
        self.verified_effect_ids.contains(&pattern)
            && Self::effect_fixtures()
                .iter()
                .any(|fixture| fixture.id == pattern)
    }
}

const fn candidate_fixture(id: u8) -> MelkLightingEffectFixture {
    MelkLightingEffectFixture {
        id,
        frame: [0x7e, 0x05, 0x03, id, 0x06, 0xff, 0xff, 0x00, 0xef],
    }
}

const MELK_OC21_EFFECT_FIXTURES: [MelkLightingEffectFixture; 13] = [
    candidate_fixture(1),
    candidate_fixture(2),
    candidate_fixture(3),
    candidate_fixture(4),
    candidate_fixture(5),
    candidate_fixture(6),
    candidate_fixture(7),
    candidate_fixture(8),
    candidate_fixture(9),
    candidate_fixture(10),
    candidate_fixture(16),
    candidate_fixture(22),
    candidate_fixture(75),
];

const BASIC_EFFECT_IDS: [u8; 46] = [
    1, 2, 212, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209,
    210, 211, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 181, 182, 183, 184, 185, 186, 187,
    188, 189, 190, 191, 192,
];
const CURTAIN_EFFECT_IDS: [u8; 20] = [
    57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76,
];
const TRANS_EFFECT_IDS: [u8; 20] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
];
const WATER_EFFECT_IDS: [u8; 18] = [
    39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56,
];
const FLOW_EFFECT_IDS: [u8; 24] = [
    143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161,
    162, 163, 164, 165, 166,
];
const TAIL_EFFECT_IDS: [u8; 16] = [
    23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
];
const RUN_EFFECT_IDS: [u8; 34] = [
    89, 91, 93, 95, 97, 99, 101, 103, 105, 107, 109, 111, 113, 115, 117, 119, 121, 123, 125, 127,
    129, 131, 133, 135, 137, 139, 141, 167, 169, 171, 173, 175, 177, 179,
];
const RUN_BACK_EFFECT_IDS: [u8; 34] = [
    90, 92, 94, 96, 98, 100, 102, 104, 106, 108, 110, 112, 114, 116, 118, 120, 122, 124, 126, 128,
    130, 132, 134, 136, 138, 140, 142, 168, 170, 172, 174, 176, 178, 180,
];
const UNMAPPED_EFFECT_IDS: [u8; 16] = [
    0, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227,
];

const MELK_EFFECT_GROUPS: [MelkLightingEffectGroup; 9] = [
    MelkLightingEffectGroup {
        name: "Basic",
        ids: &BASIC_EFFECT_IDS,
    },
    MelkLightingEffectGroup {
        name: "Curtain",
        ids: &CURTAIN_EFFECT_IDS,
    },
    MelkLightingEffectGroup {
        name: "Trans",
        ids: &TRANS_EFFECT_IDS,
    },
    MelkLightingEffectGroup {
        name: "Water",
        ids: &WATER_EFFECT_IDS,
    },
    MelkLightingEffectGroup {
        name: "Flow",
        ids: &FLOW_EFFECT_IDS,
    },
    MelkLightingEffectGroup {
        name: "Tail",
        ids: &TAIL_EFFECT_IDS,
    },
    MelkLightingEffectGroup {
        name: "Run",
        ids: &RUN_EFFECT_IDS,
    },
    MelkLightingEffectGroup {
        name: "Run Back",
        ids: &RUN_BACK_EFFECT_IDS,
    },
    MelkLightingEffectGroup {
        name: "Unmapped",
        ids: &UNMAPPED_EFFECT_IDS,
    },
];

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
    /// Returns the current evidence-record capabilities for the Aero-installed MELK-OC21.
    #[must_use]
    pub const fn capabilities() -> MelkLightingCapabilities {
        MelkLightingCapabilities::melk_oc21()
    }

    /// Returns the Rust-owned reference effect grouping for mobile clients.
    #[must_use]
    pub const fn effect_groups() -> &'static [MelkLightingEffectGroup] {
        &MELK_EFFECT_GROUPS
    }
    /// Selects the candidate profile only when family name and GATT evidence agree.
    #[must_use]
    pub fn identify(name: &str, evidence: MelkGattEvidence) -> Option<Self> {
        let name = name.trim();
        let bytes = name.as_bytes();
        let model = bytes.get(..9)?;
        if !model.eq_ignore_ascii_case(b"MELK-OC21")
            || !matches!(bytes.get(9), None | Some(b' ' | b'\t'))
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
            minimum_interval_ms: Some(50),
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

    /// Encodes bounded effect, microphone, clock, and scheduler commands from the reference.
    /// Hardware output remains separately observed; no status-query write is sent.
    #[must_use]
    pub const fn encode_control(command: MelkControl) -> [u8; MELK_FRAME_LEN] {
        use cutout_core::LightingPowerState;
        match command {
            MelkControl::Pattern(pattern) => [0x7e, 5, 3, pattern.value(), 6, 0xff, 0xff, 0, 0xef],
            MelkControl::Speed(speed) => [0x7e, 4, 2, speed, 0xff, 0xff, 0xff, 0, 0xef],
            MelkControl::MusicEffect(effect) => {
                [0x7e, 5, 3, 0x80 + effect.value(), 4, 0xff, 0xff, 0, 0xef]
            }
            MelkControl::Microphone(enabled) => {
                [0x7e, 4, 7, enabled as u8, 0xff, 0xff, 0xff, 0, 0xef]
            }
            MelkControl::Sensitivity(sensitivity) => {
                [0x7e, 4, 6, sensitivity.value(), 0xff, 0xff, 0xff, 0, 0xef]
            }
            MelkControl::Schedule(schedule) => {
                let (power, hour, minute, days, enabled) = schedule.components();
                let slot = match power {
                    LightingPowerState::On => 0,
                    LightingPowerState::Off => 1,
                };
                [
                    0x7e,
                    0,
                    0x82,
                    hour,
                    minute,
                    0,
                    slot,
                    days | ((enabled as u8) << 7),
                    0xef,
                ]
            }
            MelkControl::Clock(clock) => {
                let (hour, minute, second, weekday) = clock.components();
                [0x7e, 0, 0x83, hour, minute, second, weekday, 0, 0xef]
            }
        }
    }

    /// Plans the ordered writes needed to restore one requested controller state.
    ///
    /// Brightness is applied before power so restoring an off state cannot finish lit. Music
    /// playback emits its sensitivity/effect/microphone sequence; solid and effect playback do
    /// not claim microphone state because OC21 readback is unavailable.
    #[must_use]
    pub fn plan_state(state: RgbLightingRequestedState) -> Vec<TransportAction> {
        let mut actions = match state.playback() {
            LightingPlayback::Solid => vec![Self::write_action(RgbLightingCommand::SetSolidColor(
                state.color(),
            ))],
            LightingPlayback::Effect { pattern, speed } => vec![
                Self::control_action(MelkControl::Pattern(pattern)),
                Self::control_action(MelkControl::Speed(speed)),
            ],
            LightingPlayback::Music {
                effect,
                sensitivity,
            } => vec![
                Self::control_action(MelkControl::Sensitivity(sensitivity)),
                Self::control_action(MelkControl::MusicEffect(effect)),
                Self::control_action(MelkControl::Microphone(true)),
            ],
        };
        actions.push(Self::write_action(RgbLightingCommand::SetBrightness(
            state.brightness(),
        )));
        actions.push(Self::write_action(RgbLightingCommand::SetPower(
            state.power(),
        )));
        actions
    }

    /// Wraps an advanced MELK control as a bounded no-response write action.
    #[must_use]
    pub fn control_action(command: MelkControl) -> TransportAction {
        Self::write_frame(Self::encode_control(command))
    }

    /// Wraps a basic lighting command as a bounded no-response write action.
    #[must_use]
    pub fn write_action(command: RgbLightingCommand) -> TransportAction {
        Self::write_frame(Self::encode(command))
    }
    /// Returns the MELK initialization sequence required by some firmware revisions.
    ///
    /// The public ELK-BLEDOM integration documents these two login frames for MELK devices.
    /// They are sent once after the FFF3 write characteristic is discovered and before user
    /// commands are admitted.
    #[must_use]
    pub fn initialization_actions() -> [TransportAction; 2] {
        [
            Self::write_bytes(&[0x7e, 0x07, 0x83]),
            Self::write_bytes(&[0x7e, 0x04, 0x04]),
        ]
    }

    fn write_bytes(bytes: &[u8]) -> TransportAction {
        let Ok(bytes) = WritePayload::try_from_slice(bytes) else {
            unreachable!("fixed MELK initialization frames fit the bounded transport payload");
        };
        let policy = Self::write_policy();
        TransportAction::Write {
            channel: policy.channel,
            bytes,
            mode: policy.mode,
        }
    }

    fn write_frame(frame: [u8; MELK_FRAME_LEN]) -> TransportAction {
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
        MelkLightingCapabilities, MelkLightingProfile,
    };
    use cutout_core::{
        LightingBrightness, LightingPlayback, LightingPowerState, RgbColor, RgbLightingCommand,
        RgbLightingRequestedState, TransportAction, WriteMode,
    };

    fn payload(action: &TransportAction) -> &[u8] {
        match action {
            TransportAction::Write { bytes, .. } => bytes.as_slice(),
            _ => panic!("MELK state plans contain only writes"),
        }
    }

    #[test]
    fn plans_effect_state_in_protocol_order() {
        let brightness = LightingBrightness::try_from_percent(42).expect("bounded brightness");
        let state = RgbLightingRequestedState::new(
            LightingPowerState::Off,
            RgbColor::new(1, 2, 3),
            brightness,
        )
        .with_playback(LightingPlayback::Effect {
            pattern: 16.try_into().expect("bounded pattern"),
            speed: 200,
        });

        let actions = MelkLightingProfile::plan_state(state);

        assert_eq!(actions.len(), 4);
        assert_eq!(payload(&actions[0]), [0x7e, 5, 3, 16, 6, 255, 255, 0, 0xef]);
        assert_eq!(
            payload(&actions[1]),
            [0x7e, 4, 2, 200, 255, 255, 255, 0, 0xef]
        );
        assert_eq!(payload(&actions[2]), [0x7e, 4, 1, 42, 255, 0, 255, 0, 0xef]);
        assert_eq!(payload(&actions[3]), [0x7e, 0, 4, 0, 0, 0, 255, 0, 0xef]);
    }

    #[test]
    fn encodes_extended_melk_controls_and_bounds() {
        use cutout_core::{MelkClock, MelkControl, MelkMusicEffect, MelkPattern, MelkSchedule};
        let frame = MelkLightingProfile::encode_control;
        assert_eq!(
            frame(MelkControl::Pattern(MelkPattern::try_from(220).unwrap())),
            [0x7e, 0x05, 0x03, 220, 0x06, 0xff, 0xff, 0x00, 0xef]
        );
        assert_eq!(
            frame(MelkControl::Speed(255)),
            [0x7e, 0x04, 0x02, 255, 0xff, 0xff, 0xff, 0x00, 0xef]
        );
        assert_eq!(
            frame(MelkControl::MusicEffect(
                MelkMusicEffect::try_from(7).unwrap()
            )),
            [0x7e, 0x05, 0x03, 0x87, 0x04, 0xff, 0xff, 0x00, 0xef]
        );
        assert_eq!(
            frame(MelkControl::Microphone(true)),
            [0x7e, 0x04, 0x07, 1, 0xff, 0xff, 0xff, 0x00, 0xef]
        );
        assert_eq!(
            frame(MelkControl::Microphone(false)),
            [0x7e, 0x04, 0x07, 0, 0xff, 0xff, 0xff, 0x00, 0xef]
        );
        assert_eq!(
            frame(MelkControl::Sensitivity(42.try_into().unwrap())),
            [0x7e, 0x04, 0x06, 42, 0xff, 0xff, 0xff, 0x00, 0xef]
        );
        assert_eq!(
            frame(MelkControl::Schedule(
                MelkSchedule::new(LightingPowerState::On, 6, 30, 0x1f, true).unwrap()
            )),
            [0x7e, 0, 0x82, 6, 30, 0, 0, 0x9f, 0xef]
        );
        assert_eq!(
            frame(MelkControl::Schedule(
                MelkSchedule::new(LightingPowerState::Off, 22, 15, 0x60, false).unwrap()
            )),
            [0x7e, 0, 0x82, 22, 15, 0, 1, 0x60, 0xef]
        );
        assert_eq!(
            frame(MelkControl::Clock(MelkClock::new(12, 34, 56, 7).unwrap())),
            [0x7e, 0, 0x83, 12, 34, 56, 7, 0, 0xef]
        );
        assert!(MelkPattern::try_from(228).is_err());
        assert!(MelkMusicEffect::try_from(8).is_err());
        assert!(cutout_core::MelkSensitivity::try_from(101).is_err());
        for (hour, minute, days) in [(24, 0, 0), (0, 60, 0), (0, 0, 128)] {
            assert!(MelkSchedule::new(LightingPowerState::On, hour, minute, days, true).is_err());
        }
        for (hour, minute, second, day) in [
            (24, 0, 0, 1),
            (0, 60, 0, 1),
            (0, 0, 60, 1),
            (0, 0, 0, 0),
            (0, 0, 0, 8),
        ] {
            assert!(MelkClock::new(hour, minute, second, day).is_err());
        }
    }

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
    fn every_enabled_effect_has_a_bounded_candidate_command() {
        let capabilities = MelkLightingProfile::capabilities();
        let fixtures = MelkLightingCapabilities::effect_fixtures();
        assert_eq!(
            capabilities.verified_effect_ids,
            fixtures
                .iter()
                .map(|fixture| fixture.id)
                .collect::<Vec<_>>()
                .as_slice()
        );
        for fixture in fixtures {
            assert_eq!(
                fixture.frame,
                MelkLightingProfile::encode_control(cutout_core::MelkControl::Pattern(
                    fixture.id.try_into().expect("fixture ID is bounded"),
                ))
            );
        }
    }

    #[test]
    fn effect_groups_are_rust_owned_and_cover_reference_catalog() {
        let groups = MelkLightingProfile::effect_groups();
        assert_eq!(groups.len(), 9);
        assert_eq!(groups[0].name, "Basic");
        assert_eq!(groups[0].ids.first(), Some(&1));
        assert_eq!(groups[7].name, "Run Back");
        let ids = groups
            .iter()
            .flat_map(|group| group.ids)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 228);
        assert_eq!(
            ids.iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            228
        );
        let mut sorted = ids;
        sorted.sort_unstable();
        assert_eq!(sorted, (0..=227).map(|id| id as u8).collect::<Vec<_>>());
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
    fn initialization_actions_emit_the_documented_login_sequence() {
        let actions = MelkLightingProfile::initialization_actions();
        assert_eq!(payload(&actions[0]), [0x7e, 0x07, 0x83]);
        assert_eq!(payload(&actions[1]), [0x7e, 0x04, 0x04]);
        for action in actions {
            let TransportAction::Write { channel, mode, .. } = action else {
                panic!("initialization must produce writes");
            };
            assert_eq!(channel, MELK_WRITE_CHANNEL);
            assert_eq!(mode, WriteMode::WithoutResponse);
        }
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
    fn rejects_unknown_melk_models_even_with_matching_gatt() {
        assert_eq!(
            MelkLightingProfile::identify("MELK-OC99  6A", MelkGattEvidence::observed()),
            None
        );
    }

    #[test]
    fn write_policy_separates_transport_and_confirmation_channels() {
        let policy = MelkLightingProfile::write_policy();

        assert_eq!(policy.channel, MELK_WRITE_CHANNEL);
        assert_eq!(policy.mode, WriteMode::WithoutResponse);
        assert_eq!(policy.confirmation_channel, MELK_NOTIFY_CHANNEL);
        assert_eq!(policy.minimum_interval_ms, Some(50));
    }

    #[test]
    fn capabilities_are_conservative_and_capture_backed() {
        let capabilities = MelkLightingProfile::capabilities();

        assert_eq!(
            capabilities.verified_effect_ids,
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 16, 22, 75]
        );
        assert!(capabilities.supports_effect(1));
        assert!(capabilities.supports_effect(16));
        assert!(capabilities.supports_effect(22));
        assert!(capabilities.supports_effect(75));
        assert!(capabilities.supports_effect(2));
        assert!(!capabilities.supports_effect(212));
        assert!(!capabilities.controller_microphone);
        assert!(!capabilities.schedules);
        assert!(!capabilities.addressable_zones);
        assert!(!capabilities.scenes);
    }
}
