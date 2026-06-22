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

/// Request encoder for NOSFET Aero/Veteran-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AeroRequestEncoder;

impl AeroRequestEncoder {
    /// Encodes a supported Aero/Veteran-family probe.
    #[must_use]
    pub const fn encode(probe: AeroProbe) -> Option<EncodedRequest<AeroProbe>> {
        let _ = probe;
        None
    }

    /// Encodes a generic command if it belongs to the Aero/Veteran probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<EncodedRequest<AeroProbe>> {
        Self::encode(AeroProbe::from_command_kind(kind)?)
    }
}

/// Request encoder for source-backed Begode/Falcon-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FalconRequestEncoder;

impl FalconRequestEncoder {
    /// Encodes a supported Begode/Falcon-family probe.
    #[must_use]
    pub fn encode(probe: FalconProbe) -> Option<EncodedRequest<FalconProbe>> {
        let payload = match probe {
            FalconProbe::Identity => Some(b"N".as_slice()),
            FalconProbe::FirmwareInfo => Some(b"V".as_slice()),
            FalconProbe::Telemetry | FalconProbe::BatteryInfo => None,
        }?;

        Some(EncodedRequest {
            probe,
            command: probe.command_kind(),
            payload: request_payload(payload),
            mode: WriteMode::WithoutResponse,
        })
    }

    /// Encodes a generic command if it belongs to the Begode/Falcon probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<EncodedRequest<FalconProbe>> {
        FalconProbe::from_command_kind(kind).and_then(Self::encode)
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

    #[test]
    fn falcon_encoder_uses_expected_request_bytes() {
        let identity = FalconRequestEncoder::encode(FalconProbe::Identity)
            .expect("identity request is supported");
        let firmware = FalconRequestEncoder::encode(FalconProbe::FirmwareInfo)
            .expect("firmware request is supported");

        assert_eq!(identity.payload.as_slice(), b"N");
        assert_eq!(identity.command, CommandKind::RequestIdentity);
        assert_eq!(identity.mode, WriteMode::WithoutResponse);
        assert_eq!(firmware.payload.as_slice(), b"V");
    }

    #[test]
    fn passive_aero_encoder_remains_write_free() {
        assert_eq!(AeroRequestEncoder::encode(AeroProbe::Identity), None);
        assert_eq!(
            AeroRequestEncoder::encode_command(CommandKind::RequestTelemetry),
            None
        );
    }
}
