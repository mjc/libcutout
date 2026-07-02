//! Concrete `UniFFI` mobile binding surface for Cutout.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_core::{
    CommandKindDto, ControlRefusalReasonDto, DeviceCommandDto, GattChannel, GattFingerprint,
    GattRoles, MeasuredI16Dto, MeasuredI32Dto, MeasuredI64Dto, MeasuredU8Dto, MeasuredU64Dto,
    MonotonicMillisDto, MonotonicTimestamp, NotificationByteLenDto, NotificationEvidenceDto,
    NotificationIngestOutcomeDto, ParserDiagnosticCountDto, ParserDiagnosticsDto,
    ParserDroppedBytesDto, ParserErrorDto, ParserFrameLenDto, ParserGapEvidenceDto,
    PayloadBodyLenDto, PevcapCapture, PevcapEncoding, PevcapHeader, PevcapRecord,
    PevcapResolvedIdentity, ProtocolFamily, ProtocolFamilyDto, ReservedPayloadEvidenceDto,
    SemanticEventCountDto, SessionInputDto, SessionOutputDto, TelemetrySnapshotDto,
    TransportActionDto, TransportWriteLimit, TransportWriteLimitDto, ValueQualityDto,
    ValueSourceDto, VerificationStatus, VerificationStatusDto, VerifiedValue,
    WallClockUnixTimestamp,
};
use cutout_protocols::{
    ConcreteAeroReadOnlySession, ConcreteFalconProfileDto, ConcreteFalconReadOnlySession,
    ConcreteSessionErrorDto, ConcreteSessionStepResultDto, new_nosfet_aero_read_only_session,
    try_new_begode_falcon_read_only_session,
};

uniffi::setup_scaffolding!();

/// Mobile discovery candidate support state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileDiscoveryCandidateSupportDto {
    /// Candidate has a supported read-only route.
    Supported,

    /// Candidate looks relevant but is not supported for launch.
    Unsupported,
}

/// Mobile EUC read-only session model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileElectricUnicycleModelDto {
    /// NOSFET Aero session.
    Aero,

    /// Begode Falcon session.
    Falcon,
}

/// Mobile discovery candidate for picker UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDiscoveryCandidateDto {
    /// Platform-local peripheral identifier; not a Bluetooth MAC address.
    pub platform_identifier: String,

    /// Observed display name for UI use.
    pub display_name: String,

    /// Product category for grouping and copy.
    pub product_category: String,

    /// Human-readable evidence summary.
    pub evidence: String,

    /// Row detail or disabled reason.
    pub detail: String,

    /// Whether this advertisement is relevant to the mobile picker.
    pub is_picker_candidate: bool,

    /// Support state.
    pub support: MobileDiscoveryCandidateSupportDto,

    /// Supported connection route key, when connecting is allowed.
    pub connection_route: Option<String>,

    /// Electric-unicycle session model to construct for the route.
    pub electric_unicycle_model: Option<MobileElectricUnicycleModelDto>,

    /// Disabled reason, when connecting is not allowed.
    pub disabled_reason: Option<String>,
}

/// Build a mobile discovery candidate from advertisement evidence.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI exports owned sequences"
)]
#[uniffi::export]
pub fn mobile_discovery_candidate_from_advertisement(
    platform_identifier: String,
    local_name: Option<String>,
    advertised_service_uuids: Vec<u16>,
) -> MobileDiscoveryCandidateDto {
    let display_name = local_name.unwrap_or_else(|| "Unknown Bluetooth device".to_owned());
    let lower_name = display_name.to_ascii_lowercase();
    if advertised_service_uuids.contains(&0xffe0) {
        let electric_unicycle_model = mobile_electric_unicycle_model_from_name(&lower_name);
        return MobileDiscoveryCandidateDto {
            platform_identifier,
            display_name,
            product_category: "Electric unicycle".to_owned(),
            evidence: "advertisement hint".to_owned(),
            detail: electric_unicycle_model
                .map_or("Model not confirmed", |model| match model {
                    MobileElectricUnicycleModelDto::Aero => "NOSFET Aero candidate",
                    MobileElectricUnicycleModelDto::Falcon => "Begode Falcon candidate",
                })
                .to_owned(),
            is_picker_candidate: true,
            support: electric_unicycle_model
                .map_or(MobileDiscoveryCandidateSupportDto::Unsupported, |_| {
                    MobileDiscoveryCandidateSupportDto::Supported
                }),
            connection_route: electric_unicycle_model.map(|_| "electric_unicycle".to_owned()),
            electric_unicycle_model,
            disabled_reason: electric_unicycle_model
                .is_none()
                .then(|| "Model not confirmed".to_owned()),
        };
    }

    if advertised_service_uuids.contains(&0xfff0)
        || lower_name.contains("vesc")
        || lower_name.contains("focer")
        || lower_name.contains("onewheel")
        || lower_name.contains("floatwheel")
    {
        return MobileDiscoveryCandidateDto {
            platform_identifier,
            display_name,
            product_category: "VESC Onewheel".to_owned(),
            evidence: "VESC advertisement hint".to_owned(),
            detail: "Not yet supported".to_owned(),
            is_picker_candidate: true,
            support: MobileDiscoveryCandidateSupportDto::Unsupported,
            connection_route: None,
            electric_unicycle_model: None,
            disabled_reason: Some("Not yet supported".to_owned()),
        };
    }

    MobileDiscoveryCandidateDto {
        platform_identifier,
        display_name,
        product_category: "Unknown rideable".to_owned(),
        evidence: "advertisement observed".to_owned(),
        detail: "Not yet supported".to_owned(),
        is_picker_candidate: false,
        support: MobileDiscoveryCandidateSupportDto::Unsupported,
        connection_route: None,
        electric_unicycle_model: None,
        disabled_reason: Some("Not yet supported".to_owned()),
    }
}

fn mobile_electric_unicycle_model_from_name(
    lower_name: &str,
) -> Option<MobileElectricUnicycleModelDto> {
    if lower_name.contains("falcon") {
        return Some(MobileElectricUnicycleModelDto::Falcon);
    }

    if lower_name.contains("aero") || lower_name.contains("nosfet") {
        return Some(MobileElectricUnicycleModelDto::Aero);
    }

    None
}

/// Mobile DTO command kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCommandDto {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request telemetry.
    RequestTelemetry,

    /// Request firmware information.
    RequestFirmwareInfo,

    /// Request battery information.
    RequestBatteryInfo,

    /// Request diagnostics.
    RequestDiagnostics,

    /// Sound a horn or alert.
    SoundHorn,
}

/// Mobile DTO input kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionInputKindDto {
    /// Link-up input.
    LinkUp,

    /// Link-down input.
    LinkDown,

    /// Notification input.
    Notification,

    /// Timer tick input.
    Tick,

    /// Device command input.
    Command,
}

/// Mobile DTO input.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionInputDto {
    /// Input kind.
    pub kind: MobileSessionInputKindDto,

    /// Monotonic timestamp.
    pub monotonic_ms: MobileMonotonicMillisDto,

    /// Maximum write length, when known.
    pub max_write_len: Option<MobileTransportWriteLimitDto>,

    /// Transport channel bytes for notification inputs.
    pub channel: Vec<u8>,

    /// Owned notification bytes.
    pub bytes: Vec<u8>,

    /// Command for command inputs.
    pub command: Option<MobileCommandDto>,
}

/// Mobile DTO monotonic timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMonotonicMillisDto {
    /// Timestamp value in milliseconds.
    pub milliseconds: u64,
}

impl MobileMonotonicMillisDto {
    const fn from_core_ffi_timestamp(timestamp: MonotonicMillisDto) -> Self {
        Self {
            milliseconds: timestamp.milliseconds,
        }
    }

    fn into_core_ffi(self) -> MonotonicMillisDto {
        MonotonicMillisDto {
            milliseconds: self.milliseconds,
        }
    }

    fn into_core(self) -> MonotonicTimestamp {
        MonotonicTimestamp::new(self.milliseconds)
    }
}

/// Mobile DTO wall-clock Unix timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileWallClockUnixMillisDto {
    /// Timestamp value in Unix epoch milliseconds.
    pub milliseconds: u64,
}

impl MobileWallClockUnixMillisDto {
    fn into_core(self) -> WallClockUnixTimestamp {
        WallClockUnixTimestamp::new(self.milliseconds)
    }
}

/// Mobile maximum transport write payload length.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileTransportWriteLimitDto {
    /// Length in bytes.
    pub bytes: u16,
}

impl MobileTransportWriteLimitDto {
    const fn into_core_ffi(self) -> TransportWriteLimitDto {
        TransportWriteLimitDto { bytes: self.bytes }
    }
}

impl From<TransportWriteLimitDto> for MobileTransportWriteLimitDto {
    fn from(value: TransportWriteLimitDto) -> Self {
        Self { bytes: value.bytes }
    }
}

/// Mobile notification payload length.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileNotificationByteLenDto {
    /// Length in bytes.
    pub bytes: u64,
}

impl From<NotificationByteLenDto> for MobileNotificationByteLenDto {
    fn from(value: NotificationByteLenDto) -> Self {
        Self {
            bytes: value.bytes as u64,
        }
    }
}

/// Mobile protocol payload body length.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePayloadBodyLenDto {
    /// Length in bytes.
    pub bytes: u64,
}

impl From<PayloadBodyLenDto> for MobilePayloadBodyLenDto {
    fn from(value: PayloadBodyLenDto) -> Self {
        Self {
            bytes: value.bytes as u64,
        }
    }
}

/// Mobile semantic event count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSemanticEventCountDto {
    /// Count of emitted semantic events.
    pub count: u64,
}

impl From<SemanticEventCountDto> for MobileSemanticEventCountDto {
    fn from(value: SemanticEventCountDto) -> Self {
        Self {
            count: value.count as u64,
        }
    }
}

/// Mobile dropped parser byte count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserDroppedBytesDto {
    /// Count of dropped bytes.
    pub bytes: u64,
}

impl From<ParserDroppedBytesDto> for MobileParserDroppedBytesDto {
    fn from(value: ParserDroppedBytesDto) -> Self {
        Self { bytes: value.bytes }
    }
}

/// Mobile parser diagnostic event count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserDiagnosticCountDto {
    /// Count of parser diagnostic events.
    pub count: u64,
}

impl From<ParserDiagnosticCountDto> for MobileParserDiagnosticCountDto {
    fn from(value: ParserDiagnosticCountDto) -> Self {
        Self { count: value.count }
    }
}

/// Mobile output kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionOutputKindDto {
    /// Subscribe transport action.
    Subscribe,

    /// Write transport action.
    Write,

    /// Non-transport event.
    Event,

    /// Disconnect transport action.
    Disconnect,

    /// Typed protocol notification ingest outcome.
    NotificationIngest,
}

/// Mobile output DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionOutputDto {
    /// Output kind.
    pub kind: MobileSessionOutputKindDto,

    /// Transport channel bytes.
    pub channel: Vec<u8>,

    /// Transport payload bytes.
    pub bytes: Vec<u8>,

    /// Typed parser-first ingest outcome.
    pub ingest: Option<MobileNotificationIngestOutcomeDto>,
}

/// Mobile notification ingest outcome kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileNotificationIngestOutcomeKindDto {
    /// Notification produced semantic protocol events.
    SemanticEvents,

    /// Notification bytes are a valid partial frame.
    BufferedFragment,

    /// Notification was recognized but failed parser validation.
    ParserDiagnostic,

    /// Notification carried a known reserved/opaque payload.
    KnownReserved,

    /// Notification reached a known parser gap.
    ParserGap,

    /// Notification was intentionally ignored.
    Ignored,
}

/// Mobile parser error kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileParserErrorKindDto {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame,

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before the expected data arrived.
    Timeout,

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

/// Mobile parser error DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserErrorDto {
    /// Error kind.
    pub kind: MobileParserErrorKindDto,

    /// Claimed or observed frame length.
    pub claimed: Option<MobileParserFrameLenDto>,

    /// Configured maximum accepted frame length.
    pub max: Option<MobileParserFrameLenDto>,

    /// Elapsed monotonic time.
    pub elapsed_ms: Option<MobileMonotonicMillisDto>,

    /// Timeout threshold in monotonic time.
    pub timeout_ms: Option<MobileMonotonicMillisDto>,
}

/// Mobile parser frame length DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserFrameLenDto {
    /// Length in bytes.
    pub bytes: u64,
}

/// Mobile reserved payload evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileReservedPayloadEvidenceDto {
    /// Selector/page identifier when present.
    pub selector: Option<u8>,

    /// Tag/opcode when present.
    pub tag: Option<u16>,

    /// Reserved payload body length.
    pub body_len: MobilePayloadBodyLenDto,

    /// Evidence verification status.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile parser gap evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserGapEvidenceDto {
    /// Selector/page identifier when present.
    pub selector: Option<u8>,

    /// Tag/opcode when present.
    pub tag: Option<u16>,

    /// Unparsed body length.
    pub body_len: MobilePayloadBodyLenDto,
}

/// Mobile notification evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileNotificationEvidenceDto {
    /// Protocol family that accepted or classified the notification.
    pub family: Option<MobileProtocolFamilyDto>,

    /// GATT channel UUID bytes.
    pub channel: Vec<u8>,

    /// Notification payload length without retaining payload bytes.
    pub len: MobileNotificationByteLenDto,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MobileMonotonicMillisDto,
}

/// Mobile parser-first notification ingest outcome DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileNotificationIngestOutcomeDto {
    /// Outcome kind.
    pub kind: MobileNotificationIngestOutcomeKindDto,

    /// Shared notification evidence without raw payload bytes.
    pub notification: MobileNotificationEvidenceDto,

    /// Number of semantic events emitted from this notification.
    pub event_count: Option<MobileSemanticEventCountDto>,

    /// Parser error for diagnostic outcomes.
    pub parser_error: Option<MobileParserErrorDto>,

    /// Reserved payload evidence for known-reserved outcomes.
    pub reserved: Option<MobileReservedPayloadEvidenceDto>,

    /// Parser gap evidence for parser-gap outcomes.
    pub gap: Option<MobileParserGapEvidenceDto>,
}

/// Mobile step-error kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionStepErrorKindDto {
    /// Mobile input was malformed before it reached the protocol session.
    InvalidInput,

    /// Command was refused.
    CommandRefused,

    /// Falcon profile was not supported.
    UnsupportedFalconProfile,

    /// Session output buffer filled before a step could finish.
    OutputBufferFull,
}

/// Mobile session step error DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionStepErrorDto {
    /// Error kind.
    pub kind: MobileSessionStepErrorKindDto,

    /// Command associated with the error, if any.
    pub command: Option<MobileCommandDto>,

    /// Refusal reason, if any.
    pub reason: Option<String>,
}

/// Mobile result of one session step.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionStepResultDto {
    /// Owned outputs from the step.
    pub outputs: Vec<MobileSessionOutputDto>,

    /// Stable error from the step, if any.
    pub error: Option<MobileSessionStepErrorDto>,
}

/// Mobile telemetry snapshot DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileTelemetrySnapshotDto {
    /// Snapshot timestamp.
    pub at_ms: Option<MobileMonotonicMillisDto>,

    /// Reported or calculated speed in millimeters per second.
    pub speed: Option<MobileMeasuredI32Dto>,

    /// Reported voltage in millivolts.
    pub voltage: Option<MobileMeasuredI32Dto>,

    /// Reported battery current in milliamps.
    pub battery_current: Option<MobileMeasuredI32Dto>,

    /// Reported motor current in milliamps.
    pub motor_current: Option<MobileMeasuredI32Dto>,

    /// Reported power in milliwatts.
    pub power: Option<MobileMeasuredI64Dto>,

    /// Reported controller temperature in millicelsius.
    pub controller_temperature: Option<MobileMeasuredI32Dto>,

    /// Reported motor temperature in millicelsius.
    pub motor_temperature: Option<MobileMeasuredI32Dto>,

    /// Reported battery temperature in millicelsius.
    pub battery_temperature: Option<MobileMeasuredI32Dto>,

    /// Reported PWM duty in permille.
    pub pwm: Option<MobileMeasuredI16Dto>,

    /// Reported distance in millimeters.
    pub distance: Option<MobileMeasuredU64Dto>,

    /// Reported pitch in millidegrees.
    pub pitch: Option<MobileMeasuredI32Dto>,

    /// Reported roll in millidegrees.
    pub roll: Option<MobileMeasuredI32Dto>,

    /// Reported battery level.
    pub battery_level_reported: Option<MobileMeasuredU8Dto>,

    /// Estimated battery percent.
    pub battery_level_estimated: Option<MobileMeasuredU8Dto>,
}

/// Confidence level for BMS topology mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileBmsTopologyConfidenceDto {
    /// Topology and group mapping were verified from known device evidence.
    Verified,

    /// Topology is plausible but not fully confirmed.
    Inferred,

    /// BMS data exists, but group mapping is not trustworthy yet.
    Unverified,
}

/// Alert level for BMS groups and decoded fault summaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileBmsAlertLevelDto {
    /// Value is in the nominal range.
    Nominal,

    /// Value deserves attention but is not yet critical.
    Warning,

    /// Value is critical or should block stronger claims in the UI.
    Critical,

    /// Data exists but cannot be classified yet.
    Unknown,
}

/// Topology summary for a BMS snapshot.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBmsTopologyDto {
    /// Human-readable topology label such as `20S4P split pack`.
    pub layout_label: String,

    /// Series group count when known.
    pub series_group_count: Option<u16>,

    /// Parallel count when known.
    pub parallel_count: Option<u16>,

    /// Number of packs or modules visible in the snapshot.
    pub pack_count: u8,

    /// Number of BMS controllers reporting in the snapshot.
    pub bms_count: u8,

    /// Confidence in the current topology mapping.
    pub confidence: MobileBmsTopologyConfidenceDto,
}

/// Per-group BMS reading for cells or grouped series strings.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBmsGroupSnapshotDto {
    /// One-based group index used in the UI.
    pub index: u16,

    /// Optional explicit label such as `left pack`.
    pub label: Option<String>,

    /// Group voltage in millivolts.
    pub voltage: Option<MobileMeasuredI32Dto>,

    /// Group temperature in millicelsius.
    pub temperature: Option<MobileMeasuredI32Dto>,

    /// Estimated internal resistance in milliohms.
    pub resistance_milliohms: Option<u16>,

    /// Whether balancing is active for this group when known.
    pub is_balancing: Option<bool>,

    /// Alert level for coloring and prioritization.
    pub alert_level: MobileBmsAlertLevelDto,

    /// Optional UI-safe detail string such as a trend note.
    pub detail: Option<String>,
}

/// Decoded BMS fault or advisory.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBmsFaultDto {
    /// Fault code or bitmask label.
    pub code: String,

    /// Human-readable fault label.
    pub label: String,

    /// Severity for operator-facing treatment.
    pub alert_level: MobileBmsAlertLevelDto,
}

/// Typed BMS snapshot for mobile pack and cells screens.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBmsSnapshotDto {
    /// Topology summary for this reading.
    pub topology: MobileBmsTopologyDto,

    /// State of charge or usable energy percent when known.
    pub energy_percent: Option<MobileMeasuredU8Dto>,

    /// Pack voltage in millivolts.
    pub voltage: Option<MobileMeasuredI32Dto>,

    /// Pack current in milliamps.
    pub current: Option<MobileMeasuredI32Dto>,

    /// Cell-group delta in millivolts.
    pub cell_delta_millivolts: Option<MobileMeasuredI32Dto>,

    /// One-based index of the lowest group when known.
    pub lowest_group_index: Option<u16>,

    /// Highest observed temperature in millicelsius.
    pub highest_temperature: Option<MobileMeasuredI32Dto>,

    /// Human-readable label for the hottest area.
    pub highest_temperature_label: Option<String>,

    /// Pack-level balancing summary.
    pub balancing_summary: Option<String>,

    /// Additional balancing detail.
    pub balancing_detail: Option<String>,

    /// Pack-level fault summary.
    pub fault_summary: Option<String>,

    /// Additional fault detail.
    pub fault_detail: Option<String>,

    /// Per-group readings.
    pub groups: Vec<MobileBmsGroupSnapshotDto>,

    /// Decoded faults or advisories.
    pub faults: Vec<MobileBmsFaultDto>,

    /// Optional unsupported-device capture action title.
    pub capture_action_title: Option<String>,

    /// Optional state label for the capture action.
    pub capture_action_state: Option<String>,
}

/// Mobile measured i32 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMeasuredI32Dto {
    /// Fixed-unit value.
    pub value: i32,

    /// Value source.
    pub source: MobileValueSourceDto,

    /// Value quality.
    pub quality: MobileValueQualityDto,

    /// Value verification status.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile measured i64 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMeasuredI64Dto {
    /// Fixed-unit value.
    pub value: i64,

    /// Value source.
    pub source: MobileValueSourceDto,

    /// Value quality.
    pub quality: MobileValueQualityDto,

    /// Value verification status.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile measured i16 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMeasuredI16Dto {
    /// Fixed-unit value.
    pub value: i16,

    /// Value source.
    pub source: MobileValueSourceDto,

    /// Value quality.
    pub quality: MobileValueQualityDto,

    /// Value verification status.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile measured u8 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMeasuredU8Dto {
    /// Fixed-unit value.
    pub value: u8,

    /// Value source.
    pub source: MobileValueSourceDto,

    /// Value quality.
    pub quality: MobileValueQualityDto,

    /// Value verification status.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile measured u64 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMeasuredU64Dto {
    /// Fixed-unit value.
    pub value: u64,

    /// Value source.
    pub source: MobileValueSourceDto,

    /// Value quality.
    pub quality: MobileValueQualityDto,

    /// Value verification status.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile parser diagnostics DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserDiagnosticsDto {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: MobileParserDroppedBytesDto,

    /// Parser resynchronization attempts.
    pub resyncs: MobileParserDiagnosticCountDto,

    /// Malformed frame count.
    pub malformed_frames: MobileParserDiagnosticCountDto,

    /// Bad checksum count.
    pub bad_checksums: MobileParserDiagnosticCountDto,

    /// Parser timeout count.
    pub timeouts: MobileParserDiagnosticCountDto,

    /// Oversized frame count.
    pub oversized_frames: MobileParserDiagnosticCountDto,

    /// Unmatched reply count.
    pub unmatched_replies: MobileParserDiagnosticCountDto,
}

/// Mobile Falcon construction profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileFalconProfileDto {
    /// Default known Falcon profile.
    Default,

    /// Deliberate unsupported sentinel used to keep binding errors typed.
    Unsupported,
}

/// Mobile PEVCAP export encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePevcapEncodingDto {
    /// JSON Lines PEVCAP stream.
    Jsonl,

    /// Binary PEVCAP container.
    Binary,
}

/// Mobile protocol-family identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileProtocolFamilyDto {
    /// Veteran/LeaperKim/NOSFET frame family.
    VeteranLeaperkimNosfet,

    /// Begode/Gotway frame family.
    BegodeGotway,

    /// VESC UART/CAN-derived family.
    Vesc,
}

/// Mobile value source.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileValueSourceDto {
    /// Value was reported directly by the device.
    Reported,

    /// Value was calculated from other values.
    Calculated,

    /// Value was estimated from incomplete evidence.
    Estimated,
}

/// Mobile value quality.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileValueQualityDto {
    /// Value is directly supported by observed data.
    Known,

    /// Value is inferred from partial or indirect evidence.
    Inferred,
}

/// Mobile verification status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVerificationStatusDto {
    /// Not yet verified.
    Unverified,

    /// Inferred from partial evidence.
    Inferred,

    /// Verified against source-attributed protocol documentation.
    SourceVerified,

    /// Verified against actual Bluetooth hardware.
    HardwareVerified,

    /// Verified against source-attributed documentation and hardware.
    SourceAndHardwareVerified,
}

/// Mobile GATT characteristic role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileGattRoleDto {
    /// Characteristic supports reads.
    Read,

    /// Characteristic supports writes with response.
    Write,

    /// Characteristic supports writes without response.
    WriteWithoutResponse,

    /// Characteristic supports notifications.
    Notify,

    /// Characteristic supports indications.
    Indicate,
}

/// Mobile GATT service/characteristic fingerprint.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileGattFingerprintDto {
    /// Service UUID bytes.
    pub service: Vec<u8>,

    /// Characteristic UUID bytes.
    pub characteristic: Vec<u8>,

    /// Observed characteristic roles.
    pub roles: Vec<MobileGattRoleDto>,

    /// Verification status for this fingerprint.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile verified string field.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileVerifiedStringDto {
    /// Field value.
    pub value: String,

    /// Verification status for the value.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile resolved identity metadata.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileResolvedIdentityDto {
    /// Resolved protocol family, when known.
    pub protocol_family: Option<MobileProtocolFamilyDto>,

    /// Resolved model name, when known.
    pub model: Option<MobileVerifiedStringDto>,

    /// Resolved firmware string, when known.
    pub firmware: Option<MobileVerifiedStringDto>,
}

/// Mobile Falcon construction error.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileSessionConstructorError {
    /// Requested Falcon profile is not supported.
    #[error("unsupported Falcon profile")]
    UnsupportedFalconProfile,
}

/// Mobile PEVCAP export error.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileCaptureExportError {
    /// Header metadata exceeded PEVCAP bounded fields.
    #[error("invalid capture header")]
    InvalidHeader,

    /// PEVCAP encoding failed.
    #[error("capture encode failed")]
    EncodeFailed,
}

/// Mobile PEVCAP input error.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileInvalidInputError {
    /// UUID byte slices must be exactly 128-bit Bluetooth UUIDs.
    #[error("invalid UUID byte length")]
    InvalidUuidLength,
}

/// Mobile-facing builder for a PEVCAP capture export.
#[derive(Debug, uniffi::Object)]
pub struct MobilePevcapCaptureBuilder {
    wall_clock_start_unix_ms: WallClockUnixTimestamp,
    platform_id: String,
    write_limit: Option<TransportWriteLimit>,
    advertised_services: Mutex<Vec<GattChannel>>,
    gatt_fingerprints: Mutex<Vec<GattFingerprint>>,
    resolved_identity: Mutex<Option<PevcapResolvedIdentity>>,
    annotations: Mutex<Vec<String>>,
    records: Mutex<Vec<PevcapRecord>>,
}

#[uniffi::export]
impl MobilePevcapCaptureBuilder {
    /// Creates an empty PEVCAP capture builder.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(
        wall_clock_start_unix_ms: MobileWallClockUnixMillisDto,
        platform_id: String,
        write_limit: Option<MobileTransportWriteLimitDto>,
    ) -> Arc<Self> {
        Arc::new(Self {
            wall_clock_start_unix_ms: wall_clock_start_unix_ms.into_core(),
            platform_id,
            write_limit: write_limit.map(|value| TransportWriteLimit::from_bytes(value.bytes)),
            advertised_services: Mutex::new(Vec::new()),
            gatt_fingerprints: Mutex::new(Vec::new()),
            resolved_identity: Mutex::new(None),
            annotations: Mutex::new(Vec::new()),
            records: Mutex::new(Vec::new()),
        })
    }

    /// Adds an advertised service UUID observed by the mobile BLE stack.
    ///
    /// # Errors
    ///
    /// Returns [`MobileInvalidInputError::InvalidUuidLength`] when `service`
    /// is not exactly 16 bytes.
    #[allow(clippy::needless_pass_by_value)]
    pub fn add_advertised_service(&self, service: Vec<u8>) -> Result<(), MobileInvalidInputError> {
        let service = mobile_gatt_channel(&service)?;
        self.advertised_services
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(service);
        Ok(())
    }

    /// Adds an observed GATT service/characteristic fingerprint.
    ///
    /// # Errors
    ///
    /// Returns [`MobileInvalidInputError::InvalidUuidLength`] when either UUID
    /// is not exactly 16 bytes.
    pub fn add_gatt_fingerprint(
        &self,
        fingerprint: MobileGattFingerprintDto,
    ) -> Result<(), MobileInvalidInputError> {
        let fingerprint = GattFingerprint::try_from(fingerprint)?;
        self.gatt_fingerprints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fingerprint);
        Ok(())
    }

    /// Sets the resolved model/firmware identity for the capture.
    pub fn set_resolved_identity(&self, identity: MobileResolvedIdentityDto) {
        *self
            .resolved_identity
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(identity.into());
    }

    /// Adds a capture annotation, preserving key/value text exactly.
    pub fn add_annotation(&self, annotation: String) {
        self.annotations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(annotation);
    }

    /// Records a link-up lifecycle event.
    pub fn record_link_up(
        &self,
        monotonic_ms: MobileMonotonicMillisDto,
        max_write_len: Option<MobileTransportWriteLimitDto>,
    ) {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(PevcapRecord::link_up(
                monotonic_ms.into_core(),
                max_write_len.map(|value| TransportWriteLimit::from_bytes(value.bytes)),
            ));
    }

    /// Records a link-down lifecycle event.
    pub fn record_link_down(&self, monotonic_ms: MobileMonotonicMillisDto) {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(PevcapRecord::link_down(monotonic_ms.into_core()));
    }

    /// Records inbound notification bytes.
    ///
    /// # Errors
    ///
    /// Returns [`MobileInvalidInputError::InvalidUuidLength`] when either UUID
    /// is not exactly 16 bytes.
    #[allow(clippy::needless_pass_by_value)]
    pub fn record_notification(
        &self,
        monotonic_ms: MobileMonotonicMillisDto,
        characteristic: Vec<u8>,
        service: Vec<u8>,
        bytes: Vec<u8>,
    ) -> Result<(), MobileInvalidInputError> {
        let characteristic = mobile_gatt_channel(&characteristic)?;
        let service = mobile_gatt_channel(&service)?;
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(PevcapRecord::inbound_notification(
                monotonic_ms.into_core(),
                characteristic,
                service,
                bytes,
            ));
        Ok(())
    }

    /// Exports the current capture as PEVCAP bytes.
    ///
    /// # Errors
    ///
    /// Returns [`MobileCaptureExportError`] when metadata is outside PEVCAP
    /// bounds or encoding fails.
    pub fn export(
        &self,
        encoding: MobilePevcapEncodingDto,
    ) -> Result<Vec<u8>, MobileCaptureExportError> {
        self.capture()?
            .encode(encoding.into())
            .map_err(|_err| MobileCaptureExportError::EncodeFailed)
    }
}

impl MobilePevcapCaptureBuilder {
    fn capture(&self) -> Result<PevcapCapture, MobileCaptureExportError> {
        let annotations = self
            .annotations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let advertised_services = self
            .advertised_services
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let gatt_fingerprints = self
            .gatt_fingerprints
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let resolved_identity = self
            .resolved_identity
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let annotation_refs = annotations.iter().map(String::as_str).collect::<Vec<_>>();
        let header = PevcapHeader::new(
            self.wall_clock_start_unix_ms,
            self.platform_id.as_str(),
            self.write_limit,
            &advertised_services,
            &gatt_fingerprints,
            None,
            resolved_identity,
            env!("CARGO_PKG_VERSION"),
            [0; 32],
            &annotation_refs,
        )
        .map_err(|_err| MobileCaptureExportError::InvalidHeader)?;
        let records = self
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        Ok(PevcapCapture::new(header, records))
    }
}

impl From<MobileProtocolFamilyDto> for ProtocolFamily {
    fn from(family: MobileProtocolFamilyDto) -> Self {
        match family {
            MobileProtocolFamilyDto::VeteranLeaperkimNosfet => Self::VeteranLeaperkimNosfet,
            MobileProtocolFamilyDto::BegodeGotway => Self::BegodeGotway,
            MobileProtocolFamilyDto::Vesc => Self::Vesc,
        }
    }
}

impl From<ProtocolFamilyDto> for MobileProtocolFamilyDto {
    fn from(family: ProtocolFamilyDto) -> Self {
        match family {
            ProtocolFamilyDto::VeteranLeaperkimNosfet => Self::VeteranLeaperkimNosfet,
            ProtocolFamilyDto::BegodeGotway => Self::BegodeGotway,
            ProtocolFamilyDto::Vesc => Self::Vesc,
        }
    }
}

impl From<ValueSourceDto> for MobileValueSourceDto {
    fn from(source: ValueSourceDto) -> Self {
        match source {
            ValueSourceDto::Reported => Self::Reported,
            ValueSourceDto::Calculated => Self::Calculated,
            ValueSourceDto::Estimated => Self::Estimated,
        }
    }
}

impl From<ValueQualityDto> for MobileValueQualityDto {
    fn from(quality: ValueQualityDto) -> Self {
        match quality {
            ValueQualityDto::Known => Self::Known,
            ValueQualityDto::Inferred => Self::Inferred,
        }
    }
}

impl From<VerificationStatusDto> for MobileVerificationStatusDto {
    fn from(status: VerificationStatusDto) -> Self {
        match status {
            VerificationStatusDto::Unverified => Self::Unverified,
            VerificationStatusDto::Inferred => Self::Inferred,
            VerificationStatusDto::SourceVerified => Self::SourceVerified,
            VerificationStatusDto::HardwareVerified => Self::HardwareVerified,
            VerificationStatusDto::SourceAndHardwareVerified => Self::SourceAndHardwareVerified,
        }
    }
}

impl From<MobileVerificationStatusDto> for VerificationStatus {
    fn from(status: MobileVerificationStatusDto) -> Self {
        match status {
            MobileVerificationStatusDto::Unverified => Self::Unverified,
            MobileVerificationStatusDto::Inferred => Self::Inferred,
            MobileVerificationStatusDto::SourceVerified => Self::SourceVerified,
            MobileVerificationStatusDto::HardwareVerified => Self::HardwareVerified,
            MobileVerificationStatusDto::SourceAndHardwareVerified => {
                Self::SourceAndHardwareVerified
            }
        }
    }
}

impl From<MobileGattRoleDto> for GattRoles {
    fn from(role: MobileGattRoleDto) -> Self {
        match role {
            MobileGattRoleDto::Read => Self::empty().with_read(),
            MobileGattRoleDto::Write => Self::empty().with_write(),
            MobileGattRoleDto::WriteWithoutResponse => Self::empty().with_write_without_response(),
            MobileGattRoleDto::Notify => Self::empty().with_notify(),
            MobileGattRoleDto::Indicate => Self::empty().with_indicate(),
        }
    }
}

fn mobile_gatt_roles(roles: Vec<MobileGattRoleDto>) -> GattRoles {
    roles
        .into_iter()
        .fold(GattRoles::empty(), |accumulator, role| match role {
            MobileGattRoleDto::Read => accumulator.with_read(),
            MobileGattRoleDto::Write => accumulator.with_write(),
            MobileGattRoleDto::WriteWithoutResponse => accumulator.with_write_without_response(),
            MobileGattRoleDto::Notify => accumulator.with_notify(),
            MobileGattRoleDto::Indicate => accumulator.with_indicate(),
        })
}

impl TryFrom<MobileGattFingerprintDto> for GattFingerprint {
    type Error = MobileInvalidInputError;

    fn try_from(fingerprint: MobileGattFingerprintDto) -> Result<Self, Self::Error> {
        Ok(Self {
            service: mobile_gatt_channel(&fingerprint.service)?,
            characteristic: mobile_gatt_channel(&fingerprint.characteristic)?,
            roles: mobile_gatt_roles(fingerprint.roles),
            verification: fingerprint.verification.into(),
        })
    }
}

impl From<MobileVerifiedStringDto> for VerifiedValue<String> {
    fn from(value: MobileVerifiedStringDto) -> Self {
        Self {
            value: value.value,
            verification: value.verification.into(),
        }
    }
}

impl From<MobileResolvedIdentityDto> for PevcapResolvedIdentity {
    fn from(identity: MobileResolvedIdentityDto) -> Self {
        Self {
            protocol_family: identity.protocol_family.map(Into::into),
            model: identity.model.map(Into::into),
            firmware: identity.firmware.map(Into::into),
        }
    }
}

impl From<MobilePevcapEncodingDto> for PevcapEncoding {
    fn from(encoding: MobilePevcapEncodingDto) -> Self {
        match encoding {
            MobilePevcapEncodingDto::Jsonl => Self::Jsonl,
            MobilePevcapEncodingDto::Binary => Self::Binary,
        }
    }
}

fn mobile_gatt_channel(channel: &[u8]) -> Result<GattChannel, MobileInvalidInputError> {
    mobile_channel_bytes(channel)
        .map(GattChannel::from_bytes)
        .ok_or(MobileInvalidInputError::InvalidUuidLength)
}

/// Mobile-facing wrapper for a NOSFET Aero read-only session.
#[derive(Debug, uniffi::Object)]
pub struct AeroReadOnlySession {
    inner: Mutex<ConcreteAeroReadOnlySession>,
}

#[uniffi::export]
impl AeroReadOnlySession {
    /// Creates a NOSFET Aero read-only session.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(new_nosfet_aero_read_only_session()),
        })
    }

    /// Drives one input and returns owned outputs plus any stable error DTO.
    pub fn ingest_checked(&self, input: MobileSessionInputDto) -> MobileSessionStepResultDto {
        let input = match SessionInputDto::try_from(input) {
            Ok(input) => input,
            Err(reason) => return invalid_mobile_input(reason),
        };
        MobileSessionStepResultDto::from(self.lock_inner().ingest_checked(&input))
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    pub fn drain_outputs(&self) -> Vec<MobileSessionOutputDto> {
        self.lock_inner()
            .drain_outputs()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    pub fn current_snapshot(&self) -> MobileTelemetrySnapshotDto {
        self.lock_inner().current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    pub fn diagnostics(&self) -> MobileParserDiagnosticsDto {
        self.lock_inner().diagnostics().into()
    }
}

impl AeroReadOnlySession {
    fn lock_inner(&self) -> MutexGuard<'_, ConcreteAeroReadOnlySession> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for AeroReadOnlySession {
    fn default() -> Self {
        Self {
            inner: Mutex::new(new_nosfet_aero_read_only_session()),
        }
    }
}

impl TryFrom<MobileSessionInputDto> for SessionInputDto {
    type Error = &'static str;

    fn try_from(input: MobileSessionInputDto) -> Result<Self, Self::Error> {
        Ok(match input.kind {
            MobileSessionInputKindDto::LinkUp => Self::LinkUp {
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
                max_write_len: input
                    .max_write_len
                    .map(MobileTransportWriteLimitDto::into_core_ffi),
            },
            MobileSessionInputKindDto::LinkDown => Self::LinkDown,
            MobileSessionInputKindDto::Notification => Self::Notification {
                channel: mobile_channel_bytes(&input.channel)
                    .ok_or("invalid_channel_uuid_length")?,
                bytes: input.bytes,
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
            },
            MobileSessionInputKindDto::Tick => Self::Tick {
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
            },
            MobileSessionInputKindDto::Command => {
                Self::Command(input.command.ok_or("missing_command")?.into())
            }
        })
    }
}

fn invalid_mobile_input(reason: &'static str) -> MobileSessionStepResultDto {
    MobileSessionStepResultDto {
        outputs: Vec::new(),
        error: Some(MobileSessionStepErrorDto {
            kind: MobileSessionStepErrorKindDto::InvalidInput,
            command: None,
            reason: Some(reason.to_owned()),
        }),
    }
}

fn mobile_channel_bytes(channel: &[u8]) -> Option<[u8; 16]> {
    channel.try_into().ok()
}

impl From<MobileCommandDto> for DeviceCommandDto {
    fn from(command: MobileCommandDto) -> Self {
        match command {
            MobileCommandDto::RequestIdentity => Self::RequestIdentity,
            MobileCommandDto::RequestTelemetry => Self::RequestTelemetry,
            MobileCommandDto::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            MobileCommandDto::RequestBatteryInfo => Self::RequestBatteryInfo,
            MobileCommandDto::RequestDiagnostics => Self::RequestDiagnostics,
            MobileCommandDto::SoundHorn => Self::SoundHorn,
        }
    }
}

impl TryFrom<CommandKindDto> for MobileCommandDto {
    type Error = ();

    fn try_from(command: CommandKindDto) -> Result<Self, Self::Error> {
        match command {
            CommandKindDto::RequestIdentity => Ok(Self::RequestIdentity),
            CommandKindDto::RequestTelemetry => Ok(Self::RequestTelemetry),
            CommandKindDto::RequestFirmwareInfo => Ok(Self::RequestFirmwareInfo),
            CommandKindDto::RequestBatteryInfo => Ok(Self::RequestBatteryInfo),
            CommandKindDto::RequestDiagnostics => Ok(Self::RequestDiagnostics),
            CommandKindDto::SoundHorn => Ok(Self::SoundHorn),
            CommandKindDto::RequestSettings
            | CommandKindDto::SetLights
            | CommandKindDto::SetRawMotorCurrent => Err(()),
        }
    }
}

impl From<SessionOutputDto> for MobileSessionOutputDto {
    fn from(output: SessionOutputDto) -> Self {
        match output {
            SessionOutputDto::Transport(TransportActionDto::Subscribe { channel }) => Self {
                kind: MobileSessionOutputKindDto::Subscribe,
                channel: channel.to_vec(),
                bytes: Vec::new(),
                ingest: None,
            },
            SessionOutputDto::Transport(TransportActionDto::Write { channel, bytes, .. }) => Self {
                kind: MobileSessionOutputKindDto::Write,
                channel: channel.to_vec(),
                bytes,
                ingest: None,
            },
            SessionOutputDto::Transport(TransportActionDto::Disconnect) => Self {
                kind: MobileSessionOutputKindDto::Disconnect,
                channel: Vec::new(),
                bytes: Vec::new(),
                ingest: None,
            },
            SessionOutputDto::Event(_) => Self {
                kind: MobileSessionOutputKindDto::Event,
                channel: Vec::new(),
                bytes: Vec::new(),
                ingest: None,
            },
            SessionOutputDto::NotificationIngest(outcome) => Self {
                kind: MobileSessionOutputKindDto::NotificationIngest,
                channel: Vec::new(),
                bytes: Vec::new(),
                ingest: Some(outcome.into()),
            },
        }
    }
}

impl From<NotificationIngestOutcomeDto> for MobileNotificationIngestOutcomeDto {
    fn from(outcome: NotificationIngestOutcomeDto) -> Self {
        match outcome {
            NotificationIngestOutcomeDto::SemanticEvents {
                notification,
                event_count,
            } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::SemanticEvents,
                notification: notification.into(),
                event_count: Some(event_count.into()),
                parser_error: None,
                reserved: None,
                gap: None,
            },
            NotificationIngestOutcomeDto::BufferedFragment(notification) => Self {
                kind: MobileNotificationIngestOutcomeKindDto::BufferedFragment,
                notification: notification.into(),
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: None,
            },
            NotificationIngestOutcomeDto::ParserDiagnostic {
                notification,
                error,
            } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::ParserDiagnostic,
                notification: notification.into(),
                event_count: None,
                parser_error: Some(error.into()),
                reserved: None,
                gap: None,
            },
            NotificationIngestOutcomeDto::KnownReserved {
                notification,
                payload,
            } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::KnownReserved,
                notification: notification.into(),
                event_count: None,
                parser_error: None,
                reserved: Some(payload.into()),
                gap: None,
            },
            NotificationIngestOutcomeDto::ParserGap { notification, gap } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::ParserGap,
                notification: notification.into(),
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: Some(gap.into()),
            },
            NotificationIngestOutcomeDto::Ignored(notification) => Self {
                kind: MobileNotificationIngestOutcomeKindDto::Ignored,
                notification: notification.into(),
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: None,
            },
        }
    }
}

impl From<MeasuredI32Dto> for MobileMeasuredI32Dto {
    fn from(measured: MeasuredI32Dto) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

impl From<MeasuredI64Dto> for MobileMeasuredI64Dto {
    fn from(measured: MeasuredI64Dto) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

impl From<MeasuredI16Dto> for MobileMeasuredI16Dto {
    fn from(measured: MeasuredI16Dto) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

impl From<MeasuredU8Dto> for MobileMeasuredU8Dto {
    fn from(measured: MeasuredU8Dto) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

impl From<MeasuredU64Dto> for MobileMeasuredU64Dto {
    fn from(measured: MeasuredU64Dto) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

impl From<NotificationEvidenceDto> for MobileNotificationEvidenceDto {
    fn from(evidence: NotificationEvidenceDto) -> Self {
        Self {
            family: evidence.family.map(Into::into),
            channel: evidence.channel.to_vec(),
            len: evidence.len.into(),
            monotonic_ms: MobileMonotonicMillisDto::from_core_ffi_timestamp(evidence.monotonic_ms),
        }
    }
}

impl From<ParserFrameLenDto> for MobileParserFrameLenDto {
    fn from(value: ParserFrameLenDto) -> Self {
        Self {
            bytes: value.bytes as u64,
        }
    }
}

impl From<ParserErrorDto> for MobileParserErrorDto {
    fn from(error: ParserErrorDto) -> Self {
        match error {
            ParserErrorDto::OversizedFrame { claimed, max } => Self {
                kind: MobileParserErrorKindDto::OversizedFrame,
                claimed: Some(claimed.into()),
                max: Some(max.into()),
                elapsed_ms: None,
                timeout_ms: None,
            },
            ParserErrorDto::BadChecksum => Self {
                kind: MobileParserErrorKindDto::BadChecksum,
                claimed: None,
                max: None,
                elapsed_ms: None,
                timeout_ms: None,
            },
            ParserErrorDto::MalformedFrame => Self {
                kind: MobileParserErrorKindDto::MalformedFrame,
                claimed: None,
                max: None,
                elapsed_ms: None,
                timeout_ms: None,
            },
            ParserErrorDto::Timeout {
                elapsed_ms,
                timeout_ms,
            } => Self {
                kind: MobileParserErrorKindDto::Timeout,
                claimed: None,
                max: None,
                elapsed_ms: Some(MobileMonotonicMillisDto::from_core_ffi_timestamp(
                    elapsed_ms,
                )),
                timeout_ms: Some(MobileMonotonicMillisDto::from_core_ffi_timestamp(
                    timeout_ms,
                )),
            },
            ParserErrorDto::UnmatchedReply => Self {
                kind: MobileParserErrorKindDto::UnmatchedReply,
                claimed: None,
                max: None,
                elapsed_ms: None,
                timeout_ms: None,
            },
        }
    }
}

impl From<ReservedPayloadEvidenceDto> for MobileReservedPayloadEvidenceDto {
    fn from(evidence: ReservedPayloadEvidenceDto) -> Self {
        Self {
            selector: evidence.selector,
            tag: evidence.tag,
            body_len: evidence.body_len.into(),
            verification: evidence.verification.into(),
        }
    }
}

impl From<ParserGapEvidenceDto> for MobileParserGapEvidenceDto {
    fn from(evidence: ParserGapEvidenceDto) -> Self {
        Self {
            selector: evidence.selector,
            tag: evidence.tag,
            body_len: evidence.body_len.into(),
        }
    }
}

impl From<ConcreteSessionStepResultDto> for MobileSessionStepResultDto {
    fn from(result: ConcreteSessionStepResultDto) -> Self {
        Self {
            outputs: result.outputs.into_iter().map(Into::into).collect(),
            error: result.error.map(Into::into),
        }
    }
}

impl From<ConcreteSessionErrorDto> for MobileSessionStepErrorDto {
    fn from(error: ConcreteSessionErrorDto) -> Self {
        match error {
            ConcreteSessionErrorDto::CommandRefused { refusal } => Self {
                kind: MobileSessionStepErrorKindDto::CommandRefused,
                command: MobileCommandDto::try_from(refusal.command).ok(),
                reason: Some(control_refusal_reason_text(refusal.reason).to_owned()),
            },
            ConcreteSessionErrorDto::UnsupportedFalconProfile { .. } => Self {
                kind: MobileSessionStepErrorKindDto::UnsupportedFalconProfile,
                command: None,
                reason: None,
            },
            ConcreteSessionErrorDto::OutputBufferFull { .. } => Self {
                kind: MobileSessionStepErrorKindDto::OutputBufferFull,
                command: None,
                reason: Some("session_output_buffer_full".to_owned()),
            },
        }
    }
}

impl From<TelemetrySnapshotDto> for MobileTelemetrySnapshotDto {
    fn from(snapshot: TelemetrySnapshotDto) -> Self {
        Self {
            at_ms: snapshot
                .at_ms
                .map(MobileMonotonicMillisDto::from_core_ffi_timestamp),
            speed: snapshot.speed.map(Into::into),
            voltage: snapshot.voltage.map(Into::into),
            battery_current: snapshot.battery_current.map(Into::into),
            motor_current: snapshot.motor_current.map(Into::into),
            power: snapshot.power.map(Into::into),
            controller_temperature: snapshot.controller_temperature.map(Into::into),
            motor_temperature: snapshot.motor_temperature.map(Into::into),
            battery_temperature: snapshot.battery_temperature.map(Into::into),
            pwm: snapshot.pwm.map(Into::into),
            distance: snapshot.distance.map(Into::into),
            pitch: snapshot.pitch.map(Into::into),
            roll: snapshot.roll.map(Into::into),
            battery_level_reported: snapshot.battery_level_reported.map(Into::into),
            battery_level_estimated: snapshot.battery_level_estimated.map(Into::into),
        }
    }
}

impl From<ParserDiagnosticsDto> for MobileParserDiagnosticsDto {
    fn from(diagnostics: ParserDiagnosticsDto) -> Self {
        Self {
            dropped_bytes: diagnostics.dropped_bytes.into(),
            resyncs: diagnostics.resyncs.into(),
            malformed_frames: diagnostics.malformed_frames.into(),
            bad_checksums: diagnostics.bad_checksums.into(),
            timeouts: diagnostics.timeouts.into(),
            oversized_frames: diagnostics.oversized_frames.into(),
            unmatched_replies: diagnostics.unmatched_replies.into(),
        }
    }
}

impl From<MobileFalconProfileDto> for ConcreteFalconProfileDto {
    fn from(profile: MobileFalconProfileDto) -> Self {
        match profile {
            MobileFalconProfileDto::Default => Self::Default,
            MobileFalconProfileDto::Unsupported => Self::Unsupported,
        }
    }
}

impl From<ConcreteSessionErrorDto> for MobileSessionConstructorError {
    fn from(error: ConcreteSessionErrorDto) -> Self {
        match error {
            ConcreteSessionErrorDto::CommandRefused { .. }
            | ConcreteSessionErrorDto::UnsupportedFalconProfile { .. }
            | ConcreteSessionErrorDto::OutputBufferFull { .. } => Self::UnsupportedFalconProfile,
        }
    }
}

fn control_refusal_reason_text(reason: ControlRefusalReasonDto) -> &'static str {
    match reason {
        ControlRefusalReasonDto::WrongSafetyClass => "wrong_safety_class",
        ControlRefusalReasonDto::MissingArm => "missing_arm",
        ControlRefusalReasonDto::WrongModel => "wrong_model",
        ControlRefusalReasonDto::ExpiredArm => "expired_arm",
        ControlRefusalReasonDto::CurrentLimitExceeded => "current_limit_exceeded",
        ControlRefusalReasonDto::UnsupportedCommand => "unsupported_command",
        ControlRefusalReasonDto::ActuationEncoderUnavailable => "actuation_encoder_unavailable",
    }
}

/// Mobile-facing wrapper for a Begode Falcon read-only session.
#[derive(Debug, uniffi::Object)]
pub struct FalconReadOnlySession {
    inner: Mutex<ConcreteFalconReadOnlySession>,
}

#[uniffi::export]
impl FalconReadOnlySession {
    /// Creates a Begode Falcon read-only session with the default profile.
    ///
    /// # Errors
    ///
    /// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
    /// default profile is unavailable.
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, MobileSessionConstructorError> {
        Self::with_profile(MobileFalconProfileDto::Default)
    }

    /// Creates a Begode Falcon read-only session with an explicit profile.
    ///
    /// # Errors
    ///
    /// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
    /// selected profile is unavailable.
    #[uniffi::constructor]
    pub fn with_profile(
        profile: MobileFalconProfileDto,
    ) -> Result<Arc<Self>, MobileSessionConstructorError> {
        Ok(Arc::new(Self {
            inner: Mutex::new(try_new_begode_falcon_read_only_session(profile.into())?),
        }))
    }

    /// Drives one input and returns owned outputs plus any stable error DTO.
    pub fn ingest_checked(&self, input: MobileSessionInputDto) -> MobileSessionStepResultDto {
        let input = match SessionInputDto::try_from(input) {
            Ok(input) => input,
            Err(reason) => return invalid_mobile_input(reason),
        };
        MobileSessionStepResultDto::from(self.lock_inner().ingest_checked(&input))
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    pub fn drain_outputs(&self) -> Vec<MobileSessionOutputDto> {
        self.lock_inner()
            .drain_outputs()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    pub fn current_snapshot(&self) -> MobileTelemetrySnapshotDto {
        self.lock_inner().current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    pub fn diagnostics(&self) -> MobileParserDiagnosticsDto {
        self.lock_inner().diagnostics().into()
    }
}

impl FalconReadOnlySession {
    fn lock_inner(&self) -> MutexGuard<'_, ConcreteFalconReadOnlySession> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_discovery_candidate_preserves_ios_local_id_without_mac() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-aero".to_owned(),
            Some("NOSFET Aero".to_owned()),
            vec![0xffe0],
        );

        assert_eq!(candidate.platform_identifier, "ios-local-aero");
        assert_eq!(candidate.display_name, "NOSFET Aero");
        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(candidate.evidence, "advertisement hint");
        assert_eq!(candidate.detail, "NOSFET Aero candidate");
        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            MobileDiscoveryCandidateSupportDto::Supported
        );
        assert_eq!(
            candidate.connection_route,
            Some("electric_unicycle".to_owned())
        );
        assert_eq!(
            candidate.electric_unicycle_model,
            Some(MobileElectricUnicycleModelDto::Aero)
        );
        assert_eq!(candidate.disabled_reason, None);
    }

    #[test]
    fn mobile_discovery_candidate_routes_explicit_falcon_name_to_session() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-falcon".to_owned(),
            Some("Begode Falcon".to_owned()),
            vec![0xffe0],
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            MobileDiscoveryCandidateSupportDto::Supported
        );
        assert_eq!(
            candidate.connection_route,
            Some("electric_unicycle".to_owned())
        );
        assert_eq!(
            candidate.electric_unicycle_model,
            Some(MobileElectricUnicycleModelDto::Falcon)
        );
    }

    #[test]
    fn mobile_discovery_candidate_keeps_generic_brand_name_unconfirmed() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-begode".to_owned(),
            Some("GotWay_002441".to_owned()),
            vec![0xffe0],
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            MobileDiscoveryCandidateSupportDto::Unsupported
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason.as_deref(),
            Some("Model not confirmed")
        );
    }

    #[test]
    fn mobile_bms_snapshot_dto_preserves_topology_and_group_detail() {
        let snapshot = MobileBmsSnapshotDto {
            topology: MobileBmsTopologyDto {
                layout_label: "20S4P split pack".to_owned(),
                series_group_count: Some(20),
                parallel_count: Some(4),
                pack_count: 2,
                bms_count: 2,
                confidence: MobileBmsTopologyConfidenceDto::Verified,
            },
            energy_percent: Some(MobileMeasuredU8Dto {
                value: 72,
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            voltage: Some(MobileMeasuredI32Dto {
                value: 81_600,
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            current: None,
            cell_delta_millivolts: Some(MobileMeasuredI32Dto {
                value: 18,
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            lowest_group_index: Some(17),
            highest_temperature: Some(MobileMeasuredI32Dto {
                value: 37_800,
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            highest_temperature_label: Some("right pack".to_owned()),
            balancing_summary: Some("idle • top groups only".to_owned()),
            balancing_detail: Some("3 groups bleeding: 03, 11, 19".to_owned()),
            fault_summary: Some("no active faults".to_owned()),
            fault_detail: Some("last: under-voltage warning · 3 days ago".to_owned()),
            groups: vec![MobileBmsGroupSnapshotDto {
                index: 17,
                label: Some("group 17".to_owned()),
                voltage: Some(MobileMeasuredI32Dto {
                    value: 4_071,
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                temperature: Some(MobileMeasuredI32Dto {
                    value: 34_900,
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                resistance_milliohms: Some(21),
                is_balancing: Some(true),
                alert_level: MobileBmsAlertLevelDto::Warning,
                detail: Some("drops first during acceleration".to_owned()),
            }],
            faults: vec![MobileBmsFaultDto {
                code: "0x0040".to_owned(),
                label: "needs decoder".to_owned(),
                alert_level: MobileBmsAlertLevelDto::Critical,
            }],
            capture_action_title: Some("record unsupported pack".to_owned()),
            capture_action_state: Some("disabled for launch".to_owned()),
        };

        assert_eq!(snapshot.topology.series_group_count, Some(20));
        assert_eq!(snapshot.topology.pack_count, 2);
        assert_eq!(snapshot.topology.bms_count, 2);
        assert_eq!(
            snapshot.topology.confidence,
            MobileBmsTopologyConfidenceDto::Verified
        );
        assert_eq!(snapshot.lowest_group_index, Some(17));
        assert_eq!(
            snapshot
                .groups
                .first()
                .and_then(|group| group.label.as_deref()),
            Some("group 17")
        );
        assert_eq!(
            snapshot.groups.first().and_then(|group| group.is_balancing),
            Some(true)
        );
        assert_eq!(
            snapshot
                .groups
                .first()
                .map(|group| group.resistance_milliohms),
            Some(Some(21))
        );
        assert_eq!(
            snapshot.faults.first().map(|fault| fault.code.as_str()),
            Some("0x0040")
        );
        assert_eq!(
            snapshot.capture_action_title.as_deref(),
            Some("record unsupported pack")
        );
    }

    #[test]
    fn mobile_discovery_candidate_keeps_unconfirmed_euc_visible_but_unrouteable() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-unknown-euc".to_owned(),
            Some("EUC-unknown".to_owned()),
            vec![0xffe0],
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            MobileDiscoveryCandidateSupportDto::Unsupported
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Model not confirmed".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_exposes_disabled_reason_for_unsupported() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-unknown".to_owned(),
            Some("Little FOCer".to_owned()),
            vec![0xfff0],
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(candidate.product_category, "VESC Onewheel");
        assert_eq!(
            candidate.support,
            MobileDiscoveryCandidateSupportDto::Unsupported
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Not yet supported".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_hides_unrelated_bluetooth() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-keyboard".to_owned(),
            Some("Keyboard".to_owned()),
            vec![],
        );

        assert!(!candidate.is_picker_candidate);
    }

    const fn ms(value: u64) -> MobileMonotonicMillisDto {
        MobileMonotonicMillisDto {
            milliseconds: value,
        }
    }

    const fn wc(value: u64) -> MobileWallClockUnixMillisDto {
        MobileWallClockUnixMillisDto {
            milliseconds: value,
        }
    }

    const fn notification_len(value: usize) -> NotificationByteLenDto {
        NotificationByteLenDto { bytes: value }
    }

    const fn mobile_notification_len(value: u64) -> MobileNotificationByteLenDto {
        MobileNotificationByteLenDto { bytes: value }
    }

    const fn body_len(value: usize) -> PayloadBodyLenDto {
        PayloadBodyLenDto { bytes: value }
    }

    const fn mobile_body_len(value: u64) -> MobilePayloadBodyLenDto {
        MobilePayloadBodyLenDto { bytes: value }
    }

    const fn frame_len(value: usize) -> ParserFrameLenDto {
        ParserFrameLenDto { bytes: value }
    }

    const fn mobile_frame_len(value: u64) -> MobileParserFrameLenDto {
        MobileParserFrameLenDto { bytes: value }
    }

    const fn event_count(value: usize) -> SemanticEventCountDto {
        SemanticEventCountDto { count: value }
    }

    const fn mobile_event_count(value: u64) -> MobileSemanticEventCountDto {
        MobileSemanticEventCountDto { count: value }
    }

    const fn mobile_diag_count(value: u64) -> MobileParserDiagnosticCountDto {
        MobileParserDiagnosticCountDto { count: value }
    }

    const fn mobile_write_len(value: u16) -> MobileTransportWriteLimitDto {
        MobileTransportWriteLimitDto { bytes: value }
    }

    fn notification_fixture() -> NotificationEvidenceDto {
        NotificationEvidenceDto {
            family: Some(ProtocolFamilyDto::VeteranLeaperkimNosfet),
            channel: [0x7a; 16],
            len: notification_len(17),
            monotonic_ms: MonotonicMillisDto { milliseconds: 42 },
        }
    }

    fn mobile_ingest(output: NotificationIngestOutcomeDto) -> MobileNotificationIngestOutcomeDto {
        let mobile = MobileSessionOutputDto::from(SessionOutputDto::NotificationIngest(output));

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::NotificationIngest);
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        mobile.ingest.expect("ingest output carries typed outcome")
    }

    #[test]
    fn mobile_notification_ingest_dto_preserves_each_typed_outcome_class() {
        let semantic = mobile_ingest(NotificationIngestOutcomeDto::SemanticEvents {
            notification: notification_fixture(),
            event_count: event_count(3),
        });
        assert_eq!(
            semantic.kind,
            MobileNotificationIngestOutcomeKindDto::SemanticEvents
        );
        assert_eq!(
            semantic.notification.family,
            Some(MobileProtocolFamilyDto::VeteranLeaperkimNosfet)
        );
        assert_eq!(semantic.notification.channel, vec![0x7a; 16]);
        assert_eq!(semantic.notification.len, mobile_notification_len(17));
        assert_eq!(semantic.event_count, Some(mobile_event_count(3)));
        assert_eq!(semantic.parser_error, None);

        let buffered = mobile_ingest(NotificationIngestOutcomeDto::BufferedFragment(
            notification_fixture(),
        ));
        assert_eq!(
            buffered.kind,
            MobileNotificationIngestOutcomeKindDto::BufferedFragment
        );
        assert_eq!(buffered.event_count, None);

        let diagnostic = mobile_ingest(NotificationIngestOutcomeDto::ParserDiagnostic {
            notification: notification_fixture(),
            error: ParserErrorDto::OversizedFrame {
                claimed: frame_len(257),
                max: frame_len(256),
            },
        });
        assert_eq!(
            diagnostic.parser_error,
            Some(MobileParserErrorDto {
                kind: MobileParserErrorKindDto::OversizedFrame,
                claimed: Some(mobile_frame_len(257)),
                max: Some(mobile_frame_len(256)),
                elapsed_ms: None,
                timeout_ms: None,
            })
        );

        let reserved = mobile_ingest(NotificationIngestOutcomeDto::KnownReserved {
            notification: notification_fixture(),
            payload: ReservedPayloadEvidenceDto {
                selector: Some(8),
                tag: Some(0x5a5c),
                body_len: body_len(84),
                verification: VerificationStatusDto::SourceVerified,
            },
        });
        assert_eq!(
            reserved.reserved,
            Some(MobileReservedPayloadEvidenceDto {
                selector: Some(8),
                tag: Some(0x5a5c),
                body_len: mobile_body_len(84),
                verification: MobileVerificationStatusDto::SourceVerified,
            })
        );

        let gap = mobile_ingest(NotificationIngestOutcomeDto::ParserGap {
            notification: notification_fixture(),
            gap: ParserGapEvidenceDto {
                selector: Some(9),
                tag: None,
                body_len: body_len(11),
            },
        });
        assert_eq!(
            gap.gap,
            Some(MobileParserGapEvidenceDto {
                selector: Some(9),
                tag: None,
                body_len: mobile_body_len(11),
            })
        );

        let ignored = mobile_ingest(NotificationIngestOutcomeDto::Ignored(notification_fixture()));
        assert_eq!(
            ignored.kind,
            MobileNotificationIngestOutcomeKindDto::Ignored
        );
        assert_eq!(ignored.event_count, None);
    }

    #[test]
    fn aero_wrapper_constructs_and_exposes_diagnostics() {
        let session = AeroReadOnlySession::new();

        assert_eq!(session.diagnostics().malformed_frames, mobile_diag_count(0));
    }

    #[test]
    fn falcon_wrapper_surfaces_unsupported_command_error() {
        let session = FalconReadOnlySession::new().expect("default profile should construct");

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SoundHorn),
        });

        assert!(matches!(
            result.error,
            Some(MobileSessionStepErrorDto {
                kind: MobileSessionStepErrorKindDto::CommandRefused,
                ..
            })
        ));
    }

    #[test]
    fn wrapper_rejects_missing_mobile_command_input() {
        let session = AeroReadOnlySession::new();

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: None,
        });

        assert_eq!(
            result.error,
            Some(MobileSessionStepErrorDto {
                kind: MobileSessionStepErrorKindDto::InvalidInput,
                command: None,
                reason: Some("missing_command".to_owned()),
            })
        );
        assert!(result.outputs.is_empty());
    }

    #[test]
    fn wrapper_rejects_invalid_notification_channel_lengths() {
        let session = AeroReadOnlySession::new();

        for channel in [vec![0; 15], vec![0; 17]] {
            let result = session.ingest_checked(MobileSessionInputDto {
                kind: MobileSessionInputKindDto::Notification,
                monotonic_ms: ms(0),
                max_write_len: None,
                channel,
                bytes: Vec::new(),
                command: None,
            });

            assert_eq!(
                result.error,
                Some(MobileSessionStepErrorDto {
                    kind: MobileSessionStepErrorKindDto::InvalidInput,
                    command: None,
                    reason: Some("invalid_channel_uuid_length".to_owned()),
                })
            );
            assert!(result.outputs.is_empty());
        }
    }

    #[test]
    fn unsupported_command_kind_does_not_remap_to_request_diagnostics() {
        assert!(MobileCommandDto::try_from(CommandKindDto::RequestSettings).is_err());
        assert!(MobileCommandDto::try_from(CommandKindDto::SetLights).is_err());
        assert!(MobileCommandDto::try_from(CommandKindDto::SetRawMotorCurrent).is_err());
    }

    #[test]
    fn falcon_wrapper_rejects_unsupported_profile() {
        let result = FalconReadOnlySession::with_profile(MobileFalconProfileDto::Unsupported);

        assert!(matches!(
            result,
            Err(MobileSessionConstructorError::UnsupportedFalconProfile)
        ));
    }

    #[test]
    fn aero_wrapper_accepts_owned_notification_input() {
        let session = AeroReadOnlySession::new();
        let link_result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::LinkUp,
            monotonic_ms: ms(1),
            max_write_len: Some(mobile_write_len(185)),
            channel: Vec::new(),
            bytes: Vec::new(),
            command: None,
        });
        let channel = link_result
            .outputs
            .iter()
            .find(|output| output.kind == MobileSessionOutputKindDto::Subscribe)
            .expect("link-up should subscribe")
            .channel
            .clone();

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Notification,
            monotonic_ms: ms(2),
            max_write_len: None,
            channel,
            bytes: hex_literal::hex!(
                "dc5a5c532a7c000000000000ab41001700000cff\
                 000000000226021ca8f607801afa000080c80000\
                 808080808080022880803080800e310e310e2f0e\
                 2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
                 310e2e9e05e3ad"
            )
            .to_vec(),
            command: None,
        });

        assert_eq!(result.error, None);
        let ingest = result
            .outputs
            .iter()
            .find_map(|output| output.ingest.as_ref())
            .expect("notification should emit typed ingest outcome");
        assert_eq!(
            ingest.kind,
            MobileNotificationIngestOutcomeKindDto::SemanticEvents
        );
        assert_eq!(
            ingest.notification.family,
            Some(MobileProtocolFamilyDto::VeteranLeaperkimNosfet)
        );
        assert_eq!(ingest.notification.len, mobile_notification_len(87));
        assert_eq!(ingest.notification.monotonic_ms, ms(2));
        assert_eq!(ingest.event_count, Some(mobile_event_count(5)));
        assert_eq!(ingest.parser_error, None);
        assert_eq!(ingest.reserved, None);
        assert_eq!(ingest.gap, None);
        assert!(result.outputs.iter().all(|output| output.bytes.is_empty()));
        let snapshot = session.current_snapshot();
        assert_eq!(
            snapshot.voltage,
            Some(MobileMeasuredI32Dto {
                value: 108_760,
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            })
        );
        assert!(snapshot.battery_current.is_some());
        assert!(snapshot.power.is_some());
        assert!(snapshot.controller_temperature.is_some() || snapshot.motor_temperature.is_some());
        assert!(snapshot.pwm.is_some());
        assert!(snapshot.battery_level_estimated.is_some());
    }

    #[test]
    fn mobile_capture_builder_exports_cli_readable_jsonl() {
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            Some(mobile_write_len(185)),
        );
        builder.add_annotation("capture_label=powered_on_stationary".into());
        builder.add_annotation("capture_privacy=redacted".into());
        builder.add_annotation("capture_distribution=redistributable".into());
        builder.add_annotation("capture_evidence=hardware_tested".into());
        builder.record_link_up(ms(1), Some(mobile_write_len(185)));
        builder
            .record_notification(
                ms(2),
                vec![0x11; 16],
                vec![0x22; 16],
                vec![0xde, 0xad, 0xbe, 0xef],
            )
            .expect("valid notification UUIDs");

        let bytes = builder
            .export(MobilePevcapEncodingDto::Jsonl)
            .expect("JSONL export succeeds");
        let capture =
            PevcapCapture::decode(&bytes, PevcapEncoding::Jsonl).expect("JSONL is PEVCAP");

        assert_eq!(capture.header.platform_id, "ios-corebluetooth");
        assert_eq!(
            capture.header.annotations.as_slice(),
            [
                "capture_label=powered_on_stationary",
                "capture_privacy=redacted",
                "capture_distribution=redistributable",
                "capture_evidence=hardware_tested",
            ]
        );
        assert_eq!(capture.records.len(), 2);
        assert_eq!(capture.records[1].bytes.as_ref(), [0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn mobile_capture_builder_exports_ble_inventory_metadata() {
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );
        let service = vec![0x22; 16];
        let characteristic = vec![0x11; 16];

        builder
            .add_advertised_service(service.clone())
            .expect("valid advertised service UUID");
        builder
            .add_gatt_fingerprint(MobileGattFingerprintDto {
                service: service.clone(),
                characteristic: characteristic.clone(),
                roles: vec![
                    MobileGattRoleDto::Read,
                    MobileGattRoleDto::WriteWithoutResponse,
                    MobileGattRoleDto::Notify,
                ],
                verification: MobileVerificationStatusDto::HardwareVerified,
            })
            .expect("valid GATT fingerprint UUIDs");

        let bytes = builder
            .export(MobilePevcapEncodingDto::Jsonl)
            .expect("JSONL export succeeds");
        let capture =
            PevcapCapture::decode(&bytes, PevcapEncoding::Jsonl).expect("JSONL is PEVCAP");
        let [fingerprint] = capture.header.gatt_fingerprints.as_slice() else {
            panic!("expected one GATT fingerprint");
        };

        assert_eq!(capture.header.advertised_services[0].as_bytes(), [0x22; 16]);
        assert_eq!(fingerprint.service.as_bytes(), [0x22; 16]);
        assert_eq!(fingerprint.characteristic.as_bytes(), [0x11; 16]);
        assert!(fingerprint.roles.supports_read());
        assert!(fingerprint.roles.supports_write_without_response());
        assert!(fingerprint.roles.supports_notify());
        assert_eq!(
            fingerprint.verification,
            VerificationStatus::HardwareVerified
        );
    }

    #[test]
    fn mobile_capture_builder_rejects_invalid_uuid_lengths() {
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );
        let err = builder
            .add_advertised_service(vec![0x22; 15])
            .expect_err("short advertised service UUID is rejected");

        assert_eq!(err, MobileInvalidInputError::InvalidUuidLength);

        let err = builder
            .record_notification(ms(2), vec![0x11; 17], vec![0x22; 16], vec![0xde])
            .expect_err("long characteristic UUID is rejected");

        assert_eq!(err, MobileInvalidInputError::InvalidUuidLength);

        let err = builder
            .add_gatt_fingerprint(MobileGattFingerprintDto {
                service: vec![0x22; 16],
                characteristic: vec![0x11; 15],
                roles: vec![MobileGattRoleDto::Notify],
                verification: MobileVerificationStatusDto::HardwareVerified,
            })
            .expect_err("short fingerprint characteristic UUID is rejected");

        assert_eq!(err, MobileInvalidInputError::InvalidUuidLength);
    }

    #[test]
    fn mobile_capture_builder_exports_resolved_identity_metadata() {
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );

        builder.set_resolved_identity(MobileResolvedIdentityDto {
            protocol_family: Some(MobileProtocolFamilyDto::BegodeGotway),
            model: Some(MobileVerifiedStringDto {
                value: "Begode Falcon".into(),
                verification: MobileVerificationStatusDto::Inferred,
            }),
            firmware: Some(MobileVerifiedStringDto {
                value: "GW2015004".into(),
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
        });

        let bytes = builder
            .export(MobilePevcapEncodingDto::Jsonl)
            .expect("JSONL export succeeds");
        let capture =
            PevcapCapture::decode(&bytes, PevcapEncoding::Jsonl).expect("JSONL is PEVCAP");
        let identity = capture
            .header
            .resolved_identity
            .expect("resolved identity should export");

        assert_eq!(identity.protocol_family, Some(ProtocolFamily::BegodeGotway));
        assert_eq!(identity.model.expect("model").value, "Begode Falcon");
        assert_eq!(identity.firmware.expect("firmware").value, "GW2015004");
    }

    #[test]
    fn mobile_capture_builder_exports_binary_container() {
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );
        builder.record_link_down(ms(9));

        let bytes = builder
            .export(MobilePevcapEncodingDto::Binary)
            .expect("binary export succeeds");
        let capture =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary is PEVCAP");

        assert_eq!(capture.header.platform_id, "ios-corebluetooth");
        assert_eq!(capture.records.len(), 1);
        assert_eq!(capture.records[0].monotonic_ms, MonotonicTimestamp::new(9));
    }
}
