#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]

//! Protocol-family scaffolding for Cutout.

mod gatt;
pub use gatt::*;
mod begode_frame;
pub use begode_frame::{BEGODE_FRAME_LEN, BegodeFrame, BegodeFrameError, BegodeFrameReassembler};
mod begode_bms;
pub use begode_bms::{
    BEGODE_BMS_CELL_VALUES_PER_PAGE, BegodeBmsCellPage, BegodeBmsPageError, BegodeBmsSummary,
};
mod begode_banner;
pub use begode_banner::{
    BegodeBanner, BegodeFirmwarePrefix, BegodeImuKind, parse_begode_ascii_banner,
};
mod begode_telemetry;
pub use begode_telemetry::{
    BEGODE_FALCON_TARGET_VOLTAGE_PROFILE, BEGODE_FIELD_ALERT_FLAGS,
    BEGODE_FIELD_LED_AND_LIGHT_MODE, BEGODE_FIELD_POWER_OFF_TIMER_MINUTES,
    BEGODE_FIELD_SETTINGS_BITS, BEGODE_FIELD_TILTBACK_SPEED_KMH, BegodeCapacityEvidence,
    BegodeCapacitySelection, BegodeCellModel, BegodeExtraTelemetry, BegodeLiveATelemetry,
    BegodeLiveBTelemetry, BegodePackEvidenceConsistency, BegodePackLayoutEvidence,
    BegodePackLayoutSelection, BegodePackVoltageProfile, BegodeTelemetryContext,
    BegodeTelemetryError, BegodeUnitMode, BegodeVoltageEvidence, BegodeVoltageProfileSelection,
    begode_falcon_target_voltage_profile, estimate_begode_battery_percent,
    select_begode_pack_capacity_from_annotations, select_begode_pack_layout_from_annotations,
    select_begode_pack_voltage_profile, select_begode_pack_voltage_profile_from_annotations,
    validate_begode_pack_evidence,
};
mod battery_profile;
pub use battery_profile::{
    BatteryVoltagePoint, BatteryVoltageProfile, SAMSUNG_50S_CELL_POINTS, SAMSUNG_50S_PROFILE,
};
mod family;
pub use family::*;
mod ffi;
pub use ffi::*;
mod fixture;
pub use fixture::*;
mod identification;
pub use identification::{
    IdentityConfidence, IdentityEvidence, StagedIdentityInput, StagedIdentityResolution,
    identify_model,
};
mod probe;
pub use probe::{AeroProbe, FalconProbe, ProtocolProbe};
mod registry;
pub use registry::BEGODE_FALCON_REGISTRY_ENTRY;
mod request_encoder;
mod session;
mod veteran_bms;
mod veteran_frame;
mod veteran_telemetry;
pub use request_encoder::{
    AeroRequestEncoder, EncodedRequest, FalconRequestEncoder, RequestDisposition,
};
#[cfg(feature = "dangerous-controls")]
pub use session::DangerousControlSession;
pub use session::{
    BegodeFalconModel, BegodeNotificationDecoder, BenignControlOperation,
    DangerousActuationOperation, Manufacturer, NoopNotificationDecoder, NosfetAeroModel,
    ProtocolModelSpec, ProtocolOperation, ReadOnlyModelSpec, ReadOnlyNotificationDecoder,
    ReadOnlyOperation, ReadOnlySession, SettingsWriteOperation, SupportsBenignControls,
    SupportsDangerousActuation, SupportsReadRequests, SupportsSettingsWrites,
    VeteranNotificationDecoder,
};
pub use veteran_bms::{
    VETERAN_BMS_CELL_VALUES_OFFSET, VETERAN_BMS_CELL_VALUES_PER_PAGE,
    VETERAN_BMS_PACK_CURRENT_VALUES_OFFSET, VETERAN_BMS_TEMPERATURE_VALUES_OFFSET,
    VETERAN_BMS_TEMPERATURE_VALUES_PER_PAGE, VeteranBmsCellPage, VeteranBmsMetadataPage,
    VeteranBmsPageError, VeteranBmsPageEvidence, VeteranBmsTemperaturePage,
    classify_veteran_bms_selector, decode_veteran_bms_page,
};
pub use veteran_frame::{
    MAX_VETERAN_FRAME_LEN, VeteranFrame, VeteranFrameReassembler, VeteranReassemblyError,
};
pub use veteran_telemetry::{
    NOSFET_AERO_MAX_VOLTAGE_MV, NOSFET_AERO_MIN_VOLTAGE_MV,
    VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS, VETERAN_FIELD_CHARGE_MODE,
    VETERAN_FIELD_FIRMWARE_VERSION, VETERAN_FIELD_PEDALS_MODE, VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
    VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, VeteranFirmwareVersion, VeteranModelProfile,
    VeteranTelemetry, VeteranTelemetryError, estimate_nosfet_aero_battery_percent,
    estimate_veteran_battery_percent,
};

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-protocols"
}

#[cfg(test)]
mod tests {
    use super::crate_name;
    use cutout_core::{
        Capabilities, CommandKind, LinkInfo, ProtocolSession, SessionInput, SessionOutput,
        TransportAction, WriteMode,
    };

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-protocols");
    }

    #[test]
    fn nosfet_aero_model_session_exposes_read_only_capabilities() {
        let capabilities = crate::ReadOnlySession::<crate::NosfetAeroModel, false>::capabilities();

        assert_eq!(
            capabilities,
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
                CommandKind::RequestDiagnostics,
            ])
        );
    }

    #[test]
    fn nosfet_aero_model_session_requests_subscription_on_link_up() {
        let mut session = crate::ReadOnlySession::<crate::NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );

        assert!(output.iter().any(|item| matches!(
            item,
            SessionOutput::Transport(TransportAction::Subscribe {
                channel: crate::VETERAN_DATA_CHANNEL
            })
        )));
    }

    #[test]
    fn begode_falcon_model_session_requests_subscription_on_link_up() {
        let mut session = crate::ReadOnlySession::<crate::BegodeFalconModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );

        assert!(output.iter().any(|item| matches!(
            item,
            SessionOutput::Transport(TransportAction::Subscribe {
                channel: crate::BEGODE_DATA_CHANNEL
            })
        )));
    }

    #[test]
    fn aero_request_fixture_rejects_oversized_payload() {
        let bytes = [0; cutout_core::MAX_TRANSPORT_WRITE_LEN + 1];

        assert!(matches!(
            crate::RequestFixture::new(
                crate::DeviceFamily::NosfetAero,
                crate::ProtocolProbe::Aero(crate::AeroProbe::Telemetry),
                WriteMode::WithResponse,
                &bytes,
                crate::FixtureChannels::default(),
                crate::FixtureProvenance::VendorDocumentation,
                crate::HardwareVerification::Unverified,
            ),
            Err(crate::RequestFixtureError::PayloadTooLong(_))
        ));
    }
}
