use core::marker::PhantomData;
use cutout_core::{
    BATTERY_TEMPERATURE_VALUES_PER_PAGE, BatteryInfo, BatteryPageKind, BatteryPageMetadata,
    BatteryPagePayload, Capabilities, CommandKind, DeviceCommand, DeviceEvent, DiagnosticDetail,
    DiagnosticReadback, DiagnosticSeverity, FirmwareInfo, GattChannel, GattFingerprint, GattRoles,
    Measured, ModelRegistryEntry, MonotonicMillis, NotificationByteLen, NotificationIngestOutcome,
    ParserDiagnostics, ParserError, ParserGapEvidence, PayloadBodyLen, ProtocolFamily,
    ProtocolSelector, ProtocolSession, RawFieldValue, RawTelemetryReadback, ReadOnlyResponse,
    ReservedPayloadEvidence, SafetyClass, SemanticEventCount, SessionInput, SessionOutput,
    TransportAction, ValueQuality, VerificationStatus, WritePayload,
};

use crate::{
    AeroProbe, AeroRequestEncoder, BEGODE_DATA_CHANNEL, BEGODE_SERVICE_CHANNEL, BegodeBmsCellPage,
    BegodeBmsPageError, BegodeBmsSummary, BegodeFrame, BegodeFrameError, BegodeFrameParseResult,
    BegodeFrameReassembler, BegodeLiveATelemetry, BegodeLiveBTelemetry, BegodePackVoltageProfile,
    BegodeTelemetryContext, BegodeTelemetryError, FalconProbe, FalconRequestEncoder,
    RequestDisposition, VESC_NOTIFY_CHANNEL, VESC_WRITE_CHANNEL, VETERAN_DATA_CHANNEL,
    VescBoardProfile, VescCodecError, VescFaultCode, VescReadOnlyReply, VescReadOnlyRequest,
    VescReadOnlyStreamDecoder, VescRequestEncoder, VescStatsTelemetry, VescValuesTelemetry,
    VeteranBmsCellPage, VeteranBmsMetadataPage, VeteranBmsPageEvidence, VeteranBmsTemperaturePage,
    VeteranFrame, VeteranFrameParseResult, VeteranFrameReassembler, VeteranReassemblyError,
    VeteranTelemetry, VeteranTelemetryError, begode_falcon_target_voltage_profile,
    decode_veteran_bms_page,
};

/// Raw VESC electrical RPM telemetry field id.
pub const VESC_RAW_ERPM_FIELD_ID: u16 = 0x8001;

/// Raw VESC relative tachometer telemetry field id.
pub const VESC_RAW_TACHOMETER_FIELD_ID: u16 = 0x8002;

/// Raw VESC controller id telemetry field id.
pub const VESC_RAW_CONTROLLER_ID_FIELD_ID: u16 = 0x8003;

/// Raw VESC fault-code telemetry field id.
pub const VESC_RAW_FAULT_CODE_FIELD_ID: u16 = 0x8004;

/// Raw VESC average speed statistics field id.
pub const VESC_RAW_STATS_SPEED_AVG_FIELD_ID: u16 = 0x8101;

/// Raw VESC average power statistics field id.
pub const VESC_RAW_STATS_POWER_AVG_FIELD_ID: u16 = 0x8102;

/// Raw VESC average current statistics field id.
pub const VESC_RAW_STATS_CURRENT_AVG_FIELD_ID: u16 = 0x8103;

/// Raw VESC statistics count-time field id.
pub const VESC_RAW_STATS_COUNT_TIME_FIELD_ID: u16 = 0x8104;

/// Static manufacturer identifier for a supported model spec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Manufacturer {
    /// NOSFET hardware using the Veteran/LeaperKim/NOSFET protocol family.
    Nosfet,

    /// Begode/Gotway hardware.
    Begode,

    /// Generic VESC-compatible controller.
    Vesc,
}

/// Static protocol model contract.
pub trait ProtocolModelSpec {
    /// Device manufacturer.
    const MANUFACTURER: Manufacturer;

    /// Protocol family used by this model.
    const PROTOCOL: ProtocolFamily;

    /// Stable model name.
    const MODEL: &'static str;
}

/// Model spec that owns its compile-time registry metadata.
pub trait RegisteredModelSpec: ProtocolModelSpec {
    /// Registry entry exported by this model type.
    const REGISTRY_ENTRY: ModelRegistryEntry;
}

/// Type-level operation class marker.
pub trait ProtocolOperation: Sized {
    /// Safety class for this operation class.
    const SAFETY_CLASS: SafetyClass;

    /// Returns the operation safety class.
    #[must_use]
    fn safety_class(self) -> SafetyClass {
        Self::SAFETY_CLASS
    }
}

/// Read-only request operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadOnlyOperation;

impl ProtocolOperation for ReadOnlyOperation {
    const SAFETY_CLASS: SafetyClass = SafetyClass::ReadOnly;
}

/// Settings writes that require stationary-state validation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SettingsWriteOperation;

impl ProtocolOperation for SettingsWriteOperation {
    const SAFETY_CLASS: SafetyClass = SafetyClass::StationaryOnly;
}

/// Benign controls such as lights or horn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BenignControlOperation;

impl ProtocolOperation for BenignControlOperation {
    const SAFETY_CLASS: SafetyClass = SafetyClass::BenignControl;
}

/// Dangerous actuation or motion-affecting controls.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DangerousActuationOperation;

impl ProtocolOperation for DangerousActuationOperation {
    const SAFETY_CLASS: SafetyClass = SafetyClass::Actuation;
}

/// Type-level read-only request capability.
pub trait SupportsReadRequests: ProtocolModelSpec {
    /// Family-specific read probe enum.
    type Probe;

    /// Operation marker for read requests.
    const READ_OPERATION: ReadOnlyOperation = ReadOnlyOperation;

    /// Commands this read-only model session can schedule.
    const READ_CAPABILITIES: Capabilities;

    /// GATT characteristic to write read-only request frames to.
    const WRITE_CHANNEL: GattChannel;

    /// GATT characteristic to subscribe to after link-up.
    const SUBSCRIBE_CHANNEL: GattChannel;

    /// Stateful decoder for accepted notifications from this model.
    type NotificationDecoder: ReadOnlyNotificationDecoder + Default;

    /// Encodes a supported read-only command for this model family.
    fn encode_read_command(kind: CommandKind) -> Option<RequestDisposition<Self::Probe>>;
}

/// Decoder hook for read-only model notification streams.
pub trait ReadOnlyNotificationDecoder {
    /// Resets model-specific parser state.
    fn reset(&mut self);

    /// Handles an accepted notification payload.
    fn handle_notification(
        &mut self,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicMillis,
        output: &mut Vec<SessionOutput>,
    );
}

/// No-op notification decoder for models without typed notification decoding yet.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopNotificationDecoder;

impl ReadOnlyNotificationDecoder for NoopNotificationDecoder {
    fn reset(&mut self) {}

    fn handle_notification(
        &mut self,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicMillis,
        output: &mut Vec<SessionOutput>,
    ) {
        output.push(SessionOutput::NotificationIngest(
            NotificationIngestOutcome::ignored_wrong_channel(
                channel,
                NotificationByteLen::new(bytes.len()),
                monotonic_ms,
            ),
        ));
    }
}

/// Veteran/LeaperKim/NOSFET notification decoder for NOSFET Aero telemetry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VeteranNotificationDecoder {
    reassembler: VeteranFrameReassembler,
}

impl ReadOnlyNotificationDecoder for VeteranNotificationDecoder {
    fn reset(&mut self) {
        self.reassembler.reset();
    }

    fn handle_notification(
        &mut self,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicMillis,
        output: &mut Vec<SessionOutput>,
    ) {
        let mut completed_frames = 0usize;
        let mut buffered = false;
        for byte in bytes {
            match self.reassembler.feed_byte_result(*byte) {
                Ok(VeteranFrameParseResult::Complete(frame)) => {
                    completed_frames += 1;
                    buffered = false;
                    let output_len_before = output.len();
                    push_veteran_frame(&frame, monotonic_ms, output);
                    push_veteran_ingest_outcome_for_frame(
                        &frame,
                        channel,
                        monotonic_ms,
                        output.len() - output_len_before,
                        output,
                    );
                }
                Ok(VeteranFrameParseResult::Buffered) => {
                    buffered = true;
                }
                Ok(VeteranFrameParseResult::Seeking) => {
                    buffered = false;
                }
                Err(VeteranReassemblyError::CrcMismatch) => {
                    push_parser_error(ParserError::BadChecksum, output);
                    output.push(SessionOutput::NotificationIngest(
                        NotificationIngestOutcome::parser_diagnostic(
                            ProtocolFamily::VeteranLeaperkimNosfet,
                            channel,
                            NotificationByteLen::new(bytes.len()),
                            monotonic_ms,
                            ParserError::BadChecksum,
                        ),
                    ));
                    return;
                }
                Err(VeteranReassemblyError::InvalidFrame) => {
                    push_parser_error(ParserError::MalformedFrame, output);
                    output.push(SessionOutput::NotificationIngest(
                        NotificationIngestOutcome::parser_diagnostic(
                            ProtocolFamily::VeteranLeaperkimNosfet,
                            channel,
                            NotificationByteLen::new(bytes.len()),
                            monotonic_ms,
                            ParserError::MalformedFrame,
                        ),
                    ));
                    return;
                }
            }
        }

        if completed_frames == 0 {
            output.push(SessionOutput::NotificationIngest(if buffered {
                NotificationIngestOutcome::buffered_fragment(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    channel,
                    NotificationByteLen::new(bytes.len()),
                    monotonic_ms,
                )
            } else {
                NotificationIngestOutcome::ignored_wrong_channel(
                    channel,
                    NotificationByteLen::new(bytes.len()),
                    monotonic_ms,
                )
            }));
        }
    }
}

fn push_veteran_ingest_outcome_for_frame(
    frame: &VeteranFrame,
    channel: GattChannel,
    monotonic_ms: MonotonicMillis,
    event_count: usize,
    output: &mut Vec<SessionOutput>,
) {
    let frame_len = NotificationByteLen::new(frame.as_slice().len());
    if let Some(evidence) = VeteranBmsPageEvidence::from_frame(frame)
        && evidence.kind == BatteryPageKind::Raw
    {
        if evidence.selector == ProtocolSelector::new(8) {
            output.push(SessionOutput::NotificationIngest(
                NotificationIngestOutcome::known_reserved(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    channel,
                    frame_len,
                    monotonic_ms,
                    ReservedPayloadEvidence {
                        selector: Some(evidence.selector),
                        tag: None,
                        body_len: PayloadBodyLen::new(evidence.body.len()),
                        verification: VerificationStatus::HardwareVerified,
                    },
                ),
            ));
        } else {
            output.push(SessionOutput::NotificationIngest(
                NotificationIngestOutcome::parser_gap(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    channel,
                    frame_len,
                    monotonic_ms,
                    ParserGapEvidence {
                        selector: Some(evidence.selector),
                        tag: None,
                        body_len: PayloadBodyLen::new(evidence.body.len()),
                    },
                ),
            ));
        }
        return;
    }

    output.push(SessionOutput::NotificationIngest(
        NotificationIngestOutcome::semantic_events(
            ProtocolFamily::VeteranLeaperkimNosfet,
            channel,
            frame_len,
            monotonic_ms,
            SemanticEventCount::new(event_count),
        ),
    ));
}

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
        monotonic_ms: MonotonicMillis,
        output: &mut Vec<SessionOutput>,
    ) {
        let output_len_before = output.len();
        for byte in bytes {
            match self.reassembler.feed_byte_result_at(*byte, monotonic_ms) {
                Ok(BegodeFrameParseResult::Complete(frame)) => {
                    push_begode_frame(
                        &mut self.context,
                        self.pack_voltage_profile,
                        &frame,
                        monotonic_ms,
                        output,
                    );
                }
                Ok(BegodeFrameParseResult::Seeking | BegodeFrameParseResult::Buffered) => {}
                Err(BegodeFrameError::InvalidFrame) => {
                    push_parser_error(ParserError::MalformedFrame, output);
                    output.push(SessionOutput::NotificationIngest(
                        NotificationIngestOutcome::parser_diagnostic(
                            ProtocolFamily::BegodeGotway,
                            channel,
                            NotificationByteLen::new(bytes.len()),
                            monotonic_ms,
                            ParserError::MalformedFrame,
                        ),
                    ));
                    return;
                }
            }
        }

        let emitted = output.len() - output_len_before;
        output.push(SessionOutput::NotificationIngest(if emitted > 0 {
            NotificationIngestOutcome::semantic_events(
                ProtocolFamily::BegodeGotway,
                channel,
                NotificationByteLen::new(bytes.len()),
                monotonic_ms,
                SemanticEventCount::new(emitted),
            )
        } else {
            NotificationIngestOutcome::buffered_fragment(
                ProtocolFamily::BegodeGotway,
                channel,
                NotificationByteLen::new(bytes.len()),
                monotonic_ms,
            )
        }));
    }
}

/// Generic VESC notification decoder for read-only UART replies.
#[derive(Debug, Default)]
pub struct VescNotificationDecoder {
    stream: VescReadOnlyStreamDecoder,
    board_profile: Option<VescBoardProfile>,
}

impl VescNotificationDecoder {
    /// Creates a VESC decoder that can calculate speed from explicit board geometry.
    #[must_use]
    pub fn with_board_profile(board_profile: VescBoardProfile) -> Self {
        Self {
            stream: VescReadOnlyStreamDecoder::new(),
            board_profile: Some(board_profile),
        }
    }
}

impl ReadOnlyNotificationDecoder for VescNotificationDecoder {
    fn reset(&mut self) {
        self.stream = VescReadOnlyStreamDecoder::new();
    }

    fn handle_notification(
        &mut self,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicMillis,
        output: &mut Vec<SessionOutput>,
    ) {
        let output_len_before = output.len();
        match self.stream.feed(bytes) {
            Ok(replies) => {
                for reply in replies {
                    push_vesc_reply(&reply, monotonic_ms, self.board_profile, output);
                }
            }
            Err(VescCodecError::UnsupportedReply) => {
                push_parser_error(ParserError::UnmatchedReply, output);
                output.push(SessionOutput::NotificationIngest(
                    NotificationIngestOutcome::parser_diagnostic(
                        ProtocolFamily::Vesc,
                        channel,
                        NotificationByteLen::new(bytes.len()),
                        monotonic_ms,
                        ParserError::UnmatchedReply,
                    ),
                ));
                return;
            }
            Err(
                VescCodecError::DecodeFailed
                | VescCodecError::EncodedFrameTooLong
                | VescCodecError::EncodeFailed,
            ) => {
                push_parser_error(ParserError::MalformedFrame, output);
                output.push(SessionOutput::NotificationIngest(
                    NotificationIngestOutcome::parser_diagnostic(
                        ProtocolFamily::Vesc,
                        channel,
                        NotificationByteLen::new(bytes.len()),
                        monotonic_ms,
                        ParserError::MalformedFrame,
                    ),
                ));
                return;
            }
        }

        let emitted = output.len() - output_len_before;
        output.push(SessionOutput::NotificationIngest(if emitted > 0 {
            NotificationIngestOutcome::semantic_events(
                ProtocolFamily::Vesc,
                channel,
                NotificationByteLen::new(bytes.len()),
                monotonic_ms,
                SemanticEventCount::new(emitted),
            )
        } else {
            NotificationIngestOutcome::buffered_fragment(
                ProtocolFamily::Vesc,
                channel,
                NotificationByteLen::new(bytes.len()),
                monotonic_ms,
            )
        }));
    }
}

fn push_vesc_reply(
    reply: &VescReadOnlyReply,
    monotonic_ms: MonotonicMillis,
    board_profile: Option<VescBoardProfile>,
    output: &mut Vec<SessionOutput>,
) {
    match reply {
        VescReadOnlyReply::FirmwareInfo {
            major,
            minor,
            test_version_number,
            ..
        } => output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
            ReadOnlyResponse::Firmware(FirmwareInfo {
                firmware_major: Some(Measured::reported(u16::from(*major))),
                firmware_minor: Some(Measured::reported(u16::from(*minor))),
                firmware_patch: Some(Measured::reported(u16::from(*test_version_number))),
                ..FirmwareInfo::default()
            }),
        ))),
        VescReadOnlyReply::Values(values) => {
            output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                vesc_values_to_delta(*values, monotonic_ms, board_profile),
            )));
            output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                ReadOnlyResponse::RawTelemetry(vesc_values_to_raw_telemetry(*values)),
            )));
        }
        VescReadOnlyReply::Stats(stats) => {
            output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                ReadOnlyResponse::Diagnostics(vesc_stats_to_diagnostics(*stats)),
            )));
        }
    }
}

fn vesc_values_to_delta(
    values: VescValuesTelemetry,
    monotonic_ms: MonotonicMillis,
    board_profile: Option<VescBoardProfile>,
) -> cutout_core::TelemetryDelta {
    cutout_core::TelemetryDelta {
        at_ms: monotonic_ms,
        speed_mm_s: board_profile
            .and_then(|profile| profile.speed_mm_s_from_erpm(values.rpm_erpm))
            .map(Measured::calculated),
        voltage_mv: Some(Measured::reported(values.voltage_mv)),
        battery_current_ma: Some(Measured::reported(values.input_current_ma)),
        ..cutout_core::TelemetryDelta::empty(monotonic_ms)
    }
}

fn vesc_values_to_raw_telemetry(values: VescValuesTelemetry) -> RawTelemetryReadback {
    RawTelemetryReadback {
        fields: [
            Some(RawFieldValue::new(
                VESC_RAW_ERPM_FIELD_ID,
                i64::from(values.rpm_erpm),
            )),
            Some(RawFieldValue::new(
                VESC_RAW_TACHOMETER_FIELD_ID,
                i64::from(values.tachometer),
            )),
            Some(RawFieldValue::new(
                VESC_RAW_CONTROLLER_ID_FIELD_ID,
                i64::from(values.controller_id),
            )),
            Some(RawFieldValue::new(
                VESC_RAW_FAULT_CODE_FIELD_ID,
                i64::from(vesc_fault_code_raw(values.fault_code)),
            )),
        ],
    }
}

const fn vesc_fault_code_raw(code: VescFaultCode) -> u8 {
    match code {
        VescFaultCode::None => 0,
        VescFaultCode::AbsOverCurrent => 1,
        VescFaultCode::Other(value) => value,
    }
}

fn vesc_stats_to_diagnostics(stats: VescStatsTelemetry) -> DiagnosticReadback {
    DiagnosticReadback {
        details: [
            Some(vesc_diagnostic_detail(
                VESC_RAW_STATS_SPEED_AVG_FIELD_ID,
                i64::from(stats.speed_avg_milli),
            )),
            Some(vesc_diagnostic_detail(
                VESC_RAW_STATS_POWER_AVG_FIELD_ID,
                i64::from(stats.power_avg_mw),
            )),
            Some(vesc_diagnostic_detail(
                VESC_RAW_STATS_CURRENT_AVG_FIELD_ID,
                i64::from(stats.current_avg_ma),
            )),
            Some(vesc_diagnostic_detail(
                VESC_RAW_STATS_COUNT_TIME_FIELD_ID,
                i64::from(stats.count_time_ms),
            )),
        ],
    }
}

const fn vesc_diagnostic_detail(id: u16, value: i64) -> DiagnosticDetail {
    DiagnosticDetail {
        field: RawFieldValue::new(id, value),
        severity: DiagnosticSeverity::Info,
        quality: ValueQuality::Known,
        verification: VerificationStatus::Inferred,
    }
}

fn push_begode_frame(
    context: &mut BegodeTelemetryContext,
    pack_voltage_profile: BegodePackVoltageProfile,
    frame: &BegodeFrame,
    monotonic_ms: MonotonicMillis,
    output: &mut Vec<SessionOutput>,
) {
    match frame.tag().get() {
        0x00 => match BegodeLiveATelemetry::decode(frame, pack_voltage_profile) {
            Ok(telemetry) => output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                context.live_a_to_delta(telemetry, monotonic_ms),
            ))),
            Err(error) => push_begode_telemetry_error(error, output),
        },
        0x01 => match BegodeBmsSummary::decode(frame) {
            Ok(summary) => {
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    summary.to_delta(monotonic_ms),
                )));
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    summary.to_battery_response(),
                )));
            }
            Err(error) => push_begode_bms_error(error, output),
        },
        0x02 | 0x03 => match BegodeBmsCellPage::decode(frame) {
            Ok(page) => output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                page.to_battery_response(),
            ))),
            Err(error) => push_begode_bms_error(error, output),
        },
        0x04 => match BegodeLiveBTelemetry::decode(frame) {
            Ok(telemetry) => {
                context.observe_live_b(telemetry);
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    context.live_b_to_delta(telemetry, monotonic_ms),
                )));
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    context.live_b_to_settings_response(telemetry),
                )));
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    telemetry.to_diagnostics_response(),
                )));
            }
            Err(error) => push_begode_telemetry_error(error, output),
        },
        0x07 => match crate::BegodeExtraTelemetry::decode(frame) {
            Ok(telemetry) => output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                telemetry.to_delta(monotonic_ms),
            ))),
            Err(error) => push_begode_telemetry_error(error, output),
        },
        _ => {
            push_parser_error(ParserError::MalformedFrame, output);
        }
    }
}

fn push_begode_telemetry_error(error: BegodeTelemetryError, output: &mut Vec<SessionOutput>) {
    match error {
        BegodeTelemetryError::UnexpectedFrameTag { .. } => {
            push_parser_error(ParserError::MalformedFrame, output);
        }
    }
}

fn push_begode_bms_error(error: BegodeBmsPageError, output: &mut Vec<SessionOutput>) {
    match error {
        BegodeBmsPageError::UnexpectedFrameTag { .. } => {
            push_parser_error(ParserError::MalformedFrame, output);
        }
    }
}

fn push_veteran_frame(
    frame: &VeteranFrame,
    monotonic_ms: MonotonicMillis,
    output: &mut Vec<SessionOutput>,
) {
    match VeteranTelemetry::decode(frame) {
        Ok(telemetry) => {
            output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                telemetry.to_delta(monotonic_ms),
            )));
            output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                telemetry.to_firmware_response(),
            )));
            for response in telemetry.to_settings_responses() {
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    response,
                )));
            }
            if let Some(evidence) = VeteranBmsPageEvidence::from_frame(frame) {
                if evidence.kind != BatteryPageKind::Raw
                    && let Some(payload) = veteran_bms_payload(evidence)
                {
                    output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                        ReadOnlyResponse::Battery(payload),
                    )));
                }
            }
        }
        Err(VeteranTelemetryError::FrameTooShort) => {
            push_parser_error(ParserError::MalformedFrame, output);
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
            u8::try_from(page.cell_mv.len()).unwrap_or_default()
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
    let mut temperatures = [None; BATTERY_TEMPERATURE_VALUES_PER_PAGE];
    for (slot, temperature_mc) in temperatures.iter_mut().zip(page.temperatures_mc) {
        *slot = Some(Measured::reported(temperature_mc));
    }
    let battery = BatteryInfo {
        temperature_mc: temperatures[0],
        ..BatteryInfo::default()
    };
    BatteryPagePayload::temperature_values(
        BatteryPageMetadata::temperature(page.selector, VerificationStatus::HardwareVerified),
        battery,
        temperatures,
    )
}

fn veteran_bms_metadata_payload(page: VeteranBmsMetadataPage) -> BatteryPagePayload {
    let battery = BatteryInfo {
        current_ma: Some(Measured::reported(page.current_0_ma)),
        ..BatteryInfo::default()
    };
    BatteryPagePayload::from_page(
        BatteryPageMetadata::metadata(page.selector, VerificationStatus::HardwareVerified),
        battery,
    )
}

fn diagnostics_for(error: ParserError) -> ParserDiagnostics {
    let mut diagnostics = ParserDiagnostics::default();
    diagnostics.record_error(error);
    diagnostics
}

fn push_parser_error(error: ParserError, output: &mut Vec<SessionOutput>) {
    output.push(SessionOutput::Event(DeviceEvent::DiagnosticError(
        cutout_core::DiagnosticError::from_parser_error(error),
    )));
    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
        diagnostics_for(error),
    )));
}

/// Type-level settings-write capability.
pub trait SupportsSettingsWrites: ProtocolModelSpec {
    /// Commands this model can write after stationary-state validation.
    const WRITE_CAPABILITIES: Capabilities;
}

/// Type-level benign-control capability.
pub trait SupportsBenignControls: ProtocolModelSpec {
    /// Commands this model can control through benign write paths.
    const CONTROL_CAPABILITIES: Capabilities;
}

/// Type-level dangerous-actuation capability.
pub trait SupportsDangerousActuation: ProtocolModelSpec {
    /// Commands this model can use for direct actuation.
    const ACTUATION_CAPABILITIES: Capabilities;
}

/// Type-level read-only model contract.
pub trait ReadOnlyModelSpec: SupportsReadRequests {
    /// Commands this read-only model session can schedule.
    const CAPABILITIES: Capabilities = Self::READ_CAPABILITIES;
}

impl<M: SupportsReadRequests> ReadOnlyModelSpec for M {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadOnlyCommandGate {
    SupportedRead(CommandKind),
    Unsupported(CommandKind),
}

fn gate_read_only_command<M: SupportsReadRequests>(command: DeviceCommand) -> ReadOnlyCommandGate {
    let kind = command.kind();
    if kind.safety_class() == SafetyClass::ReadOnly
        && M::READ_CAPABILITIES.supports_command_kind(kind)
    {
        ReadOnlyCommandGate::SupportedRead(kind)
    } else {
        ReadOnlyCommandGate::Unsupported(kind)
    }
}

/// NOSFET Aero read-only model spec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NosfetAeroModel;

impl ProtocolModelSpec for NosfetAeroModel {
    const MANUFACTURER: Manufacturer = Manufacturer::Nosfet;
    const MODEL: &'static str = "NOSFET Aero";
    const PROTOCOL: ProtocolFamily = ProtocolFamily::VeteranLeaperkimNosfet;
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
        manufacturer: "Begode",
        model: Self::MODEL,
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

/// Generic VESC read-only model spec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VescGenericModel;

impl ProtocolModelSpec for VescGenericModel {
    const MANUFACTURER: Manufacturer = Manufacturer::Vesc;
    const MODEL: &'static str = "Generic VESC";
    const PROTOCOL: ProtocolFamily = ProtocolFamily::Vesc;
}

impl SupportsReadRequests for VescGenericModel {
    type Probe = VescReadOnlyRequest;

    const READ_CAPABILITIES: Capabilities = Capabilities::from_supported_commands([
        CommandKind::RequestFirmwareInfo,
        CommandKind::RequestTelemetry,
        CommandKind::RequestDiagnostics,
    ]);
    const WRITE_CHANNEL: GattChannel = VESC_WRITE_CHANNEL;
    const SUBSCRIBE_CHANNEL: GattChannel = VESC_NOTIFY_CHANNEL;
    type NotificationDecoder = VescNotificationDecoder;

    fn encode_read_command(kind: CommandKind) -> Option<RequestDisposition<Self::Probe>> {
        VescRequestEncoder::encode_command(kind)
    }
}

fn handle_read_only_session<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool>(
    connected: &mut bool,
    decoder: &mut M::NotificationDecoder,
    input: SessionInput<'_>,
    output: &mut Vec<SessionOutput>,
) {
    match input {
        SessionInput::LinkUp(info) => {
            *connected = true;
            decoder.reset();
            output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
            output.push(SessionOutput::Transport(TransportAction::Subscribe {
                channel: M::SUBSCRIBE_CHANNEL,
            }));
        }
        SessionInput::LinkDown => {
            *connected = false;
            decoder.reset();
            output.push(SessionOutput::Event(DeviceEvent::LinkDown));
        }
        SessionInput::Tick { monotonic_ms } => {
            output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
        }
        SessionInput::Notification {
            channel,
            bytes,
            monotonic_ms,
        } => {
            if *connected && (ACCEPT_ANY_NOTIFICATION || channel == M::SUBSCRIBE_CHANNEL) {
                decoder.handle_notification(channel, bytes, monotonic_ms, output);
            }
        }
        SessionInput::Command(command) => {
            if let ReadOnlyCommandGate::SupportedRead(kind) = gate_read_only_command::<M>(command) {
                push_read_request::<M>(kind, output);
            }
        }
    }
}

fn push_read_request<M: SupportsReadRequests>(kind: CommandKind, output: &mut Vec<SessionOutput>) {
    if let Some(RequestDisposition::Write(request)) = M::encode_read_command(kind)
        && let Ok(bytes) = WritePayload::try_from_slice(request.payload.as_slice())
    {
        output.push(SessionOutput::Transport(TransportAction::Write {
            channel: M::WRITE_CHANNEL,
            bytes,
            mode: request.mode,
        }));
    }
}

/// Generic read-only session shell for one statically-known model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadOnlySession<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool> {
    connected: bool,
    decoder: M::NotificationDecoder,
    model: PhantomData<fn() -> M>,
}

impl<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool> Default
    for ReadOnlySession<M, ACCEPT_ANY_NOTIFICATION>
{
    fn default() -> Self {
        Self {
            connected: false,
            decoder: M::NotificationDecoder::default(),
            model: PhantomData,
        }
    }
}

impl<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool>
    ReadOnlySession<M, ACCEPT_ANY_NOTIFICATION>
{
    /// Creates a read-only session with an explicitly configured notification decoder.
    #[must_use]
    pub const fn with_decoder(decoder: M::NotificationDecoder) -> Self {
        Self {
            connected: false,
            decoder,
            model: PhantomData,
        }
    }

    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub const fn capabilities() -> Capabilities {
        M::CAPABILITIES
    }

    /// Returns this session's manufacturer.
    #[must_use]
    pub const fn manufacturer() -> Manufacturer {
        M::MANUFACTURER
    }

    /// Returns this session's protocol family.
    #[must_use]
    pub const fn protocol() -> ProtocolFamily {
        M::PROTOCOL
    }

    /// Returns this session's stable model name.
    #[must_use]
    pub const fn model() -> &'static str {
        M::MODEL
    }
}

impl<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool> ProtocolSession
    for ReadOnlySession<M, ACCEPT_ANY_NOTIFICATION>
{
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        handle_read_only_session::<M, ACCEPT_ANY_NOTIFICATION>(
            &mut self.connected,
            &mut self.decoder,
            input,
            output,
        );
    }
}

/// Feature-gated dangerous-control shell.
#[cfg(feature = "dangerous-controls")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DangerousControlSession<M: SupportsDangerousActuation> {
    policy: cutout_core::DangerousActuationPolicy,
    arm: Option<cutout_core::DangerousActuationArm>,
    monotonic_ms: MonotonicMillis,
    model: PhantomData<fn() -> M>,
}

#[cfg(feature = "dangerous-controls")]
impl<M: SupportsDangerousActuation> DangerousControlSession<M> {
    /// Creates a dangerous-control shell with a model-specific policy.
    #[must_use]
    pub const fn new(policy: cutout_core::DangerousActuationPolicy) -> Self {
        Self {
            policy,
            arm: None,
            monotonic_ms: 0,
            model: PhantomData,
        }
    }

    /// Installs an explicit arming token.
    pub const fn arm(&mut self, arm: cutout_core::DangerousActuationArm) {
        self.arm = Some(arm);
    }

    fn push_refusal(
        output: &mut Vec<SessionOutput>,
        command: CommandKind,
        safety_class: SafetyClass,
        reason: cutout_core::ControlRefusalReason,
    ) {
        output.push(SessionOutput::Event(DeviceEvent::ControlRefusal(
            cutout_core::ControlRefusal {
                command,
                safety_class,
                reason,
            },
        )));
    }
}

#[cfg(feature = "dangerous-controls")]
impl<M: SupportsDangerousActuation> ProtocolSession for DangerousControlSession<M> {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::Tick { monotonic_ms } => {
                self.monotonic_ms = monotonic_ms;
            }
            SessionInput::Command(command) => {
                let kind = command.kind();
                let safety_class = command.safety_class();
                if !M::ACTUATION_CAPABILITIES.supports_command_kind(kind) {
                    Self::push_refusal(
                        output,
                        kind,
                        safety_class,
                        cutout_core::ControlRefusalReason::UnsupportedCommand,
                    );
                    return;
                }

                match self.policy.authorize(command, self.monotonic_ms, self.arm) {
                    Ok(metadata) => Self::push_refusal(
                        output,
                        metadata.kind,
                        metadata.safety_class,
                        cutout_core::ControlRefusalReason::UnsupportedCommand,
                    ),
                    Err(reason) => Self::push_refusal(
                        output,
                        kind,
                        safety_class,
                        cutout_core::ControlRefusalReason::from(reason),
                    ),
                }
            }
            SessionInput::LinkUp(_)
            | SessionInput::LinkDown
            | SessionInput::Notification { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;
    use cutout_core::{
        BatteryPageKind, LinkInfo, Measured, RawFieldValue, ReadOnlyResponse, TelemetryDelta,
        TransportAction, VerificationStatus, WriteMode,
    };
    use proptest::prelude::*;

    const TEST_CHANNEL: GattChannel = GattChannel::from_bytes([0x11; 16]);

    struct TestModel;

    impl ProtocolModelSpec for TestModel {
        const MANUFACTURER: Manufacturer = Manufacturer::Nosfet;
        const MODEL: &'static str = "test";
        const PROTOCOL: ProtocolFamily = ProtocolFamily::VeteranLeaperkimNosfet;
    }

    impl SupportsReadRequests for TestModel {
        type Probe = AeroProbe;

        const READ_CAPABILITIES: Capabilities =
            Capabilities::from_supported_commands([CommandKind::RequestTelemetry]);
        const WRITE_CHANNEL: GattChannel = TEST_CHANNEL;
        const SUBSCRIBE_CHANNEL: GattChannel = TEST_CHANNEL;
        type NotificationDecoder = NoopNotificationDecoder;

        fn encode_read_command(kind: CommandKind) -> Option<RequestDisposition<Self::Probe>> {
            AeroRequestEncoder::encode_command(kind)
        }
    }

    #[cfg(feature = "dangerous-controls")]
    impl SupportsDangerousActuation for TestModel {
        const ACTUATION_CAPABILITIES: Capabilities =
            Capabilities::from_supported_commands([CommandKind::SetRawMotorCurrent]);
    }

    fn live_aero_frame() -> [u8; 87] {
        hex_literal::hex!(
            "dc5a5c532a7c000000000000ab41001700000cff\
             000000000226021ca8f607801afa000080c80000\
             808080808080022880803080800e310e310e2f0e\
             2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
             310e2e9e05e3ad"
        )
    }

    fn live_aero_selector_3_frame() -> [u8; 99] {
        hex_literal::hex!(
            "dc5a5c5f2a09000000170000ab6c001700000bea\
             045c00000226021ca8f607801b1f000080c80000\
             808080808080030689065706a20686067c06f700\
             00000000000000000000000e0e0e0200000000a5\
             11000053f401c50000000000bffffaf33f9782"
        )
    }

    fn live_aero_selector_0_frame() -> [u8; 77] {
        hex_literal::hex!(
            "dc5a5c492a6a000000000000ab41001700000d1b\
             007d00000226021ca8f607801b0a000080c80000\
             808080808080000003f8ffffffffff3211ffae09\
             760dfe0195000000000002000242d923fb"
        )
    }

    fn live_aero_selector_8_frame() -> [u8; 75] {
        hex_literal::hex!(
            "dc5a5c4729f2000000170000ab6c001700000be9\
             045a00000226021ca8f607801b25000080c80000\
             808080808080080000803200364f371e00000100\
             808028062e7964800080801540e23a"
        )
    }

    fn live_aero_selector_0_frame_with_selector(selector: u8) -> [u8; 77] {
        let mut frame = live_aero_selector_0_frame();
        frame[VeteranBmsPageEvidence::SELECTOR_OFFSET] = selector;
        let declared_len = usize::from(frame[3]);
        let crc = crc32fast::hash(&frame[..declared_len]);
        frame[declared_len..declared_len + 4].copy_from_slice(&crc.to_be_bytes());
        frame
    }

    fn live_begode_a_frame() -> [u8; 24] {
        hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a")
    }

    fn live_begode_b_frame() -> [u8; 24] {
        hex_literal::hex!("55aa000000320000000f003200030502000004185a5a5a5a")
    }

    fn live_begode_b_imperial_frame() -> [u8; 24] {
        hex_literal::hex!("55aa000000320001000f003200030502000004185a5a5a5a")
    }

    fn vesc_selective_values_frame() -> [u8; 28] {
        [
            2, 23, 50, 0, 2, 161, 138, 0, 0, 0, 0, 0, 4, 0, 0, 3, 221, 1, 119, 255, 255, 170, 43,
            0, 20, 45, 58, 3,
        ]
    }

    fn vesc_stats_frame() -> [u8; 54] {
        [
            2, 49, 128, 0, 0, 7, 255, 63, 128, 0, 0, 64, 0, 0, 0, 64, 64, 0, 0, 64, 128, 0, 0, 64,
            160, 0, 0, 64, 192, 0, 0, 64, 224, 0, 0, 65, 0, 0, 0, 65, 16, 0, 0, 65, 32, 0, 0, 65,
            48, 0, 0, 213, 206, 3,
        ]
    }

    fn telemetry_events(output: &[SessionOutput]) -> Vec<TelemetryDelta> {
        output
            .iter()
            .filter_map(|item| match item {
                SessionOutput::Event(DeviceEvent::Telemetry(delta)) => Some(*delta),
                _ => None,
            })
            .collect()
    }

    fn read_only_response_events(output: &[SessionOutput]) -> Vec<ReadOnlyResponse> {
        output
            .iter()
            .filter_map(|item| match item {
                SessionOutput::Event(DeviceEvent::ReadOnlyResponse(response)) => Some(*response),
                _ => None,
            })
            .collect()
    }

    fn diagnostic_error_events(output: &[SessionOutput]) -> Vec<cutout_core::DiagnosticError> {
        output
            .iter()
            .filter_map(|item| match item {
                SessionOutput::Event(DeviceEvent::DiagnosticError(error)) => Some(*error),
                _ => None,
            })
            .collect()
    }

    fn diagnostic_counter_events(output: &[SessionOutput]) -> Vec<ParserDiagnostics> {
        output
            .iter()
            .filter_map(|item| match item {
                SessionOutput::Event(DeviceEvent::Diagnostics(diagnostics)) => Some(*diagnostics),
                _ => None,
            })
            .collect()
    }

    fn notification_ingest_outcomes(output: &[SessionOutput]) -> Vec<NotificationIngestOutcome> {
        output
            .iter()
            .filter_map(|item| match item {
                SessionOutput::NotificationIngest(outcome) => Some(*outcome),
                _ => None,
            })
            .collect()
    }

    fn aero_output_for_notification(bytes: &[u8]) -> Vec<SessionOutput> {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        output.clear();

        session.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes,
                monotonic_ms: 42,
            },
            &mut output,
        );

        output
    }

    fn aero_output_for_notification_chunks(chunks: &[&[u8]]) -> Vec<SessionOutput> {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        output.clear();

        for (index, bytes) in chunks.iter().enumerate() {
            session.handle(
                SessionInput::Notification {
                    channel: VETERAN_DATA_CHANNEL,
                    bytes,
                    monotonic_ms: 42 + u64::try_from(index).expect("chunk index fits"),
                },
                &mut output,
            );
        }

        output
    }

    fn falcon_telemetry_for_notifications(notifications: &[&[u8]]) -> Vec<TelemetryDelta> {
        let mut session = ReadOnlySession::<BegodeFalconModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        for (index, bytes) in notifications.iter().enumerate() {
            session.handle(
                SessionInput::Notification {
                    channel: BEGODE_DATA_CHANNEL,
                    bytes,
                    monotonic_ms: 42 + u64::try_from(index).expect("fixture index fits"),
                },
                &mut output,
            );
        }

        telemetry_events(&output)
    }

    fn read_only_responses_for_notification(bytes: &[u8]) -> Vec<ReadOnlyResponse> {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes,
                monotonic_ms: 42,
            },
            &mut output,
        );

        read_only_response_events(&output)
    }

    fn live_aero_telemetry() -> TelemetryDelta {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes: &live_aero_frame(),
                monotonic_ms: 42,
            },
            &mut output,
        );

        telemetry_events(&output)
            .into_iter()
            .next()
            .expect("live Aero notification emits telemetry")
    }

    #[test]
    fn shared_read_only_session_link_up_subscribes_profile_channel() {
        let mut connected = false;
        let mut decoder = NoopNotificationDecoder;
        let mut output = Vec::new();

        handle_read_only_session::<TestModel, false>(
            &mut connected,
            &mut decoder,
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 7,
                max_write_len: Some(185),
            }),
            &mut output,
        );

        assert!(connected);
        assert!(output.iter().any(|item| matches!(
            item,
            SessionOutput::Transport(TransportAction::Subscribe { channel }) if *channel == TEST_CHANNEL
        )));
    }

    #[test]
    fn begode_malformed_frame_emits_detailed_and_aggregate_diagnostics() {
        let mut session = ReadOnlySession::<BegodeFalconModel, true>::default();
        let mut output = Vec::new();
        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );

        let mut malformed = live_begode_a_frame();
        malformed[20] = 0;
        session.handle(
            SessionInput::Notification {
                channel: BEGODE_DATA_CHANNEL,
                bytes: &malformed,
                monotonic_ms: 42,
            },
            &mut output,
        );

        assert_eq!(
            diagnostic_error_events(&output),
            vec![cutout_core::DiagnosticError::from_parser_error(
                ParserError::MalformedFrame
            )]
        );
        assert_eq!(
            diagnostic_counter_events(&output)
                .last()
                .map(|diagnostics| diagnostics.malformed_frames),
            Some(1)
        );
    }

    #[test]
    fn shared_read_only_session_accepts_matching_notifications_when_connected() {
        let mut connected = true;
        let mut decoder = NoopNotificationDecoder;
        let mut output = Vec::new();

        handle_read_only_session::<TestModel, false>(
            &mut connected,
            &mut decoder,
            SessionInput::Notification {
                channel: TEST_CHANNEL,
                bytes: &[0x01, 0x02, 0x03],
                monotonic_ms: 11,
            },
            &mut output,
        );

        assert!(output.iter().any(|item| matches!(
            item,
            SessionOutput::NotificationIngest(NotificationIngestOutcome::Ignored(evidence))
                if evidence.channel == TEST_CHANNEL
                    && evidence.monotonic_ms == 11
                    && evidence.len == NotificationByteLen::new(3)
        )));
    }

    #[test]
    fn shared_read_only_session_ignores_notifications_when_disconnected() {
        let mut connected = false;
        let mut decoder = NoopNotificationDecoder;
        let mut output = Vec::new();

        handle_read_only_session::<TestModel, false>(
            &mut connected,
            &mut decoder,
            SessionInput::Notification {
                channel: TEST_CHANNEL,
                bytes: &[0x01, 0x02, 0x03],
                monotonic_ms: 11,
            },
            &mut output,
        );

        assert!(output.is_empty());
    }

    #[test]
    fn read_only_session_shells_remain_small() {
        assert!(size_of::<ReadOnlySession<BegodeFalconModel, true>>() <= 64);
        assert!(size_of::<ReadOnlySession<NosfetAeroModel, false>>() <= 272);
    }

    #[test]
    fn begode_falcon_session_emits_live_a_telemetry() {
        let live_a = live_begode_a_frame();
        let telemetry = falcon_telemetry_for_notifications(&[&live_a]);

        assert_eq!(telemetry.len(), 1);
        assert_eq!(
            telemetry[0].voltage_mv.map(|value| value.value),
            Some(75_063)
        );
        assert_eq!(
            telemetry[0].speed_mm_s.map(|value| value.value),
            Some(13_360)
        );
        assert_eq!(
            telemetry[0].distance_mm.map(|value| value.value),
            Some(750_000)
        );
    }

    #[test]
    fn begode_falcon_session_can_use_explicit_100v_pack_profile() {
        let live_a = live_begode_a_frame();
        let mut session = ReadOnlySession::<BegodeFalconModel, true>::with_decoder(
            BegodeNotificationDecoder::with_pack_voltage_profile(
                BegodePackVoltageProfile::Begode100VFullCharge,
            ),
        );
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: BEGODE_DATA_CHANNEL,
                bytes: &live_a,
                monotonic_ms: 42,
            },
            &mut output,
        );

        let telemetry = telemetry_events(&output);
        assert_eq!(telemetry.len(), 1);
        assert_eq!(
            telemetry[0].voltage_mv.map(|value| value.value),
            Some(90_075)
        );
    }

    #[test]
    fn begode_falcon_decoder_default_uses_explicit_target_voltage_profile() {
        let decoder = BegodeNotificationDecoder::default();

        assert_eq!(
            decoder.pack_voltage_profile(),
            begode_falcon_target_voltage_profile()
        );
    }

    #[test]
    fn generic_vesc_model_session_requests_subscription_on_link_up() {
        let mut session = ReadOnlySession::<VescGenericModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );

        assert_eq!(
            output,
            vec![
                SessionOutput::Event(DeviceEvent::LinkUp(LinkInfo {
                    monotonic_ms: 1,
                    max_write_len: Some(185),
                })),
                SessionOutput::Transport(TransportAction::Subscribe {
                    channel: VESC_NOTIFY_CHANNEL,
                }),
            ]
        );
    }

    #[test]
    fn generic_vesc_session_writes_values_request_for_telemetry() {
        let mut session = ReadOnlySession::<VescGenericModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::RequestTelemetry),
            &mut output,
        );

        assert_eq!(
            output,
            vec![SessionOutput::Transport(TransportAction::Write {
                channel: VESC_WRITE_CHANNEL,
                bytes: WritePayload::try_from_slice(&[2, 1, 4, 64, 132, 3])
                    .expect("VESC values request fits"),
                mode: WriteMode::WithoutResponse,
            })]
        );
    }

    #[test]
    fn generic_vesc_session_writes_stats_request_for_diagnostics() {
        let mut session = ReadOnlySession::<VescGenericModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::RequestDiagnostics),
            &mut output,
        );

        assert_eq!(
            output,
            vec![SessionOutput::Transport(TransportAction::Write {
                channel: VESC_WRITE_CHANNEL,
                bytes: WritePayload::try_from_slice(&[2, 3, 128, 4, 21, 181, 10, 3])
                    .expect("VESC stats request fits"),
                mode: WriteMode::WithoutResponse,
            })]
        );
    }

    #[test]
    fn generic_vesc_session_rejects_actuation_without_writes() {
        let mut session = ReadOnlySession::<VescGenericModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::SetRawMotorCurrent { current_ma: 1 }),
            &mut output,
        );

        assert!(output.is_empty());
    }

    #[test]
    fn generic_vesc_session_emits_values_telemetry_and_preserves_raw_readback() {
        let mut session = ReadOnlySession::<VescGenericModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        for chunk in vesc_selective_values_frame().chunks(5) {
            session.handle(
                SessionInput::Notification {
                    channel: VESC_NOTIFY_CHANNEL,
                    bytes: chunk,
                    monotonic_ms: 42,
                },
                &mut output,
            );
        }

        let telemetry = telemetry_events(&output);
        let delta = telemetry.last().expect("VESC values telemetry");
        assert_eq!(delta.at_ms, 42);
        assert_eq!(delta.voltage_mv, Some(Measured::reported(37_500)));
        assert_eq!(delta.battery_current_ma, Some(Measured::reported(40)));
        assert_eq!(delta.speed_mm_s, None);

        let responses = read_only_response_events(&output);
        let ReadOnlyResponse::RawTelemetry(raw) =
            responses.last().expect("VESC values raw telemetry")
        else {
            panic!("expected raw telemetry response");
        };
        assert_eq!(
            raw.fields[0].expect("erpm"),
            RawFieldValue::new(VESC_RAW_ERPM_FIELD_ID, 989)
        );
        assert_eq!(
            raw.fields[1].expect("tachometer"),
            RawFieldValue::new(VESC_RAW_TACHOMETER_FIELD_ID, -21_973)
        );
        assert_eq!(
            raw.fields[2].expect("controller id"),
            RawFieldValue::new(VESC_RAW_CONTROLLER_ID_FIELD_ID, 20)
        );
        assert_eq!(
            raw.fields[3].expect("fault"),
            RawFieldValue::new(VESC_RAW_FAULT_CODE_FIELD_ID, 0)
        );
    }

    #[test]
    fn generic_vesc_session_emits_calculated_speed_when_board_profile_is_explicit() {
        let decoder = VescNotificationDecoder::with_board_profile(VescBoardProfile::new(1, 1, 60));
        let mut session = ReadOnlySession::<VescGenericModel, true>::with_decoder(decoder);
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: VESC_NOTIFY_CHANNEL,
                bytes: &vesc_selective_values_frame(),
                monotonic_ms: 42,
            },
            &mut output,
        );

        let telemetry = telemetry_events(&output);
        let delta = telemetry.last().expect("VESC values telemetry");
        assert_eq!(delta.speed_mm_s, Some(Measured::calculated(989)));

        let responses = read_only_response_events(&output);
        let ReadOnlyResponse::RawTelemetry(raw) =
            responses.last().expect("VESC values raw telemetry")
        else {
            panic!("expected raw telemetry response");
        };
        assert_eq!(
            raw.fields[0].expect("erpm"),
            RawFieldValue::new(VESC_RAW_ERPM_FIELD_ID, 989)
        );
    }

    #[test]
    fn generic_vesc_session_maps_stats_to_diagnostics_readback() {
        let mut session = ReadOnlySession::<VescGenericModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: VESC_NOTIFY_CHANNEL,
                bytes: &vesc_stats_frame(),
                monotonic_ms: 43,
            },
            &mut output,
        );

        let responses = read_only_response_events(&output);
        let ReadOnlyResponse::Diagnostics(diagnostics) =
            responses.last().expect("VESC stats diagnostics")
        else {
            panic!("expected diagnostics response");
        };
        assert_eq!(
            diagnostics.details[0].expect("speed avg").field,
            RawFieldValue::new(VESC_RAW_STATS_SPEED_AVG_FIELD_ID, 1_000)
        );
        assert_eq!(
            diagnostics.details[1].expect("power avg").field,
            RawFieldValue::new(VESC_RAW_STATS_POWER_AVG_FIELD_ID, 3_000)
        );
        assert_eq!(
            diagnostics.details[2].expect("current avg").field,
            RawFieldValue::new(VESC_RAW_STATS_CURRENT_AVG_FIELD_ID, 5_000)
        );
        assert_eq!(
            diagnostics.details[3].expect("count time").field,
            RawFieldValue::new(VESC_RAW_STATS_COUNT_TIME_FIELD_ID, 11_000)
        );
    }

    #[test]
    fn begode_falcon_session_keeps_metric_live_b_values_metric() {
        let live_b = live_begode_b_frame();
        let telemetry = falcon_telemetry_for_notifications(&[&live_b]);

        assert_eq!(telemetry.len(), 1);
        assert_eq!(
            telemetry[0].distance_mm.map(|value| value.value),
            Some(50_000)
        );
    }

    #[test]
    fn begode_falcon_session_applies_imperial_live_b_to_following_live_a() {
        let live_b = live_begode_b_imperial_frame();
        let live_a = live_begode_a_frame();
        let telemetry = falcon_telemetry_for_notifications(&[&live_b, &live_a]);

        assert_eq!(telemetry.len(), 2);
        assert_eq!(
            telemetry[0].distance_mm.map(|value| value.value),
            Some(80_467)
        );
        assert_eq!(
            telemetry[1].speed_mm_s.map(|value| value.value),
            Some(21_500)
        );
        assert_eq!(
            telemetry[1].distance_mm.map(|value| value.value),
            Some(1_207_008)
        );
    }

    #[test]
    fn begode_falcon_session_resets_imperial_unit_state_on_reconnect() {
        let live_b = live_begode_b_imperial_frame();
        let live_a = live_begode_a_frame();
        let mut session = ReadOnlySession::<BegodeFalconModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: BEGODE_DATA_CHANNEL,
                bytes: &live_b,
                monotonic_ms: 2,
            },
            &mut output,
        );
        session.handle(SessionInput::LinkDown, &mut output);
        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 3,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: BEGODE_DATA_CHANNEL,
                bytes: &live_a,
                monotonic_ms: 4,
            },
            &mut output,
        );

        let telemetry = telemetry_events(&output);
        assert_eq!(
            telemetry
                .last()
                .and_then(|delta| delta.speed_mm_s.map(|value| value.value)),
            Some(13_360)
        );
        assert_eq!(
            telemetry
                .last()
                .and_then(|delta| delta.distance_mm.map(|value| value.value)),
            Some(750_000)
        );
    }

    #[test]
    fn nosfet_aero_session_emits_voltage_from_live_fixture_notification() {
        assert_eq!(
            live_aero_telemetry().voltage_mv,
            Some(Measured::reported(108_760))
        );
    }

    #[test]
    fn nosfet_aero_session_emits_estimated_battery_percent_from_live_fixture_notification() {
        assert_eq!(
            live_aero_telemetry().battery_percent_estimated,
            Some(Measured::estimated(47))
        );
    }

    #[test]
    fn nosfet_aero_session_emits_fixed_header_telemetry_from_live_fixture_notification() {
        let telemetry = live_aero_telemetry();

        assert_eq!(telemetry.speed_mm_s, Some(Measured::reported(0)));
        assert_eq!(telemetry.motor_current_ma, Some(Measured::reported(0)));
        assert_eq!(telemetry.power_mw, Some(Measured::calculated(0)));
        assert_eq!(
            telemetry.controller_temperature_mc,
            Some(Measured::reported(33_270))
        );
        assert_eq!(telemetry.pwm_permille, Some(Measured::reported(-1_000)));
        assert_eq!(
            telemetry.distance_mm,
            Some(Measured::reported(1_551_169_000))
        );
        assert_eq!(telemetry.pitch_mdeg, Some(Measured::reported(69_060)));
    }

    #[test]
    fn nosfet_aero_session_emits_fixed_header_read_only_responses_from_live_fixture_notification() {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes: &live_aero_frame(),
                monotonic_ms: 42,
            },
            &mut output,
        );

        let responses = read_only_response_events(&output);
        assert_eq!(responses.len(), 4);

        let ReadOnlyResponse::Firmware(firmware) = responses[0] else {
            panic!("expected firmware response");
        };
        assert_eq!(firmware.firmware_major, Some(Measured::reported(43)));
        assert_eq!(firmware.firmware_minor, Some(Measured::reported(2)));
        assert_eq!(firmware.firmware_patch, Some(Measured::reported(54)));

        let fields: Vec<_> = responses[1..]
            .iter()
            .flat_map(|response| match response {
                ReadOnlyResponse::Settings(settings) => settings.entries,
                _ => [None, None, None, None],
            })
            .flatten()
            .map(|entry| entry.field)
            .collect();
        assert!(fields.contains(&RawFieldValue::new(crate::VETERAN_FIELD_PEDALS_MODE, 1_920,)));
        assert!(responses.iter().any(|response| matches!(
            response,
            ReadOnlyResponse::Battery(payload)
                if payload.page().selector == ProtocolSelector::new(2)
                    && payload.page().kind == BatteryPageKind::CellVoltage
        )));
    }

    #[test]
    fn nosfet_aero_session_emits_typed_bms_temperature_page_response() {
        let responses = read_only_responses_for_notification(&live_aero_selector_3_frame());

        assert!(responses.iter().any(|response| matches!(
            response,
            ReadOnlyResponse::Battery(payload)
                if payload.page().selector == ProtocolSelector::new(3)
                    && payload.page().kind == BatteryPageKind::Temperature
                    && payload.page().verification == VerificationStatus::HardwareVerified
                    && payload.battery().temperature_mc.expect("representative temperature").value == 16_730
                    && payload.temperatures_mc()[5].expect("sixth temperature").value == 17_830
        )));
    }

    #[test]
    fn nosfet_aero_session_emits_typed_bms_cell_page_response() {
        let responses = read_only_responses_for_notification(&live_aero_frame());

        assert!(responses.iter().any(|response| matches!(
            response,
            ReadOnlyResponse::Battery(payload)
                if payload.page().selector == ProtocolSelector::new(2)
                    && payload.page().kind == BatteryPageKind::CellVoltage
                    && payload.page().verification == VerificationStatus::HardwareVerified
        )));
    }

    #[test]
    fn nosfet_aero_session_emits_metadata_bms_current_response() {
        let responses = read_only_responses_for_notification(&live_aero_selector_0_frame());

        assert!(responses.iter().any(|response| matches!(
            response,
            ReadOnlyResponse::Battery(payload)
                if payload.page().selector == ProtocolSelector::new(0)
                    && payload.page().kind == BatteryPageKind::Metadata
                    && payload.page().verification == VerificationStatus::HardwareVerified
                    && payload.battery().current_ma == Some(Measured::reported(20))
        )));
    }

    #[test]
    fn nosfet_aero_session_does_not_emit_reserved_bms_page_as_raw_response() {
        let responses = read_only_responses_for_notification(&live_aero_selector_8_frame());

        assert!(responses.iter().all(|response| {
            !matches!(
                response,
                ReadOnlyResponse::Battery(payload)
                    if payload.page().selector == ProtocolSelector::new(8)
                        && payload.page().kind == BatteryPageKind::Raw
            )
        }));
    }

    #[test]
    fn nosfet_aero_session_reports_partial_frame_as_buffered_ingest() {
        let frame = live_aero_selector_0_frame();
        let output = aero_output_for_notification(&frame[..20]);
        let outcomes = notification_ingest_outcomes(&output);

        assert_eq!(
            outcomes,
            vec![NotificationIngestOutcome::buffered_fragment(
                ProtocolFamily::VeteranLeaperkimNosfet,
                VETERAN_DATA_CHANNEL,
                NotificationByteLen::new(20),
                42,
            )]
        );
        assert!(telemetry_events(&output).is_empty());
        assert!(read_only_response_events(&output).is_empty());
    }

    #[test]
    fn nosfet_aero_session_reports_complete_frame_as_semantic_ingest() {
        let frame = live_aero_selector_0_frame();
        let output = aero_output_for_notification(&frame);
        let outcomes = notification_ingest_outcomes(&output);

        assert_eq!(
            outcomes,
            vec![NotificationIngestOutcome::semantic_events(
                ProtocolFamily::VeteranLeaperkimNosfet,
                VETERAN_DATA_CHANNEL,
                NotificationByteLen::new(frame.len()),
                42,
                SemanticEventCount::new(5),
            )]
        );
        assert!(!telemetry_events(&output).is_empty());
        assert!(!read_only_response_events(&output).is_empty());
    }

    #[test]
    fn nosfet_aero_session_reports_selector_8_as_known_reserved_ingest() {
        let frame = live_aero_selector_8_frame();
        let output = aero_output_for_notification(&frame);
        let outcomes = notification_ingest_outcomes(&output);

        assert_eq!(
            outcomes,
            vec![NotificationIngestOutcome::known_reserved(
                ProtocolFamily::VeteranLeaperkimNosfet,
                VETERAN_DATA_CHANNEL,
                NotificationByteLen::new(frame.len()),
                42,
                ReservedPayloadEvidence {
                    selector: Some(ProtocolSelector::new(8)),
                    tag: None,
                    body_len: PayloadBodyLen::new(24),
                    verification: VerificationStatus::HardwareVerified,
                },
            )]
        );
        assert!(!read_only_response_events(&output).is_empty());
    }

    #[test]
    fn nosfet_aero_session_reports_unknown_bms_selector_as_parser_gap() {
        let frame = live_aero_selector_0_frame_with_selector(9);
        let output = aero_output_for_notification(&frame);
        let outcomes = notification_ingest_outcomes(&output);

        assert_eq!(
            outcomes,
            vec![NotificationIngestOutcome::parser_gap(
                ProtocolFamily::VeteranLeaperkimNosfet,
                VETERAN_DATA_CHANNEL,
                NotificationByteLen::new(frame.len()),
                42,
                ParserGapEvidence {
                    selector: Some(ProtocolSelector::new(9)),
                    tag: None,
                    body_len: PayloadBodyLen::new(26),
                },
            )]
        );
        assert!(read_only_response_events(&output).iter().all(|response| {
            !matches!(
                response,
                ReadOnlyResponse::Battery(payload)
                    if payload.page().selector == ProtocolSelector::new(9)
                        && payload.page().kind == BatteryPageKind::Raw
            )
        }));
    }

    #[test]
    fn nosfet_aero_session_reports_bad_crc_as_parser_diagnostic_ingest() {
        let mut frame = live_aero_selector_0_frame();
        let last = frame.last_mut().expect("fixture is nonempty");
        *last ^= 0xff;

        let output = aero_output_for_notification(&frame);
        let outcomes = notification_ingest_outcomes(&output);

        assert_eq!(
            outcomes,
            vec![NotificationIngestOutcome::parser_diagnostic(
                ProtocolFamily::VeteranLeaperkimNosfet,
                VETERAN_DATA_CHANNEL,
                NotificationByteLen::new(frame.len()),
                42,
                ParserError::BadChecksum,
            )]
        );
        assert!(
            diagnostic_error_events(&output)
                .iter()
                .any(|error| error.kind == cutout_core::DiagnosticErrorKind::BadChecksum)
        );
    }

    #[test]
    fn nosfet_aero_session_reports_one_byte_fragments_as_buffered_until_complete() {
        let frame = live_aero_selector_0_frame();
        let chunks: Vec<_> = frame.chunks(1).collect();
        let output = aero_output_for_notification_chunks(&chunks);
        let outcomes = notification_ingest_outcomes(&output);

        assert_eq!(outcomes.len(), frame.len());
        assert!(
            outcomes[..outcomes.len() - 1]
                .iter()
                .all(|outcome| matches!(outcome, NotificationIngestOutcome::BufferedFragment(_)))
        );
        assert_eq!(
            outcomes.last().copied(),
            Some(NotificationIngestOutcome::semantic_events(
                ProtocolFamily::VeteranLeaperkimNosfet,
                VETERAN_DATA_CHANNEL,
                NotificationByteLen::new(frame.len()),
                42 + u64::try_from(frame.len() - 1).expect("fixture length fits"),
                SemanticEventCount::new(5),
            ))
        );
    }

    #[test]
    fn nosfet_aero_session_reports_bms_selectors_zero_through_eight_without_parser_gaps() {
        for selector in 0..=8 {
            let frame = live_aero_selector_0_frame_with_selector(selector);
            let output = aero_output_for_notification(&frame);
            let outcomes = notification_ingest_outcomes(&output);

            assert_eq!(outcomes.len(), 1, "selector {selector}");
            match (selector, outcomes[0]) {
                (
                    8,
                    NotificationIngestOutcome::KnownReserved {
                        payload,
                        notification,
                    },
                ) => {
                    assert_eq!(
                        notification.family,
                        Some(ProtocolFamily::VeteranLeaperkimNosfet)
                    );
                    assert_eq!(payload.selector, Some(ProtocolSelector::new(8)));
                    assert_eq!(payload.body_len, PayloadBodyLen::new(26));
                    assert_eq!(payload.verification, VerificationStatus::HardwareVerified);
                }
                (0..=7, NotificationIngestOutcome::SemanticEvents { notification, .. }) => {
                    assert_eq!(
                        notification.family,
                        Some(ProtocolFamily::VeteranLeaperkimNosfet)
                    );
                }
                (_, outcome) => {
                    panic!("selector {selector} produced unexpected outcome {outcome:?}")
                }
            }
        }
    }

    #[test]
    fn nosfet_aero_session_reports_each_complete_coalesced_frame_outcome() {
        let semantic_frame = live_aero_selector_0_frame();
        let reserved_frame = live_aero_selector_8_frame();
        let mut coalesced = Vec::with_capacity(semantic_frame.len() + reserved_frame.len());
        coalesced.extend_from_slice(&semantic_frame);
        coalesced.extend_from_slice(&reserved_frame);

        let output = aero_output_for_notification(&coalesced);
        let outcomes = notification_ingest_outcomes(&output);

        assert_eq!(
            outcomes,
            vec![
                NotificationIngestOutcome::semantic_events(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    VETERAN_DATA_CHANNEL,
                    NotificationByteLen::new(semantic_frame.len()),
                    42,
                    SemanticEventCount::new(5),
                ),
                NotificationIngestOutcome::known_reserved(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    VETERAN_DATA_CHANNEL,
                    NotificationByteLen::new(reserved_frame.len()),
                    42,
                    ReservedPayloadEvidence {
                        selector: Some(ProtocolSelector::new(8)),
                        tag: None,
                        body_len: PayloadBodyLen::new(24),
                        verification: VerificationStatus::HardwareVerified,
                    },
                ),
            ]
        );
    }

    proptest! {
        #[test]
        fn nosfet_aero_session_fragmentation_outcomes_end_in_same_semantic_result(
            chunk_sizes in proptest::collection::vec(1usize..20, 1..32),
        ) {
            let frame = live_aero_selector_0_frame();
            let mut chunks = Vec::new();
            let mut offset = 0usize;
            let mut size_index = 0usize;

            while offset < frame.len() {
                let size = chunk_sizes[size_index % chunk_sizes.len()];
                let end = offset.saturating_add(size).min(frame.len());
                chunks.push(&frame[offset..end]);
                offset = end;
                size_index += 1;
            }

            let output = aero_output_for_notification_chunks(&chunks);
            let outcomes = notification_ingest_outcomes(&output);

            prop_assert_eq!(outcomes.len(), chunks.len());
            prop_assert!(outcomes[..outcomes.len() - 1]
                .iter()
                .all(|outcome| matches!(outcome, NotificationIngestOutcome::BufferedFragment(_))));
            prop_assert_eq!(
                outcomes.last().copied(),
                Some(NotificationIngestOutcome::semantic_events(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    VETERAN_DATA_CHANNEL,
                    NotificationByteLen::new(frame.len()),
                    42 + u64::try_from(chunks.len() - 1).expect("chunk count fits"),
                    SemanticEventCount::new(5),
                ))
            );
        }
    }

    #[test]
    fn read_only_session_identity_comes_from_model_spec() {
        assert_eq!(
            ReadOnlySession::<NosfetAeroModel, false>::manufacturer(),
            Manufacturer::Nosfet
        );
        assert_eq!(
            ReadOnlySession::<NosfetAeroModel, false>::protocol(),
            ProtocolFamily::VeteranLeaperkimNosfet
        );
        assert_eq!(
            ReadOnlySession::<NosfetAeroModel, false>::model(),
            "NOSFET Aero"
        );

        assert_eq!(
            ReadOnlySession::<BegodeFalconModel, true>::manufacturer(),
            Manufacturer::Begode
        );
        assert_eq!(
            ReadOnlySession::<BegodeFalconModel, true>::protocol(),
            ProtocolFamily::BegodeGotway
        );
        assert_eq!(
            ReadOnlySession::<BegodeFalconModel, true>::model(),
            "Falcon"
        );
    }

    #[test]
    fn generic_read_only_session_uses_model_capabilities() {
        assert_eq!(
            ReadOnlySession::<TestModel, false>::capabilities(),
            Capabilities::from_supported_commands([CommandKind::RequestTelemetry])
        );
    }

    #[test]
    fn model_specs_expose_read_only_operation_class() {
        assert_eq!(
            NosfetAeroModel::READ_OPERATION.safety_class(),
            SafetyClass::ReadOnly
        );
        assert_eq!(
            BegodeFalconModel::READ_OPERATION.safety_class(),
            SafetyClass::ReadOnly
        );
    }

    #[test]
    fn read_only_operation_traits_preserve_model_capabilities() {
        assert_eq!(
            <NosfetAeroModel as SupportsReadRequests>::READ_CAPABILITIES,
            ReadOnlySession::<NosfetAeroModel, false>::capabilities()
        );
        assert_eq!(
            <BegodeFalconModel as SupportsReadRequests>::READ_CAPABILITIES,
            ReadOnlySession::<BegodeFalconModel, true>::capabilities()
        );
    }

    #[test]
    fn write_and_actuation_operations_have_distinct_safety_classes() {
        assert_eq!(
            SettingsWriteOperation.safety_class(),
            SafetyClass::StationaryOnly
        );
        assert_eq!(
            BenignControlOperation.safety_class(),
            SafetyClass::BenignControl
        );
        assert_eq!(
            DangerousActuationOperation.safety_class(),
            SafetyClass::Actuation
        );
    }

    #[test]
    fn read_only_gate_accepts_supported_read_commands() {
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::RequestDiagnostics),
            ReadOnlyCommandGate::SupportedRead(CommandKind::RequestDiagnostics)
        );
        assert_eq!(
            gate_read_only_command::<BegodeFalconModel>(DeviceCommand::RequestIdentity),
            ReadOnlyCommandGate::SupportedRead(CommandKind::RequestIdentity)
        );
    }

    #[test]
    fn read_only_gate_rejects_unsupported_read_commands() {
        assert_eq!(
            gate_read_only_command::<BegodeFalconModel>(DeviceCommand::RequestDiagnostics),
            ReadOnlyCommandGate::Unsupported(CommandKind::RequestDiagnostics)
        );
    }

    #[test]
    fn read_only_gate_rejects_write_control_and_actuation_commands() {
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::RequestSettings),
            ReadOnlyCommandGate::Unsupported(CommandKind::RequestSettings)
        );
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::SetLights(
                cutout_core::LightState::On
            )),
            ReadOnlyCommandGate::Unsupported(CommandKind::SetLights)
        );
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::SoundHorn),
            ReadOnlyCommandGate::Unsupported(CommandKind::SoundHorn)
        );
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::SetRawMotorCurrent {
                current_ma: 1
            }),
            ReadOnlyCommandGate::Unsupported(CommandKind::SetRawMotorCurrent)
        );
    }

    #[test]
    fn falcon_read_only_session_writes_identity_request_bytes() {
        let mut session = ReadOnlySession::<BegodeFalconModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::RequestIdentity),
            &mut output,
        );

        assert_eq!(
            output,
            vec![SessionOutput::Transport(TransportAction::Write {
                channel: BEGODE_DATA_CHANNEL,
                bytes: WritePayload::try_from_slice(b"N").expect("fixture payload fits"),
                mode: WriteMode::WithoutResponse,
            })]
        );
    }

    #[test]
    fn falcon_read_only_session_writes_firmware_request_bytes() {
        let mut session = ReadOnlySession::<BegodeFalconModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::RequestFirmwareInfo),
            &mut output,
        );

        assert_eq!(
            output,
            vec![SessionOutput::Transport(TransportAction::Write {
                channel: BEGODE_DATA_CHANNEL,
                bytes: WritePayload::try_from_slice(b"V").expect("fixture payload fits"),
                mode: WriteMode::WithoutResponse,
            })]
        );
    }

    #[test]
    fn falcon_read_only_session_keeps_passive_requests_write_free() {
        let mut session = ReadOnlySession::<BegodeFalconModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::RequestTelemetry),
            &mut output,
        );
        session.handle(
            SessionInput::Command(DeviceCommand::RequestBatteryInfo),
            &mut output,
        );

        assert!(output.is_empty());
    }

    #[test]
    fn falcon_read_only_session_rejects_unsupported_diagnostics_without_writes() {
        let mut session = ReadOnlySession::<BegodeFalconModel, true>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::RequestDiagnostics),
            &mut output,
        );

        assert!(output.is_empty());
    }

    #[test]
    fn read_only_session_never_emits_transport_for_unsupported_commands() {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::SetRawMotorCurrent { current_ma: 1 }),
            &mut output,
        );

        assert!(
            output
                .iter()
                .all(|item| !matches!(item, SessionOutput::Transport(_)))
        );
    }

    #[cfg(feature = "dangerous-controls")]
    #[test]
    fn dangerous_control_session_refuses_missing_arm_without_transport() {
        let mut session =
            DangerousControlSession::<TestModel>::new(cutout_core::DangerousActuationPolicy {
                model: TestModel::MODEL,
                max_current_ma: 5_000,
                arm_duration_ms: 1_000,
            });
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::SetRawMotorCurrent { current_ma: 1_000 }),
            &mut output,
        );

        assert!(
            output
                .iter()
                .all(|item| !matches!(item, SessionOutput::Transport(_)))
        );
        assert_eq!(
            output,
            vec![SessionOutput::Event(DeviceEvent::ControlRefusal(
                cutout_core::ControlRefusal {
                    command: CommandKind::SetRawMotorCurrent,
                    safety_class: SafetyClass::Actuation,
                    reason: cutout_core::ControlRefusalReason::MissingArm,
                }
            ))]
        );
    }

    #[cfg(feature = "dangerous-controls")]
    #[test]
    fn dangerous_control_session_refuses_expired_arm_without_transport() {
        let policy = cutout_core::DangerousActuationPolicy {
            model: TestModel::MODEL,
            max_current_ma: 5_000,
            arm_duration_ms: 1_000,
        };
        let mut session = DangerousControlSession::<TestModel>::new(policy);
        let mut output = Vec::new();

        session.arm(policy.arm(10));
        session.handle(
            SessionInput::Tick {
                monotonic_ms: 1_011,
            },
            &mut output,
        );
        session.handle(
            SessionInput::Command(DeviceCommand::SetRawMotorCurrent { current_ma: 1_000 }),
            &mut output,
        );

        assert!(
            output
                .iter()
                .all(|item| !matches!(item, SessionOutput::Transport(_)))
        );
        assert!(
            output.contains(&SessionOutput::Event(DeviceEvent::ControlRefusal(
                cutout_core::ControlRefusal {
                    command: CommandKind::SetRawMotorCurrent,
                    safety_class: SafetyClass::Actuation,
                    reason: cutout_core::ControlRefusalReason::ExpiredArm,
                }
            )))
        );
    }

    #[cfg(feature = "dangerous-controls")]
    #[test]
    fn dangerous_control_session_refuses_wrong_model_arm_without_transport() {
        let policy = cutout_core::DangerousActuationPolicy {
            model: TestModel::MODEL,
            max_current_ma: 5_000,
            arm_duration_ms: 1_000,
        };
        let wrong_model_policy = cutout_core::DangerousActuationPolicy {
            model: "other model",
            max_current_ma: 5_000,
            arm_duration_ms: 1_000,
        };
        let mut session = DangerousControlSession::<TestModel>::new(policy);
        let mut output = Vec::new();

        session.arm(wrong_model_policy.arm(10));
        session.handle(SessionInput::Tick { monotonic_ms: 42 }, &mut output);
        session.handle(
            SessionInput::Command(DeviceCommand::SetRawMotorCurrent { current_ma: 1_000 }),
            &mut output,
        );

        assert!(
            output
                .iter()
                .all(|item| !matches!(item, SessionOutput::Transport(_)))
        );
        assert!(
            output.contains(&SessionOutput::Event(DeviceEvent::ControlRefusal(
                cutout_core::ControlRefusal {
                    command: CommandKind::SetRawMotorCurrent,
                    safety_class: SafetyClass::Actuation,
                    reason: cutout_core::ControlRefusalReason::WrongModel,
                }
            )))
        );
    }

    #[cfg(feature = "dangerous-controls")]
    #[test]
    fn dangerous_control_session_refuses_over_current_without_transport() {
        let policy = cutout_core::DangerousActuationPolicy {
            model: TestModel::MODEL,
            max_current_ma: 5_000,
            arm_duration_ms: 1_000,
        };
        let mut session = DangerousControlSession::<TestModel>::new(policy);
        let mut output = Vec::new();

        session.arm(policy.arm(10));
        session.handle(SessionInput::Tick { monotonic_ms: 42 }, &mut output);
        session.handle(
            SessionInput::Command(DeviceCommand::SetRawMotorCurrent { current_ma: 5_001 }),
            &mut output,
        );

        assert!(
            output
                .iter()
                .all(|item| !matches!(item, SessionOutput::Transport(_)))
        );
        assert!(
            output.contains(&SessionOutput::Event(DeviceEvent::ControlRefusal(
                cutout_core::ControlRefusal {
                    command: CommandKind::SetRawMotorCurrent,
                    safety_class: SafetyClass::Actuation,
                    reason: cutout_core::ControlRefusalReason::CurrentLimitExceeded,
                }
            )))
        );
    }
}
