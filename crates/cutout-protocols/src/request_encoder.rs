use arrayvec::ArrayVec;
#[cfg(test)]
use crc32fast::hash as crc32;
use cutout_core::{
    CommandKind, DeviceActionId, DeviceActionStep, DeviceCommand, DeviceSettingValue, LightState,
    PedalMode, PendingProbe, RequestKey, RequestTarget, RollAngle, SettingId, SpeedAlarmMode,
    VescControllerId, WriteMode, WritePayload,
};

use crate::control_wire::{BegodeWire, NosfetWire, VeteranWire, WireCommand};
use crate::settings_wire::*;
use crate::{
    AeroProbe, FalconProbe, RefloatReadOnlyRequest, VescCanReadOnlyRequest, VescReadOnlyCodec,
    VescReadOnlyRequest,
};

/// Bounded encoded request payload plus correlation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRequest<P> {
    /// Family-specific probe represented by this request.
    pub probe: P,

    /// Generic command kind used for scheduler and response correlation.
    pub command: CommandKind,

    /// Bounded request bytes.
    pub payload: WritePayload,

    /// GATT write mode required by this request.
    pub mode: WriteMode,
}

/// One bounded, non-mutating Begode identification request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedIdentificationProbe {
    /// Response correlated with this request.
    pub probe: PendingProbe,

    /// Bounded request bytes.
    pub payload: WritePayload,

    /// GATT write mode required by this request.
    pub mode: WriteMode,
}

/// Bounded encoded benign-control write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedControl {
    /// Generic command kind represented by this write.
    pub command: CommandKind,

    /// Bounded command bytes.
    pub payload: WritePayload,

    /// GATT write mode required by this command.
    pub mode: WriteMode,
}

/// One delayed write in a multi-step settings command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedControlStep {
    /// Delay after the previous step before this write is sent.
    pub delay_ms: u64,

    /// Bounded command bytes.
    pub payload: WritePayload,

    /// GATT write mode required by this command.
    pub mode: WriteMode,
}

/// Ordered, delayed Begode `W` submenu writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedControlSequence {
    /// Generic command kind represented by this sequence.
    pub command: CommandKind,

    /// Writes in send order, including the immediate first write.
    pub steps: ArrayVec<EncodedControlStep, 5>,
}

/// NOSFET control dialect of the community-named Veteran protocol.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NosfetDialect;

impl crate::control_wire::Dialect for NosfetDialect {
    type Protocol = crate::VeteranProtocol;
    fn select(
        command: DeviceCommand,
        context: crate::session::ControlEncodingContext,
    ) -> Option<crate::control_wire::Selection<crate::VeteranProtocol>> {
        let mode = match context {
            crate::session::ControlEncodingContext::Veteran(mode) => mode,
            crate::session::ControlEncodingContext::Default => VeteranCommandMode::Binary,
        };
        Self::select_in_mode(command, mode)
    }
}

/// NOSFET command representation selected from complete telemetry packets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VeteranCommandMode {
    /// Legacy string commands without a checksum.
    Ascii,
    /// Modern binary commands with a trailing CRC32.
    Binary,
}

/// Connection-scoped detector matching the official two-packet command-mode selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VeteranCommandModeDetector {
    previous: Option<VeteranCommandMode>,
    latched: Option<VeteranCommandMode>,
}

#[derive(Clone, Copy, Debug)]
enum NosfetCommand {
    DisplayBacklight(VeteranDisplayBacklight),
    BeeperVolume(VeteranBeeperVolume),
    DynamicAssist(VeteranDynamicAssist),
    PedalDipCompensation(VeteranPedalDipCompensation),
    LateralTiltLimit(VeteranLateralTiltLimit),
    VoltageCorrection(VeteranVoltageCorrection),
    TiltbackSpeed(VeteranSpeedSetting),
    PwmPercent(VeteranPwmSetting),
    GyroCalibration,
    BrakeOverpressureAlarm(VeteranBrakeOverpressureAlarm),
    PedalHardness(VeteranPedalHardness),
    WheelUnits(VeteranWheelUnits),
    HighSpeedMode(VeteranHighSpeedMode),
    LowBatteryMode(VeteranLowBatteryMode),
    TransportMode(VeteranTransportMode),
    AlarmSpeed(VeteranSpeedSetting),
    AngleAdjustment(VeteranAngleAdjustment),
    RidingMode(VeteranRidingMode),
}

fn nosfet_command(command: DeviceCommand) -> Option<NosfetCommand> {
    let DeviceCommand::SetSetting { id, value } = command else {
        if let DeviceCommand::InvokeAction(request) = command
            && request.id == DeviceActionId::GyroCalibration
            && matches!(
                request.step,
                DeviceActionStep::Invoke
                    | DeviceActionStep::PrepareGyroCalibration
                    | DeviceActionStep::StartGyroCalibration
            )
        {
            return Some(NosfetCommand::GyroCalibration);
        }
        return None;
    };
    let number = |value| u8::try_from(value).ok();
    match (id, value) {
        (SettingId::DisplayBrightness, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::DisplayBacklight(VeteranDisplayBacklight::new(number(value)?)?),
        ),
        (SettingId::BeeperVolumePercent, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::BeeperVolume(VeteranBeeperVolume::new(number(value)?)?),
        ),
        (SettingId::DynamicAssist, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::DynamicAssist(VeteranDynamicAssist::new(number(value)?)?),
        ),
        (SettingId::PedalDipCompensation, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::PedalDipCompensation(VeteranPedalDipCompensation::new(number(value)?)?),
        ),
        (SettingId::LateralTiltLimit, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::LateralTiltLimit(VeteranLateralTiltLimit::new(number(value)?)?),
        ),
        (SettingId::VoltageCorrection, DeviceSettingValue::Number(value)) => {
            Some(NosfetCommand::VoltageCorrection(
                VeteranVoltageCorrection::new(i8::try_from(value).ok()?)?,
            ))
        }
        (SettingId::TiltbackSpeed, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::TiltbackSpeed(VeteranSpeedSetting::new(exact_deci_kmh(value)?)?),
        ),
        (SettingId::PwmTiltback, DeviceSettingValue::Number(value)) => {
            Some(NosfetCommand::PwmPercent(
                VeteranPwmPercent::new(100_u8.checked_sub(number(value)?)?)?.into(),
            ))
        }
        (SettingId::PwmTiltback, DeviceSettingValue::Disabled) => {
            Some(NosfetCommand::PwmPercent(VeteranPwmSetting::Off))
        }
        (SettingId::BrakeOverpressureAlarm, DeviceSettingValue::Number(value)) => {
            Some(NosfetCommand::BrakeOverpressureAlarm(
                VeteranBrakeOverpressureAlarm::new(number(value)?)?,
            ))
        }
        (SettingId::PedalHardness, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::PedalHardness(VeteranPedalHardness::new(number(value)?)?),
        ),
        (SettingId::DisplayUnits, DeviceSettingValue::Choice(0)) => {
            Some(NosfetCommand::WheelUnits(VeteranWheelUnits::Metric))
        }
        (SettingId::DisplayUnits, DeviceSettingValue::Choice(1)) => {
            Some(NosfetCommand::WheelUnits(VeteranWheelUnits::Imperial))
        }
        (SettingId::HighSpeedMode, DeviceSettingValue::Boolean(value)) => Some(
            NosfetCommand::HighSpeedMode(VeteranHighSpeedMode::new(value)),
        ),
        (SettingId::LowBatteryMode, DeviceSettingValue::Boolean(value)) => Some(
            NosfetCommand::LowBatteryMode(VeteranLowBatteryMode::new(value)),
        ),
        (SettingId::TransportMode, DeviceSettingValue::Boolean(value)) => Some(
            NosfetCommand::TransportMode(VeteranTransportMode::new(value)),
        ),
        (SettingId::SpeedAlarmThreshold, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::AlarmSpeed(VeteranSpeedSetting::new(exact_deci_kmh(value)?)?),
        ),
        (SettingId::PedalAngle, DeviceSettingValue::Number(value)) => Some(
            NosfetCommand::AngleAdjustment(VeteranAngleAdjustment::new(i8::try_from(value).ok()?)?),
        ),
        (SettingId::RidingPreset, DeviceSettingValue::Choice(0)) => {
            Some(NosfetCommand::RidingMode(VeteranRidingMode::Hard))
        }
        (SettingId::RidingPreset, DeviceSettingValue::Choice(1)) => {
            Some(NosfetCommand::RidingMode(VeteranRidingMode::Medium))
        }
        (SettingId::RidingPreset, DeviceSettingValue::Choice(2)) => {
            Some(NosfetCommand::RidingMode(VeteranRidingMode::Soft))
        }
        _ => None,
    }
}

/// Converts the canonical deci-km/h setting without silently changing its value.
fn exact_deci_kmh(value: i32) -> Option<u8> {
    (value >= 0 && value % 10 == 0)
        .then(|| u8::try_from(value / 10).ok())
        .flatten()
}

impl VeteranCommandModeDetector {
    /// Returns the selected mode. The official client starts in binary mode before detection.
    #[must_use]
    pub fn mode(self) -> VeteranCommandMode {
        self.latched.unwrap_or(VeteranCommandMode::Binary)
    }

    /// Returns whether two consecutive complete packets selected a mode.
    #[must_use]
    pub const fn is_latched(self) -> bool {
        self.latched.is_some()
    }

    /// Observes one complete assembled packet length and latches two matching classifications.
    pub fn observe_complete_packet_len(&mut self, packet_len: usize) {
        if packet_len < 36 || self.latched.is_some() {
            return;
        }
        let observed = if packet_len < 47 {
            VeteranCommandMode::Ascii
        } else {
            VeteranCommandMode::Binary
        };
        if self.previous == Some(observed) {
            self.latched = Some(observed);
        } else {
            self.previous = Some(observed);
        }
    }

    /// Clears all connection-scoped evidence.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl NosfetDialect {
    /// Encodes a supported control in the NOSFET dialect of Veteran.
    #[must_use]
    pub fn encode(command: DeviceCommand) -> Option<EncodedControl> {
        Self::encode_in_mode(command, VeteranCommandMode::Binary)
    }

    /// Encodes using the connection-selected Veteran command representation.
    #[must_use]
    pub fn encode_in_mode(
        command: DeviceCommand,
        mode: VeteranCommandMode,
    ) -> Option<EncodedControl> {
        Self::select_in_mode(command, mode)?.single(command)
    }

    fn select_in_mode(
        command: DeviceCommand,
        mode: VeteranCommandMode,
    ) -> Option<crate::control_wire::Selection<crate::VeteranProtocol>> {
        if let Some(selection) = veteran_mode_control_payload(command, mode) {
            return Some(selection);
        }
        Some(match nosfet_command(command)? {
            NosfetCommand::DisplayBacklight(value) => {
                VeteranWire::DisplayBrightness.select(value.percent())
            }
            NosfetCommand::BeeperVolume(value) => VeteranWire::BeeperVolume.select(value.percent()),
            NosfetCommand::DynamicAssist(value) => {
                VeteranWire::DynamicAssist.select(value.percent())
            }
            NosfetCommand::PedalDipCompensation(value) => {
                VeteranWire::PedalDipCompensation.select(value.percent())
            }
            NosfetCommand::LateralTiltLimit(value) => {
                VeteranWire::LateralTilt.select(value.degrees())
            }
            NosfetCommand::VoltageCorrection(value) => VeteranWire::VoltageCorrection
                .select(u8::from_ne_bytes(value.tenths_of_percent().to_ne_bytes())),
            NosfetCommand::TiltbackSpeed(speed) => {
                VeteranWire::TiltbackSpeed.select(speed.kilometres_per_hour())
            }
            NosfetCommand::PwmPercent(percent) => VeteranWire::PwmTiltback.select(match percent {
                VeteranPwmSetting::Off => 200,
                VeteranPwmSetting::Margin(margin) => 100 - margin.percent(),
            }),
            NosfetCommand::GyroCalibration => VeteranWire::GyroCalibration.select(1),
            NosfetCommand::BrakeOverpressureAlarm(value) => {
                NosfetWire::BrakeOverpressureAlarm.select(value.percent())
            }
            NosfetCommand::PedalHardness(percent) => {
                VeteranWire::PedalHardness.select(percent.percent())
            }
            NosfetCommand::WheelUnits(units) => {
                VeteranWire::DisplayUnits.select(units.display_mode())
            }
            NosfetCommand::HighSpeedMode(value) => {
                VeteranWire::HighSpeedMode.select(u8::from(value.enabled()))
            }
            NosfetCommand::LowBatteryMode(value) => {
                VeteranWire::LowBatteryMode.select(u8::from(value.enabled()))
            }
            NosfetCommand::TransportMode(value) => {
                VeteranWire::TransportMode.select(u8::from(value.enabled()))
            }
            NosfetCommand::AlarmSpeed(speed) => {
                VeteranWire::SpeedAlarm.select(speed.kilometres_per_hour())
            }
            NosfetCommand::AngleAdjustment(angle) => VeteranWire::PedalAngle
                .select(u8::from_ne_bytes(angle.tenths_of_degree().to_ne_bytes())),
            NosfetCommand::RidingMode(mode_value) => {
                return veteran_riding_mode_payload(mode_value.wire_value(), mode);
            }
        })
    }
}

fn veteran_mode_control_payload(
    command: DeviceCommand,
    mode: VeteranCommandMode,
) -> Option<crate::control_wire::Selection<crate::VeteranProtocol>> {
    let (ascii, binary, value) = match command {
        DeviceCommand::SoundHorn
        | DeviceCommand::InvokeAction(cutout_core::DeviceActionRequest {
            id: DeviceActionId::Horn,
            step: DeviceActionStep::Invoke,
        }) => (VeteranWire::AsciiHorn, VeteranWire::Horn, 1),
        DeviceCommand::InvokeAction(cutout_core::DeviceActionRequest {
            id: DeviceActionId::ResetTripMeter,
            step: DeviceActionStep::Invoke,
        }) => {
            // NF2557's established reset command is the literal CLEARMETER command.
            // Do not select an unverified binary candidate merely because the
            // connection starts in binary mode.
            return Some(VeteranWire::AsciiResetTrip.select(1));
        }
        DeviceCommand::SetSetting {
            id: SettingId::HighBeam,
            value: DeviceSettingValue::Boolean(enabled),
        } => {
            // The modern Aero headlight command is a literal command in both
            // Veteran command modes. The binary LkAp field is present in the
            // shared protocol schema, but sending it to NF2557 produces no
            // physical effect; DarknessBot and EUC World use these literals.
            return Some(if enabled {
                VeteranWire::AsciiHeadlightOn.select(1)
            } else {
                VeteranWire::AsciiHeadlightOff.select(0)
            });
        }
        DeviceCommand::SetSetting {
            id: SettingId::RidingPreset,
            value: DeviceSettingValue::Choice(value),
        } => {
            return veteran_riding_mode_payload(
                match value {
                    0 => 3,
                    1 => 2,
                    2 => 1,
                    _ => return None,
                },
                mode,
            );
        }
        _ => return None,
    };
    Some(match mode {
        VeteranCommandMode::Ascii => ascii.select(value),
        VeteranCommandMode::Binary => binary.select(value),
    })
}

fn veteran_riding_mode_payload(
    value: u8,
    mode: VeteranCommandMode,
) -> Option<crate::control_wire::Selection<crate::VeteranProtocol>> {
    let ascii = match value {
        1 => VeteranWire::AsciiRidingSoft,
        2 => VeteranWire::AsciiRidingMedium,
        3 => VeteranWire::AsciiRidingHard,
        _ => return None,
    };
    Some(match mode {
        VeteranCommandMode::Ascii => ascii.select(value),
        VeteranCommandMode::Binary => VeteranWire::RidingPreset.select(value),
    })
}

/// Falcon model's control dialect of the Begode protocol.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FalconDialect;

impl crate::control_wire::Dialect for FalconDialect {
    type Protocol = crate::BegodeProtocol;
    fn select(
        command: DeviceCommand,
        _context: crate::session::ControlEncodingContext,
    ) -> Option<crate::control_wire::Selection<crate::BegodeProtocol>> {
        Self::select(command)
    }
}

#[derive(Clone, Copy, Debug)]
enum BegodeWireCommand {
    Lights(LightState),
    PedalMode(PedalMode),
    RollAngle(RollAngle),
    SpeedAlarmMode(SpeedAlarmMode),
    MaxSpeed(BegodeMaxSpeed),
    BeeperVolume(BegodeBeeperVolume),
    LedMode(BegodeLedModeSetting),
}

fn begode_wire_command(command: DeviceCommand) -> Option<BegodeWireCommand> {
    let (id, value) = match command {
        DeviceCommand::SetLights(state) => {
            return Some(BegodeWireCommand::Lights(state));
        }
        DeviceCommand::SetSetting { id, value } => (id, value),
        _ => return None,
    };
    match (id, value) {
        (SettingId::Headlight, DeviceSettingValue::Boolean(true)) => {
            Some(BegodeWireCommand::Lights(LightState::On))
        }
        (SettingId::Headlight, DeviceSettingValue::Boolean(false)) => {
            Some(BegodeWireCommand::Lights(LightState::Off))
        }
        (SettingId::PedalMode, DeviceSettingValue::Choice(0)) => {
            Some(BegodeWireCommand::PedalMode(PedalMode::Hard))
        }
        (SettingId::PedalMode, DeviceSettingValue::Choice(1)) => {
            Some(BegodeWireCommand::PedalMode(PedalMode::Medium))
        }
        (SettingId::PedalMode, DeviceSettingValue::Choice(2)) => {
            Some(BegodeWireCommand::PedalMode(PedalMode::Soft))
        }
        (SettingId::RollAngleMode, DeviceSettingValue::Choice(0)) => {
            Some(BegodeWireCommand::RollAngle(RollAngle::Low))
        }
        (SettingId::RollAngleMode, DeviceSettingValue::Choice(1)) => {
            Some(BegodeWireCommand::RollAngle(RollAngle::Medium))
        }
        (SettingId::RollAngleMode, DeviceSettingValue::Choice(2)) => {
            Some(BegodeWireCommand::RollAngle(RollAngle::High))
        }
        (SettingId::SpeedAlarmMode, DeviceSettingValue::Choice(0)) => {
            Some(BegodeWireCommand::SpeedAlarmMode(SpeedAlarmMode::Both))
        }
        (SettingId::SpeedAlarmMode, DeviceSettingValue::Choice(1)) => Some(
            BegodeWireCommand::SpeedAlarmMode(SpeedAlarmMode::StageOneOnly),
        ),
        (SettingId::MaximumSpeed, DeviceSettingValue::Number(value)) => Some(
            BegodeWireCommand::MaxSpeed(BegodeMaxSpeed::new(exact_deci_kmh(value)?)?),
        ),
        (SettingId::BeeperVolumeLevel, DeviceSettingValue::Number(value)) => Some(
            BegodeWireCommand::BeeperVolume(BegodeBeeperVolume::new(u8::try_from(value).ok()?)?),
        ),
        (SettingId::LightingPattern, DeviceSettingValue::Choice(value)) => Some(
            BegodeWireCommand::LedMode(BegodeLedModeSetting::new(u8::try_from(value).ok()?)?),
        ),
        _ => None,
    }
}

impl FalconDialect {
    /// Encodes a single control in the Falcon dialect of Begode.
    #[must_use]
    pub fn encode(command: DeviceCommand) -> Option<EncodedControl> {
        Self::select(command)?.single(command)
    }

    /// Encodes a documented Begode submenu as timed transport writes.
    #[must_use]
    pub fn encode_settings_sequence(command: DeviceCommand) -> Option<EncodedControlSequence> {
        Self::select(command)?.sequence(command)
    }

    fn select(
        command: DeviceCommand,
    ) -> Option<crate::control_wire::Selection<crate::BegodeProtocol>> {
        let (wire, value) = match begode_wire_command(command)? {
            BegodeWireCommand::Lights(LightState::On) => (BegodeWire::LightOn, 0),
            BegodeWireCommand::Lights(LightState::Off) => (BegodeWire::LightOff, 0),
            BegodeWireCommand::Lights(LightState::Strobe) => (BegodeWire::LightStrobe, 0),
            BegodeWireCommand::PedalMode(PedalMode::Hard) => (BegodeWire::PedalHard, 0),
            BegodeWireCommand::PedalMode(PedalMode::Medium) => (BegodeWire::PedalMedium, 0),
            BegodeWireCommand::PedalMode(PedalMode::Soft) => (BegodeWire::PedalSoft, 0),
            BegodeWireCommand::RollAngle(RollAngle::Low) => (BegodeWire::RollLow, 0),
            BegodeWireCommand::RollAngle(RollAngle::Medium) => (BegodeWire::RollMedium, 0),
            BegodeWireCommand::RollAngle(RollAngle::High) => (BegodeWire::RollHigh, 0),
            BegodeWireCommand::SpeedAlarmMode(SpeedAlarmMode::Both) => (BegodeWire::AlarmBoth, 0),
            BegodeWireCommand::SpeedAlarmMode(SpeedAlarmMode::StageOneOnly) => {
                (BegodeWire::AlarmStageOne, 0)
            }
            BegodeWireCommand::MaxSpeed(value) => {
                (BegodeWire::MaxSpeed, value.kilometres_per_hour())
            }
            BegodeWireCommand::BeeperVolume(value) => (BegodeWire::BeeperVolume, value.level()),
            BegodeWireCommand::LedMode(value) => (BegodeWire::LedMode, value.mode()),
            BegodeWireCommand::SpeedAlarmMode(
                SpeedAlarmMode::Off | SpeedAlarmMode::PwmTiltback,
            ) => return None,
        };
        Some(wire.select(value))
    }
}

/// Returns the complete ordered Begode identity query sequence.
#[must_use]
pub fn begode_identification_probes() -> [EncodedIdentificationProbe; 3] {
    [
        (PendingProbe::BegodeName, b"N"),
        (PendingProbe::BegodeFirmware, b"V"),
        (PendingProbe::BegodeImu, b"M"),
    ]
    .map(|(probe, payload)| EncodedIdentificationProbe {
        probe,
        payload: request_payload(payload),
        mode: WriteMode::WithoutResponse,
    })
}

/// Explicit disposition for a family-specific request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestDisposition<P> {
    /// A probe that does not require a transport write.
    Passive {
        /// Family-specific probe represented by this request.
        probe: P,

        /// Generic command kind used for scheduler and response correlation.
        command: CommandKind,
    },

    /// A probe encoded as a bounded transport write.
    Write(EncodedRequest<P>),

    /// A probe encoded as a bounded sequence of transport writes.
    Writes(ArrayVec<EncodedRequest<P>, 4>),
}

/// Request encoder for NOSFET Aero/Veteran-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AeroRequestEncoder;

impl AeroRequestEncoder {
    /// Encodes a supported Aero/Veteran-family probe.
    #[must_use]
    pub const fn encode(probe: AeroProbe) -> RequestDisposition<AeroProbe> {
        RequestDisposition::Passive {
            command: probe.command_kind(),
            probe,
        }
    }

    /// Encodes a generic command if it belongs to the Aero/Veteran probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<RequestDisposition<AeroProbe>> {
        Some(Self::encode(AeroProbe::from_command_kind(kind)?))
    }
}

/// Request encoder for source-backed Begode/Falcon-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FalconRequestEncoder;

impl FalconRequestEncoder {
    /// Encodes a supported Begode/Falcon-family probe.
    #[must_use]
    pub fn encode(probe: FalconProbe) -> RequestDisposition<FalconProbe> {
        match probe {
            FalconProbe::Identity => RequestDisposition::Write(EncodedRequest {
                probe,
                command: probe.command_kind(),
                payload: request_payload(b"N"),
                mode: WriteMode::WithoutResponse,
            }),
            FalconProbe::FirmwareInfo => RequestDisposition::Write(EncodedRequest {
                probe,
                command: probe.command_kind(),
                payload: request_payload(b"V"),
                mode: WriteMode::WithoutResponse,
            }),
            FalconProbe::Telemetry | FalconProbe::BatteryInfo => RequestDisposition::Passive {
                probe,
                command: probe.command_kind(),
            },
        }
    }

    /// Encodes a generic command if it belongs to the Begode/Falcon probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<RequestDisposition<FalconProbe>> {
        FalconProbe::from_command_kind(kind).map(Self::encode)
    }
}

/// Request encoder for generic VESC read-only probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VescRequestEncoder;

impl VescRequestEncoder {
    /// Encodes a supported VESC read-only command.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<RequestDisposition<VescReadOnlyRequest>> {
        let request = match kind {
            CommandKind::RequestFirmwareInfo => VescReadOnlyRequest::FirmwareInfo,
            CommandKind::RequestTelemetry => {
                let mut requests = ArrayVec::new();
                for request in [
                    VescReadOnlyRequest::Refloat(RefloatReadOnlyRequest::RealtimeDataIds),
                    VescReadOnlyRequest::MotorConfig,
                    VescReadOnlyRequest::Values,
                ] {
                    let mut encoded = ArrayVec::new();
                    VescReadOnlyCodec::encode_request(request, &mut encoded).ok()?;
                    requests
                        .try_push(EncodedRequest {
                            probe: request,
                            command: kind,
                            payload: WritePayload::try_from_slice(encoded.as_slice()).ok()?,
                            mode: WriteMode::WithoutResponse,
                        })
                        .ok()?;
                }
                return Some(RequestDisposition::Writes(requests));
            }
            CommandKind::RequestDiagnostics => VescReadOnlyRequest::Stats(
                crate::VescStatsMask::SPEED_AVG
                    | crate::VescStatsMask::POWER_AVG
                    | crate::VescStatsMask::CURRENT_AVG
                    | crate::VescStatsMask::COUNT_TIME,
            ),
            CommandKind::RequestIdentity
            | CommandKind::RequestBatteryInfo
            | CommandKind::RequestFaultHistory
            | CommandKind::RequestSettings
            | CommandKind::ResetTripMeter
            | CommandKind::GyroCalibration
            | CommandKind::SetSetting
            | CommandKind::SetLights
            | CommandKind::SetTaillight
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => return None,
        };
        let mut encoded = ArrayVec::new();
        VescReadOnlyCodec::encode_request(request, &mut encoded).ok()?;
        Some(RequestDisposition::Write(EncodedRequest {
            probe: request,
            command: kind,
            payload: WritePayload::try_from_slice(encoded.as_slice()).ok()?,
            mode: WriteMode::WithoutResponse,
        }))
    }
}

/// Read-only target for a VESC controller reachable through CAN forwarding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescCanTarget {
    controller_id: VescControllerId,
}

impl VescCanTarget {
    /// Creates a CAN forwarding target for a controller id.
    #[must_use]
    pub const fn new(controller_id: VescControllerId) -> Self {
        Self { controller_id }
    }

    /// Returns the CAN controller id.
    #[must_use]
    pub const fn controller_id(self) -> VescControllerId {
        self.controller_id
    }

    /// Builds the core request key used to correlate this target's command.
    #[must_use]
    pub const fn request_key(self, kind: CommandKind) -> RequestKey {
        RequestKey::for_target(
            kind,
            RequestTarget::VescCanController {
                controller_id: self.controller_id,
            },
        )
    }

    /// Encodes a supported read-only command through VESC CAN forwarding.
    #[must_use]
    pub fn encode_command(
        self,
        kind: CommandKind,
    ) -> Option<RequestDisposition<VescReadOnlyRequest>> {
        let request = match kind {
            CommandKind::RequestFirmwareInfo => VescCanReadOnlyRequest::FirmwareInfo,
            CommandKind::RequestTelemetry => VescCanReadOnlyRequest::Values,
            CommandKind::RequestDiagnostics => VescCanReadOnlyRequest::Stats(
                crate::VescStatsMask::SPEED_AVG
                    | crate::VescStatsMask::POWER_AVG
                    | crate::VescStatsMask::CURRENT_AVG
                    | crate::VescStatsMask::COUNT_TIME,
            ),
            CommandKind::RequestIdentity
            | CommandKind::RequestBatteryInfo
            | CommandKind::RequestFaultHistory
            | CommandKind::RequestSettings
            | CommandKind::ResetTripMeter
            | CommandKind::GyroCalibration
            | CommandKind::SetSetting
            | CommandKind::SetLights
            | CommandKind::SetTaillight
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => return None,
        };
        let request = VescReadOnlyRequest::ForwardCan {
            controller_id: self.controller_id,
            request,
        };
        let mut encoded = ArrayVec::new();
        VescReadOnlyCodec::encode_request(request, &mut encoded).ok()?;
        Some(RequestDisposition::Write(EncodedRequest {
            probe: request,
            command: kind,
            payload: WritePayload::try_from_slice(encoded.as_slice()).ok()?,
            mode: WriteMode::WithoutResponse,
        }))
    }
}

pub(crate) fn request_payload(bytes: &[u8]) -> WritePayload {
    let Ok(payload) = WritePayload::try_from_slice(bytes) else {
        unreachable!("bounded protocol request exceeds the transport maximum");
    };
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceFamily, ProtocolProbe, load_request_fixtures};
    use core::mem::size_of;

    #[test]
    fn aero_control_encoder_reserves_headlight_frames_for_stationary_high_beam() {
        assert_eq!(
            NosfetDialect::encode(DeviceCommand::SetLights(LightState::On)),
            None
        );
        let on = NosfetDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::HighBeam,
            value: DeviceSettingValue::Boolean(true),
        })
        .expect("NOSFET high-beam command encodes");
        let off = NosfetDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::HighBeam,
            value: DeviceSettingValue::Boolean(false),
        })
        .expect("NOSFET high-beam command encodes");

        assert_eq!(on.command, CommandKind::SetSetting);
        assert_eq!(on.payload.as_slice(), b"SetLightON");
        assert_eq!(on.mode, WriteMode::WithoutResponse);
        assert_eq!(off.command, CommandKind::SetSetting);
        assert_eq!(off.payload.as_slice(), b"SetLightOFF");
        assert_eq!(off.mode, WriteMode::WithoutResponse);
        assert_eq!(
            NosfetDialect::encode(DeviceCommand::SetSetting {
                id: SettingId::HighBeam,
                value: DeviceSettingValue::Choice(2),
            }),
            None
        );
        assert_eq!(
            NosfetDialect::encode(DeviceCommand::InvokeAction(
                cutout_core::DeviceActionRequest {
                    id: DeviceActionId::Horn,
                    step: DeviceActionStep::Invoke,
                },
            ))
            .unwrap()
            .payload
            .as_slice(),
            &hex_literal::hex!("4c6b41700e0080808001ca87e66f")
        );
    }

    #[test]
    fn aero_control_encoder_resets_the_trip_meter_with_the_documented_command() {
        let reset = NosfetDialect::encode(DeviceCommand::InvokeAction(
            cutout_core::DeviceActionRequest {
                id: DeviceActionId::ResetTripMeter,
                step: DeviceActionStep::Invoke,
            },
        ))
        .expect("trip reset is a supported Aero settings write");

        assert_eq!(reset.command, CommandKind::ResetTripMeter);
        assert_eq!(reset.payload.as_slice(), b"CLEARMETER");
        assert_eq!(reset.mode, WriteMode::WithoutResponse);
    }

    #[test]
    fn aero_command_mode_latches_only_after_two_matching_complete_packets() {
        let mut mode = VeteranCommandModeDetector::default();
        assert_eq!(mode.mode(), VeteranCommandMode::Binary);
        assert!(!mode.is_latched());

        mode.observe_complete_packet_len(5);
        mode.observe_complete_packet_len(5);
        assert_eq!(mode.mode(), VeteranCommandMode::Binary);
        assert!(!mode.is_latched());

        mode.observe_complete_packet_len(46);
        mode.observe_complete_packet_len(47);
        assert_eq!(mode.mode(), VeteranCommandMode::Binary);
        assert!(!mode.is_latched());

        mode.observe_complete_packet_len(46);
        mode.observe_complete_packet_len(46);
        assert_eq!(mode.mode(), VeteranCommandMode::Ascii);
        assert!(mode.is_latched());

        mode.observe_complete_packet_len(47);
        assert_eq!(mode.mode(), VeteranCommandMode::Ascii);
        mode.reset();
        assert_eq!(mode, VeteranCommandModeDetector::default());
    }

    #[test]
    fn aero_mode_aware_controls_match_official_ascii_and_binary_fixtures() {
        for (command, ascii, binary) in [
            (
                DeviceCommand::InvokeAction(cutout_core::DeviceActionRequest {
                    id: DeviceActionId::Horn,
                    step: DeviceActionStep::Invoke,
                }),
                b"OLDCMDb".as_slice(),
                hex_literal::hex!("4c6b41700e0080808001ca87e66f").as_slice(),
            ),
            (
                DeviceCommand::InvokeAction(cutout_core::DeviceActionRequest {
                    id: DeviceActionId::ResetTripMeter,
                    step: DeviceActionStep::Invoke,
                }),
                b"CLEARMETER".as_slice(),
                b"CLEARMETER".as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::RidingPreset,
                    value: DeviceSettingValue::Choice(2),
                },
                b"SETs".as_slice(),
                hex_literal::hex!("4c6b41700c018001a8e75480").as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::RidingPreset,
                    value: DeviceSettingValue::Choice(1),
                },
                b"SETm".as_slice(),
                hex_literal::hex!("4c6b41700c01800231ee053a").as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::RidingPreset,
                    value: DeviceSettingValue::Choice(0),
                },
                b"SETh".as_slice(),
                hex_literal::hex!("4c6b41700c01800346e935ac").as_slice(),
            ),
        ] {
            assert_eq!(
                NosfetDialect::encode_in_mode(command, VeteranCommandMode::Ascii)
                    .unwrap()
                    .payload
                    .as_slice(),
                ascii
            );
            assert_eq!(
                NosfetDialect::encode_in_mode(command, VeteranCommandMode::Binary)
                    .unwrap()
                    .payload
                    .as_slice(),
                binary
            );
        }
    }

    #[test]
    fn model_specific_aero_aliases_share_the_mode_aware_encoder() {
        let high_beam = NosfetDialect::encode_in_mode(
            DeviceCommand::SetSetting {
                id: SettingId::HighBeam,
                value: DeviceSettingValue::Boolean(true),
            },
            VeteranCommandMode::Ascii,
        )
        .unwrap();
        assert_eq!(high_beam.payload.as_slice(), b"SetLightON");

        let high_beam = NosfetDialect::encode_in_mode(
            DeviceCommand::SetSetting {
                id: SettingId::HighBeam,
                value: DeviceSettingValue::Boolean(true),
            },
            VeteranCommandMode::Binary,
        )
        .unwrap();
        assert_eq!(high_beam.payload.as_slice(), b"SetLightON");

        let riding = NosfetDialect::encode_in_mode(
            DeviceCommand::SetSetting {
                id: SettingId::RidingPreset,
                value: DeviceSettingValue::Choice(1),
            },
            VeteranCommandMode::Ascii,
        )
        .unwrap();
        assert_eq!(riding.payload.as_slice(), b"SETm");
    }

    #[test]
    fn aero_binary_settings_match_the_captured_frame_shapes_and_crc() {
        let cases = [
            (
                DeviceCommand::SetSetting {
                    id: SettingId::TiltbackSpeed,
                    value: DeviceSettingValue::Number(210),
                },
                *b"LdAp",
                17,
                &[0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 21][..],
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::PwmTiltback,
                    value: DeviceSettingValue::Number(64),
                },
                *b"LdAp",
                18,
                &[0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 64][..],
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::LateralTiltLimit,
                    value: DeviceSettingValue::Number(55),
                },
                *b"LkAp",
                22,
                &[
                    0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 55,
                ][..],
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::SpeedAlarmThreshold,
                    value: DeviceSettingValue::Number(200),
                },
                *b"LkAp",
                17,
                &[0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 20][..],
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::PedalAngle,
                    value: DeviceSettingValue::Number(-36),
                },
                *b"LkAp",
                16,
                &[0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 220][..],
            ),
        ];

        for (command, magic, length, body) in cases {
            let encoded = NosfetDialect::encode(command).expect("Aero setting encodes");
            assert_eq!(encoded.command, command.kind());
            assert_eq!(encoded.mode, WriteMode::WithoutResponse);
            assert_eq!(&encoded.payload.as_slice()[..4], &magic);
            assert_eq!(encoded.payload.as_slice()[4], length);
            let body_len = usize::from(length) - 4;
            assert_eq!(&encoded.payload.as_slice()[5..body_len], body);
            let expected_crc = crc32(&encoded.payload.as_slice()[..body_len]).to_be_bytes();
            assert_eq!(&encoded.payload.as_slice()[body_len..], &expected_crc);
        }
    }

    #[test]
    fn aero_alarm_and_lateral_tilt_do_not_target_other_settings() {
        let encode = |id, value| {
            NosfetDialect::encode(DeviceCommand::SetSetting { id, value })
                .unwrap()
                .payload
        };
        let alarm = encode(
            SettingId::SpeedAlarmThreshold,
            DeviceSettingValue::Number(560),
        );
        let tiltback = encode(SettingId::TiltbackSpeed, DeviceSettingValue::Number(560));
        assert_ne!(
            alarm, tiltback,
            "speed alarm must not change tilt-back speed"
        );
        assert_eq!(&alarm.as_slice()[..7], b"LkAp\x11\x01\x80");
        assert_eq!(&tiltback.as_slice()[..7], b"LdAp\x11\x01\x02");

        let lateral = encode(SettingId::LateralTiltLimit, DeviceSettingValue::Number(55));
        let transport = encode(SettingId::TransportMode, DeviceSettingValue::Boolean(true));
        // These share a value offset, but must address different command banks.
        assert_ne!(&lateral.as_slice()[..17], &transport.as_slice()[..17]);
        assert_eq!(&lateral.as_slice()[..7], b"LkAp\x16\x01\x80");
        assert_eq!(&transport.as_slice()[..7], b"LdAp\x16\x01\x02");
    }

    #[test]
    fn speed_encoders_reject_inexact_canonical_values_instead_of_truncating() {
        for id in [SettingId::TiltbackSpeed, SettingId::SpeedAlarmThreshold] {
            assert_eq!(
                NosfetDialect::encode(DeviceCommand::SetSetting {
                    id,
                    value: DeviceSettingValue::Number(348),
                }),
                None,
                "NOSFET must not turn 34.8 deci-km/h into 34 km/h"
            );
        }
        assert_eq!(
            FalconDialect::encode_settings_sequence(DeviceCommand::SetSetting {
                id: SettingId::MaximumSpeed,
                value: DeviceSettingValue::Number(348),
            }),
            None,
            "Falcon must not turn 34.8 deci-km/h into 34 km/h"
        );
    }

    #[test]
    fn aero_pwm_off_is_distinct_from_zero_margin() {
        let disabled = NosfetDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::PwmTiltback,
            value: DeviceSettingValue::Disabled,
        })
        .unwrap();
        assert_eq!(
            disabled.payload.as_slice(),
            &hex_literal::hex!("4c644170120102808080808080c8d24c759e")
        );
        let zero = NosfetDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::PwmTiltback,
            value: DeviceSettingValue::Number(100),
        })
        .unwrap();
        assert_eq!(zero.payload.as_slice()[13], 100);
        assert_ne!(disabled.payload, zero.payload);
    }

    #[test]
    fn aero_modern_riding_mode_uses_the_source_backed_lkap_t_frame() {
        let cases = [
            (
                DeviceSettingValue::Choice(2),
                hex_literal::hex!("4c6b41700c018001a8e75480"),
            ),
            (
                DeviceSettingValue::Choice(1),
                hex_literal::hex!("4c6b41700c01800231ee053a"),
            ),
            (
                DeviceSettingValue::Choice(0),
                hex_literal::hex!("4c6b41700c01800346e935ac"),
            ),
        ];

        for (mode, expected) in cases {
            let encoded = NosfetDialect::encode(DeviceCommand::SetSetting {
                id: SettingId::RidingPreset,
                value: mode,
            })
            .expect("modern binary T mode encodes");
            assert_eq!(encoded.command, CommandKind::SetSetting);
            assert_eq!(encoded.mode, WriteMode::WithoutResponse);
            assert_eq!(encoded.payload.as_slice(), expected);
        }
    }

    #[test]
    fn aero_extended_settings_match_euc_world_frames() {
        let cases = [
            (
                DeviceCommand::SetSetting {
                    id: SettingId::DisplayBrightness,
                    value: DeviceSettingValue::Number(50),
                },
                hex_literal::hex!("4c644170140102808080808080808032ec9452c7").as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::BeeperVolumePercent,
                    value: DeviceSettingValue::Number(75),
                },
                hex_literal::hex!("4c6441701c0102808080808080808080808080808080804b930b4f71")
                    .as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::DynamicAssist,
                    value: DeviceSettingValue::Number(60),
                },
                hex_literal::hex!("4c6441701f0102808080808080808080808080808080808080803c6831fed2")
                    .as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::PedalDipCompensation,
                    value: DeviceSettingValue::Number(40),
                },
                hex_literal::hex!(
                    "4c64417021010280808080808080808080808080808080808080808028c549a32e"
                )
                .as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::LateralTiltLimit,
                    value: DeviceSettingValue::Number(55),
                },
                hex_literal::hex!("4c6b41701601808080808080808080808037aef39e07").as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::VoltageCorrection,
                    value: DeviceSettingValue::Number(-15),
                },
                hex_literal::hex!("4c644170180102808080808080808080808080f129076df6").as_slice(),
            ),
        ];
        for (command, expected) in cases {
            let encoded = NosfetDialect::encode(command).expect("source-backed setting encodes");
            assert_eq!(encoded.payload.as_slice(), expected);
            assert_eq!(encoded.command, command.kind());
            assert_eq!(encoded.mode, WriteMode::WithoutResponse);
            assert_eq!(
                command.safety_class(),
                cutout_core::SafetyClass::StationaryOnly
            );
        }
    }

    #[test]
    fn aero_safety_modes_match_documented_toggle_frames() {
        let cases = [
            (
                DeviceCommand::SetSetting {
                    id: SettingId::HighSpeedMode,
                    value: DeviceSettingValue::Boolean(true),
                },
                hex_literal::hex!("4c6441701a01028080808080808080808080808080012c5fa11f")
                    .as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::LowBatteryMode,
                    value: DeviceSettingValue::Boolean(false),
                },
                hex_literal::hex!("4c644170190102808080808080808080808080800037773c3d").as_slice(),
            ),
            (
                DeviceCommand::SetSetting {
                    id: SettingId::TransportMode,
                    value: DeviceSettingValue::Boolean(true),
                },
                hex_literal::hex!("4c64417016010280808080808080808080012b37c934").as_slice(),
            ),
        ];

        for (command, expected) in cases {
            let encoded = NosfetDialect::encode(command).expect("Aero toggle encodes");
            assert_eq!(encoded.payload.as_slice(), expected);
            assert_eq!(encoded.command, command.kind());
            assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        }
    }

    #[test]
    fn aero_gyro_calibration_matches_the_official_frame_and_crc() {
        let command = DeviceCommand::InvokeAction(cutout_core::DeviceActionRequest {
            id: DeviceActionId::GyroCalibration,
            step: DeviceActionStep::Invoke,
        });
        let encoded = NosfetDialect::encode(command).expect("gyro calibration encodes");
        assert_eq!(
            encoded.payload.as_slice(),
            &hex_literal::hex!("4c64417015010280808080808080808001ab8c09e5")
        );
        assert_eq!(encoded.command, CommandKind::GyroCalibration);
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        assert_eq!(
            command.safety_class(),
            cutout_core::SafetyClass::StationaryOnly
        );
    }

    #[test]
    fn aero_brake_overpressure_alarm_matches_the_official_frame_and_crc() {
        let command = DeviceCommand::SetSetting {
            id: SettingId::BrakeOverpressureAlarm,
            value: DeviceSettingValue::Number(100),
        };
        let encoded = NosfetDialect::encode(command).expect("brake alarm encodes");
        assert_eq!(
            encoded.payload.as_slice(),
            &hex_literal::hex!("4c6441701e01028080808080808080808080808080808080806453681869")
        );
        assert_eq!(encoded.command, CommandKind::SetSetting);
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        assert_eq!(
            command.safety_class(),
            cutout_core::SafetyClass::StationaryOnly
        );
    }

    #[test]
    fn aero_charge_limit_diagnostic_is_read_only() {
        let command = DeviceCommand::SetSetting {
            id: SettingId::ChargeLimitDiagnostic,
            value: DeviceSettingValue::Number(46),
        };
        assert!(NosfetDialect::encode(command).is_none());
    }

    #[test]
    fn aero_md_hardness_matches_the_documented_frame_and_crc() {
        let command = DeviceCommand::SetSetting {
            id: SettingId::PedalHardness,
            value: DeviceSettingValue::Number(100),
        };
        let encoded = NosfetDialect::encode(command).expect("MD is supported");
        // Independent IEEE CRC32 fixture for EUC Planet's 15-byte ride-mode frame.
        assert_eq!(
            encoded.payload.as_slice(),
            &hex_literal::hex!("4c6441700f01028080806430847887")
        );
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        assert_eq!(
            command.safety_class(),
            cutout_core::SafetyClass::StationaryOnly
        );
    }

    #[test]
    fn aero_wheel_units_have_a_distinct_crc_checked_display_command() {
        let command = DeviceCommand::SetSetting {
            id: SettingId::DisplayUnits,
            value: DeviceSettingValue::Choice(1),
        };
        let encoded = NosfetDialect::encode(command).expect("wheel display units supported");
        assert_eq!(
            encoded.payload.as_slice(),
            &hex_literal::hex!("4c6441701701028080808080808080808080011ff96e85")
        );
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        assert_eq!(
            command.safety_class(),
            cutout_core::SafetyClass::StationaryOnly
        );
    }

    #[test]
    fn aero_high_beam_encodes_the_official_modern_literal_command() {
        let encoded = NosfetDialect::encode_in_mode(
            DeviceCommand::SetSetting {
                id: SettingId::HighBeam,
                value: DeviceSettingValue::Boolean(true),
            },
            VeteranCommandMode::Binary,
        )
        .expect("Aero high beam encodes");

        assert_eq!(encoded.command, CommandKind::SetSetting);
        assert_eq!(encoded.payload.as_slice(), b"SetLightON");
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
    }

    #[test]
    fn falcon_control_encoder_uses_explicit_begode_light_commands() {
        let on = FalconDialect::encode(DeviceCommand::SetLights(LightState::On))
            .expect("Begode lights-on command encodes");
        let off = FalconDialect::encode(DeviceCommand::SetLights(LightState::Off))
            .expect("Begode lights-off command encodes");

        assert_eq!(on.command, CommandKind::SetLights);
        assert_eq!(on.payload.as_slice(), b"Q");
        assert_eq!(on.mode, WriteMode::WithoutResponse);
        assert_eq!(off.command, CommandKind::SetLights);
        assert_eq!(off.payload.as_slice(), b"E");
        assert_eq!(off.mode, WriteMode::WithoutResponse);
        let strobe = FalconDialect::encode(DeviceCommand::SetLights(LightState::Strobe))
            .expect("Begode strobe command encodes");
        assert_eq!(strobe.command, CommandKind::SetLights);
        assert_eq!(strobe.payload.as_slice(), b"T");
        assert_eq!(strobe.mode, WriteMode::WithoutResponse);
        assert_eq!(FalconDialect::encode(DeviceCommand::SoundHorn), None);
    }

    #[test]
    fn documented_pedal_mode_encoders_match_veteran_and_begode_bytes() {
        let aero = NosfetDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::RidingPreset,
            value: DeviceSettingValue::Choice(0),
        })
        .expect("documented Veteran pedal mode encoder");
        assert_eq!(aero.command, CommandKind::SetSetting);
        assert_eq!(
            aero.payload.as_slice(),
            &hex_literal::hex!("4c6b41700c01800346e935ac")
        );

        let falcon = FalconDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::PedalMode,
            value: DeviceSettingValue::Choice(2),
        })
        .expect("documented Begode pedal mode encoder");
        assert_eq!(falcon.command, CommandKind::SetSetting);
        assert_eq!(falcon.payload.as_slice(), b"s");
    }

    #[test]
    fn documented_falcon_roll_angle_encoders_match_protocol_bytes() {
        let low = FalconDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::RollAngleMode,
            value: DeviceSettingValue::Choice(0),
        })
        .expect("Begode low roll-angle encoder");
        let medium = FalconDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::RollAngleMode,
            value: DeviceSettingValue::Choice(1),
        })
        .expect("Begode medium roll-angle encoder");
        let high = FalconDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::RollAngleMode,
            value: DeviceSettingValue::Choice(2),
        })
        .expect("Begode high roll-angle encoder");

        assert_eq!(low.command, CommandKind::SetSetting);
        assert_eq!(low.payload.as_slice(), b">");
        assert_eq!(medium.payload.as_slice(), b"=");
        assert_eq!(high.payload.as_slice(), b"<");
        assert_eq!(low.mode, WriteMode::WithoutResponse);
        assert_eq!(
            NosfetDialect::encode(DeviceCommand::SetSetting {
                id: SettingId::RollAngleMode,
                value: DeviceSettingValue::Choice(0),
            }),
            None
        );
    }

    #[test]
    fn documented_falcon_speed_alarm_encoders_match_protocol_bytes() {
        let both = FalconDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::SpeedAlarmMode,
            value: DeviceSettingValue::Choice(0),
        })
        .expect("Begode both-alarms encoder");
        let stage_one = FalconDialect::encode(DeviceCommand::SetSetting {
            id: SettingId::SpeedAlarmMode,
            value: DeviceSettingValue::Choice(1),
        })
        .expect("Begode stage-one-only encoder");

        assert_eq!(both.command, CommandKind::SetSetting);
        assert_eq!(both.payload.as_slice(), b"o");
        assert_eq!(stage_one.payload.as_slice(), b"u");
        assert_eq!(both.mode, WriteMode::WithoutResponse);
        assert_eq!(
            NosfetDialect::encode(DeviceCommand::SetSetting {
                id: SettingId::SpeedAlarmMode,
                value: DeviceSettingValue::Choice(0),
            }),
            None
        );
    }

    #[test]
    fn falcon_w_settings_encode_as_delayed_ordered_writes() {
        let max_speed = FalconDialect::encode_settings_sequence(DeviceCommand::SetSetting {
            id: SettingId::MaximumSpeed,
            value: DeviceSettingValue::Number(300),
        })
        .expect("max-speed sequence encodes");
        assert_eq!(max_speed.command, CommandKind::SetSetting);
        assert_eq!(
            max_speed
                .steps
                .iter()
                .map(|step| (step.delay_ms, step.payload.as_slice()))
                .collect::<Vec<_>>(),
            vec![
                (0, b"W".as_slice()),
                (100, b"Y".as_slice()),
                (200, b"3".as_slice()),
                (200, b"0".as_slice()),
                (200, b"b".as_slice())
            ]
        );

        let volume = FalconDialect::encode_settings_sequence(DeviceCommand::SetSetting {
            id: SettingId::BeeperVolumeLevel,
            value: DeviceSettingValue::Number(7),
        })
        .expect("beeper sequence encodes");
        assert_eq!(volume.command, CommandKind::SetSetting);
        assert_eq!(
            volume
                .steps
                .iter()
                .map(|step| (step.delay_ms, step.payload.as_slice()))
                .collect::<Vec<_>>(),
            vec![
                (0, b"W".as_slice()),
                (100, b"B".as_slice()),
                (200, b"7".as_slice()),
                (200, b"b".as_slice())
            ]
        );

        let led = FalconDialect::encode_settings_sequence(DeviceCommand::SetSetting {
            id: SettingId::LightingPattern,
            value: DeviceSettingValue::Choice(4),
        })
        .expect("LED sequence encodes");
        assert_eq!(led.command, CommandKind::SetSetting);
        assert_eq!(
            led.steps
                .iter()
                .map(|step| (step.delay_ms, step.payload.as_slice()))
                .collect::<Vec<_>>(),
            vec![
                (0, b"W".as_slice()),
                (100, b"M".as_slice()),
                (200, b"4".as_slice()),
                (200, b"b".as_slice())
            ]
        );
    }

    #[test]
    fn falcon_encoder_uses_expected_request_bytes() {
        let identity = FalconRequestEncoder::encode(FalconProbe::Identity);
        let firmware = FalconRequestEncoder::encode(FalconProbe::FirmwareInfo);
        let telemetry = FalconRequestEncoder::encode(FalconProbe::Telemetry);
        let battery = FalconRequestEncoder::encode(FalconProbe::BatteryInfo);

        assert!(matches!(
            identity,
            RequestDisposition::Write(ref request)
                if request.command == CommandKind::RequestIdentity
                    && request.mode == WriteMode::WithoutResponse
        ));
        assert!(matches!(
            firmware,
            RequestDisposition::Write(ref request)
                if request.command == CommandKind::RequestFirmwareInfo
                    && request.mode == WriteMode::WithoutResponse
        ));
        assert!(matches!(
            telemetry,
            RequestDisposition::Passive {
                probe: FalconProbe::Telemetry,
                command: CommandKind::RequestTelemetry,
            }
        ));
        assert!(matches!(
            battery,
            RequestDisposition::Passive {
                probe: FalconProbe::BatteryInfo,
                command: CommandKind::RequestBatteryInfo,
            }
        ));
        assert_eq!(
            match identity {
                RequestDisposition::Write(request) => request.payload,
                RequestDisposition::Writes(_) | RequestDisposition::Passive { .. } => {
                    unreachable!()
                }
            }
            .as_slice(),
            b"N"
        );
        assert_eq!(
            match firmware {
                RequestDisposition::Write(request) => request.payload,
                RequestDisposition::Writes(_) | RequestDisposition::Passive { .. } => {
                    unreachable!()
                }
            }
            .as_slice(),
            b"V"
        );
    }

    #[test]
    fn passive_aero_encoder_is_explicitly_passive() {
        assert_eq!(
            AeroRequestEncoder::encode(AeroProbe::Identity),
            RequestDisposition::Passive {
                probe: AeroProbe::Identity,
                command: CommandKind::RequestIdentity,
            }
        );
        assert_eq!(
            AeroRequestEncoder::encode_command(CommandKind::RequestTelemetry),
            Some(RequestDisposition::Passive {
                probe: AeroProbe::Telemetry,
                command: CommandKind::RequestTelemetry,
            })
        );
    }

    #[test]
    fn falcon_encode_command_is_write_backed_for_identity() {
        assert!(matches!(
            FalconRequestEncoder::encode_command(CommandKind::RequestIdentity),
            Some(RequestDisposition::Write(request))
                if request.command == CommandKind::RequestIdentity
                    && request.mode == WriteMode::WithoutResponse
        ));
    }

    #[test]
    fn vesc_can_target_encodes_read_only_telemetry_for_controller_id() {
        let target = VescCanTarget::new(VescControllerId::new(7));
        assert_eq!(target.controller_id(), VescControllerId::new(7));

        let request = target
            .encode_command(CommandKind::RequestTelemetry)
            .expect("telemetry can be forwarded");

        let RequestDisposition::Write(encoded) = request else {
            panic!("CAN telemetry should be write-backed");
        };
        assert_eq!(
            encoded.probe,
            VescReadOnlyRequest::ForwardCan {
                controller_id: VescControllerId::new(7),
                request: VescCanReadOnlyRequest::Values,
            }
        );
        assert_eq!(encoded.command, CommandKind::RequestTelemetry);
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        assert!(!encoded.payload.is_empty());
    }

    #[test]
    fn vesc_telemetry_request_discovers_refloat_fields_and_motor_config_before_values() {
        let request = VescRequestEncoder::encode_command(CommandKind::RequestTelemetry)
            .expect("telemetry is supported");

        let RequestDisposition::Writes(requests) = request else {
            panic!("VESC telemetry should issue the Refloat realtime sequence");
        };
        assert_eq!(requests.len(), 3);
        assert_eq!(
            requests
                .iter()
                .map(|request| request.probe)
                .collect::<ArrayVec<_, 3>>()
                .as_slice(),
            &[
                VescReadOnlyRequest::Refloat(RefloatReadOnlyRequest::RealtimeDataIds),
                VescReadOnlyRequest::MotorConfig,
                VescReadOnlyRequest::Values,
            ]
        );
        assert!(
            requests
                .iter()
                .all(|request| request.command == CommandKind::RequestTelemetry
                    && request.mode == WriteMode::WithoutResponse
                    && !request.payload.is_empty())
        );
    }

    #[test]
    fn vesc_can_target_encodes_diagnostics_with_read_only_stats_mask() {
        let request = VescCanTarget::new(VescControllerId::new(3))
            .encode_command(CommandKind::RequestDiagnostics)
            .expect("diagnostics can be forwarded");

        let RequestDisposition::Write(encoded) = request else {
            panic!("CAN diagnostics should be write-backed");
        };
        let VescReadOnlyRequest::ForwardCan {
            controller_id,
            request: VescCanReadOnlyRequest::Stats(_),
        } = encoded.probe
        else {
            panic!("diagnostics should be forwarded as CAN stats");
        };
        assert_eq!(controller_id, VescControllerId::new(3));
        assert_eq!(encoded.command, CommandKind::RequestDiagnostics);
    }

    #[test]
    fn vesc_can_target_refuses_non_read_only_and_unsupported_commands() {
        let target = VescCanTarget::new(VescControllerId::new(7));

        assert_eq!(target.encode_command(CommandKind::SetRawMotorCurrent), None);
        assert_eq!(target.encode_command(CommandKind::SetLights), None);
        assert_eq!(target.encode_command(CommandKind::SoundHorn), None);
        assert_eq!(target.encode_command(CommandKind::RequestBatteryInfo), None);
    }

    #[test]
    fn vesc_can_target_builds_core_request_key_for_correlation() {
        let target = VescCanTarget::new(VescControllerId::new(7));

        assert_eq!(
            target.request_key(CommandKind::RequestTelemetry),
            RequestKey::for_target(
                CommandKind::RequestTelemetry,
                RequestTarget::VescCanController {
                    controller_id: VescControllerId::new(7),
                }
            )
        );
    }

    #[test]
    fn falcon_encoder_matches_checked_in_write_request_fixtures() {
        let fixtures = load_request_fixtures(include_str!(
            "../fixtures/requests/falcon-read-only.requests"
        ))
        .expect("checked-in request fixtures load");

        let matched_probes = fixtures
            .iter()
            .filter(|fixture| fixture.family == DeviceFamily::BegodeFalcon)
            .filter_map(|fixture| match fixture.probe {
                ProtocolProbe::Falcon(probe) => Some((probe, fixture)),
                ProtocolProbe::Aero(_) => None,
            })
            .map(|(probe, fixture)| {
                let RequestDisposition::Write(request) = FalconRequestEncoder::encode(probe) else {
                    panic!("checked-in Falcon request fixture should be write-backed");
                };

                assert_eq!(request.probe, probe);
                assert_eq!(request.command, fixture.command);
                assert_eq!(request.mode, fixture.mode);
                assert_eq!(request.payload.as_slice(), fixture.bytes.as_slice());
                probe
            })
            .collect::<Vec<_>>();

        assert_eq!(
            matched_probes,
            vec![FalconProbe::Identity, FalconProbe::FirmwareInfo]
        );
    }

    #[test]
    fn source_backed_write_request_lengths_are_tiny_relative_to_transport_capacity() {
        let write_lengths = [
            FalconRequestEncoder::encode(FalconProbe::Identity),
            FalconRequestEncoder::encode(FalconProbe::FirmwareInfo),
        ]
        .into_iter()
        .filter_map(|disposition| match disposition {
            RequestDisposition::Write(request) => Some(request.payload.len()),
            RequestDisposition::Writes(_) | RequestDisposition::Passive { .. } => None,
        })
        .collect::<Vec<_>>();

        assert_eq!(write_lengths, vec![1, 1]);
        assert_eq!(
            write_lengths.into_iter().max(),
            Some(1),
            "Falcon N/V writes are 1 byte versus the 512-byte core transport bound"
        );
    }

    #[test]
    fn request_encoder_types_remain_bounded_in_size() {
        assert_eq!(size_of::<AeroRequestEncoder>(), 0);
        assert!(
            size_of::<RequestDisposition<AeroProbe>>()
                <= 2 * size_of::<[EncodedRequest<AeroProbe>; 4]>()
        );
        assert!(
            size_of::<RequestDisposition<FalconProbe>>()
                <= 2 * size_of::<[EncodedRequest<FalconProbe>; 4]>()
        );
    }
}
