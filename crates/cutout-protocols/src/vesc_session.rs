use cutout_core::{
    Capabilities, CommandKind, DeviceEvent, DiagnosticDetail, DiagnosticReadback,
    DiagnosticSeverity, FirmwareInfo, GattChannel, Measured, MonotonicTimestamp,
    NotificationByteLen, NotificationIngestOutcome, ParserError, ProtocolFamily, RawFieldValue,
    RawTelemetryReadback, ReadOnlyResponse, SemanticEventCount, SessionOutput, SessionOutputError,
    SessionOutputSink, ValueQuality, VerificationStatus,
};

use crate::{
    Manufacturer, ProtocolModelSpec, ReadOnlyNotificationDecoder, RequestDisposition,
    SupportsReadRequests, VESC_NOTIFY_CHANNEL, VESC_WRITE_CHANNEL, VescBoardProfile,
    VescCodecError, VescFaultCode, VescReadOnlyReply, VescReadOnlyRequest,
    VescReadOnlyStreamDecoder, VescReadOnlyStreamResult, VescRequestEncoder, VescStatsTelemetry,
    VescValuesTelemetry, session::push_parser_error,
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
        monotonic_ms: MonotonicTimestamp,
        output: &mut dyn SessionOutputSink,
    ) -> Result<(), SessionOutputError> {
        match self.stream.feed_result(bytes) {
            Ok(VescReadOnlyStreamResult::Replies(replies)) => {
                let event_count = replies
                    .iter()
                    .fold(SemanticEventCount::default(), |count, reply| {
                        count.saturating_add(vesc_reply_event_count(reply))
                    });
                for reply in &replies {
                    push_vesc_reply(reply, monotonic_ms, self.board_profile, output)?;
                }
                output.push(SessionOutput::NotificationIngest(
                    NotificationIngestOutcome::semantic_events(
                        ProtocolFamily::Vesc,
                        channel,
                        NotificationByteLen::from_bytes(bytes.len()),
                        monotonic_ms,
                        event_count,
                    ),
                ))?;
            }
            Err(VescCodecError::UnsupportedReply) => {
                push_parser_error(ParserError::UnmatchedReply, output)?;
                output.push(SessionOutput::NotificationIngest(
                    NotificationIngestOutcome::parser_diagnostic(
                        ProtocolFamily::Vesc,
                        channel,
                        NotificationByteLen::from_bytes(bytes.len()),
                        monotonic_ms,
                        ParserError::UnmatchedReply,
                    ),
                ))?;
            }
            Err(
                VescCodecError::DecodeFailed
                | VescCodecError::EncodedFrameTooLong
                | VescCodecError::EncodeFailed,
            ) => {
                push_parser_error(ParserError::MalformedFrame, output)?;
                output.push(SessionOutput::NotificationIngest(
                    NotificationIngestOutcome::parser_diagnostic(
                        ProtocolFamily::Vesc,
                        channel,
                        NotificationByteLen::from_bytes(bytes.len()),
                        monotonic_ms,
                        ParserError::MalformedFrame,
                    ),
                ))?;
            }
            Ok(VescReadOnlyStreamResult::Buffered) => {
                output.push(SessionOutput::NotificationIngest(
                    NotificationIngestOutcome::buffered_fragment(
                        ProtocolFamily::Vesc,
                        channel,
                        NotificationByteLen::from_bytes(bytes.len()),
                        monotonic_ms,
                    ),
                ))?;
            }
        }
        Ok(())
    }
}

const fn vesc_reply_event_count(reply: &VescReadOnlyReply) -> SemanticEventCount {
    match reply {
        VescReadOnlyReply::FirmwareInfo { .. } | VescReadOnlyReply::Stats(_) => {
            SemanticEventCount::from_events(1)
        }
        VescReadOnlyReply::Values(_) => SemanticEventCount::from_events(2),
    }
}

fn push_vesc_reply(
    reply: &VescReadOnlyReply,
    monotonic_ms: MonotonicTimestamp,
    board_profile: Option<VescBoardProfile>,
    output: &mut dyn SessionOutputSink,
) -> Result<(), SessionOutputError> {
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
            )))?;
            output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                ReadOnlyResponse::RawTelemetry(vesc_values_to_raw_telemetry(*values)),
            )))
        }
        VescReadOnlyReply::Stats(stats) => {
            output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                ReadOnlyResponse::Diagnostics(vesc_stats_to_diagnostics(*stats)),
            )))
        }
    }
}

fn vesc_values_to_delta(
    values: VescValuesTelemetry,
    monotonic_ms: MonotonicTimestamp,
    board_profile: Option<VescBoardProfile>,
) -> cutout_core::TelemetryDelta {
    cutout_core::TelemetryDelta {
        at_ms: monotonic_ms,
        speed: board_profile
            .and_then(|profile| profile.speed_from_erpm(values.rpm))
            .map(Measured::calculated),
        voltage: Some(Measured::reported(values.voltage)),
        battery_current: Some(Measured::reported(values.input_current)),
        ..cutout_core::TelemetryDelta::empty(monotonic_ms)
    }
}

fn vesc_values_to_raw_telemetry(values: VescValuesTelemetry) -> RawTelemetryReadback {
    RawTelemetryReadback::from_fields([
        RawFieldValue::new(VESC_RAW_ERPM_FIELD_ID, i64::from(values.rpm.as_erpm())),
        RawFieldValue::new(
            VESC_RAW_TACHOMETER_FIELD_ID,
            i64::from(values.tachometer.as_counts()),
        ),
        RawFieldValue::new(
            VESC_RAW_CONTROLLER_ID_FIELD_ID,
            i64::from(values.controller_id.get()),
        ),
        RawFieldValue::new(
            VESC_RAW_FAULT_CODE_FIELD_ID,
            i64::from(vesc_fault_code_raw(values.fault_code)),
        ),
    ])
}

const fn vesc_fault_code_raw(code: VescFaultCode) -> u8 {
    match code {
        VescFaultCode::None => 0,
        VescFaultCode::AbsOverCurrent => 1,
        VescFaultCode::Other(value) => value,
    }
}

fn vesc_stats_to_diagnostics(stats: VescStatsTelemetry) -> DiagnosticReadback {
    DiagnosticReadback::from_details([
        vesc_diagnostic_detail(
            VESC_RAW_STATS_SPEED_AVG_FIELD_ID,
            i64::from(stats.speed_avg.as_millimetres_per_second()),
        ),
        vesc_diagnostic_detail(
            VESC_RAW_STATS_POWER_AVG_FIELD_ID,
            stats.power_avg.as_milliwatts(),
        ),
        vesc_diagnostic_detail(
            VESC_RAW_STATS_CURRENT_AVG_FIELD_ID,
            i64::from(stats.current_avg.as_milliamps()),
        ),
        vesc_diagnostic_detail(
            VESC_RAW_STATS_COUNT_TIME_FIELD_ID,
            i64::try_from(stats.count_time.as_milliseconds()).unwrap_or(i64::MAX),
        ),
    ])
}

const fn vesc_diagnostic_detail(id: u16, value: i64) -> DiagnosticDetail {
    DiagnosticDetail {
        field: RawFieldValue::new(id, value),
        severity: DiagnosticSeverity::Info,
        quality: ValueQuality::Known,
        verification: VerificationStatus::Inferred,
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
