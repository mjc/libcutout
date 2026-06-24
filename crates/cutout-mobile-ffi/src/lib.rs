//! Concrete `UniFFI` mobile binding surface for Cutout.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_core::{
    CommandKindDto, ControlRefusalReasonDto, DeviceCommandDto, GattChannel, GattFingerprint,
    GattRoles, MonotonicMillis, MonotonicMillisDto, NotificationByteLenDto,
    NotificationEvidenceDto, NotificationIngestOutcomeDto, ParserDiagnosticsDto, ParserErrorDto,
    ParserGapEvidenceDto, PayloadBodyLenDto, PevcapCapture, PevcapEncoding, PevcapHeader,
    PevcapRecord, PevcapResolvedIdentity, ProtocolFamily, ProtocolFamilyDto,
    ReservedPayloadEvidenceDto, SemanticEventCountDto, SessionInputDto, SessionOutputDto,
    TelemetrySnapshotDto, TransportActionDto, VerificationStatus, VerificationStatusDto,
    VerifiedValue, WallClockUnixMillis,
};
use cutout_protocols::{
    ConcreteAeroReadOnlySession, ConcreteFalconProfileDto, ConcreteFalconReadOnlySession,
    ConcreteSessionErrorDto, ConcreteSessionStepResultDto, new_nosfet_aero_read_only_session,
    try_new_begode_falcon_read_only_session,
};

uniffi::setup_scaffolding!();

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
    pub max_write_len: Option<u16>,

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

    fn into_core(self) -> MonotonicMillis {
        MonotonicMillis::new(self.milliseconds)
    }
}

/// Mobile DTO wall-clock Unix timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileWallClockUnixMillisDto {
    /// Timestamp value in Unix epoch milliseconds.
    pub milliseconds: u64,
}

impl MobileWallClockUnixMillisDto {
    fn into_core(self) -> WallClockUnixMillis {
        WallClockUnixMillis::new(self.milliseconds)
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
    pub claimed: Option<u64>,

    /// Configured maximum accepted frame length.
    pub max: Option<u64>,

    /// Elapsed monotonic time.
    pub elapsed_ms: Option<MobileMonotonicMillisDto>,

    /// Timeout threshold in monotonic time.
    pub timeout_ms: Option<MobileMonotonicMillisDto>,
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

    /// Reported voltage in millivolts.
    pub voltage: Option<i32>,

    /// Estimated battery percent.
    pub battery_percent_estimated: Option<u8>,
}

/// Mobile parser diagnostics DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserDiagnosticsDto {
    /// Malformed frame count.
    pub malformed_frames: u64,

    /// Bad checksum count.
    pub bad_checksums: u64,

    /// Oversized frame count.
    pub oversized_frames: u64,
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

/// Mobile-facing builder for a PEVCAP capture export.
#[derive(Debug, uniffi::Object)]
pub struct MobilePevcapCaptureBuilder {
    wall_clock_start_unix_ms: WallClockUnixMillis,
    platform_id: String,
    write_limit: Option<u16>,
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
        write_limit: Option<u16>,
    ) -> Arc<Self> {
        Arc::new(Self {
            wall_clock_start_unix_ms: wall_clock_start_unix_ms.into_core(),
            platform_id,
            write_limit,
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
        max_write_len: Option<u16>,
    ) {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(PevcapRecord::link_up(
                monotonic_ms.into_core(),
                max_write_len,
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

impl From<MobileSessionInputDto> for SessionInputDto {
    fn from(input: MobileSessionInputDto) -> Self {
        match input.kind {
            MobileSessionInputKindDto::LinkUp => Self::LinkUp {
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
                max_write_len: input.max_write_len,
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
            MobileCommandDto::SoundHorn => Self::SoundHorn,
        }
    }
}

impl From<CommandKindDto> for MobileCommandDto {
    fn from(command: CommandKindDto) -> Self {
        match command {
            CommandKindDto::RequestIdentity => Self::RequestIdentity,
            CommandKindDto::RequestTelemetry => Self::RequestTelemetry,
            CommandKindDto::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            CommandKindDto::RequestBatteryInfo => Self::RequestBatteryInfo,
            CommandKindDto::RequestDiagnostics
            | CommandKindDto::RequestSettings
            | CommandKindDto::SetLights
            | CommandKindDto::SetRawMotorCurrent => Self::RequestDiagnostics,
            CommandKindDto::SoundHorn => Self::SoundHorn,
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

impl From<ParserErrorDto> for MobileParserErrorDto {
    fn from(error: ParserErrorDto) -> Self {
        match error {
            ParserErrorDto::OversizedFrame { claimed, max } => Self {
                kind: MobileParserErrorKindDto::OversizedFrame,
                claimed: Some(claimed as u64),
                max: Some(max as u64),
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
                command: Some(refusal.command.into()),
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
            voltage: snapshot.voltage.map(|value| value.value),
            battery_percent_estimated: snapshot.battery_percent_estimated.map(|value| value.value),
        }
    }
}

impl From<ParserDiagnosticsDto> for MobileParserDiagnosticsDto {
    fn from(diagnostics: ParserDiagnosticsDto) -> Self {
        Self {
            malformed_frames: diagnostics.malformed_frames,
            bad_checksums: diagnostics.bad_checksums,
            oversized_frames: diagnostics.oversized_frames,
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

    const fn event_count(value: usize) -> SemanticEventCountDto {
        SemanticEventCountDto { count: value }
    }

    const fn mobile_event_count(value: u64) -> MobileSemanticEventCountDto {
        MobileSemanticEventCountDto { count: value }
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
                claimed: 257,
                max: 256,
            },
        });
        assert_eq!(
            diagnostic.parser_error,
            Some(MobileParserErrorDto {
                kind: MobileParserErrorKindDto::OversizedFrame,
                claimed: Some(257),
                max: Some(256),
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

        assert_eq!(session.diagnostics().malformed_frames, 0);
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
            max_write_len: Some(185),
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
        assert_eq!(session.current_snapshot().voltage, Some(108_760));
    }

    #[test]
    fn mobile_capture_builder_exports_cli_readable_jsonl() {
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            Some(185),
        );
        builder.add_annotation("capture_label=powered_on_stationary".into());
        builder.add_annotation("capture_privacy=redacted".into());
        builder.add_annotation("capture_distribution=redistributable".into());
        builder.add_annotation("capture_evidence=hardware_tested".into());
        builder.record_link_up(ms(1), Some(185));
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
        assert_eq!(capture.records[0].monotonic_ms, MonotonicMillis::new(9));
    }
}
