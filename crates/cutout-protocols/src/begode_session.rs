use cutout_core::{
    Capabilities, CommandKind, DeviceEvent, GattChannel, GattFingerprint, GattRoles,
    ModelRegistryEntry, MonotonicTimestamp, NotificationByteLen, NotificationIngestOutcome,
    ParserError, ProtocolFamily, SemanticEventCount, SessionOutput, SessionOutputError,
    SessionOutputSink, VerificationStatus,
};

use crate::{
    BEGODE_DATA_CHANNEL, BEGODE_SERVICE_CHANNEL, BegodeBmsCellPage, BegodeBmsPageError,
    BegodeBmsSummary, BegodeFrame, BegodeFrameError, BegodeFrameParseResult,
    BegodeFrameReassembler, BegodeLiveATelemetry, BegodeLiveBTelemetry, BegodePackVoltageProfile,
    BegodeTelemetryContext, BegodeTelemetryError, FalconProbe, FalconRequestEncoder, Manufacturer,
    ProtocolModelSpec, ReadOnlyNotificationDecoder, RegisteredModelSpec, RequestDisposition,
    SupportsReadRequests, begode_falcon_target_voltage_profile, session::push_parser_error,
};

/// Begode/Gotway notification decoder for Falcon read-only telemetry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BegodeNotificationDecoder {
    reassembler: BegodeFrameReassembler,
    context: BegodeTelemetryContext,
    pack_voltage_profile: BegodePackVoltageProfile,
}

impl Default for BegodeNotificationDecoder {
    fn default() -> Self {
        Self::with_pack_voltage_profile(begode_falcon_target_voltage_profile())
    }
}

impl BegodeNotificationDecoder {
    /// Creates a Begode decoder with an explicit pack-voltage profile.
    #[must_use]
    pub fn with_pack_voltage_profile(profile: BegodePackVoltageProfile) -> Self {
        Self {
            reassembler: BegodeFrameReassembler::default(),
            context: BegodeTelemetryContext::default(),
            pack_voltage_profile: profile,
        }
    }

    /// Returns the pack-voltage profile used for Live A scaling.
    #[must_use]
    pub const fn pack_voltage_profile(&self) -> BegodePackVoltageProfile {
        self.pack_voltage_profile
    }
}

impl ReadOnlyNotificationDecoder for BegodeNotificationDecoder {
    fn reset(&mut self) {
        self.reassembler.reset();
        self.context.reset();
    }

    fn handle_notification(
        &mut self,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicTimestamp,
        output: &mut dyn SessionOutputSink,
    ) -> Result<(), SessionOutputError> {
        let mut event_count = SemanticEventCount::default();
        for byte in bytes {
            match self.reassembler.feed_byte_result_at(*byte, monotonic_ms) {
                Ok(BegodeFrameParseResult::Complete(frame)) => {
                    event_count = event_count.saturating_add(push_begode_frame(
                        &mut self.context,
                        self.pack_voltage_profile,
                        &frame,
                        monotonic_ms,
                        output,
                    )?);
                }
                Ok(BegodeFrameParseResult::Seeking | BegodeFrameParseResult::Buffered) => {}
                Err(BegodeFrameError::InvalidFrame) => {
                    push_parser_error(ParserError::MalformedFrame, output)?;
                    output.push(SessionOutput::NotificationIngest(
                        NotificationIngestOutcome::parser_diagnostic(
                            ProtocolFamily::BegodeGotway,
                            channel,
                            NotificationByteLen::from_bytes(bytes.len()),
                            monotonic_ms,
                            ParserError::MalformedFrame,
                        ),
                    ))?;
                    return Ok(());
                }
            }
        }

        output.push(SessionOutput::NotificationIngest(
            if event_count.as_events() > 0 {
                NotificationIngestOutcome::semantic_events(
                    ProtocolFamily::BegodeGotway,
                    channel,
                    NotificationByteLen::from_bytes(bytes.len()),
                    monotonic_ms,
                    event_count,
                )
            } else {
                NotificationIngestOutcome::buffered_fragment(
                    ProtocolFamily::BegodeGotway,
                    channel,
                    NotificationByteLen::from_bytes(bytes.len()),
                    monotonic_ms,
                )
            },
        ))?;
        Ok(())
    }
}

fn push_begode_frame(
    context: &mut BegodeTelemetryContext,
    pack_voltage_profile: BegodePackVoltageProfile,
    frame: &BegodeFrame,
    monotonic_ms: MonotonicTimestamp,
    output: &mut dyn SessionOutputSink,
) -> Result<SemanticEventCount, SessionOutputError> {
    match frame.tag().get() {
        0x00 => match BegodeLiveATelemetry::decode(frame, pack_voltage_profile) {
            Ok(telemetry) => {
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    context.live_a_to_delta(telemetry, monotonic_ms),
                )))?;
                Ok(SemanticEventCount::from_events(1))
            }
            Err(error) => push_begode_telemetry_error(error, output),
        },
        0x01 => match BegodeBmsSummary::decode(frame) {
            Ok(summary) => {
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    summary.to_delta(monotonic_ms),
                )))?;
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    summary.to_battery_response(),
                )))?;
                Ok(SemanticEventCount::from_events(2))
            }
            Err(error) => push_begode_bms_error(error, output),
        },
        0x02 | 0x03 => match BegodeBmsCellPage::decode(frame) {
            Ok(page) => {
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    page.to_battery_response(),
                )))?;
                Ok(SemanticEventCount::from_events(1))
            }
            Err(error) => push_begode_bms_error(error, output),
        },
        0x04 => match BegodeLiveBTelemetry::decode(frame) {
            Ok(telemetry) => {
                context.observe_live_b(telemetry);
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    context.live_b_to_delta(telemetry, monotonic_ms),
                )))?;
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    context.live_b_to_settings_response(telemetry),
                )))?;
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    telemetry.to_diagnostics_response(),
                )))?;
                Ok(SemanticEventCount::from_events(3))
            }
            Err(error) => push_begode_telemetry_error(error, output),
        },
        0x07 => match crate::BegodeExtraTelemetry::decode(frame) {
            Ok(telemetry) => {
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    telemetry.to_delta(monotonic_ms),
                )))?;
                Ok(SemanticEventCount::from_events(1))
            }
            Err(error) => push_begode_telemetry_error(error, output),
        },
        _ => {
            push_parser_error(ParserError::MalformedFrame, output)?;
            Ok(SemanticEventCount::from_events(2))
        }
    }
}

fn push_begode_telemetry_error(
    error: BegodeTelemetryError,
    output: &mut dyn SessionOutputSink,
) -> Result<SemanticEventCount, SessionOutputError> {
    match error {
        BegodeTelemetryError::UnexpectedFrameTag { .. } => {
            push_parser_error(ParserError::MalformedFrame, output)?;
            Ok(SemanticEventCount::from_events(2))
        }
    }
}

fn push_begode_bms_error(
    error: BegodeBmsPageError,
    output: &mut dyn SessionOutputSink,
) -> Result<SemanticEventCount, SessionOutputError> {
    match error {
        BegodeBmsPageError::UnexpectedFrameTag { .. } => {
            push_parser_error(ParserError::MalformedFrame, output)?;
            Ok(SemanticEventCount::from_events(2))
        }
    }
}

/// Begode Falcon read-only model spec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeFalconModel;

const BEGODE_FALCON_MODEL_GATT: [GattFingerprint; 1] = [GattFingerprint {
    service: BEGODE_SERVICE_CHANNEL,
    characteristic: BEGODE_DATA_CHANNEL,
    roles: GattRoles::empty()
        .with_write_without_response()
        .with_notify(),
    verification: VerificationStatus::SourceVerified,
}];

impl ProtocolModelSpec for BegodeFalconModel {
    const MANUFACTURER: Manufacturer = Manufacturer::Begode;
    const MODEL: &'static str = "Falcon";
    const PROTOCOL: ProtocolFamily = ProtocolFamily::BegodeGotway;
}

impl RegisteredModelSpec for BegodeFalconModel {
    const REGISTRY_ENTRY: ModelRegistryEntry = ModelRegistryEntry {
        manufacturer: cutout_core::ManufacturerKey::new("Begode"),
        model: cutout_core::ModelKey::new(Self::MODEL),
        protocol_family: Self::PROTOCOL,
        advertised_name_hints: &["Falcon", "Begode", "Gotway"],
        wire_model_id: None,
        battery: None,
        bms: None,
        gatt: &BEGODE_FALCON_MODEL_GATT,
        capabilities: <Self as SupportsReadRequests>::READ_CAPABILITIES,
        verification: VerificationStatus::Inferred,
    };
}

impl SupportsReadRequests for BegodeFalconModel {
    type Probe = FalconProbe;

    const READ_CAPABILITIES: Capabilities = Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestFirmwareInfo,
        CommandKind::RequestTelemetry,
        CommandKind::RequestBatteryInfo,
    ]);
    const WRITE_CHANNEL: GattChannel = BEGODE_DATA_CHANNEL;
    const SUBSCRIBE_CHANNEL: GattChannel = BEGODE_DATA_CHANNEL;
    type NotificationDecoder = BegodeNotificationDecoder;

    fn encode_read_command(kind: CommandKind) -> Option<RequestDisposition<Self::Probe>> {
        FalconRequestEncoder::encode_command(kind)
    }
}
