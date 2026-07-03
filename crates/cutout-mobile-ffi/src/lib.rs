//! Concrete `UniFFI` mobile binding surface for Cutout.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_core::{
    BatteryInfoDto, BatteryReadbackAvailabilityDto, BatteryReadbackDto, ChargeModeDto,
    CommandKindDto, ControlRefusalReasonDto, DeviceCommandDto, FaultCode, FaultCodeDto,
    FaultHistoryAvailability, FaultHistoryAvailabilityDto, FaultHistoryEntry, FaultHistoryEntryDto,
    FaultHistoryReadback, FaultHistoryReadbackDto, GattChannel, GattFingerprint, GattRoles,
    IgnoredNotificationEvidenceDto, IgnoredNotificationReasonDto, MeasuredChargeModeDto,
    MeasuredI16Dto, MeasuredI32Dto, MeasuredI64Dto, MeasuredU8Dto, MeasuredU64Dto,
    MonotonicMillisDto, MonotonicTimestamp, NotificationByteLenDto, NotificationEvidenceDto,
    NotificationIngestOutcomeDto, ParserDiagnosticCountDto, ParserDiagnosticsDto,
    ParserDroppedBytesDto, ParserErrorDto, ParserFrameLenDto, ParserGapEvidenceDto,
    PayloadBodyLenDto, PevcapCapture, PevcapEncoding, PevcapHeader, PevcapRecord,
    PevcapResolvedIdentity, ProtocolFamily, ProtocolFamilyDto, RawFieldValue, RawFieldValueDto,
    ReadOnlyOutputPayload, ReservedPayloadEvidenceDto, SemanticEventCountDto, SessionInputDto,
    SessionOutputDto, SettingsEntry, SettingsEntryDto, SettingsReadback,
    SettingsReadbackAvailability, SettingsReadbackAvailabilityDto, SettingsReadbackDto,
    Speed as CoreSpeed, TelemetrySnapshotDto, TransportActionDto, TransportWriteLimit,
    TransportWriteLimitDto, ValueQuality, ValueQualityDto, ValueSource, ValueSourceDto,
    VerificationStatus, VerificationStatusDto, VerifiedValue, WallClockUnixTimestamp,
};
use cutout_protocols::{
    BEGODE_FIELD_TILTBACK_SPEED_KMH, ConcreteAeroReadOnlySession, ConcreteFalconProfileDto,
    ConcreteFalconReadOnlySession, ConcreteSessionErrorDto, ConcreteSessionStepResultDto,
    IdentityBannerEvidence, ProtocolFamilyClassification, ProtocolModelIdentityEvidence,
    StagedIdentityInput, StagedIdentityOutcome, VETERAN_FIELD_PEDALS_MODE,
    VETERAN_FIELD_SPEED_ALERT_DECI_KMH, VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
    identify_known_model, new_nosfet_aero_read_only_session,
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

/// Build a mobile discovery candidate from Veteran/NOSFET protocol identity.
#[must_use]
#[uniffi::export]
pub fn mobile_discovery_candidate_from_veteran_protocol_identity(
    platform_identifier: String,
    display_name: String,
    model_id: u16,
) -> MobileDiscoveryCandidateDto {
    let resolution = identify_known_model(&StagedIdentityInput {
        advertised_name: None,
        gatt: &[] as &[GattFingerprint],
        stream_family: ProtocolFamilyClassification::Pending,
        banner_model: IdentityBannerEvidence::Missing,
        protocol_model: ProtocolModelIdentityEvidence::model_id(
            ProtocolFamily::VeteranLeaperkimNosfet,
            model_id,
        ),
    });

    let Some(model) = resolution.model else {
        return MobileDiscoveryCandidateDto {
            platform_identifier,
            display_name,
            product_category: "Electric unicycle".to_owned(),
            evidence: "Veteran protocol model id".to_owned(),
            detail: format!("Unknown Veteran/NOSFET model id {model_id}"),
            is_picker_candidate: true,
            support: MobileDiscoveryCandidateSupportDto::Unsupported,
            connection_route: None,
            electric_unicycle_model: None,
            disabled_reason: Some("Model not supported".to_owned()),
        };
    };

    let electric_unicycle_model = match (
        model.protocol_family,
        model.wire_model_id.map(|wire_model_id| wire_model_id.value),
    ) {
        (ProtocolFamily::VeteranLeaperkimNosfet, Some(43)) => {
            Some(MobileElectricUnicycleModelDto::Aero)
        }
        _ => None,
    };
    let supported = electric_unicycle_model.is_some();

    MobileDiscoveryCandidateDto {
        platform_identifier,
        display_name,
        product_category: "Electric unicycle".to_owned(),
        evidence: "Veteran protocol model id".to_owned(),
        detail: match resolution.outcome {
            StagedIdentityOutcome::Matched => {
                format!("{} confirmed by model id {model_id}", model.model)
            }
            _ => format!("Veteran/NOSFET model id {model_id} confirmed"),
        },
        is_picker_candidate: true,
        support: if supported {
            MobileDiscoveryCandidateSupportDto::Supported
        } else {
            MobileDiscoveryCandidateSupportDto::Unsupported
        },
        connection_route: supported.then(|| "electric_unicycle".to_owned()),
        electric_unicycle_model,
        disabled_reason: (!supported).then(|| "Model not supported".to_owned()),
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

    /// Request historical fault information.
    RequestFaultHistory,

    /// Request current settings without changing device state.
    RequestSettings,

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

    /// Read-only settings response.
    SettingsReadback,

    /// Read-only fault-history response.
    FaultHistoryReadback,

    /// Read-only BMS or pack-health response.
    BmsSnapshot,
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

    /// Typed read-only settings response.
    pub settings_readback: Option<MobileSettingsReadbackDto>,

    /// Typed read-only fault-history response.
    pub fault_history_readback: Option<MobileFaultHistoryReadbackDto>,

    /// Typed read-only BMS or pack-health response.
    pub bms_snapshot: Option<MobileBmsSnapshotDto>,

    /// Veteran/NOSFET protocol model id when an Aero-family session decoded it.
    pub veteran_protocol_model_id: Option<u16>,
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

/// Mobile ignored notification reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileIgnoredNotificationReasonDto {
    /// Notification arrived on a channel the selected protocol does not consume.
    WrongChannel,

    /// Notification could not be associated with a supported protocol family.
    UnsupportedFamily,

    /// Notification was classified to a family but not to a supported channel.
    UnsupportedChannel,

    /// Notification was accepted by a known family but no semantic mapping exists yet.
    AcceptedButUnmapped,

    /// Notification advanced frame-boundary search without completing a frame.
    SeekingFrameBoundary,

    /// Notification was classified and intentionally dropped by policy.
    IntentionallyDropped,
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

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,

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

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,
}

/// Mobile notification evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileNotificationEvidenceDto {
    /// Protocol family that accepted or classified the notification.
    pub family: MobileProtocolFamilyDto,

    /// GATT channel UUID bytes.
    pub channel: Vec<u8>,

    /// Notification payload length without retaining payload bytes.
    pub len: MobileNotificationByteLenDto,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MobileMonotonicMillisDto,
}

/// Mobile ignored notification evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileIgnoredNotificationEvidenceDto {
    /// Protocol family when classification got that far.
    pub family: Option<MobileProtocolFamilyDto>,

    /// GATT channel UUID bytes.
    pub channel: Vec<u8>,

    /// Notification payload length without retaining payload bytes.
    pub len: MobileNotificationByteLenDto,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MobileMonotonicMillisDto,

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,
}

/// Mobile parser-first notification ingest outcome DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileNotificationIngestOutcomeDto {
    /// Outcome kind.
    pub kind: MobileNotificationIngestOutcomeKindDto,

    /// Accepted notification evidence without raw payload bytes.
    pub notification: Option<MobileNotificationEvidenceDto>,

    /// Number of semantic events emitted from this notification.
    pub event_count: Option<MobileSemanticEventCountDto>,

    /// Parser error for diagnostic outcomes.
    pub parser_error: Option<MobileParserErrorDto>,

    /// Reserved payload evidence for known-reserved outcomes.
    pub reserved: Option<MobileReservedPayloadEvidenceDto>,

    /// Parser gap evidence for parser-gap outcomes.
    pub gap: Option<MobileParserGapEvidenceDto>,

    /// Ignored notification evidence.
    pub ignored: Option<MobileIgnoredNotificationEvidenceDto>,

    /// Reason an ignored notification did not enter a decoder path.
    pub ignored_reason: Option<MobileIgnoredNotificationReasonDto>,
}

/// Mobile step-error kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionStepErrorKindDto {
    /// Command was refused.
    CommandRefused,

    /// Falcon profile was not supported.
    UnsupportedFalconProfile,
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

    /// Reported or calculated speed.
    pub speed: Option<SpeedReading>,

    /// Conservative operating state inferred from currently available telemetry.
    pub operating_state: RideOperatingState,

    /// Reported voltage.
    pub voltage: Option<VoltageReading>,

    /// Reported battery current.
    pub battery_current: Option<BatteryCurrentReading>,

    /// Reported motor current.
    pub motor_current: Option<PhaseCurrentReading>,

    /// Reported power.
    pub power: Option<PowerReading>,

    /// Signed power/current flow direction when known enough for conservative UI labels.
    pub power_flow: Option<PowerFlowDirection>,

    /// Reported controller temperature.
    pub controller_temperature: Option<TemperatureReading>,

    /// Reported motor temperature.
    pub motor_temperature: Option<TemperatureReading>,

    /// Reported battery temperature.
    pub battery_temperature: Option<TemperatureReading>,

    /// Reported PWM duty in permille.
    pub pwm: Option<DutyCycle>,

    /// Reported distance.
    pub distance: Option<DistanceReading>,

    /// Reported pitch.
    pub pitch: Option<AngleReading>,

    /// Reported roll.
    pub roll: Option<AngleReading>,

    /// Reported battery level.
    pub battery_level_reported: Option<BatteryLevelReading>,

    /// Estimated battery percent.
    pub battery_level_estimated: Option<BatteryLevelReading>,
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

    /// Group voltage.
    pub voltage: Option<VoltageReading>,

    /// Group temperature.
    pub temperature: Option<TemperatureReading>,

    /// Estimated internal resistance.
    pub resistance: Option<Resistance>,

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
    /// Whether BMS or pack-health data is available for display.
    pub availability: MobileReadbackAvailabilityDto,

    /// Topology summary for this reading.
    pub topology: MobileBmsTopologyDto,

    /// State of charge or usable energy percent when known.
    pub energy_percent: Option<BatteryLevelReading>,

    /// Pack voltage.
    pub voltage: Option<VoltageReading>,

    /// Pack current.
    pub current: Option<BatteryCurrentReading>,

    /// Cell-group voltage delta.
    pub cell_delta: Option<VoltageDeltaReading>,

    /// One-based index of the lowest group when known.
    pub lowest_group_index: Option<u16>,

    /// Highest observed temperature.
    pub highest_temperature: Option<TemperatureReading>,

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

impl From<BatteryInfoDto> for MobileBmsSnapshotDto {
    fn from(battery: BatteryInfoDto) -> Self {
        Self::from_page(MobileReadbackAvailabilityDto::Available, battery)
    }
}

impl From<BatteryReadbackDto> for MobileBmsSnapshotDto {
    fn from(readback: BatteryReadbackDto) -> Self {
        let availability = readback.availability.into();
        match (availability, readback.page) {
            (MobileReadbackAvailabilityDto::Available, Some(page)) => {
                Self::from_page(availability, page)
            }
            _ => Self::empty_readback(availability),
        }
    }
}

impl MobileBmsSnapshotDto {
    fn from_page(availability: MobileReadbackAvailabilityDto, battery: BatteryInfoDto) -> Self {
        let groups = bms_groups_from_cell_voltages(&battery.cell_voltages);
        Self {
            availability,
            topology: MobileBmsTopologyDto::unknown_readback(),
            energy_percent: battery
                .level_reported
                .or(battery.level_estimated)
                .map(Into::into),
            voltage: battery.voltage.map(Into::into),
            current: battery.current.map(Into::into),
            cell_delta: cell_voltage_delta(&battery.cell_voltages),
            lowest_group_index: lowest_cell_voltage_group_index(&battery.cell_voltages),
            highest_temperature: highest_battery_temperature(
                battery.temperature,
                battery.temperatures,
            ),
            highest_temperature_label: None,
            balancing_summary: None,
            balancing_detail: None,
            fault_summary: None,
            fault_detail: None,
            groups,
            faults: Vec::new(),
            capture_action_title: None,
            capture_action_state: None,
        }
    }

    fn empty_readback(availability: MobileReadbackAvailabilityDto) -> Self {
        Self {
            availability,
            topology: MobileBmsTopologyDto::unknown_readback(),
            energy_percent: None,
            voltage: None,
            current: None,
            cell_delta: None,
            lowest_group_index: None,
            highest_temperature: None,
            highest_temperature_label: None,
            balancing_summary: None,
            balancing_detail: None,
            fault_summary: None,
            fault_detail: None,
            groups: Vec::new(),
            faults: Vec::new(),
            capture_action_title: None,
            capture_action_state: None,
        }
    }
}

fn bms_groups_from_cell_voltages(
    cell_voltages: &[MeasuredI32Dto],
) -> Vec<MobileBmsGroupSnapshotDto> {
    cell_voltages
        .iter()
        .enumerate()
        .filter_map(|(index, voltage)| {
            let group_index = one_based_group_index(index)?;
            Some(MobileBmsGroupSnapshotDto {
                index: group_index,
                label: Some(format!("group {group_index}")),
                voltage: Some((*voltage).into()),
                temperature: None,
                resistance: None,
                is_balancing: None,
                alert_level: MobileBmsAlertLevelDto::Nominal,
                detail: None,
            })
        })
        .collect()
}

fn one_based_group_index(index: usize) -> Option<u16> {
    index
        .checked_add(1)
        .and_then(|index| u16::try_from(index).ok())
}

fn cell_voltage_delta(cell_voltages: &[MeasuredI32Dto]) -> Option<VoltageDeltaReading> {
    let min = cell_voltages.iter().map(|voltage| voltage.value).min()?;
    let max = cell_voltages.iter().map(|voltage| voltage.value).max()?;
    let first = cell_voltages.first()?;
    Some(VoltageDeltaReading {
        value: VoltageDelta {
            value: max.saturating_sub(min),
        },
        source: MobileValueSourceDto::Calculated,
        quality: MobileValueQualityDto::Known,
        verification: first.verification.into(),
    })
}

fn lowest_cell_voltage_group_index(cell_voltages: &[MeasuredI32Dto]) -> Option<u16> {
    cell_voltages
        .iter()
        .enumerate()
        .min_by_key(|(_, voltage)| voltage.value)
        .and_then(|(index, _)| one_based_group_index(index))
}

impl MobileBmsTopologyDto {
    fn unknown_readback() -> Self {
        Self {
            layout_label: "unknown BMS topology".to_owned(),
            series_group_count: None,
            parallel_count: None,
            pack_count: 0,
            bms_count: 0,
            confidence: MobileBmsTopologyConfidenceDto::Unverified,
        }
    }
}

/// Conservative signed power/current flow direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum PowerFlowDirection {
    /// Positive discharge from pack to controller/motor.
    Discharge,

    /// No measurable signed pack flow.
    Zero,

    /// Negative signed flow without enough motion or plug context to label charge or regen.
    NegativeUnknown,
}

/// Conservative EUC ride operating state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum RideOperatingState {
    /// No live evidence has established whether the EUC is parked, riding, or charging.
    Unknown,

    /// Telemetry indicates the EUC is stationary.
    Parked,

    /// Telemetry indicates the EUC is moving under ride context.
    Riding,

    /// Explicit charge/plug evidence indicates the EUC is charging.
    Charging,
}

macro_rules! mobile_quantity {
    ($quantity:ident, $reading:ident, $raw:ty, $quantity_doc:literal, $reading_doc:literal) => {
        #[doc = $quantity_doc]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
        pub struct $quantity {
            /// Fixed-unit value owned by this quantity type.
            pub value: $raw,
        }

        #[doc = $reading_doc]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
        pub struct $reading {
            /// Semantic quantity value.
            pub value: $quantity,

            /// Value source.
            pub source: MobileValueSourceDto,

            /// Value quality.
            pub quality: MobileValueQualityDto,

            /// Value verification status.
            pub verification: MobileVerificationStatusDto,
        }
    };
}

mobile_quantity!(Speed, SpeedReading, i32, "Speed.", "Measured speed.");
mobile_quantity!(
    Voltage,
    VoltageReading,
    i32,
    "Voltage.",
    "Measured voltage."
);
mobile_quantity!(
    BatteryCurrent,
    BatteryCurrentReading,
    i32,
    "Battery current.",
    "Measured battery current."
);
mobile_quantity!(
    PhaseCurrent,
    PhaseCurrentReading,
    i32,
    "Phase current.",
    "Measured phase current."
);
mobile_quantity!(Power, PowerReading, i64, "Power.", "Measured power.");
mobile_quantity!(
    Temperature,
    TemperatureReading,
    i32,
    "Temperature.",
    "Measured temperature."
);
mobile_quantity!(
    Distance,
    DistanceReading,
    u64,
    "Distance.",
    "Measured distance."
);
mobile_quantity!(Angle, AngleReading, i32, "Angle.", "Measured angle.");
mobile_quantity!(
    BatteryLevel,
    BatteryLevelReading,
    u8,
    "Battery level.",
    "Measured battery level."
);
mobile_quantity!(
    VoltageDelta,
    VoltageDeltaReading,
    i32,
    "Voltage delta.",
    "Measured voltage delta."
);

/// Electrical resistance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct Resistance {
    /// Fixed-unit resistance value.
    pub value: u16,
}

/// PWM duty cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DutyCycle {
    /// Permille duty cycle.
    pub permille: i16,
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

/// Availability of a read-only response for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileReadbackAvailabilityDto {
    /// The device reported the requested readback.
    Available,

    /// The readback is expected for this device/profile but was not available.
    Unavailable,

    /// The readback is not supported for this device/profile.
    Unsupported,
}

/// Raw numeric field from a read-only settings response.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRawFieldValueDto {
    /// Protocol-family field identifier.
    pub id: u16,

    /// Sign-extended value exactly as reported by the protocol layer.
    pub value: i64,
}

/// Generic read-only settings entry for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingsEntryDto {
    /// Raw field value with protocol identity preserved.
    pub field: MobileRawFieldValueDto,

    /// Source of the settings value.
    pub source: MobileValueSourceDto,

    /// Confidence in the settings value.
    pub quality: MobileValueQualityDto,

    /// Verification state for the settings value.
    pub verification: MobileVerificationStatusDto,
}

/// Product-shaped EUC garage settings projection for mobile UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileEucGarageSettingsDto {
    /// Whether this projected settings readback is available.
    pub availability: MobileReadbackAvailabilityDto,

    /// Beep margin speed setting, when understood.
    pub beep_margin: Option<SpeedReading>,

    /// Tiltback speed setting, when understood.
    pub tiltback: Option<SpeedReading>,

    /// Pedal mode setting, when understood.
    pub pedal_mode: Option<MobilePedalModeDto>,
}

/// Read-only pedal mode projection for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePedalModeDto {
    /// Unnormalized raw Veteran pedal mode value.
    pub raw_mode: Option<u16>,
}

/// Bounded read-only settings response for mobile UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingsReadbackDto {
    /// Whether the requested settings readback is available for display.
    pub availability: MobileReadbackAvailabilityDto,

    /// Product-shaped EUC garage settings projection.
    pub euc_garage: MobileEucGarageSettingsDto,

    /// Present settings entries.
    pub entries: Vec<MobileSettingsEntryDto>,
}

/// Last reported fault for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileFaultHistoryEntryDto {
    /// Protocol-specific fault code without proven semantic mapping.
    pub code: MobileFaultCodeDto,

    /// Source of the fault code.
    pub source: MobileValueSourceDto,

    /// Confidence in the fault-code interpretation.
    pub quality: MobileValueQualityDto,

    /// Verification state for the fault-code interpretation.
    pub verification: MobileVerificationStatusDto,
}

/// Protocol-specific fault code for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileFaultCodeDto {
    /// Raw protocol field/value pair for an unknown fault code.
    pub raw: MobileRawFieldValueDto,
}

/// Read-only last-fault history for mobile UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileFaultHistoryReadbackDto {
    /// Whether fault history is available for display.
    pub availability: MobileReadbackAvailabilityDto,

    /// Last reported fault, if any.
    pub last_fault: Option<MobileFaultHistoryEntryDto>,

    /// Distance since the last fault, if reported separately.
    pub since_distance: Option<DistanceReading>,
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
    #[allow(clippy::needless_pass_by_value)]
    pub fn add_advertised_service(&self, service: Vec<u8>) {
        self.advertised_services
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(mobile_gatt_channel(&service));
    }

    /// Adds an observed GATT service/characteristic fingerprint.
    pub fn add_gatt_fingerprint(&self, fingerprint: MobileGattFingerprintDto) {
        self.gatt_fingerprints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fingerprint.into());
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
    #[allow(clippy::needless_pass_by_value)]
    pub fn record_notification(
        &self,
        monotonic_ms: MobileMonotonicMillisDto,
        characteristic: Vec<u8>,
        service: Vec<u8>,
        bytes: Vec<u8>,
    ) {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(PevcapRecord::inbound_notification(
                monotonic_ms.into_core(),
                mobile_gatt_channel(&characteristic),
                mobile_gatt_channel(&service),
                bytes,
            ));
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

impl From<ValueSource> for MobileValueSourceDto {
    fn from(source: ValueSource) -> Self {
        match source {
            ValueSource::Reported => Self::Reported,
            ValueSource::Calculated => Self::Calculated,
            ValueSource::Estimated => Self::Estimated,
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

impl From<ValueQuality> for MobileValueQualityDto {
    fn from(quality: ValueQuality) -> Self {
        match quality {
            ValueQuality::Known => Self::Known,
            ValueQuality::Inferred => Self::Inferred,
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

impl From<VerificationStatus> for MobileVerificationStatusDto {
    fn from(status: VerificationStatus) -> Self {
        match status {
            VerificationStatus::Unverified => Self::Unverified,
            VerificationStatus::Inferred => Self::Inferred,
            VerificationStatus::SourceVerified => Self::SourceVerified,
            VerificationStatus::HardwareVerified => Self::HardwareVerified,
            VerificationStatus::SourceAndHardwareVerified => Self::SourceAndHardwareVerified,
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

impl From<RawFieldValue> for MobileRawFieldValueDto {
    fn from(field: RawFieldValue) -> Self {
        Self {
            id: field.id,
            value: field.value,
        }
    }
}

impl From<RawFieldValueDto> for MobileRawFieldValueDto {
    fn from(field: RawFieldValueDto) -> Self {
        Self {
            id: field.id,
            value: field.value,
        }
    }
}

impl From<SettingsEntry> for MobileSettingsEntryDto {
    fn from(entry: SettingsEntry) -> Self {
        Self {
            field: entry.field.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

impl From<SettingsEntryDto> for MobileSettingsEntryDto {
    fn from(entry: SettingsEntryDto) -> Self {
        Self {
            field: entry.field.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

impl MobileEucGarageSettingsDto {
    fn from_entries(
        availability: MobileReadbackAvailabilityDto,
        entries: &[MobileSettingsEntryDto],
    ) -> Self {
        if availability != MobileReadbackAvailabilityDto::Available {
            return Self {
                availability,
                beep_margin: None,
                tiltback: None,
                pedal_mode: None,
            };
        }

        Self {
            availability,
            beep_margin: settings_speed(
                entries,
                VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
                speed_from_deci_kmh,
            ),
            tiltback: settings_speed(
                entries,
                VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                speed_from_deci_kmh,
            )
            .or_else(|| settings_speed(entries, BEGODE_FIELD_TILTBACK_SPEED_KMH, speed_from_kmh)),
            pedal_mode: settings_entry(entries, VETERAN_FIELD_PEDALS_MODE)
                .and_then(|entry| u16::try_from(entry.field.value).ok())
                .map(|raw_mode| MobilePedalModeDto {
                    raw_mode: Some(raw_mode),
                }),
        }
    }
}

fn settings_entry(
    entries: &[MobileSettingsEntryDto],
    field_id: u16,
) -> Option<MobileSettingsEntryDto> {
    entries
        .iter()
        .copied()
        .find(|entry| entry.field.id == field_id)
}

fn settings_speed(
    entries: &[MobileSettingsEntryDto],
    field_id: u16,
    convert: fn(i64) -> Option<CoreSpeed>,
) -> Option<SpeedReading> {
    settings_entry(entries, field_id).and_then(|entry| {
        convert(entry.field.value).map(|speed| SpeedReading {
            value: Speed {
                value: speed.as_millimetres_per_second(),
            },
            source: entry.source,
            quality: entry.quality,
            verification: entry.verification,
        })
    })
}

fn speed_from_deci_kmh(value: i64) -> Option<CoreSpeed> {
    speed_from_milli_kmh(value.checked_mul(100)?)
}

fn speed_from_kmh(value: i64) -> Option<CoreSpeed> {
    speed_from_milli_kmh(value.checked_mul(1_000)?)
}

fn speed_from_milli_kmh(value: i64) -> Option<CoreSpeed> {
    (value >= 0)
        .then_some(value)?
        .checked_mul(5)?
        .checked_div(18)?
        .try_into()
        .ok()
        .map(CoreSpeed::from_millimetres_per_second)
}

impl From<SettingsReadback> for MobileSettingsReadbackDto {
    fn from(readback: SettingsReadback) -> Self {
        let availability = readback.availability().into();
        let entries: Vec<_> = readback
            .entries()
            .into_iter()
            .flatten()
            .map(Into::into)
            .collect();
        let euc_garage = MobileEucGarageSettingsDto::from_entries(availability, &entries);

        Self {
            availability,
            euc_garage,
            entries,
        }
    }
}

impl From<SettingsReadbackDto> for MobileSettingsReadbackDto {
    fn from(readback: SettingsReadbackDto) -> Self {
        let availability = readback.availability.into();
        let entries: Vec<_> = (availability == MobileReadbackAvailabilityDto::Available)
            .then_some(readback.entries)
            .into_iter()
            .flatten()
            .map(Into::into)
            .collect();
        let euc_garage = MobileEucGarageSettingsDto::from_entries(availability, &entries);

        Self {
            availability,
            euc_garage,
            entries,
        }
    }
}

impl From<SettingsReadbackAvailability> for MobileReadbackAvailabilityDto {
    fn from(availability: SettingsReadbackAvailability) -> Self {
        match availability {
            SettingsReadbackAvailability::Available => Self::Available,
            SettingsReadbackAvailability::Unavailable => Self::Unavailable,
            SettingsReadbackAvailability::Unsupported => Self::Unsupported,
        }
    }
}

impl From<SettingsReadbackAvailabilityDto> for MobileReadbackAvailabilityDto {
    fn from(availability: SettingsReadbackAvailabilityDto) -> Self {
        match availability {
            SettingsReadbackAvailabilityDto::Available => Self::Available,
            SettingsReadbackAvailabilityDto::Unavailable => Self::Unavailable,
            SettingsReadbackAvailabilityDto::Unsupported => Self::Unsupported,
        }
    }
}

impl From<FaultHistoryAvailability> for MobileReadbackAvailabilityDto {
    fn from(availability: FaultHistoryAvailability) -> Self {
        match availability {
            FaultHistoryAvailability::Available => Self::Available,
            FaultHistoryAvailability::Unavailable => Self::Unavailable,
            FaultHistoryAvailability::Unsupported => Self::Unsupported,
        }
    }
}

impl From<FaultHistoryAvailabilityDto> for MobileReadbackAvailabilityDto {
    fn from(availability: FaultHistoryAvailabilityDto) -> Self {
        match availability {
            FaultHistoryAvailabilityDto::Available => Self::Available,
            FaultHistoryAvailabilityDto::Unavailable => Self::Unavailable,
            FaultHistoryAvailabilityDto::Unsupported => Self::Unsupported,
        }
    }
}

impl From<BatteryReadbackAvailabilityDto> for MobileReadbackAvailabilityDto {
    fn from(availability: BatteryReadbackAvailabilityDto) -> Self {
        match availability {
            BatteryReadbackAvailabilityDto::Available => Self::Available,
            BatteryReadbackAvailabilityDto::Unavailable => Self::Unavailable,
            BatteryReadbackAvailabilityDto::Unsupported => Self::Unsupported,
        }
    }
}

impl From<FaultHistoryEntry> for MobileFaultHistoryEntryDto {
    fn from(entry: FaultHistoryEntry) -> Self {
        Self {
            code: entry.code.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

impl From<FaultHistoryEntryDto> for MobileFaultHistoryEntryDto {
    fn from(entry: FaultHistoryEntryDto) -> Self {
        Self {
            code: entry.code.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

impl From<FaultCodeDto> for MobileFaultCodeDto {
    fn from(code: FaultCodeDto) -> Self {
        Self {
            raw: code.raw.into(),
        }
    }
}

impl From<FaultCode> for MobileFaultCodeDto {
    fn from(code: FaultCode) -> Self {
        Self {
            raw: code.raw.into(),
        }
    }
}

impl From<FaultHistoryReadback> for MobileFaultHistoryReadbackDto {
    fn from(readback: FaultHistoryReadback) -> Self {
        Self {
            availability: readback.availability().into(),
            last_fault: readback.last_fault().map(Into::into),
            since_distance: readback
                .since_distance()
                .map(MeasuredU64Dto::from)
                .map(Into::into),
        }
    }
}

impl From<FaultHistoryReadbackDto> for MobileFaultHistoryReadbackDto {
    fn from(readback: FaultHistoryReadbackDto) -> Self {
        let availability = readback.availability.into();
        let (last_fault, since_distance) =
            if availability == MobileReadbackAvailabilityDto::Available {
                (
                    readback.last_fault.map(Into::into),
                    readback.since_distance.map(Into::into),
                )
            } else {
                (None, None)
            };

        Self {
            availability,
            last_fault,
            since_distance,
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

impl From<MobileGattFingerprintDto> for GattFingerprint {
    fn from(fingerprint: MobileGattFingerprintDto) -> Self {
        Self {
            service: mobile_gatt_channel(&fingerprint.service),
            characteristic: mobile_gatt_channel(&fingerprint.characteristic),
            roles: mobile_gatt_roles(fingerprint.roles),
            verification: fingerprint.verification.into(),
        }
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

fn mobile_gatt_channel(channel: &[u8]) -> GattChannel {
    GattChannel::from_bytes(mobile_channel_bytes(channel))
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
        let input = SessionInputDto::from(input);
        mobile_aero_session_step_result(self.lock_inner().ingest_checked(&input))
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    pub fn drain_outputs(&self) -> Vec<MobileSessionOutputDto> {
        self.lock_inner()
            .drain_outputs()
            .into_iter()
            .map(mobile_aero_session_output)
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

impl From<MobileSessionInputDto> for SessionInputDto {
    fn from(input: MobileSessionInputDto) -> Self {
        match input.kind {
            MobileSessionInputKindDto::LinkUp => Self::LinkUp {
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
                max_write_len: input
                    .max_write_len
                    .map(MobileTransportWriteLimitDto::into_core_ffi),
            },
            MobileSessionInputKindDto::LinkDown => Self::LinkDown,
            MobileSessionInputKindDto::Notification => Self::Notification {
                channel: mobile_channel_bytes(&input.channel),
                bytes: input.bytes,
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
            },
            MobileSessionInputKindDto::Tick => Self::Tick {
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
            },
            MobileSessionInputKindDto::Command => Self::Command(
                input
                    .command
                    .unwrap_or(MobileCommandDto::RequestTelemetry)
                    .into(),
            ),
        }
    }
}

fn mobile_channel_bytes(channel: &[u8]) -> [u8; 16] {
    let mut bytes = [0; 16];
    let len = channel.len().min(bytes.len());
    bytes[..len].copy_from_slice(&channel[..len]);
    bytes
}

impl From<MobileCommandDto> for DeviceCommandDto {
    fn from(command: MobileCommandDto) -> Self {
        match command {
            MobileCommandDto::RequestIdentity => Self::RequestIdentity,
            MobileCommandDto::RequestTelemetry => Self::RequestTelemetry,
            MobileCommandDto::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            MobileCommandDto::RequestBatteryInfo => Self::RequestBatteryInfo,
            MobileCommandDto::RequestDiagnostics => Self::RequestDiagnostics,
            MobileCommandDto::RequestFaultHistory => Self::RequestFaultHistory,
            MobileCommandDto::RequestSettings => Self::RequestSettings,
            MobileCommandDto::SoundHorn => Self::SoundHorn,
        }
    }
}

fn mobile_command_from_command_kind(command: CommandKindDto) -> Option<MobileCommandDto> {
    match command {
        CommandKindDto::RequestIdentity => Some(MobileCommandDto::RequestIdentity),
        CommandKindDto::RequestTelemetry => Some(MobileCommandDto::RequestTelemetry),
        CommandKindDto::RequestFirmwareInfo => Some(MobileCommandDto::RequestFirmwareInfo),
        CommandKindDto::RequestBatteryInfo => Some(MobileCommandDto::RequestBatteryInfo),
        CommandKindDto::RequestDiagnostics => Some(MobileCommandDto::RequestDiagnostics),
        CommandKindDto::RequestFaultHistory => Some(MobileCommandDto::RequestFaultHistory),
        CommandKindDto::RequestSettings => Some(MobileCommandDto::RequestSettings),
        CommandKindDto::SoundHorn => Some(MobileCommandDto::SoundHorn),
        CommandKindDto::SetLights | CommandKindDto::SetRawMotorCurrent => None,
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
                settings_readback: None,
                fault_history_readback: None,
                bms_snapshot: None,
                veteran_protocol_model_id: None,
            },
            SessionOutputDto::Transport(TransportActionDto::Write { channel, bytes, .. }) => Self {
                kind: MobileSessionOutputKindDto::Write,
                channel: channel.to_vec(),
                bytes,
                ingest: None,
                settings_readback: None,
                fault_history_readback: None,
                bms_snapshot: None,
                veteran_protocol_model_id: None,
            },
            SessionOutputDto::Transport(TransportActionDto::Disconnect) => Self {
                kind: MobileSessionOutputKindDto::Disconnect,
                channel: Vec::new(),
                bytes: Vec::new(),
                ingest: None,
                settings_readback: None,
                fault_history_readback: None,
                bms_snapshot: None,
                veteran_protocol_model_id: None,
            },
            SessionOutputDto::ReadOnly(response) => match response.payload {
                ReadOnlyOutputPayload::Settings(settings) => Self {
                    kind: MobileSessionOutputKindDto::SettingsReadback,
                    channel: Vec::new(),
                    bytes: Vec::new(),
                    ingest: None,
                    settings_readback: Some(settings.into()),
                    fault_history_readback: None,
                    bms_snapshot: None,
                    veteran_protocol_model_id: None,
                },
                ReadOnlyOutputPayload::FaultHistory(fault_history) => Self {
                    kind: MobileSessionOutputKindDto::FaultHistoryReadback,
                    channel: Vec::new(),
                    bytes: Vec::new(),
                    ingest: None,
                    settings_readback: None,
                    fault_history_readback: Some(fault_history.into()),
                    bms_snapshot: None,
                    veteran_protocol_model_id: None,
                },
                ReadOnlyOutputPayload::Battery(battery) => Self {
                    kind: MobileSessionOutputKindDto::BmsSnapshot,
                    channel: Vec::new(),
                    bytes: Vec::new(),
                    ingest: None,
                    settings_readback: None,
                    fault_history_readback: None,
                    bms_snapshot: Some(battery.into()),
                    veteran_protocol_model_id: None,
                },
                ReadOnlyOutputPayload::Firmware(_)
                | ReadOnlyOutputPayload::Diagnostics(_)
                | ReadOnlyOutputPayload::RawTelemetry(_) => Self {
                    kind: MobileSessionOutputKindDto::Event,
                    channel: Vec::new(),
                    bytes: Vec::new(),
                    ingest: None,
                    settings_readback: None,
                    fault_history_readback: None,
                    bms_snapshot: None,
                    veteran_protocol_model_id: None,
                },
            },
            SessionOutputDto::Event(_) => Self {
                kind: MobileSessionOutputKindDto::Event,
                channel: Vec::new(),
                bytes: Vec::new(),
                ingest: None,
                settings_readback: None,
                fault_history_readback: None,
                bms_snapshot: None,
                veteran_protocol_model_id: None,
            },
            SessionOutputDto::NotificationIngest(outcome) => Self {
                kind: MobileSessionOutputKindDto::NotificationIngest,
                channel: Vec::new(),
                bytes: Vec::new(),
                ingest: Some(outcome.into()),
                settings_readback: None,
                fault_history_readback: None,
                bms_snapshot: None,
                veteran_protocol_model_id: None,
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
                notification: Some(notification.into()),
                event_count: Some(event_count.into()),
                parser_error: None,
                reserved: None,
                gap: None,
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::BufferedFragment(notification) => Self {
                kind: MobileNotificationIngestOutcomeKindDto::BufferedFragment,
                notification: Some(notification.into()),
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: None,
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::ParserDiagnostic {
                notification,
                error,
            } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::ParserDiagnostic,
                notification: Some(notification.into()),
                event_count: None,
                parser_error: Some(error.into()),
                reserved: None,
                gap: None,
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::KnownReserved {
                notification,
                payload,
            } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::KnownReserved,
                notification: Some(notification.into()),
                event_count: None,
                parser_error: None,
                reserved: Some(payload.into()),
                gap: None,
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::ParserGap { notification, gap } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::ParserGap,
                notification: Some(notification.into()),
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: Some(gap.into()),
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::Ignored { evidence, reason } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::Ignored,
                notification: None,
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: None,
                ignored: Some(evidence.into()),
                ignored_reason: Some(reason.into()),
            },
        }
    }
}

impl From<MeasuredI16Dto> for DutyCycle {
    fn from(measured: MeasuredI16Dto) -> Self {
        Self {
            permille: measured.value,
        }
    }
}

macro_rules! mobile_quantity_from_measured {
    ($measured:ty, $quantity:ident, $reading:ident) => {
        impl From<$measured> for $reading {
            fn from(measured: $measured) -> Self {
                Self {
                    value: $quantity {
                        value: measured.value,
                    },
                    source: measured.source.into(),
                    quality: measured.quality.into(),
                    verification: measured.verification.into(),
                }
            }
        }
    };
}

mobile_quantity_from_measured!(MeasuredI32Dto, Speed, SpeedReading);
mobile_quantity_from_measured!(MeasuredI32Dto, Voltage, VoltageReading);
mobile_quantity_from_measured!(MeasuredI32Dto, BatteryCurrent, BatteryCurrentReading);
mobile_quantity_from_measured!(MeasuredI32Dto, PhaseCurrent, PhaseCurrentReading);
mobile_quantity_from_measured!(MeasuredI64Dto, Power, PowerReading);
mobile_quantity_from_measured!(MeasuredI32Dto, Temperature, TemperatureReading);
mobile_quantity_from_measured!(MeasuredU64Dto, Distance, DistanceReading);
mobile_quantity_from_measured!(MeasuredI32Dto, Angle, AngleReading);
mobile_quantity_from_measured!(MeasuredU8Dto, BatteryLevel, BatteryLevelReading);
mobile_quantity_from_measured!(MeasuredI32Dto, VoltageDelta, VoltageDeltaReading);

fn highest_battery_temperature(
    temperature: Option<MeasuredI32Dto>,
    temperatures: Vec<Option<MeasuredI32Dto>>,
) -> Option<TemperatureReading> {
    temperature
        .into_iter()
        .chain(temperatures.into_iter().flatten())
        .max_by_key(|reading| reading.value)
        .map(Into::into)
}

fn power_flow_from_signed_current(current: MeasuredI32Dto) -> PowerFlowDirection {
    match current.value.cmp(&0) {
        std::cmp::Ordering::Greater => PowerFlowDirection::Discharge,
        std::cmp::Ordering::Equal => PowerFlowDirection::Zero,
        std::cmp::Ordering::Less => PowerFlowDirection::NegativeUnknown,
    }
}

fn ride_operating_state(
    charge_mode: Option<MeasuredChargeModeDto>,
    speed: Option<MeasuredI32Dto>,
) -> RideOperatingState {
    match charge_mode.map(|mode| mode.value) {
        Some(ChargeModeDto::Charging) => RideOperatingState::Charging,
        Some(ChargeModeDto::NotCharging) | None => match speed.map(|speed| speed.value.cmp(&0)) {
            Some(std::cmp::Ordering::Equal) => RideOperatingState::Parked,
            Some(_) => RideOperatingState::Riding,
            None => RideOperatingState::Unknown,
        },
    }
}

impl From<NotificationEvidenceDto> for MobileNotificationEvidenceDto {
    fn from(evidence: NotificationEvidenceDto) -> Self {
        Self {
            family: evidence.family.into(),
            channel: evidence.channel.to_vec(),
            len: evidence.len.into(),
            monotonic_ms: MobileMonotonicMillisDto::from_core_ffi_timestamp(evidence.monotonic_ms),
        }
    }
}

impl From<IgnoredNotificationEvidenceDto> for MobileIgnoredNotificationEvidenceDto {
    fn from(evidence: IgnoredNotificationEvidenceDto) -> Self {
        Self {
            family: evidence.family.map(Into::into),
            channel: evidence.channel.to_vec(),
            len: evidence.len.into(),
            monotonic_ms: MobileMonotonicMillisDto::from_core_ffi_timestamp(evidence.monotonic_ms),
            retained_payload: evidence.retained_payload,
        }
    }
}

impl From<IgnoredNotificationReasonDto> for MobileIgnoredNotificationReasonDto {
    fn from(reason: IgnoredNotificationReasonDto) -> Self {
        match reason {
            IgnoredNotificationReasonDto::WrongChannel => Self::WrongChannel,
            IgnoredNotificationReasonDto::UnsupportedFamily => Self::UnsupportedFamily,
            IgnoredNotificationReasonDto::UnsupportedChannel => Self::UnsupportedChannel,
            IgnoredNotificationReasonDto::AcceptedButUnmapped => Self::AcceptedButUnmapped,
            IgnoredNotificationReasonDto::SeekingFrameBoundary => Self::SeekingFrameBoundary,
            IgnoredNotificationReasonDto::IntentionallyDropped => Self::IntentionallyDropped,
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
            retained_payload: evidence.retained_payload,
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
            retained_payload: evidence.retained_payload,
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

fn mobile_aero_session_step_result(
    result: ConcreteSessionStepResultDto,
) -> MobileSessionStepResultDto {
    MobileSessionStepResultDto {
        outputs: result
            .outputs
            .into_iter()
            .map(mobile_aero_session_output)
            .collect(),
        error: result.error.map(Into::into),
    }
}

fn mobile_aero_session_output(output: SessionOutputDto) -> MobileSessionOutputDto {
    let veteran_protocol_model_id = match &output {
        SessionOutputDto::ReadOnly(response) => match &response.payload {
            ReadOnlyOutputPayload::Firmware(firmware) => {
                firmware.firmware_major.map(|major| major.value)
            }
            _ => None,
        },
        _ => None,
    };

    MobileSessionOutputDto {
        veteran_protocol_model_id,
        ..MobileSessionOutputDto::from(output)
    }
}

impl From<ConcreteSessionErrorDto> for MobileSessionStepErrorDto {
    fn from(error: ConcreteSessionErrorDto) -> Self {
        match error {
            ConcreteSessionErrorDto::CommandRefused { refusal } => Self {
                kind: MobileSessionStepErrorKindDto::CommandRefused,
                command: mobile_command_from_command_kind(refusal.command),
                reason: Some(control_refusal_reason_text(refusal.reason).to_owned()),
            },
            ConcreteSessionErrorDto::UnsupportedFalconProfile { .. } => Self {
                kind: MobileSessionStepErrorKindDto::UnsupportedFalconProfile,
                command: None,
                reason: None,
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
            operating_state: ride_operating_state(snapshot.charge_mode, snapshot.speed),
            voltage: snapshot.voltage.map(Into::into),
            battery_current: snapshot.battery_current.map(Into::into),
            motor_current: snapshot.motor_current.map(Into::into),
            power: snapshot.power.map(Into::into),
            power_flow: snapshot.battery_current.map(power_flow_from_signed_current),
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
            | ConcreteSessionErrorDto::UnsupportedFalconProfile { .. } => {
                Self::UnsupportedFalconProfile
            }
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
        let input = SessionInputDto::from(input);
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
    fn mobile_discovery_candidate_routes_veteran_protocol_model_id_to_aero() {
        let candidate = mobile_discovery_candidate_from_veteran_protocol_identity(
            "ios-local-aero".to_owned(),
            "NF2557".to_owned(),
            43,
        );

        assert_eq!(candidate.platform_identifier, "ios-local-aero");
        assert_eq!(candidate.display_name, "NF2557");
        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(candidate.evidence, "Veteran protocol model id");
        assert_eq!(candidate.detail, "NOSFET Aero confirmed by model id 43");
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
    fn mobile_discovery_candidate_keeps_unknown_veteran_model_id_unrouteable() {
        let candidate = mobile_discovery_candidate_from_veteran_protocol_identity(
            "ios-local-veteran".to_owned(),
            "Veteran stream".to_owned(),
            99,
        );

        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(candidate.evidence, "Veteran protocol model id");
        assert_eq!(candidate.detail, "Unknown Veteran/NOSFET model id 99");
        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            MobileDiscoveryCandidateSupportDto::Unsupported
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason.as_deref(),
            Some("Model not supported")
        );
    }

    fn bms_snapshot_fixture() -> MobileBmsSnapshotDto {
        MobileBmsSnapshotDto {
            availability: MobileReadbackAvailabilityDto::Available,
            topology: MobileBmsTopologyDto {
                layout_label: "20S4P split pack".to_owned(),
                series_group_count: Some(20),
                parallel_count: Some(4),
                pack_count: 2,
                bms_count: 2,
                confidence: MobileBmsTopologyConfidenceDto::Verified,
            },
            energy_percent: Some(BatteryLevelReading {
                value: BatteryLevel { value: 72 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            voltage: Some(VoltageReading {
                value: Voltage { value: 81_600 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            current: None,
            cell_delta: Some(VoltageDeltaReading {
                value: VoltageDelta { value: 18 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            lowest_group_index: Some(17),
            highest_temperature: Some(TemperatureReading {
                value: Temperature { value: 37_800 },
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
                voltage: Some(VoltageReading {
                    value: Voltage { value: 4_071 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                temperature: Some(TemperatureReading {
                    value: Temperature { value: 34_900 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                resistance: Some(Resistance { value: 21 }),
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
        }
    }

    #[test]
    fn mobile_bms_snapshot_dto_preserves_topology_and_group_detail() {
        let snapshot = bms_snapshot_fixture();

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
                .and_then(|group| group.resistance)
                .map(|resistance| resistance.value),
            Some(21)
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
    fn mobile_settings_readback_preserves_present_fields_and_metadata() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(0x0102, -17),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::HardwareVerified,
            }),
            None,
            Some(SettingsEntry {
                field: RawFieldValue::new(0x0203, 42),
                source: ValueSource::Estimated,
                quality: ValueQuality::Inferred,
                verification: VerificationStatus::Inferred,
            }),
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.availability,
            MobileReadbackAvailabilityDto::Available
        );
        assert_eq!(
            mobile.entries,
            vec![
                MobileSettingsEntryDto {
                    field: MobileRawFieldValueDto {
                        id: 0x0102,
                        value: -17,
                    },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                },
                MobileSettingsEntryDto {
                    field: MobileRawFieldValueDto {
                        id: 0x0203,
                        value: 42,
                    },
                    source: MobileValueSourceDto::Estimated,
                    quality: MobileValueQualityDto::Inferred,
                    verification: MobileVerificationStatusDto::Inferred,
                },
            ]
        );
        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Available,
                beep_margin: None,
                tiltback: None,
                pedal_mode: None,
            }
        );
    }

    #[test]
    fn mobile_settings_readback_projects_known_euc_garage_settings() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_SPEED_ALERT_DECI_KMH, 116),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceAndHardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, 420),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::HardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_PEDALS_MODE, 1_920),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Available,
                beep_margin: Some(SpeedReading {
                    value: Speed { value: 3_222 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::SourceAndHardwareVerified,
                }),
                tiltback: Some(SpeedReading {
                    value: Speed { value: 11_666 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                pedal_mode: Some(MobilePedalModeDto {
                    raw_mode: Some(1_920),
                }),
            }
        );
    }

    #[test]
    fn mobile_settings_readback_projects_begode_tiltback_fallback() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_TILTBACK_SPEED_KMH, 50),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
            None,
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Available,
                beep_margin: None,
                tiltback: Some(SpeedReading {
                    value: Speed { value: 13_888 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::SourceVerified,
                }),
                pedal_mode: None,
            }
        );
    }

    #[test]
    fn mobile_settings_readback_rejects_unrepresentable_euc_garage_speeds() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_SPEED_ALERT_DECI_KMH, -1),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceAndHardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, i64::MAX),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::HardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_TILTBACK_SPEED_KMH, i64::MAX),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Available,
                beep_margin: None,
                tiltback: None,
                pedal_mode: None,
            }
        );
        assert_eq!(mobile.entries.len(), 3);
    }

    #[test]
    fn mobile_settings_readback_preserves_unsupported_availability() {
        let mobile = MobileSettingsReadbackDto::from(SettingsReadback::unsupported());

        assert_eq!(
            mobile,
            MobileSettingsReadbackDto {
                availability: MobileReadbackAvailabilityDto::Unsupported,
                euc_garage: MobileEucGarageSettingsDto {
                    availability: MobileReadbackAvailabilityDto::Unsupported,
                    beep_margin: None,
                    tiltback: None,
                    pedal_mode: None,
                },
                entries: Vec::new(),
            }
        );
    }

    #[test]
    fn mobile_settings_readback_dto_strips_entries_when_not_available() {
        let mobile = MobileSettingsReadbackDto::from(SettingsReadbackDto {
            availability: SettingsReadbackAvailabilityDto::Unsupported,
            entries: vec![SettingsEntryDto {
                field: RawFieldValueDto {
                    id: VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                    value: 420,
                },
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }],
        });

        assert_eq!(
            mobile.availability,
            MobileReadbackAvailabilityDto::Unsupported
        );
        assert!(mobile.entries.is_empty());
        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Unsupported,
                beep_margin: None,
                tiltback: None,
                pedal_mode: None,
            }
        );
    }

    #[test]
    fn mobile_fault_history_preserves_unknown_fault_code_and_since_distance() {
        let mobile = MobileFaultHistoryReadbackDto::from(FaultHistoryReadbackDto {
            availability: FaultHistoryAvailabilityDto::Available,
            last_fault: Some(FaultHistoryEntryDto {
                code: FaultCodeDto {
                    raw: RawFieldValueDto {
                        id: 0x0040,
                        value: 1,
                    },
                },
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }),
            since_distance: Some(MeasuredU64Dto {
                value: 61_456_941,
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }),
        });

        assert_eq!(
            mobile.availability,
            MobileReadbackAvailabilityDto::Available
        );
        assert_eq!(
            mobile.last_fault.expect("fault"),
            MobileFaultHistoryEntryDto {
                code: MobileFaultCodeDto {
                    raw: MobileRawFieldValueDto {
                        id: 0x0040,
                        value: 1,
                    },
                },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }
        );
        assert_eq!(
            mobile.since_distance.expect("distance").value,
            Distance { value: 61_456_941 }
        );
    }

    #[test]
    fn mobile_fault_history_dto_strips_payload_when_not_available() {
        let mobile = MobileFaultHistoryReadbackDto::from(FaultHistoryReadbackDto {
            availability: FaultHistoryAvailabilityDto::Unavailable,
            last_fault: Some(FaultHistoryEntryDto {
                code: FaultCodeDto {
                    raw: RawFieldValueDto {
                        id: 0x0040,
                        value: 1,
                    },
                },
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }),
            since_distance: Some(MeasuredU64Dto {
                value: 61_456_941,
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }),
        });

        assert_eq!(
            mobile.availability,
            MobileReadbackAvailabilityDto::Unavailable
        );
        assert_eq!(mobile.last_fault, None);
        assert_eq!(mobile.since_distance, None);
    }

    #[test]
    fn generic_firmware_output_does_not_invent_veteran_protocol_model_id() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestFirmwareInfo,
            payload: ReadOnlyOutputPayload::Firmware(cutout_core::FirmwareInfoDto {
                protocol_version: None,
                firmware_major: Some(cutout_core::MeasuredU16Dto {
                    value: 43,
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                firmware_minor: None,
                firmware_patch: None,
                build_id: None,
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::Event);
        assert_eq!(mobile.veteran_protocol_model_id, None);
    }

    #[test]
    fn aero_firmware_output_carries_veteran_protocol_model_id() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestFirmwareInfo,
            payload: ReadOnlyOutputPayload::Firmware(cutout_core::FirmwareInfoDto {
                protocol_version: None,
                firmware_major: Some(cutout_core::MeasuredU16Dto {
                    value: 43,
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                firmware_minor: None,
                firmware_patch: None,
                build_id: None,
            }),
        });

        let mobile = mobile_aero_session_output(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::Event);
        assert_eq!(mobile.veteran_protocol_model_id, Some(43));
    }

    #[test]
    fn mobile_session_output_preserves_settings_readback_event() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestSettings,
            payload: ReadOnlyOutputPayload::Settings(SettingsReadbackDto {
                availability: SettingsReadbackAvailabilityDto::Available,
                entries: vec![SettingsEntryDto {
                    field: RawFieldValueDto {
                        id: 0x0102,
                        value: -17,
                    },
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }],
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::SettingsReadback);
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        assert_eq!(mobile.ingest, None);
        assert_eq!(
            mobile.settings_readback,
            Some(MobileSettingsReadbackDto {
                availability: MobileReadbackAvailabilityDto::Available,
                euc_garage: MobileEucGarageSettingsDto {
                    availability: MobileReadbackAvailabilityDto::Available,
                    beep_margin: None,
                    tiltback: None,
                    pedal_mode: None,
                },
                entries: vec![MobileSettingsEntryDto {
                    field: MobileRawFieldValueDto {
                        id: 0x0102,
                        value: -17,
                    },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }],
            })
        );
        assert_eq!(mobile.fault_history_readback, None);
        assert_eq!(mobile.bms_snapshot, None);
    }

    #[test]
    fn mobile_session_output_preserves_fault_history_readback_event() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestFaultHistory,
            payload: ReadOnlyOutputPayload::FaultHistory(FaultHistoryReadbackDto {
                availability: FaultHistoryAvailabilityDto::Available,
                last_fault: Some(FaultHistoryEntryDto {
                    code: FaultCodeDto {
                        raw: RawFieldValueDto {
                            id: 0x0040,
                            value: 1,
                        },
                    },
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                since_distance: None,
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(
            mobile.kind,
            MobileSessionOutputKindDto::FaultHistoryReadback
        );
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        assert_eq!(mobile.ingest, None);
        assert_eq!(mobile.settings_readback, None);
        assert_eq!(mobile.bms_snapshot, None);
        assert_eq!(
            mobile.fault_history_readback,
            Some(MobileFaultHistoryReadbackDto {
                availability: MobileReadbackAvailabilityDto::Available,
                last_fault: Some(MobileFaultHistoryEntryDto {
                    code: MobileFaultCodeDto {
                        raw: MobileRawFieldValueDto {
                            id: 0x0040,
                            value: 1,
                        },
                    },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                since_distance: None,
            })
        );
    }

    #[test]
    fn mobile_command_mapping_keeps_readback_commands_distinct() {
        assert_eq!(
            mobile_command_from_command_kind(CommandKindDto::RequestDiagnostics),
            Some(MobileCommandDto::RequestDiagnostics)
        );
        assert_eq!(
            mobile_command_from_command_kind(CommandKindDto::RequestFaultHistory),
            Some(MobileCommandDto::RequestFaultHistory)
        );
        assert_eq!(
            mobile_command_from_command_kind(CommandKindDto::RequestSettings),
            Some(MobileCommandDto::RequestSettings)
        );
        assert_eq!(
            mobile_command_from_command_kind(CommandKindDto::SetRawMotorCurrent),
            None
        );
    }

    #[test]
    fn mobile_session_output_maps_battery_readback_to_bms_snapshot() {
        let reported = |value| MeasuredI32Dto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::HardwareVerified,
        };
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestBatteryInfo,
            payload: ReadOnlyOutputPayload::Battery(BatteryReadbackDto {
                availability: BatteryReadbackAvailabilityDto::Available,
                page: Some(BatteryInfoDto {
                    page: cutout_core::BatteryPageMetadataDto {
                        selector: 3,
                        kind: cutout_core::BatteryPageKindDto::Temperature,
                        verification: VerificationStatusDto::HardwareVerified,
                    },
                    voltage: Some(reported(81_600)),
                    current: Some(reported(-1_250)),
                    bms_pack_current_0: None,
                    bms_pack_current_1: None,
                    level_reported: Some(MeasuredU8Dto {
                        value: 72,
                        source: ValueSourceDto::Reported,
                        quality: ValueQualityDto::Known,
                        verification: VerificationStatusDto::HardwareVerified,
                    }),
                    level_estimated: None,
                    temperature: Some(reported(31_000)),
                    temperatures: vec![None, Some(reported(37_800)), Some(reported(35_200))],
                    cell_voltages: vec![reported(3_633), reported(3_626), reported(3_634)],
                    raw_state: None,
                }),
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::BmsSnapshot);
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        assert_eq!(mobile.ingest, None);
        assert_eq!(mobile.settings_readback, None);
        assert_eq!(mobile.fault_history_readback, None);
        let snapshot = mobile.bms_snapshot.expect("BMS snapshot");
        assert_eq!(
            snapshot.availability,
            MobileReadbackAvailabilityDto::Available
        );
        assert_eq!(snapshot.topology.layout_label, "unknown BMS topology");
        assert_eq!(
            snapshot.topology.confidence,
            MobileBmsTopologyConfidenceDto::Unverified
        );
        assert_eq!(snapshot.groups.len(), 3);
        assert_eq!(snapshot.groups[0].index, 1);
        assert_eq!(snapshot.groups[0].label.as_deref(), Some("group 1"));
        assert_eq!(
            snapshot.groups[0]
                .voltage
                .as_ref()
                .map(|voltage| voltage.value),
            Some(Voltage { value: 3_633 })
        );
        assert_eq!(snapshot.lowest_group_index, Some(2));
        assert_eq!(
            snapshot.cell_delta.expect("cell delta").value,
            VoltageDelta { value: 8 }
        );
        assert_eq!(
            snapshot.energy_percent.expect("reported level").value,
            BatteryLevel { value: 72 }
        );
        assert_eq!(
            snapshot.voltage.expect("pack voltage").value,
            Voltage { value: 81_600 }
        );
        assert_eq!(
            snapshot.current.expect("pack current").value,
            BatteryCurrent { value: -1_250 }
        );
        assert_eq!(
            snapshot
                .highest_temperature
                .expect("highest temperature")
                .value,
            Temperature { value: 37_800 }
        );
    }

    #[test]
    fn mobile_session_output_preserves_unsupported_battery_readback() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestBatteryInfo,
            payload: ReadOnlyOutputPayload::Battery(BatteryReadbackDto {
                availability: BatteryReadbackAvailabilityDto::Unsupported,
                page: None,
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::BmsSnapshot);
        let snapshot = mobile.bms_snapshot.expect("BMS snapshot");
        assert_eq!(
            snapshot.availability,
            MobileReadbackAvailabilityDto::Unsupported
        );
        assert_eq!(snapshot.energy_percent, None);
        assert_eq!(snapshot.voltage, None);
        assert_eq!(snapshot.current, None);
        assert!(snapshot.groups.is_empty());
    }

    #[test]
    fn mobile_battery_readback_dto_strips_page_when_not_available() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestBatteryInfo,
            payload: ReadOnlyOutputPayload::Battery(BatteryReadbackDto {
                availability: BatteryReadbackAvailabilityDto::Unsupported,
                page: Some(BatteryInfoDto {
                    page: cutout_core::BatteryPageMetadataDto {
                        selector: 3,
                        kind: cutout_core::BatteryPageKindDto::Temperature,
                        verification: VerificationStatusDto::HardwareVerified,
                    },
                    voltage: Some(measured_i32(81_600)),
                    current: Some(measured_i32(-1_250)),
                    bms_pack_current_0: None,
                    bms_pack_current_1: None,
                    level_reported: Some(MeasuredU8Dto {
                        value: 72,
                        source: ValueSourceDto::Reported,
                        quality: ValueQualityDto::Known,
                        verification: VerificationStatusDto::HardwareVerified,
                    }),
                    level_estimated: None,
                    temperature: Some(measured_i32(31_000)),
                    temperatures: vec![Some(measured_i32(37_800))],
                    cell_voltages: vec![measured_i32(3_633)],
                    raw_state: None,
                }),
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::BmsSnapshot);
        let snapshot = mobile.bms_snapshot.expect("BMS snapshot");
        assert_eq!(
            snapshot.availability,
            MobileReadbackAvailabilityDto::Unsupported
        );
        assert_eq!(snapshot.energy_percent, None);
        assert_eq!(snapshot.voltage, None);
        assert_eq!(snapshot.current, None);
        assert_eq!(snapshot.cell_delta, None);
        assert_eq!(snapshot.highest_temperature, None);
        assert!(snapshot.groups.is_empty());
    }

    #[test]
    fn bms_group_projection_skips_unrepresentable_group_indices() {
        let mut cell_voltages = vec![measured_i32(3_600); usize::from(u16::MAX) + 1];
        cell_voltages[usize::from(u16::MAX)] = measured_i32(3_500);

        let groups = bms_groups_from_cell_voltages(&cell_voltages);

        assert_eq!(groups.len(), usize::from(u16::MAX));
        assert_eq!(groups.last().map(|group| group.index), Some(u16::MAX));
        assert_eq!(lowest_cell_voltage_group_index(&cell_voltages), None);
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

    #[test]
    fn power_flow_direction_does_not_invent_charge_or_regen_from_negative_current() {
        assert_eq!(
            power_flow_from_signed_current(measured_i32(2_000)),
            PowerFlowDirection::Discharge
        );
        assert_eq!(
            power_flow_from_signed_current(measured_i32(0)),
            PowerFlowDirection::Zero
        );
        assert_eq!(
            power_flow_from_signed_current(measured_i32(-2_000)),
            PowerFlowDirection::NegativeUnknown
        );
    }

    #[test]
    fn ride_operating_state_uses_charge_mode_before_speed() {
        assert_eq!(
            ride_operating_state(None, None),
            RideOperatingState::Unknown
        );
        assert_eq!(
            ride_operating_state(None, Some(measured_i32(0))),
            RideOperatingState::Parked
        );
        assert_eq!(
            ride_operating_state(None, Some(measured_i32(1_000))),
            RideOperatingState::Riding
        );
        assert_eq!(
            ride_operating_state(None, Some(measured_i32(-1_000))),
            RideOperatingState::Riding
        );
        assert_eq!(
            ride_operating_state(
                Some(MeasuredChargeModeDto {
                    value: ChargeModeDto::Charging,
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                Some(measured_i32(0)),
            ),
            RideOperatingState::Charging
        );
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

    const fn measured_i32(value: i32) -> MeasuredI32Dto {
        MeasuredI32Dto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::SourceVerified,
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
            family: ProtocolFamilyDto::VeteranLeaperkimNosfet,
            channel: [0x7a; 16],
            len: notification_len(17),
            monotonic_ms: MonotonicMillisDto { milliseconds: 42 },
        }
    }

    fn ignored_notification_fixture() -> IgnoredNotificationEvidenceDto {
        IgnoredNotificationEvidenceDto {
            family: None,
            channel: [0x7b; 16],
            len: notification_len(19),
            monotonic_ms: MonotonicMillisDto { milliseconds: 43 },
            retained_payload: vec![0xde, 0xad, 0xbe, 0xef],
        }
    }

    fn mobile_ingest(output: NotificationIngestOutcomeDto) -> MobileNotificationIngestOutcomeDto {
        let mobile = MobileSessionOutputDto::from(SessionOutputDto::NotificationIngest(output));

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::NotificationIngest);
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        mobile.ingest.expect("ingest output carries typed outcome")
    }

    fn assert_ignored_notification_outcome(ignored: &MobileNotificationIngestOutcomeDto) {
        assert_eq!(
            ignored.kind,
            MobileNotificationIngestOutcomeKindDto::Ignored
        );
        assert_eq!(ignored.notification, None);
        assert_eq!(
            ignored.ignored_reason,
            Some(MobileIgnoredNotificationReasonDto::WrongChannel)
        );
        let evidence = ignored
            .ignored
            .as_ref()
            .expect("ignored outcome carries ignored evidence");
        assert_eq!(evidence.family, None);
        assert_eq!(evidence.retained_payload, vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(ignored.event_count, None);
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
        let semantic_notification = semantic
            .notification
            .as_ref()
            .expect("semantic outcome carries accepted notification evidence");
        assert_eq!(
            semantic_notification.family,
            MobileProtocolFamilyDto::VeteranLeaperkimNosfet
        );
        assert_eq!(semantic_notification.channel, vec![0x7a; 16]);
        assert_eq!(semantic_notification.len, mobile_notification_len(17));
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
                retained_payload: vec![0x08, 0xaa],
                verification: VerificationStatusDto::SourceVerified,
            },
        });
        assert_eq!(
            reserved.reserved,
            Some(MobileReservedPayloadEvidenceDto {
                selector: Some(8),
                tag: Some(0x5a5c),
                body_len: mobile_body_len(84),
                retained_payload: vec![0x08, 0xaa],
                verification: MobileVerificationStatusDto::SourceVerified,
            })
        );

        let gap = mobile_ingest(NotificationIngestOutcomeDto::ParserGap {
            notification: notification_fixture(),
            gap: ParserGapEvidenceDto {
                selector: Some(9),
                tag: None,
                body_len: body_len(11),
                retained_payload: vec![0x09, 0xbb],
            },
        });
        assert_eq!(
            gap.gap,
            Some(MobileParserGapEvidenceDto {
                selector: Some(9),
                tag: None,
                body_len: mobile_body_len(11),
                retained_payload: vec![0x09, 0xbb],
            })
        );

        let ignored = mobile_ingest(NotificationIngestOutcomeDto::Ignored {
            evidence: ignored_notification_fixture(),
            reason: IgnoredNotificationReasonDto::WrongChannel,
        });
        assert_ignored_notification_outcome(&ignored);
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
        let notification = ingest
            .notification
            .as_ref()
            .expect("semantic ingest carries accepted notification evidence");
        assert_eq!(
            notification.family,
            MobileProtocolFamilyDto::VeteranLeaperkimNosfet
        );
        assert_eq!(notification.len, mobile_notification_len(87));
        assert_eq!(notification.monotonic_ms, ms(2));
        assert_eq!(ingest.event_count, Some(mobile_event_count(5)));
        assert_eq!(ingest.parser_error, None);
        assert_eq!(ingest.reserved, None);
        assert_eq!(ingest.gap, None);
        assert!(result.outputs.iter().all(|output| output.bytes.is_empty()));
        let snapshot = session.current_snapshot();
        assert_eq!(
            snapshot.voltage,
            Some(VoltageReading {
                value: Voltage { value: 108_760 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            })
        );
        assert_eq!(snapshot.operating_state, RideOperatingState::Parked);
        assert!(matches!(
            snapshot.battery_current,
            Some(BatteryCurrentReading {
                value: BatteryCurrent { .. },
                ..
            })
        ));
        assert!(matches!(
            snapshot.power,
            Some(PowerReading {
                value: Power { .. },
                ..
            })
        ));
        assert!(snapshot.power_flow.is_some());
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
        builder.record_notification(
            ms(2),
            vec![0x11; 16],
            vec![0x22; 16],
            vec![0xde, 0xad, 0xbe, 0xef],
        );

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

        builder.add_advertised_service(service.clone());
        builder.add_gatt_fingerprint(MobileGattFingerprintDto {
            service: service.clone(),
            characteristic: characteristic.clone(),
            roles: vec![
                MobileGattRoleDto::Read,
                MobileGattRoleDto::WriteWithoutResponse,
                MobileGattRoleDto::Notify,
            ],
            verification: MobileVerificationStatusDto::HardwareVerified,
        });

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
