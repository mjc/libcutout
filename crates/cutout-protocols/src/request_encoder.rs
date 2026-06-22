use arrayvec::ArrayVec;
use cutout_core::{CommandKind, WriteMode};

use crate::{AeroProbe, FalconProbe};

const MAX_REQUEST_LEN: usize = 24;

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
