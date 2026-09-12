use arrayvec::ArrayVec;
use crc32fast::hash as crc32;
use cutout_core::{
    CommandKind, DeviceCommand, LightState, PedalMode, PendingProbe, RequestKey, RequestTarget,
    RollAngle, SpeedAlarmMode, VescControllerId, WriteMode, WritePayload,
};

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

/// NOSFET Aero benign-control encoder.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AeroControlEncoder;

impl AeroControlEncoder {
    /// Encodes a supported NOSFET Aero benign control.
    #[must_use]
    pub fn encode(command: DeviceCommand) -> Option<EncodedControl> {
        let payload = match command {
            DeviceCommand::SetLights(LightState::On) => b"SetLightON".as_slice(),
            DeviceCommand::SetLights(LightState::Off) => b"SetLightOFF".as_slice(),
            DeviceCommand::SetPedalMode(PedalMode::Hard) => b"SETh".as_slice(),
            DeviceCommand::SetPedalMode(PedalMode::Medium) => b"SETm".as_slice(),
            DeviceCommand::SetPedalMode(PedalMode::Soft) => b"SETs".as_slice(),
            DeviceCommand::ResetTripMeter => b"CLEARMETER".as_slice(),
            DeviceCommand::SetAeroDisplayBacklight(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
                        value.percent(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroBeeperVolume(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                        ],
                        value.percent(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroDynamicAssist(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                        ],
                        value.percent(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroPedalDipCompensation(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                        ],
                        value.percent(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroLateralTiltLimit(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LkAp",
                        &[
                            0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                        ],
                        value.degrees(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroVoltageCorrection(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80, 0x80,
                        ],
                        u8::from_ne_bytes(value.tenths_of_percent().to_ne_bytes()),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroMaxChargeVoltageRaw(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                        ],
                        value.raw(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroTiltbackSpeed(speed) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80],
                        speed.kilometres_per_hour(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroPwmPercent(percent) => {
                let wire_value = match percent {
                    cutout_core::AeroPwmSetting::Off => 200,
                    cutout_core::AeroPwmSetting::Margin(margin) => 100 - margin.percent(),
                };
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
                        wire_value,
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroPwmOff => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
                        200,
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroGyroCalibration => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                        ],
                        1,
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroBrakeOverpressureAlarm(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                        ],
                        value.percent(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroRidingMode(mode) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(*b"LkAp", &[0x01, 0x80], mode.wire_value())?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroPedalHardness(percent) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[0x01, 0x02, 0x80, 0x80, 0x80],
                        percent.percent(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroWheelUnits(units) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80,
                        ],
                        units.display_mode(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroHighSpeedMode(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80, 0x80, 0x80, 0x80,
                        ],
                        u8::from(value.enabled()),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroLowBatteryMode(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                            0x80, 0x80, 0x80,
                        ],
                        u8::from(value.enabled()),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroTransportMode(value) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LdAp",
                        &[
                            0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
                        ],
                        u8::from(value.enabled()),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroAlarmSpeed(speed) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LkAp",
                        &[0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
                        speed.kilometres_per_hour(),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            DeviceCommand::SetAeroAngleAdjustment(angle) => {
                return Some(EncodedControl {
                    command: command.kind(),
                    payload: aero_binary_frame(
                        *b"LkAp",
                        &[0x01, 0x80, 0x80, 0x80, 0x80, 0x80],
                        u8::from_ne_bytes(angle.tenths_of_degree().to_ne_bytes()),
                    )?,
                    mode: WriteMode::WithoutResponse,
                });
            }
            _ => return None,
        };
        Some(EncodedControl {
            command: command.kind(),
            payload: request_payload(payload),
            mode: WriteMode::WithoutResponse,
        })
    }

    /// Encodes the official NOSFET high-beam frame.
    ///
    /// The official Android app writes only the `LkAp` frame here. The
    /// `LdAp` frame used by older reverse-engineered captures is not a
    /// companion for Aero's light command; sending it makes the wheel emit
    /// an extra acknowledgement beep.
    #[must_use]
    pub fn encode_settings_sequence(command: DeviceCommand) -> Option<EncodedControlSequence> {
        let state = match command {
            DeviceCommand::SetAeroHighBeam(LightState::On) => 1,
            DeviceCommand::SetAeroHighBeam(LightState::Off) => 0,
            _ => return None,
        };
        let mut steps = ArrayVec::new();
        steps.push(EncodedControlStep {
            delay_ms: 0,
            payload: aero_binary_frame(*b"LkAp", &[0x01, 0x80, 0x80], state)?,
            mode: WriteMode::WithoutResponse,
        });
        Some(EncodedControlSequence {
            command: command.kind(),
            steps,
        })
    }
}

fn aero_binary_frame(magic: [u8; 4], payload_head: &[u8], value: u8) -> Option<WritePayload> {
    let length = payload_head.len() + 10;
    let length = u8::try_from(length).ok()?;
    let mut frame = ArrayVec::<u8, 33>::new();
    frame.try_extend_from_slice(&magic).ok()?;
    frame.push(length);
    frame.try_extend_from_slice(payload_head).ok()?;
    frame.push(value);
    let crc = crc32(frame.as_slice()).to_be_bytes();
    frame.try_extend_from_slice(&crc).ok()?;
    Some(request_payload(frame.as_slice()))
}

/// Begode Falcon benign-control encoder.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FalconControlEncoder;

impl FalconControlEncoder {
    /// Encodes a supported Begode Falcon benign control.
    #[must_use]
    pub fn encode(command: DeviceCommand) -> Option<EncodedControl> {
        let payload = match command {
            DeviceCommand::SetLights(LightState::On) => b"Q".as_slice(),
            DeviceCommand::SetLights(LightState::Off) => b"E".as_slice(),
            DeviceCommand::SetLights(LightState::Strobe) => b"T".as_slice(),
            DeviceCommand::SetPedalMode(PedalMode::Hard) => b"h".as_slice(),
            DeviceCommand::SetPedalMode(PedalMode::Medium) => b"f".as_slice(),
            DeviceCommand::SetPedalMode(PedalMode::Soft) => b"s".as_slice(),
            DeviceCommand::SetRollAngle(RollAngle::Low) => b">".as_slice(),
            DeviceCommand::SetRollAngle(RollAngle::Medium) => b"=".as_slice(),
            DeviceCommand::SetRollAngle(RollAngle::High) => b"<".as_slice(),
            DeviceCommand::SetSpeedAlarmMode(SpeedAlarmMode::Both) => b"o".as_slice(),
            DeviceCommand::SetSpeedAlarmMode(SpeedAlarmMode::StageOneOnly) => b"u".as_slice(),
            _ => return None,
        };
        Some(EncodedControl {
            command: command.kind(),
            payload: request_payload(payload),
            mode: WriteMode::WithoutResponse,
        })
    }

    /// Encodes a documented Begode `W` submenu as timed transport writes.
    #[must_use]
    pub fn encode_settings_sequence(command: DeviceCommand) -> Option<EncodedControlSequence> {
        let mut steps = ArrayVec::new();
        let mut push = |delay_ms, payload: &[u8]| {
            steps.push(EncodedControlStep {
                delay_ms,
                payload: request_payload(payload),
                mode: WriteMode::WithoutResponse,
            });
        };
        match command {
            DeviceCommand::SetBegodeMaxSpeed(speed) => {
                let value = speed.kilometres_per_hour();
                push(0, b"W");
                push(100, b"Y");
                push(200, &[b'0' + value / 10]);
                push(200, &[b'0' + value % 10]);
                push(200, b"b");
            }
            DeviceCommand::SetBegodeBeeperVolume(volume) => {
                push(0, b"W");
                push(100, b"B");
                push(200, &[b'0' + volume.level()]);
                push(200, b"b");
            }
            DeviceCommand::SetBegodeLedMode(mode) => {
                push(0, b"W");
                push(100, b"M");
                push(200, &[b'0' + mode.mode()]);
                push(200, b"b");
            }
            _ => return None,
        }
        Some(EncodedControlSequence {
            command: command.kind(),
            steps,
        })
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
            | CommandKind::SetAeroTiltbackSpeed
            | CommandKind::SetAeroPwmPercent
            | CommandKind::SetAeroPwmOff
            | CommandKind::SetAeroGyroCalibration
            | CommandKind::SetAeroRidingMode
            | CommandKind::SetAeroBrakeOverpressureAlarm
            | CommandKind::SetAeroPedalHardness
            | CommandKind::SetAeroDisplayBacklight
            | CommandKind::SetAeroBeeperVolume
            | CommandKind::SetAeroDynamicAssist
            | CommandKind::SetAeroPedalDipCompensation
            | CommandKind::SetAeroLateralTiltLimit
            | CommandKind::SetAeroVoltageCorrection
            | CommandKind::SetAeroMaxChargeVoltageRaw
            | CommandKind::SetAeroWheelUnits
            | CommandKind::SetAeroHighSpeedMode
            | CommandKind::SetAeroLowBatteryMode
            | CommandKind::SetAeroTransportMode
            | CommandKind::SetAeroAlarmSpeed
            | CommandKind::SetAeroAngleAdjustment
            | CommandKind::SetAeroHighBeam
            | CommandKind::SetAccelerationAssist
            | CommandKind::SetLights
            | CommandKind::SetPedalMode
            | CommandKind::SetRollAngle
            | CommandKind::SetSpeedAlarmMode
            | CommandKind::SetBegodeMaxSpeed
            | CommandKind::SetBegodeBeeperVolume
            | CommandKind::SetBegodeLedMode
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
            | CommandKind::SetAeroTiltbackSpeed
            | CommandKind::SetAeroPwmPercent
            | CommandKind::SetAeroPwmOff
            | CommandKind::SetAeroGyroCalibration
            | CommandKind::SetAeroRidingMode
            | CommandKind::SetAeroBrakeOverpressureAlarm
            | CommandKind::SetAeroPedalHardness
            | CommandKind::SetAeroDisplayBacklight
            | CommandKind::SetAeroBeeperVolume
            | CommandKind::SetAeroDynamicAssist
            | CommandKind::SetAeroPedalDipCompensation
            | CommandKind::SetAeroLateralTiltLimit
            | CommandKind::SetAeroVoltageCorrection
            | CommandKind::SetAeroMaxChargeVoltageRaw
            | CommandKind::SetAeroWheelUnits
            | CommandKind::SetAeroHighSpeedMode
            | CommandKind::SetAeroLowBatteryMode
            | CommandKind::SetAeroTransportMode
            | CommandKind::SetAeroAlarmSpeed
            | CommandKind::SetAeroAngleAdjustment
            | CommandKind::SetAeroHighBeam
            | CommandKind::SetAccelerationAssist
            | CommandKind::SetLights
            | CommandKind::SetPedalMode
            | CommandKind::SetRollAngle
            | CommandKind::SetSpeedAlarmMode
            | CommandKind::SetBegodeMaxSpeed
            | CommandKind::SetBegodeBeeperVolume
            | CommandKind::SetBegodeLedMode
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

fn request_payload(bytes: &[u8]) -> WritePayload {
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
    use cutout_core::{BegodeBeeperVolume, BegodeLedModeSetting, BegodeMaxSpeed};

    #[test]
    fn aero_control_encoder_uses_silent_ascii_light_commands() {
        let on = AeroControlEncoder::encode(DeviceCommand::SetLights(LightState::On))
            .expect("NOSFET lights-on command encodes");
        let off = AeroControlEncoder::encode(DeviceCommand::SetLights(LightState::Off))
            .expect("NOSFET lights-off command encodes");

        assert_eq!(on.command, CommandKind::SetLights);
        assert_eq!(on.payload.as_slice(), b"SetLightON");
        assert_eq!(on.mode, WriteMode::WithoutResponse);
        assert_eq!(off.command, CommandKind::SetLights);
        assert_eq!(off.payload.as_slice(), b"SetLightOFF");
        assert_eq!(off.mode, WriteMode::WithoutResponse);
        assert_eq!(
            AeroControlEncoder::encode(DeviceCommand::SetLights(LightState::Strobe)),
            None
        );
        assert_eq!(AeroControlEncoder::encode(DeviceCommand::SoundHorn), None);
    }

    #[test]
    fn aero_control_encoder_resets_the_trip_meter_with_the_documented_command() {
        let reset = AeroControlEncoder::encode(DeviceCommand::ResetTripMeter)
            .expect("trip reset is a supported Aero settings write");

        assert_eq!(reset.command, CommandKind::ResetTripMeter);
        assert_eq!(reset.payload.as_slice(), b"CLEARMETER");
        assert_eq!(reset.mode, WriteMode::WithoutResponse);
    }

    #[test]
    fn aero_binary_settings_match_the_captured_frame_shapes_and_crc() {
        let cases = [
            (
                DeviceCommand::SetAeroTiltbackSpeed(
                    cutout_core::AeroSpeedSetting::new(21).expect("21 km/h fits"),
                ),
                *b"LdAp",
                17,
                &[0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 21][..],
            ),
            (
                DeviceCommand::SetAeroPwmPercent(
                    cutout_core::AeroPwmPercent::new(64)
                        .expect("64 percent fits")
                        .into(),
                ),
                *b"LdAp",
                18,
                &[0x01, 0x02, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 36][..],
            ),
            (
                DeviceCommand::SetAeroAlarmSpeed(
                    cutout_core::AeroSpeedSetting::new(20).expect("20 km/h fits"),
                ),
                *b"LkAp",
                17,
                &[0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 20][..],
            ),
            (
                DeviceCommand::SetAeroAngleAdjustment(
                    cutout_core::AeroAngleAdjustment::new(-36).expect("-3.6 degrees fits"),
                ),
                *b"LkAp",
                16,
                &[0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 220][..],
            ),
        ];

        for (command, magic, length, body) in cases {
            let encoded = AeroControlEncoder::encode(command).expect("Aero setting encodes");
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
    fn aero_pwm_off_is_distinct_from_zero_margin() {
        let dedicated_off = AeroControlEncoder::encode(DeviceCommand::SetAeroPwmOff);
        assert_eq!(
            dedicated_off.unwrap().payload.as_slice(),
            &hex_literal::hex!("4c644170120102808080808080c8d24c759e")
        );
        let off = AeroControlEncoder::encode(DeviceCommand::SetAeroPwmPercent(
            cutout_core::AeroPwmSetting::Off,
        ))
        .unwrap();
        assert_eq!(
            off.payload.as_slice(),
            &hex_literal::hex!("4c644170120102808080808080c8d24c759e")
        );
        let zero = AeroControlEncoder::encode(DeviceCommand::SetAeroPwmPercent(
            cutout_core::AeroPwmPercent::new(0).unwrap().into(),
        ))
        .unwrap();
        assert_eq!(zero.payload.as_slice()[13], 100);
        assert_ne!(off.payload, zero.payload);
    }

    #[test]
    fn aero_modern_riding_mode_uses_the_source_backed_lkap_t_frame() {
        let cases = [
            (
                cutout_core::AeroRidingMode::Soft,
                hex_literal::hex!("4c6b41700c018001a8e75480"),
            ),
            (
                cutout_core::AeroRidingMode::Medium,
                hex_literal::hex!("4c6b41700c01800231ee053a"),
            ),
            (
                cutout_core::AeroRidingMode::Hard,
                hex_literal::hex!("4c6b41700c01800346e935ac"),
            ),
        ];

        for (mode, expected) in cases {
            let encoded = AeroControlEncoder::encode(DeviceCommand::SetAeroRidingMode(mode))
                .expect("modern binary T mode encodes");
            assert_eq!(encoded.command, CommandKind::SetAeroRidingMode);
            assert_eq!(encoded.mode, WriteMode::WithoutResponse);
            assert_eq!(encoded.payload.as_slice(), expected);
        }
    }

    #[test]
    fn aero_extended_settings_match_euc_world_frames() {
        let cases = [
            (
                DeviceCommand::SetAeroDisplayBacklight(
                    cutout_core::AeroDisplayBacklight::new(50).unwrap(),
                ),
                hex_literal::hex!("4c644170140102808080808080808032ec9452c7").as_slice(),
            ),
            (
                DeviceCommand::SetAeroBeeperVolume(cutout_core::AeroBeeperVolume::new(75).unwrap()),
                hex_literal::hex!("4c6441701c0102808080808080808080808080808080804b930b4f71")
                    .as_slice(),
            ),
            (
                DeviceCommand::SetAeroDynamicAssist(
                    cutout_core::AeroDynamicAssist::new(60).unwrap(),
                ),
                hex_literal::hex!("4c6441701f0102808080808080808080808080808080808080803c6831fed2")
                    .as_slice(),
            ),
            (
                DeviceCommand::SetAeroPedalDipCompensation(
                    cutout_core::AeroPedalDipCompensation::new(40).unwrap(),
                ),
                hex_literal::hex!(
                    "4c64417021010280808080808080808080808080808080808080808028c549a32e"
                )
                .as_slice(),
            ),
            (
                DeviceCommand::SetAeroLateralTiltLimit(
                    cutout_core::AeroLateralTiltLimit::new(55).unwrap(),
                ),
                hex_literal::hex!("4c6b41701601808080808080808080808037aef39e07").as_slice(),
            ),
            (
                DeviceCommand::SetAeroVoltageCorrection(
                    cutout_core::AeroVoltageCorrection::new(-15).unwrap(),
                ),
                hex_literal::hex!("4c644170180102808080808080808080808080f129076df6").as_slice(),
            ),
        ];
        for (command, expected) in cases {
            let encoded =
                AeroControlEncoder::encode(command).expect("source-backed setting encodes");
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
                DeviceCommand::SetAeroHighSpeedMode(cutout_core::AeroHighSpeedMode::new(true)),
                hex_literal::hex!("4c6441701a01028080808080808080808080808080012c5fa11f")
                    .as_slice(),
            ),
            (
                DeviceCommand::SetAeroLowBatteryMode(cutout_core::AeroLowBatteryMode::new(false)),
                hex_literal::hex!("4c644170190102808080808080808080808080800037773c3d").as_slice(),
            ),
            (
                DeviceCommand::SetAeroTransportMode(cutout_core::AeroTransportMode::new(true)),
                hex_literal::hex!("4c64417016010280808080808080808080012b37c934").as_slice(),
            ),
        ];

        for (command, expected) in cases {
            let encoded = AeroControlEncoder::encode(command).expect("Aero toggle encodes");
            assert_eq!(encoded.payload.as_slice(), expected);
            assert_eq!(encoded.command, command.kind());
            assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        }
    }

    #[test]
    fn aero_gyro_calibration_matches_the_official_frame_and_crc() {
        let command = DeviceCommand::SetAeroGyroCalibration;
        let encoded = AeroControlEncoder::encode(command).expect("gyro calibration encodes");
        assert_eq!(
            encoded.payload.as_slice(),
            &hex_literal::hex!("4c64417015010280808080808080808001ab8c09e5")
        );
        assert_eq!(encoded.command, CommandKind::SetAeroGyroCalibration);
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        assert_eq!(
            command.safety_class(),
            cutout_core::SafetyClass::StationaryOnly
        );
    }

    #[test]
    fn aero_brake_overpressure_alarm_matches_the_official_frame_and_crc() {
        let command = DeviceCommand::SetAeroBrakeOverpressureAlarm(
            cutout_core::AeroBrakeOverpressureAlarm::new(100).expect("100 percent fits"),
        );
        let encoded = AeroControlEncoder::encode(command).expect("brake alarm encodes");
        assert_eq!(
            encoded.payload.as_slice(),
            &hex_literal::hex!("4c6441701e01028080808080808080808080808080808080806453681869")
        );
        assert_eq!(encoded.command, CommandKind::SetAeroBrakeOverpressureAlarm);
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        assert_eq!(
            command.safety_class(),
            cutout_core::SafetyClass::StationaryOnly
        );
    }

    #[test]
    fn aero_max_charge_voltage_raw_matches_the_official_ldap_frame_and_crc() {
        let command = DeviceCommand::SetAeroMaxChargeVoltageRaw(
            cutout_core::AeroMaxChargeVoltageRaw::new(46).expect("official raw MxV value fits"),
        );
        let encoded = AeroControlEncoder::encode(command).expect("MxV encodes");
        assert_eq!(encoded.command, CommandKind::SetAeroMaxChargeVoltageRaw);
        assert_eq!(encoded.mode, WriteMode::WithoutResponse);
        let bytes = encoded.payload.as_slice();
        assert_eq!(&bytes[..5], b"LdAp\x1d");
        assert_eq!(bytes[24], 46);
        assert_eq!(bytes.len(), 29);
        let declared_len = usize::from(bytes[4]);
        assert_eq!(declared_len, 29);
        assert_eq!(
            &bytes[declared_len - 4..],
            &crc32(&bytes[..declared_len - 4]).to_be_bytes()
        );
    }

    #[test]
    fn aero_md_hardness_matches_the_documented_frame_and_crc() {
        let command = DeviceCommand::SetAeroPedalHardness(
            cutout_core::AeroPedalHardness::new(100).expect("100 percent is documented"),
        );
        let encoded = AeroControlEncoder::encode(command).expect("MD is supported");
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
        let command = DeviceCommand::SetAeroWheelUnits(cutout_core::AeroWheelUnits::Imperial);
        let encoded = AeroControlEncoder::encode(command).expect("wheel display units supported");
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
    fn aero_high_beam_encodes_the_official_single_lkap_frame() {
        let sequence = AeroControlEncoder::encode_settings_sequence(
            DeviceCommand::SetAeroHighBeam(LightState::On),
        )
        .expect("Aero high beam sequence encodes");

        assert_eq!(sequence.command, CommandKind::SetAeroHighBeam);
        assert_eq!(sequence.steps.len(), 1);
        assert_eq!(sequence.steps[0].delay_ms, 0);
        assert_eq!(&sequence.steps[0].payload.as_slice()[..5], b"LkAp\r");
        assert_eq!(
            &sequence.steps[0].payload.as_slice()[5..9],
            &[1, 0x80, 0x80, 1]
        );
        let step = &sequence.steps[0];
        let frame_len = usize::from(step.payload.as_slice()[4]);
        let crc_offset = frame_len - 4;
        let expected_crc = crc32(&step.payload.as_slice()[..crc_offset]).to_be_bytes();
        assert_eq!(&step.payload.as_slice()[crc_offset..], &expected_crc);
        assert_eq!(step.mode, WriteMode::WithoutResponse);
    }

    #[test]
    fn falcon_control_encoder_uses_explicit_begode_light_commands() {
        let on = FalconControlEncoder::encode(DeviceCommand::SetLights(LightState::On))
            .expect("Begode lights-on command encodes");
        let off = FalconControlEncoder::encode(DeviceCommand::SetLights(LightState::Off))
            .expect("Begode lights-off command encodes");

        assert_eq!(on.command, CommandKind::SetLights);
        assert_eq!(on.payload.as_slice(), b"Q");
        assert_eq!(on.mode, WriteMode::WithoutResponse);
        assert_eq!(off.command, CommandKind::SetLights);
        assert_eq!(off.payload.as_slice(), b"E");
        assert_eq!(off.mode, WriteMode::WithoutResponse);
        let strobe = FalconControlEncoder::encode(DeviceCommand::SetLights(LightState::Strobe))
            .expect("Begode strobe command encodes");
        assert_eq!(strobe.command, CommandKind::SetLights);
        assert_eq!(strobe.payload.as_slice(), b"T");
        assert_eq!(strobe.mode, WriteMode::WithoutResponse);
        assert_eq!(FalconControlEncoder::encode(DeviceCommand::SoundHorn), None);
    }

    #[test]
    fn documented_pedal_mode_encoders_match_veteran_and_begode_bytes() {
        let aero = AeroControlEncoder::encode(DeviceCommand::SetPedalMode(PedalMode::Hard))
            .expect("documented Veteran pedal mode encoder");
        assert_eq!(aero.command, CommandKind::SetPedalMode);
        assert_eq!(aero.payload.as_slice(), b"SETh");

        let falcon = FalconControlEncoder::encode(DeviceCommand::SetPedalMode(PedalMode::Soft))
            .expect("documented Begode pedal mode encoder");
        assert_eq!(falcon.command, CommandKind::SetPedalMode);
        assert_eq!(falcon.payload.as_slice(), b"s");
    }

    #[test]
    fn documented_falcon_roll_angle_encoders_match_protocol_bytes() {
        let low = FalconControlEncoder::encode(DeviceCommand::SetRollAngle(RollAngle::Low))
            .expect("Begode low roll-angle encoder");
        let medium = FalconControlEncoder::encode(DeviceCommand::SetRollAngle(RollAngle::Medium))
            .expect("Begode medium roll-angle encoder");
        let high = FalconControlEncoder::encode(DeviceCommand::SetRollAngle(RollAngle::High))
            .expect("Begode high roll-angle encoder");

        assert_eq!(low.command, CommandKind::SetRollAngle);
        assert_eq!(low.payload.as_slice(), b">");
        assert_eq!(medium.payload.as_slice(), b"=");
        assert_eq!(high.payload.as_slice(), b"<");
        assert_eq!(low.mode, WriteMode::WithoutResponse);
        assert_eq!(
            AeroControlEncoder::encode(DeviceCommand::SetRollAngle(RollAngle::Low)),
            None
        );
    }

    #[test]
    fn documented_falcon_speed_alarm_encoders_match_protocol_bytes() {
        let both =
            FalconControlEncoder::encode(DeviceCommand::SetSpeedAlarmMode(SpeedAlarmMode::Both))
                .expect("Begode both-alarms encoder");
        let stage_one = FalconControlEncoder::encode(DeviceCommand::SetSpeedAlarmMode(
            SpeedAlarmMode::StageOneOnly,
        ))
        .expect("Begode stage-one-only encoder");

        assert_eq!(both.command, CommandKind::SetSpeedAlarmMode);
        assert_eq!(both.payload.as_slice(), b"o");
        assert_eq!(stage_one.payload.as_slice(), b"u");
        assert_eq!(both.mode, WriteMode::WithoutResponse);
        assert_eq!(
            AeroControlEncoder::encode(DeviceCommand::SetSpeedAlarmMode(SpeedAlarmMode::Both,)),
            None
        );
    }

    #[test]
    fn falcon_w_settings_encode_as_delayed_ordered_writes() {
        let max_speed =
            FalconControlEncoder::encode_settings_sequence(DeviceCommand::SetBegodeMaxSpeed(
                BegodeMaxSpeed::new(30).expect("30 km/h is encodable"),
            ))
            .expect("max-speed sequence encodes");
        assert_eq!(max_speed.command, CommandKind::SetBegodeMaxSpeed);
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

        let volume =
            FalconControlEncoder::encode_settings_sequence(DeviceCommand::SetBegodeBeeperVolume(
                BegodeBeeperVolume::new(7).expect("volume 7 is encodable"),
            ))
            .expect("beeper sequence encodes");
        assert_eq!(volume.command, CommandKind::SetBegodeBeeperVolume);
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

        let led = FalconControlEncoder::encode_settings_sequence(DeviceCommand::SetBegodeLedMode(
            BegodeLedModeSetting::new(4).expect("LED mode 4 is encodable"),
        ))
        .expect("LED sequence encodes");
        assert_eq!(led.command, CommandKind::SetBegodeLedMode);
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
