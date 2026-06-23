use arrayvec::ArrayVec;
use cutout_core::{CommandKind, RequestKey, RequestTarget, VescControllerId, WriteMode};

use crate::{
    AeroProbe, FalconProbe, VESC_MAX_FRAME_LEN, VescCanReadOnlyRequest, VescReadOnlyCodec,
    VescReadOnlyRequest,
};

const MAX_REQUEST_LEN: usize = VESC_MAX_FRAME_LEN;

/// Bounded encoded request payload plus correlation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRequest<P> {
    /// Family-specific probe represented by this request.
    pub probe: P,

    /// Generic command kind used for scheduler and response correlation.
    pub command: CommandKind,

    /// Bounded request bytes.
    pub payload: ArrayVec<u8, MAX_REQUEST_LEN>,

    /// GATT write mode required by this request.
    pub mode: WriteMode,
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
            CommandKind::RequestTelemetry => VescReadOnlyRequest::Values,
            CommandKind::RequestDiagnostics => VescReadOnlyRequest::Stats(
                crate::VescStatsMask::SPEED_AVG
                    | crate::VescStatsMask::POWER_AVG
                    | crate::VescStatsMask::CURRENT_AVG
                    | crate::VescStatsMask::COUNT_TIME,
            ),
            CommandKind::RequestIdentity
            | CommandKind::RequestBatteryInfo
            | CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => return None,
        };
        let mut payload = ArrayVec::new();
        VescReadOnlyCodec::encode_request(request, &mut payload).ok()?;
        Some(RequestDisposition::Write(EncodedRequest {
            probe: request,
            command: kind,
            payload,
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
            | CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => return None,
        };
        let request = VescReadOnlyRequest::ForwardCan {
            controller_id: self.controller_id,
            request,
        };
        let mut payload = ArrayVec::new();
        VescReadOnlyCodec::encode_request(request, &mut payload).ok()?;
        Some(RequestDisposition::Write(EncodedRequest {
            probe: request,
            command: kind,
            payload,
            mode: WriteMode::WithoutResponse,
        }))
    }
}

fn request_payload(bytes: &[u8]) -> ArrayVec<u8, MAX_REQUEST_LEN> {
    let mut payload = ArrayVec::new();
    for byte in bytes {
        let pushed = payload.try_push(*byte);
        debug_assert!(pushed.is_ok());
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceFamily, ProtocolProbe, load_request_fixtures};
    use core::mem::size_of;

    #[test]
    fn falcon_encoder_uses_expected_request_bytes() {
        let identity = FalconRequestEncoder::encode(FalconProbe::Identity);
        let firmware = FalconRequestEncoder::encode(FalconProbe::FirmwareInfo);
        let telemetry = FalconRequestEncoder::encode(FalconProbe::Telemetry);
        let battery = FalconRequestEncoder::encode(FalconProbe::BatteryInfo);

        assert!(matches!(
            identity,
            RequestDisposition::Write(EncodedRequest {
                command: CommandKind::RequestIdentity,
                mode: WriteMode::WithoutResponse,
                ..
            })
        ));
        assert!(matches!(
            firmware,
            RequestDisposition::Write(EncodedRequest {
                command: CommandKind::RequestFirmwareInfo,
                mode: WriteMode::WithoutResponse,
                ..
            })
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
                RequestDisposition::Passive { .. } => unreachable!(),
            }
            .as_slice(),
            b"N"
        );
        assert_eq!(
            match firmware {
                RequestDisposition::Write(request) => request.payload,
                RequestDisposition::Passive { .. } => unreachable!(),
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
            Some(RequestDisposition::Write(EncodedRequest {
                command: CommandKind::RequestIdentity,
                mode: WriteMode::WithoutResponse,
                ..
            }))
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
            RequestDisposition::Passive { .. } => None,
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
            size_of::<RequestDisposition<AeroProbe>>() <= size_of::<EncodedRequest<AeroProbe>>()
        );
        assert!(
            size_of::<RequestDisposition<FalconProbe>>()
                <= size_of::<EncodedRequest<FalconProbe>>()
        );
    }
}
