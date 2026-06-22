use cutout_core::{CommandKind, WriteMode, WritePayload, WritePayloadTooLong};
use thiserror::Error;

use crate::{DeviceFamily, ProtocolProbe};

/// Optional service/characteristic channels observed for a fixture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FixtureChannels {
    /// Optional GATT service or endpoint group identifier.
    pub service: Option<cutout_core::GattChannel>,

    /// Optional GATT characteristic or write endpoint identifier.
    pub characteristic: Option<cutout_core::GattChannel>,
}

/// Provenance category for capture-backed request fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureProvenance {
    /// Observed from a Bluetooth capture.
    BluetoothCapture,

    /// Observed from an application trace.
    AppTrace,

    /// Taken from source-attributed vendor or protocol documentation.
    VendorDocumentation,
}

/// Whether request fixture bytes have been verified against real hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareVerification {
    /// Fixture bytes have not been verified against real Bluetooth hardware.
    Unverified,

    /// Fixture bytes have been verified against real Bluetooth hardware.
    VerifiedOnBluetooth,
}

/// Capture/spec-backed request fixture record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestFixture {
    /// Device family the fixture applies to.
    pub family: DeviceFamily,

    /// Family-specific probe encoded by this fixture.
    pub probe: ProtocolProbe,

    /// Generic command kind used for scheduler and response correlation.
    pub command: CommandKind,

    /// Transport write behavior observed for the request.
    pub mode: WriteMode,

    /// Bounded request bytes.
    pub bytes: WritePayload,

    /// Optional service/characteristic evidence.
    pub channels: FixtureChannels,

    /// Source category for the fixture evidence.
    pub provenance: FixtureProvenance,

    /// Hardware verification state for the fixture.
    pub hardware_verification: HardwareVerification,
}

impl RequestFixture {
    /// Creates a request fixture after validating family/probe and byte bounds.
    ///
    /// # Errors
    ///
    /// Returns [`RequestFixtureError::FamilyMismatch`] when the probe belongs
    /// to a different family, or [`RequestFixtureError::PayloadTooLong`] when
    /// the request bytes exceed the core transport write bound.
    pub fn new(
        family: DeviceFamily,
        probe: ProtocolProbe,
        mode: WriteMode,
        bytes: &[u8],
        channels: FixtureChannels,
        provenance: FixtureProvenance,
        hardware_verification: HardwareVerification,
    ) -> Result<Self, RequestFixtureError> {
        let probe_family = probe.family();
        if family != probe_family {
            return Err(RequestFixtureError::FamilyMismatch {
                family,
                probe_family,
            });
        }
        Ok(Self {
            family,
            probe,
            command: probe.command_kind(),
            mode,
            bytes: WritePayload::try_from_slice(bytes)
                .map_err(RequestFixtureError::PayloadTooLong)?,
            channels,
            provenance,
            hardware_verification,
        })
    }
}

/// Request fixture validation error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RequestFixtureError {
    /// Probe belongs to a different protocol family.
    #[error("fixture family {family:?} does not match probe family {probe_family:?}")]
    FamilyMismatch {
        /// Fixture device family.
        family: DeviceFamily,

        /// Family implied by the probe.
        probe_family: DeviceFamily,
    },

    /// Command is unsupported by a protocol family.
    #[error("command {command:?} is unsupported by fixture family {family:?}")]
    UnsupportedCommand {
        /// Device family requested for mapping.
        family: DeviceFamily,

        /// Unsupported command.
        command: CommandKind,
    },

    /// Request bytes exceed the transport payload bound.
    #[error(transparent)]
    PayloadTooLong(WritePayloadTooLong),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AeroProbe, VETERAN_DATA_CHANNEL, VETERAN_SERVICE_CHANNEL};

    #[test]
    fn request_fixture_keeps_evidence_metadata() {
        let channels = FixtureChannels {
            service: Some(VETERAN_SERVICE_CHANNEL),
            characteristic: Some(VETERAN_DATA_CHANNEL),
        };
        let fixture = RequestFixture::new(
            DeviceFamily::NosfetAero,
            ProtocolProbe::Aero(AeroProbe::Diagnostics),
            WriteMode::WithResponse,
            &[0x10, 0x20, 0x30],
            channels,
            FixtureProvenance::AppTrace,
            HardwareVerification::VerifiedOnBluetooth,
        )
        .expect("fixture should validate");

        assert_eq!(fixture.family, DeviceFamily::NosfetAero);
        assert_eq!(fixture.probe, ProtocolProbe::Aero(AeroProbe::Diagnostics));
        assert_eq!(fixture.command, CommandKind::RequestDiagnostics);
        assert_eq!(fixture.mode, WriteMode::WithResponse);
        assert_eq!(fixture.bytes.as_slice(), &[0x10, 0x20, 0x30]);
        assert_eq!(fixture.channels, channels);
        assert_eq!(fixture.provenance, FixtureProvenance::AppTrace);
        assert_eq!(
            fixture.hardware_verification,
            HardwareVerification::VerifiedOnBluetooth
        );
    }

    #[test]
    fn request_fixture_rejects_family_mismatch() {
        assert!(matches!(
            RequestFixture::new(
                DeviceFamily::BegodeFalcon,
                ProtocolProbe::Aero(AeroProbe::Identity),
                WriteMode::WithoutResponse,
                b"N",
                FixtureChannels::default(),
                FixtureProvenance::VendorDocumentation,
                HardwareVerification::Unverified,
            ),
            Err(RequestFixtureError::FamilyMismatch {
                family: DeviceFamily::BegodeFalcon,
                probe_family: DeviceFamily::NosfetAero,
            })
        ));
    }
}
