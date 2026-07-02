use cutout_core::{
    BATTERY_TEMPERATURE_VALUES_PER_PAGE, BatteryCurrent, BatteryInfo, BatteryPageKind,
    BatteryPageMetadata, BatteryPagePayload, BatterySpec, Capabilities, CommandKind, Count,
    DeviceEvent, GattChannel, GattFingerprint, GattRoles, Measured, ModelRegistryEntry,
    MonotonicTimestamp, NotificationByteLen, NotificationIngestOutcome, ParserError,
    ParserGapEvidence, PayloadBodyLen, PayloadClassifier, ProtocolFamily, ProtocolSelector,
    Quantity, ReadOnlyResponse, ReservedPayloadEvidence, SemanticEventCount, SeriesCount,
    SessionOutput, SessionOutputError, SessionOutputSink, Temperature, Unit, VerificationStatus,
    VerifiedValue, Voltage,
};

use crate::{
    AeroProbe, AeroRequestEncoder, Manufacturer, ProtocolModelSpec, ReadOnlyNotificationDecoder,
    RegisteredModelSpec, RequestDisposition, SupportsReadRequests, VETERAN_DATA_CHANNEL,
    VETERAN_SERVICE_CHANNEL, VeteranBmsCellPage, VeteranBmsMetadataPage, VeteranBmsPageEvidence,
    VeteranBmsTemperaturePage, VeteranFrame, VeteranFrameParseResult, VeteranFrameReassembler,
    VeteranReassemblyError, VeteranTelemetry, VeteranTelemetryError, decode_veteran_bms_page,
    session::push_parser_error,
};

/// Veteran/LeaperKim/NOSFET notification decoder for NOSFET Aero telemetry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VeteranNotificationDecoder {
    pub(crate) reassembler: VeteranFrameReassembler,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompletedFrame;

impl Unit for CompletedFrame {
    type Dimension = Count;
}

type CompletedFrames = Quantity<Count, CompletedFrame, usize>;

impl ReadOnlyNotificationDecoder for VeteranNotificationDecoder {
    fn reset(&mut self) {
        self.reassembler.reset();
    }

    fn handle_notification(
        &mut self,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicTimestamp,
        output: &mut dyn SessionOutputSink,
    ) -> Result<(), SessionOutputError> {
        let mut completed_frames = CompletedFrames::default();
        let mut buffered = false;
        for byte in bytes {
            match self.reassembler.feed_byte_result(*byte) {
                Ok(VeteranFrameParseResult::Complete(frame)) => {
                    completed_frames = completed_frames.next();
                    buffered = false;
                    let event_count = push_veteran_frame(&frame, monotonic_ms, output)?;
                    push_veteran_ingest_outcome_for_frame(
                        &frame,
                        channel,
                        NotificationByteLen::from_bytes(bytes.len()),
                        monotonic_ms,
                        event_count,
                        output,
                    )?;
                }
                Ok(VeteranFrameParseResult::Buffered) => {
                    buffered = true;
                }
                Ok(VeteranFrameParseResult::Seeking) => {
                    buffered = false;
                }
                Err(VeteranReassemblyError::CrcMismatch) => {
                    push_parser_error(ParserError::BadChecksum, output)?;
                    output.push(SessionOutput::NotificationIngest(
                        NotificationIngestOutcome::parser_diagnostic(
                            ProtocolFamily::VeteranLeaperkimNosfet,
                            channel,
                            NotificationByteLen::from_bytes(bytes.len()),
                            monotonic_ms,
                            ParserError::BadChecksum,
                        ),
                    ))?;
                    return Ok(());
                }
                Err(VeteranReassemblyError::OversizedFrame { claimed, max }) => {
                    let error = ParserError::OversizedFrame { claimed, max };
                    push_parser_error(error, output)?;
                    output.push(SessionOutput::NotificationIngest(
                        NotificationIngestOutcome::parser_diagnostic(
                            ProtocolFamily::VeteranLeaperkimNosfet,
                            channel,
                            NotificationByteLen::from_bytes(bytes.len()),
                            monotonic_ms,
                            error,
                        ),
                    ))?;
                    return Ok(());
                }
                Err(VeteranReassemblyError::InvalidFrame) => {
                    push_parser_error(ParserError::MalformedFrame, output)?;
                    output.push(SessionOutput::NotificationIngest(
                        NotificationIngestOutcome::parser_diagnostic(
                            ProtocolFamily::VeteranLeaperkimNosfet,
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

        if completed_frames.has_no_events() {
            output.push(SessionOutput::NotificationIngest(if buffered {
                NotificationIngestOutcome::buffered_fragment(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    channel,
                    NotificationByteLen::from_bytes(bytes.len()),
                    monotonic_ms,
                )
            } else {
                NotificationIngestOutcome::ignored_wrong_channel(
                    channel,
                    NotificationByteLen::from_bytes(bytes.len()),
                    monotonic_ms,
                )
            }))?;
        }
        Ok(())
    }
}

fn push_veteran_ingest_outcome_for_frame(
    frame: &VeteranFrame,
    channel: GattChannel,
    notification_len: NotificationByteLen,
    monotonic_ms: MonotonicTimestamp,
    event_count: SemanticEventCount,
    output: &mut dyn SessionOutputSink,
) -> Result<(), SessionOutputError> {
    if let Some(evidence) = VeteranBmsPageEvidence::from_frame(frame)
        && evidence.kind == BatteryPageKind::Raw
    {
        if evidence.selector == ProtocolSelector::new(8) {
            output.push(SessionOutput::NotificationIngest(
                NotificationIngestOutcome::known_reserved(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    channel,
                    notification_len,
                    monotonic_ms,
                    ReservedPayloadEvidence {
                        classifier: PayloadClassifier::selector(evidence.selector),
                        body_len: PayloadBodyLen::from_bytes(evidence.body.len()),
                        verification: VerificationStatus::HardwareVerified,
                    },
                ),
            ))?;
        } else {
            output.push(SessionOutput::NotificationIngest(
                NotificationIngestOutcome::parser_gap(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    channel,
                    notification_len,
                    monotonic_ms,
                    ParserGapEvidence {
                        classifier: PayloadClassifier::selector(evidence.selector),
                        body_len: PayloadBodyLen::from_bytes(evidence.body.len()),
                    },
                ),
            ))?;
        }
        return Ok(());
    }

    output.push(SessionOutput::NotificationIngest(
        NotificationIngestOutcome::semantic_events(
            ProtocolFamily::VeteranLeaperkimNosfet,
            channel,
            notification_len,
            monotonic_ms,
            event_count,
        ),
    ))
}

fn push_veteran_frame(
    frame: &VeteranFrame,
    monotonic_ms: MonotonicTimestamp,
    output: &mut dyn SessionOutputSink,
) -> Result<SemanticEventCount, SessionOutputError> {
    match VeteranTelemetry::decode(frame) {
        Ok(telemetry) => {
            let settings_responses = telemetry.to_settings_responses();
            let settings_count = SemanticEventCount::from_events(settings_responses.len());
            output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                telemetry.to_delta(monotonic_ms),
            )))?;
            output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                telemetry.to_firmware_response(),
            )))?;
            for response in settings_responses {
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    response,
                )))?;
            }
            if let Some(evidence) = VeteranBmsPageEvidence::from_frame(frame) {
                if evidence.kind != BatteryPageKind::Raw
                    && let Some(payload) = veteran_bms_payload(evidence)
                {
                    output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                        ReadOnlyResponse::Battery(payload),
                    )))?;
                    return Ok(SemanticEventCount::from_events(3).saturating_add(settings_count));
                }
            }
            Ok(SemanticEventCount::from_events(2).saturating_add(settings_count))
        }
        Err(VeteranTelemetryError::FrameTooShort) => {
            push_parser_error(ParserError::MalformedFrame, output)?;
            Ok(SemanticEventCount::from_events(2))
        }
    }
}

fn veteran_bms_payload(evidence: VeteranBmsPageEvidence<'_>) -> Option<BatteryPagePayload> {
    if evidence.kind == BatteryPageKind::Temperature {
        return VeteranBmsTemperaturePage::from_body(evidence.selector, evidence.body)
            .ok()
            .map(veteran_bms_temperature_payload);
    }
    if evidence.kind == BatteryPageKind::Metadata {
        return VeteranBmsMetadataPage::from_body(evidence.selector, evidence.body)
            .ok()
            .map(veteran_bms_metadata_payload);
    }

    let observed_cell_values = VeteranBmsCellPage::from_body(evidence.selector, evidence.body)
        .map_or(0, |page| {
            u8::try_from(page.cell_voltage.len()).unwrap_or_default()
        });
    decode_veteran_bms_page(
        evidence.selector,
        observed_cell_values,
        BatteryInfo::default(),
        VerificationStatus::HardwareVerified,
    )
    .ok()
}

fn veteran_bms_temperature_payload(page: VeteranBmsTemperaturePage) -> BatteryPagePayload {
    let first_temperature = page
        .temperatures
        .first()
        .copied()
        .unwrap_or(Temperature::from_millicelsius(0));
    let mut temperatures = [None; BATTERY_TEMPERATURE_VALUES_PER_PAGE];
    for (slot, temperature) in temperatures.iter_mut().zip(page.temperatures) {
        *slot = Some(Measured::reported(temperature));
    }
    let battery = BatteryInfo {
        temperature: Some(Measured::reported(first_temperature)),
        ..BatteryInfo::default()
    };
    BatteryPagePayload::temperature_values(
        BatteryPageMetadata::temperature(page.selector, VerificationStatus::HardwareVerified),
        battery,
        temperatures,
    )
}

pub(crate) fn veteran_bms_metadata_payload(page: VeteranBmsMetadataPage) -> BatteryPagePayload {
    let battery = BatteryInfo {
        current: Some(Measured::reported(BatteryCurrent::from_milliamps(
            page.currents.current_0().as_milliamps(),
        ))),
        ..BatteryInfo::default()
    };
    BatteryPagePayload::raw(
        BatteryPageMetadata::metadata(page.selector, VerificationStatus::HardwareVerified),
        battery,
    )
    .with_bms_pack_currents(page.currents)
}

/// NOSFET Aero read-only model spec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NosfetAeroModel;

const NOSFET_AERO_MODEL_GATT: [GattFingerprint; 1] = [GattFingerprint {
    service: VETERAN_SERVICE_CHANNEL,
    characteristic: VETERAN_DATA_CHANNEL,
    roles: GattRoles::empty()
        .with_read()
        .with_write()
        .with_write_without_response()
        .with_notify(),
    verification: VerificationStatus::HardwareVerified,
}];

impl ProtocolModelSpec for NosfetAeroModel {
    const MANUFACTURER: Manufacturer = Manufacturer::Nosfet;
    const MODEL: &'static str = "NOSFET Aero";
    const PROTOCOL: ProtocolFamily = ProtocolFamily::VeteranLeaperkimNosfet;
}

impl RegisteredModelSpec for NosfetAeroModel {
    const REGISTRY_ENTRY: ModelRegistryEntry = ModelRegistryEntry {
        manufacturer: cutout_core::ManufacturerKey::new("NOSFET"),
        model: cutout_core::ModelKey::new(Self::MODEL),
        protocol_family: Self::PROTOCOL,
        advertised_name_hints: &["NF2557", "Aero", "NOSFET"],
        wire_model_id: Some(VerifiedValue {
            value: 43,
            verification: VerificationStatus::HardwareVerified,
        }),
        battery: Some(BatterySpec {
            series_cells: SeriesCount::new(30),
            nominal_capacity: None,
            voltage_range: Voltage::from_millivolts(91_000)..=Voltage::from_millivolts(126_000),
            verification: VerificationStatus::HardwareVerified,
        }),
        bms: None,
        gatt: &NOSFET_AERO_MODEL_GATT,
        capabilities: <Self as SupportsReadRequests>::READ_CAPABILITIES,
        verification: VerificationStatus::HardwareVerified,
    };
}

impl SupportsReadRequests for NosfetAeroModel {
    type Probe = AeroProbe;

    const READ_CAPABILITIES: Capabilities = Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestFirmwareInfo,
        CommandKind::RequestTelemetry,
        CommandKind::RequestBatteryInfo,
        CommandKind::RequestDiagnostics,
    ]);
    const WRITE_CHANNEL: GattChannel = VETERAN_DATA_CHANNEL;
    const SUBSCRIBE_CHANNEL: GattChannel = VETERAN_DATA_CHANNEL;
    type NotificationDecoder = VeteranNotificationDecoder;

    fn encode_read_command(kind: CommandKind) -> Option<RequestDisposition<Self::Probe>> {
        AeroRequestEncoder::encode_command(kind)
    }
}
