use crate::{
    BatteryInfo, BatteryPageKind, BatteryPageMetadata, BatteryPagePayload, CommandKind,
    ControlRefusal, ControlRefusalReason, DeviceCommand, DeviceEvent, DiagnosticDetail,
    DiagnosticError, DiagnosticErrorKind, DiagnosticReadback, DiagnosticSeverity, FirmwareInfo,
    LightState, Measured, ParserDiagnostics, RawFieldValue, ReadOnlyResponse, SafetyClass,
    SessionInput, SessionOutput, SettingsEntry, SettingsReadback, TelemetryDelta, TransportAction,
    ValueQuality, ValueSource, VerificationStatus, WriteMode,
};

/// UniFFI-ready owned read-only response DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOnlyResponseDto {
    /// Command kind associated with this response.
    pub command_kind: CommandKindDto,

    /// Owned response payload.
    pub payload: ReadOnlyResponsePayloadDto,
}

impl From<ReadOnlyResponse> for ReadOnlyResponseDto {
    fn from(response: ReadOnlyResponse) -> Self {
        Self {
            command_kind: response.command_kind().into(),
            payload: response.into(),
        }
    }
}

/// UniFFI-ready owned read-only response payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadOnlyResponsePayloadDto {
    /// Firmware or protocol version response.
    Firmware(FirmwareInfoDto),

    /// Battery or BMS response.
    Battery(BatteryInfoDto),

    /// Settings readback response.
    Settings(SettingsReadbackDto),

    /// Diagnostic readback response.
    Diagnostics(DiagnosticReadbackDto),
}

impl From<ReadOnlyResponse> for ReadOnlyResponsePayloadDto {
    fn from(response: ReadOnlyResponse) -> Self {
        match response {
            ReadOnlyResponse::Firmware(firmware) => Self::Firmware(firmware.into()),
            ReadOnlyResponse::Battery(battery) => Self::Battery(battery.into()),
            ReadOnlyResponse::Settings(settings) => Self::Settings(settings.into()),
            ReadOnlyResponse::Diagnostics(diagnostics) => Self::Diagnostics(diagnostics.into()),
        }
    }
}

/// UniFFI-ready command kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandKindDto {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request a telemetry update.
    RequestTelemetry,

    /// Request firmware or protocol version information.
    RequestFirmwareInfo,

    /// Request battery or BMS information.
    RequestBatteryInfo,

    /// Request device diagnostics.
    RequestDiagnostics,

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights,

    /// Sound a device horn or alert.
    SoundHorn,

    /// Set raw motor current.
    SetRawMotorCurrent,
}

impl From<CommandKind> for CommandKindDto {
    fn from(kind: CommandKind) -> Self {
        match kind {
            CommandKind::RequestIdentity => Self::RequestIdentity,
            CommandKind::RequestTelemetry => Self::RequestTelemetry,
            CommandKind::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            CommandKind::RequestBatteryInfo => Self::RequestBatteryInfo,
            CommandKind::RequestDiagnostics => Self::RequestDiagnostics,
            CommandKind::RequestSettings => Self::RequestSettings,
            CommandKind::SetLights => Self::SetLights,
            CommandKind::SoundHorn => Self::SoundHorn,
            CommandKind::SetRawMotorCurrent => Self::SetRawMotorCurrent,
        }
    }
}

/// UniFFI-ready device command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceCommandDto {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request a telemetry update.
    RequestTelemetry,

    /// Request firmware or protocol version information.
    RequestFirmwareInfo,

    /// Request battery or BMS information.
    RequestBatteryInfo,

    /// Request device diagnostics.
    RequestDiagnostics,

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights(LightStateDto),

    /// Sound a device horn or alert.
    SoundHorn,

    /// Set raw motor current.
    SetRawMotorCurrent {
        /// Target motor/phase current in milliamps.
        current_ma: i32,
    },
}

impl From<DeviceCommand> for DeviceCommandDto {
    fn from(command: DeviceCommand) -> Self {
        match command {
            DeviceCommand::RequestIdentity => Self::RequestIdentity,
            DeviceCommand::RequestTelemetry => Self::RequestTelemetry,
            DeviceCommand::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            DeviceCommand::RequestBatteryInfo => Self::RequestBatteryInfo,
            DeviceCommand::RequestDiagnostics => Self::RequestDiagnostics,
            DeviceCommand::RequestSettings => Self::RequestSettings,
            DeviceCommand::SetLights(state) => Self::SetLights(state.into()),
            DeviceCommand::SoundHorn => Self::SoundHorn,
            DeviceCommand::SetRawMotorCurrent { current_ma } => {
                Self::SetRawMotorCurrent { current_ma }
            }
        }
    }
}

/// UniFFI-ready light state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightStateDto {
    /// Lights off.
    Off,

    /// Lights on.
    On,
}

impl From<LightState> for LightStateDto {
    fn from(state: LightState) -> Self {
        match state {
            LightState::Off => Self::Off,
            LightState::On => Self::On,
        }
    }
}

/// UniFFI-ready safety class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyClassDto {
    /// Read-only request with no state change expected.
    ReadOnly,

    /// Benign control such as lights or horn.
    BenignControl,

    /// Setting that should only be changed while stationary.
    StationaryOnly,

    /// Direct actuation or motion-affecting control.
    Actuation,

    /// Firmware update or firmware mutation operation.
    Firmware,
}

impl From<SafetyClass> for SafetyClassDto {
    fn from(class: SafetyClass) -> Self {
        match class {
            SafetyClass::ReadOnly => Self::ReadOnly,
            SafetyClass::BenignControl => Self::BenignControl,
            SafetyClass::StationaryOnly => Self::StationaryOnly,
            SafetyClass::Actuation => Self::Actuation,
            SafetyClass::Firmware => Self::Firmware,
        }
    }
}

/// UniFFI-ready battery page kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatteryPageKindDto {
    /// Metadata-only page.
    Metadata,

    /// Typed cell-voltage page.
    CellVoltage,

    /// Typed temperature/status page.
    Temperature,

    /// Raw or reserved page.
    Raw,
}

impl From<BatteryPageKind> for BatteryPageKindDto {
    fn from(kind: BatteryPageKind) -> Self {
        match kind {
            BatteryPageKind::Metadata => Self::Metadata,
            BatteryPageKind::CellVoltage => Self::CellVoltage,
            BatteryPageKind::Temperature => Self::Temperature,
            BatteryPageKind::Raw => Self::Raw,
        }
    }
}

/// UniFFI-ready value source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueSourceDto {
    /// Value was reported directly by the device.
    Reported,

    /// Value was calculated from other values.
    Calculated,

    /// Value was estimated from incomplete evidence.
    Estimated,
}

impl From<ValueSource> for ValueSourceDto {
    fn from(source: ValueSource) -> Self {
        match source {
            ValueSource::Reported => Self::Reported,
            ValueSource::Calculated => Self::Calculated,
            ValueSource::Estimated => Self::Estimated,
        }
    }
}

/// UniFFI-ready value quality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueQualityDto {
    /// Value is directly supported by observed data.
    Known,

    /// Value is inferred from partial or indirect evidence.
    Inferred,
}

impl From<ValueQuality> for ValueQualityDto {
    fn from(quality: ValueQuality) -> Self {
        match quality {
            ValueQuality::Known => Self::Known,
            ValueQuality::Inferred => Self::Inferred,
        }
    }
}

/// UniFFI-ready verification status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationStatusDto {
    /// Not yet verified.
    Unverified,

    /// Inferred from partial evidence.
    Inferred,

    /// Verified against source-attributed protocol documentation.
    SourceVerified,

    /// Verified against Bluetooth hardware.
    HardwareVerified,

    /// Verified against both source and hardware evidence.
    SourceAndHardwareVerified,
}

impl From<VerificationStatus> for VerificationStatusDto {
    fn from(verification: VerificationStatus) -> Self {
        match verification {
            VerificationStatus::Unverified => Self::Unverified,
            VerificationStatus::Inferred => Self::Inferred,
            VerificationStatus::SourceVerified => Self::SourceVerified,
            VerificationStatus::HardwareVerified => Self::HardwareVerified,
            VerificationStatus::SourceAndHardwareVerified => Self::SourceAndHardwareVerified,
        }
    }
}

/// UniFFI-ready diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverityDto {
    /// Informational diagnostic.
    Info,

    /// Warning diagnostic.
    Warning,

    /// Error diagnostic.
    Error,
}

impl From<DiagnosticSeverity> for DiagnosticSeverityDto {
    fn from(severity: DiagnosticSeverity) -> Self {
        match severity {
            DiagnosticSeverity::Info => Self::Info,
            DiagnosticSeverity::Warning => Self::Warning,
            DiagnosticSeverity::Error => Self::Error,
        }
    }
}

/// UniFFI-ready measured i32 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredI32Dto {
    /// Fixed-unit value.
    pub value: i32,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<Measured<i32>> for MeasuredI32Dto {
    fn from(measured: Measured<i32>) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

/// UniFFI-ready measured i64 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredI64Dto {
    /// Fixed-unit value.
    pub value: i64,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<Measured<i64>> for MeasuredI64Dto {
    fn from(measured: Measured<i64>) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

/// UniFFI-ready measured i16 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredI16Dto {
    /// Fixed-unit value.
    pub value: i16,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<Measured<i16>> for MeasuredI16Dto {
    fn from(measured: Measured<i16>) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

/// UniFFI-ready measured u8 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredU8Dto {
    /// Fixed-unit value.
    pub value: u8,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<Measured<u8>> for MeasuredU8Dto {
    fn from(measured: Measured<u8>) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

/// UniFFI-ready measured u16 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredU16Dto {
    /// Fixed-unit value.
    pub value: u16,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<Measured<u16>> for MeasuredU16Dto {
    fn from(measured: Measured<u16>) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

/// UniFFI-ready measured u64 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredU64Dto {
    /// Fixed-unit value.
    pub value: u64,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<Measured<u64>> for MeasuredU64Dto {
    fn from(measured: Measured<u64>) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

/// UniFFI-ready raw field value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawFieldValueDto {
    /// Protocol-family field identifier.
    pub id: u16,

    /// Sign-extended raw field value.
    pub value: i64,
}

impl From<RawFieldValue> for RawFieldValueDto {
    fn from(field: RawFieldValue) -> Self {
        Self {
            id: field.id,
            value: field.value,
        }
    }
}

/// UniFFI-ready battery page metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryPageMetadataDto {
    /// BMS page selector.
    pub selector: u8,

    /// Battery page kind.
    pub kind: BatteryPageKindDto,

    /// Page interpretation verification.
    pub verification: VerificationStatusDto,
}

impl From<BatteryPageMetadata> for BatteryPageMetadataDto {
    fn from(page: BatteryPageMetadata) -> Self {
        Self {
            selector: page.selector,
            kind: page.kind.into(),
            verification: page.verification.into(),
        }
    }
}

/// UniFFI-ready battery or BMS response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatteryInfoDto {
    /// Page metadata for this battery response.
    pub page: BatteryPageMetadataDto,

    /// Pack or input voltage in millivolts.
    pub voltage_mv: Option<MeasuredI32Dto>,

    /// Pack or battery current in milliamps.
    pub current_ma: Option<MeasuredI32Dto>,

    /// Battery percentage reported by the device.
    pub percent_reported: Option<MeasuredU8Dto>,

    /// Battery percentage estimated by Cutout.
    pub percent_estimated: Option<MeasuredU8Dto>,

    /// Battery or BMS temperature in millicelsius.
    pub temperature_mc: Option<MeasuredI32Dto>,

    /// Raw battery or BMS state field.
    pub raw_state: Option<RawFieldValueDto>,
}

impl From<BatteryPagePayload> for BatteryInfoDto {
    fn from(payload: BatteryPagePayload) -> Self {
        let battery = payload.battery();
        Self::from_payload_parts(payload.page(), battery)
    }
}

impl BatteryInfoDto {
    fn from_payload_parts(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self {
            page: page.into(),
            voltage_mv: battery.voltage_mv.map(Into::into),
            current_ma: battery.current_ma.map(Into::into),
            percent_reported: battery.percent_reported.map(Into::into),
            percent_estimated: battery.percent_estimated.map(Into::into),
            temperature_mc: battery.temperature_mc.map(Into::into),
            raw_state: battery.raw_state.map(Into::into),
        }
    }
}

/// UniFFI-ready firmware or protocol version response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareInfoDto {
    /// Protocol version, when reported.
    pub protocol_version: Option<MeasuredU16Dto>,

    /// Firmware major version, when reported.
    pub firmware_major: Option<MeasuredU16Dto>,

    /// Firmware minor version, when reported.
    pub firmware_minor: Option<MeasuredU16Dto>,

    /// Firmware patch version, when reported.
    pub firmware_patch: Option<MeasuredU16Dto>,

    /// Raw build identifier, when present.
    pub build_id: Option<RawFieldValueDto>,
}

impl From<FirmwareInfo> for FirmwareInfoDto {
    fn from(firmware: FirmwareInfo) -> Self {
        Self {
            protocol_version: firmware.protocol_version.map(Into::into),
            firmware_major: firmware.firmware_major.map(Into::into),
            firmware_minor: firmware.firmware_minor.map(Into::into),
            firmware_patch: firmware.firmware_patch.map(Into::into),
            build_id: firmware.build_id.map(Into::into),
        }
    }
}

/// UniFFI-ready settings readback response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsReadbackDto {
    /// Present settings entries.
    pub entries: Vec<SettingsEntryDto>,
}

impl From<SettingsReadback> for SettingsReadbackDto {
    fn from(settings: SettingsReadback) -> Self {
        Self {
            entries: settings
                .entries
                .into_iter()
                .flatten()
                .map(Into::into)
                .collect(),
        }
    }
}

/// UniFFI-ready settings entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsEntryDto {
    /// Raw settings field.
    pub field: RawFieldValueDto,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<SettingsEntry> for SettingsEntryDto {
    fn from(entry: SettingsEntry) -> Self {
        Self {
            field: entry.field.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

/// UniFFI-ready diagnostic readback response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticReadbackDto {
    /// Present diagnostic details.
    pub details: Vec<DiagnosticDetailDto>,
}

impl From<DiagnosticReadback> for DiagnosticReadbackDto {
    fn from(diagnostics: DiagnosticReadback) -> Self {
        Self {
            details: diagnostics
                .details
                .into_iter()
                .flatten()
                .map(Into::into)
                .collect(),
        }
    }
}

/// UniFFI-ready diagnostic detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticDetailDto {
    /// Raw diagnostic field.
    pub field: RawFieldValueDto,

    /// Diagnostic severity.
    pub severity: DiagnosticSeverityDto,

    /// Diagnostic quality.
    pub quality: ValueQualityDto,

    /// Diagnostic verification status.
    pub verification: VerificationStatusDto,
}

impl From<DiagnosticDetail> for DiagnosticDetailDto {
    fn from(detail: DiagnosticDetail) -> Self {
        Self {
            field: detail.field.into(),
            severity: detail.severity.into(),
            quality: detail.quality.into(),
            verification: detail.verification.into(),
        }
    }
}

/// UniFFI-ready session input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionInputDto {
    /// The underlying transport link is available.
    LinkUp {
        /// Host monotonic connection timestamp.
        monotonic_ms: u64,

        /// Maximum write payload length reported by the host, when known.
        max_write_len: Option<u16>,
    },

    /// The underlying transport link is no longer available.
    LinkDown,

    /// Notification bytes received from a transport endpoint.
    Notification {
        /// Transport endpoint that produced the bytes.
        channel: [u8; 16],

        /// Owned notification payload.
        bytes: Vec<u8>,

        /// Host monotonic receive timestamp.
        monotonic_ms: u64,
    },

    /// Timer tick supplied by the host.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: u64,
    },

    /// Command requested by the host application.
    Command(DeviceCommandDto),
}

impl From<SessionInput<'_>> for SessionInputDto {
    fn from(input: SessionInput<'_>) -> Self {
        match input {
            SessionInput::LinkUp(link) => Self::LinkUp {
                monotonic_ms: link.monotonic_ms,
                max_write_len: link.max_write_len,
            },
            SessionInput::LinkDown => Self::LinkDown,
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => Self::Notification {
                channel: channel.as_bytes(),
                bytes: bytes.to_vec(),
                monotonic_ms,
            },
            SessionInput::Tick { monotonic_ms } => Self::Tick { monotonic_ms },
            SessionInput::Command(command) => Self::Command(command.into()),
        }
    }
}

/// UniFFI-ready session output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionOutputDto {
    /// Transport action to execute outside the protocol engine.
    Transport(TransportActionDto),

    /// Semantic event to report to the application.
    Event(SessionEventDto),
}

impl From<SessionOutput> for SessionOutputDto {
    fn from(output: SessionOutput) -> Self {
        match output {
            SessionOutput::Transport(action) => Self::Transport(action.into()),
            SessionOutput::Event(event) => Self::Event(event.into()),
        }
    }
}

/// UniFFI-ready transport action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportActionDto {
    /// Subscribe to notifications from a transport endpoint.
    Subscribe {
        /// Transport endpoint to subscribe to.
        channel: [u8; 16],
    },

    /// Write bytes to a transport endpoint.
    Write {
        /// Transport endpoint to write to.
        channel: [u8; 16],

        /// Owned bytes to write.
        bytes: Vec<u8>,

        /// Transport write behavior.
        mode: WriteModeDto,
    },

    /// Disconnect the underlying transport.
    Disconnect,
}

impl From<TransportAction> for TransportActionDto {
    fn from(action: TransportAction) -> Self {
        match action {
            TransportAction::Subscribe { channel } => Self::Subscribe {
                channel: channel.as_bytes(),
            },
            TransportAction::Write {
                channel,
                bytes,
                mode,
            } => Self::Write {
                channel: channel.as_bytes(),
                bytes: bytes.as_slice().to_vec(),
                mode: mode.into(),
            },
            TransportAction::Disconnect => Self::Disconnect,
        }
    }
}

/// UniFFI-ready write mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteModeDto {
    /// Write with transport-level acknowledgement.
    WithResponse,

    /// Write without transport-level acknowledgement.
    WithoutResponse,
}

impl From<WriteMode> for WriteModeDto {
    fn from(mode: WriteMode) -> Self {
        match mode {
            WriteMode::WithResponse => Self::WithResponse,
            WriteMode::WithoutResponse => Self::WithoutResponse,
        }
    }
}

/// UniFFI-ready semantic session event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEventDto {
    /// Link-up event accepted by the session.
    LinkUp {
        /// Host monotonic connection timestamp.
        monotonic_ms: u64,

        /// Maximum write payload length reported by the host, when known.
        max_write_len: Option<u16>,
    },

    /// Link-down event accepted by the session.
    LinkDown,

    /// Notification metadata accepted by the session.
    NotificationReceived {
        /// Transport endpoint that produced the bytes.
        channel: [u8; 16],

        /// Host monotonic receive timestamp.
        monotonic_ms: u64,

        /// Number of notification bytes observed.
        len: u64,
    },

    /// Tick event accepted by the session.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: u64,
    },

    /// Telemetry update emitted by a protocol session.
    Telemetry(TelemetryDeltaDto),

    /// Read-only response emitted by a protocol session.
    ReadOnlyResponse(ReadOnlyResponseDto),

    /// Control command refused before transport writes.
    ControlRefusal(ControlRefusalDto),

    /// Parser diagnostics emitted by a protocol session.
    Diagnostics(ParserDiagnosticsDto),

    /// Detailed parser diagnostic error emitted by a protocol session.
    DiagnosticError(DiagnosticErrorDto),
}

impl From<DeviceEvent> for SessionEventDto {
    fn from(event: DeviceEvent) -> Self {
        match event {
            DeviceEvent::LinkUp(link) => Self::LinkUp {
                monotonic_ms: link.monotonic_ms,
                max_write_len: link.max_write_len,
            },
            DeviceEvent::LinkDown => Self::LinkDown,
            DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms,
                len,
            } => Self::NotificationReceived {
                channel: channel.as_bytes(),
                monotonic_ms,
                len: len as u64,
            },
            DeviceEvent::Tick { monotonic_ms } => Self::Tick { monotonic_ms },
            DeviceEvent::Telemetry(delta) => Self::Telemetry(delta.into()),
            DeviceEvent::ReadOnlyResponse(response) => Self::ReadOnlyResponse(response.into()),
            DeviceEvent::ControlRefusal(refusal) => Self::ControlRefusal(refusal.into()),
            DeviceEvent::Diagnostics(diagnostics) => Self::Diagnostics(diagnostics.into()),
            DeviceEvent::DiagnosticError(error) => Self::DiagnosticError(error.into()),
        }
    }
}

/// UniFFI-ready telemetry delta.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryDeltaDto {
    /// Host monotonic timestamp for this update.
    pub at_ms: u64,

    /// Reported or calculated speed in millimeters per second.
    pub speed_mm_s: Option<MeasuredI32Dto>,

    /// Reported or measured input voltage in millivolts.
    pub voltage_mv: Option<MeasuredI32Dto>,

    /// Battery/input current in milliamps.
    pub battery_current_ma: Option<MeasuredI32Dto>,

    /// Motor/phase current in milliamps.
    pub motor_current_ma: Option<MeasuredI32Dto>,

    /// Electrical power in milliwatts.
    pub power_mw: Option<MeasuredI64Dto>,

    /// Controller temperature in millicelsius.
    pub controller_temperature_mc: Option<MeasuredI32Dto>,

    /// Motor temperature in millicelsius.
    pub motor_temperature_mc: Option<MeasuredI32Dto>,

    /// Battery temperature in millicelsius.
    pub battery_temperature_mc: Option<MeasuredI32Dto>,

    /// PWM duty in permille.
    pub pwm_permille: Option<MeasuredI16Dto>,

    /// Total or trip distance in millimeters.
    pub distance_mm: Option<MeasuredU64Dto>,

    /// Pitch in millidegrees.
    pub pitch_mdeg: Option<MeasuredI32Dto>,

    /// Roll in millidegrees.
    pub roll_mdeg: Option<MeasuredI32Dto>,

    /// Battery percentage reported by the device.
    pub battery_percent_reported: Option<MeasuredU8Dto>,

    /// Battery percentage estimated by Cutout.
    pub battery_percent_estimated: Option<MeasuredU8Dto>,
}

impl From<TelemetryDelta> for TelemetryDeltaDto {
    fn from(delta: TelemetryDelta) -> Self {
        Self {
            at_ms: delta.at_ms,
            speed_mm_s: delta.speed_mm_s.map(Into::into),
            voltage_mv: delta.voltage_mv.map(Into::into),
            battery_current_ma: delta.battery_current_ma.map(Into::into),
            motor_current_ma: delta.motor_current_ma.map(Into::into),
            power_mw: delta.power_mw.map(Into::into),
            controller_temperature_mc: delta.controller_temperature_mc.map(Into::into),
            motor_temperature_mc: delta.motor_temperature_mc.map(Into::into),
            battery_temperature_mc: delta.battery_temperature_mc.map(Into::into),
            pwm_permille: delta.pwm_permille.map(Into::into),
            distance_mm: delta.distance_mm.map(Into::into),
            pitch_mdeg: delta.pitch_mdeg.map(Into::into),
            roll_mdeg: delta.roll_mdeg.map(Into::into),
            battery_percent_reported: delta.battery_percent_reported.map(Into::into),
            battery_percent_estimated: delta.battery_percent_estimated.map(Into::into),
        }
    }
}

/// UniFFI-ready control refusal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlRefusalDto {
    /// Command that was refused.
    pub command: CommandKindDto,

    /// Safety class of the refused command.
    pub safety_class: SafetyClassDto,

    /// Refusal reason.
    pub reason: ControlRefusalReasonDto,
}

impl From<ControlRefusal> for ControlRefusalDto {
    fn from(refusal: ControlRefusal) -> Self {
        Self {
            command: refusal.command.into(),
            safety_class: refusal.safety_class.into(),
            reason: refusal.reason.into(),
        }
    }
}

/// UniFFI-ready control refusal reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlRefusalReasonDto {
    /// Command is not classified for this control shell.
    WrongSafetyClass,

    /// No required arming token was supplied.
    MissingArm,

    /// Arming token was issued for another model.
    WrongModel,

    /// Arming token has expired.
    ExpiredArm,

    /// Requested value exceeds the configured current limit.
    CurrentLimitExceeded,

    /// Command is not supported by this model/session.
    UnsupportedCommand,
}

impl From<ControlRefusalReason> for ControlRefusalReasonDto {
    fn from(reason: ControlRefusalReason) -> Self {
        match reason {
            ControlRefusalReason::WrongSafetyClass => Self::WrongSafetyClass,
            ControlRefusalReason::MissingArm => Self::MissingArm,
            ControlRefusalReason::WrongModel => Self::WrongModel,
            ControlRefusalReason::ExpiredArm => Self::ExpiredArm,
            ControlRefusalReason::CurrentLimitExceeded => Self::CurrentLimitExceeded,
            ControlRefusalReason::UnsupportedCommand => Self::UnsupportedCommand,
        }
    }
}

/// UniFFI-ready parser diagnostics counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserDiagnosticsDto {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: u64,

    /// Parser resynchronization attempts.
    pub resyncs: u64,

    /// Frames rejected because their checksum did not match.
    pub bad_checksums: u64,

    /// Parser deadlines that elapsed before expected data arrived.
    pub timeouts: u64,

    /// Frames rejected because they exceeded parser limits.
    pub oversized_frames: u64,

    /// Frames rejected because their structure was invalid.
    pub malformed_frames: u64,

    /// Replies that could not be matched to an in-flight request.
    pub unmatched_replies: u64,
}

impl From<ParserDiagnostics> for ParserDiagnosticsDto {
    fn from(diagnostics: ParserDiagnostics) -> Self {
        Self {
            dropped_bytes: diagnostics.dropped_bytes,
            resyncs: diagnostics.resyncs,
            bad_checksums: diagnostics.bad_checksums,
            timeouts: diagnostics.timeouts,
            oversized_frames: diagnostics.oversized_frames,
            malformed_frames: diagnostics.malformed_frames,
            unmatched_replies: diagnostics.unmatched_replies,
        }
    }
}

/// UniFFI-ready diagnostic error kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticErrorKindDto {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame,

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before expected data arrived.
    Timeout,

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

impl From<DiagnosticErrorKind> for DiagnosticErrorKindDto {
    fn from(kind: DiagnosticErrorKind) -> Self {
        match kind {
            DiagnosticErrorKind::OversizedFrame => Self::OversizedFrame,
            DiagnosticErrorKind::BadChecksum => Self::BadChecksum,
            DiagnosticErrorKind::MalformedFrame => Self::MalformedFrame,
            DiagnosticErrorKind::Timeout => Self::Timeout,
            DiagnosticErrorKind::UnmatchedReply => Self::UnmatchedReply,
        }
    }
}

/// UniFFI-ready diagnostic error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticErrorDto {
    /// Stable diagnostic error discriminator.
    pub kind: DiagnosticErrorKindDto,

    /// Claimed or observed frame length for oversized-frame errors.
    pub claimed_len: Option<u64>,

    /// Configured maximum frame length for oversized-frame errors.
    pub max_len: Option<u64>,

    /// Elapsed monotonic milliseconds for timeout errors.
    pub elapsed_ms: Option<u64>,

    /// Timeout threshold in monotonic milliseconds for timeout errors.
    pub timeout_ms: Option<u64>,
}

impl From<DiagnosticError> for DiagnosticErrorDto {
    fn from(error: DiagnosticError) -> Self {
        Self {
            kind: error.kind.into(),
            claimed_len: error.claimed_len.map(|len| len as u64),
            max_len: error.max_len.map(|len| len as u64),
            elapsed_ms: error.elapsed_ms,
            timeout_ms: error.timeout_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        BatteryInfo, BatteryPageMetadata, BatteryPagePayload, DeviceCommand, DeviceEvent,
        DiagnosticDetail, DiagnosticReadback, DiagnosticSeverity, FirmwareInfo, GattChannel,
        LinkInfo, Measured, RawFieldValue, ReadOnlyResponse, SessionInput, SessionOutput,
        SettingsEntry, SettingsReadback, TransportAction, ValueQuality, ValueSource,
        VerificationStatus, WriteMode, WritePayload,
    };

    use super::*;

    #[test]
    fn read_only_battery_dto_preserves_page_and_unknown_values() {
        let response = ReadOnlyResponse::Battery(BatteryPagePayload::raw(
            BatteryPageMetadata::raw(8, VerificationStatus::SourceVerified),
            BatteryInfo {
                voltage_mv: Some(Measured::reported(80_000)),
                current_ma: None,
                percent_reported: Some(Measured::reported(72)),
                percent_estimated: None,
                temperature_mc: Some(Measured::reported(25_000)),
                raw_state: Some(RawFieldValue::new(0x0008, 0x55aa)),
            },
        ));

        let dto = ReadOnlyResponseDto::from(response);

        assert_eq!(dto.command_kind, CommandKindDto::RequestBatteryInfo);
        let ReadOnlyResponsePayloadDto::Battery(battery) = dto.payload else {
            panic!("expected battery DTO");
        };
        assert_eq!(battery.page.selector, 8);
        assert_eq!(battery.page.kind, BatteryPageKindDto::Raw);
        assert_eq!(
            battery.page.verification,
            VerificationStatusDto::SourceVerified
        );
        assert_eq!(battery.voltage_mv.expect("voltage").value, 80_000);
        assert_eq!(battery.current_ma, None);
        assert_eq!(battery.percent_reported.expect("percent").value, 72);
        assert_eq!(battery.percent_estimated, None);
        assert_eq!(battery.temperature_mc.expect("temperature").value, 25_000);
        assert_eq!(
            battery.raw_state,
            Some(RawFieldValueDto {
                id: 0x0008,
                value: 0x55aa
            })
        );
    }

    #[test]
    fn read_only_firmware_dto_preserves_optional_fields() {
        let response = ReadOnlyResponse::Firmware(FirmwareInfo {
            protocol_version: Some(Measured::reported(2)),
            firmware_major: Some(Measured::reported(43)),
            firmware_minor: None,
            firmware_patch: Some(Measured::reported(7)),
            build_id: Some(RawFieldValue::new(0x002a, 99)),
        });

        let dto = ReadOnlyResponseDto::from(response);

        assert_eq!(dto.command_kind, CommandKindDto::RequestFirmwareInfo);
        let ReadOnlyResponsePayloadDto::Firmware(firmware) = dto.payload else {
            panic!("expected firmware DTO");
        };
        assert_eq!(firmware.protocol_version.expect("protocol").value, 2);
        assert_eq!(firmware.firmware_major.expect("major").value, 43);
        assert_eq!(firmware.firmware_minor, None);
        assert_eq!(firmware.firmware_patch.expect("patch").value, 7);
        assert_eq!(
            firmware.build_id,
            Some(RawFieldValueDto {
                id: 0x002a,
                value: 99
            })
        );
    }

    #[test]
    fn read_only_settings_dto_owns_present_entries_only() {
        let response = ReadOnlyResponse::Settings(SettingsReadback {
            entries: [
                Some(SettingsEntry {
                    field: RawFieldValue::new(0x0014, 30),
                    source: ValueSource::Reported,
                    quality: ValueQuality::Known,
                    verification: VerificationStatus::HardwareVerified,
                }),
                None,
                Some(SettingsEntry {
                    field: RawFieldValue::new(0x0018, 45),
                    source: ValueSource::Estimated,
                    quality: ValueQuality::Inferred,
                    verification: VerificationStatus::Inferred,
                }),
                None,
            ],
        });

        let dto = ReadOnlyResponseDto::from(response);

        assert_eq!(dto.command_kind, CommandKindDto::RequestSettings);
        let ReadOnlyResponsePayloadDto::Settings(settings) = dto.payload else {
            panic!("expected settings DTO");
        };
        assert_eq!(settings.entries.len(), 2);
        assert_eq!(settings.entries[0].field.id, 0x0014);
        assert_eq!(settings.entries[0].source, ValueSourceDto::Reported);
        assert_eq!(settings.entries[1].field.value, 45);
        assert_eq!(settings.entries[1].quality, ValueQualityDto::Inferred);
    }

    #[test]
    fn read_only_diagnostics_dto_owns_present_details_only() {
        let response = ReadOnlyResponse::Diagnostics(DiagnosticReadback {
            details: [
                Some(DiagnosticDetail {
                    field: RawFieldValue::new(0x0005, 1),
                    severity: DiagnosticSeverity::Warning,
                    quality: ValueQuality::Known,
                    verification: VerificationStatus::SourceVerified,
                }),
                None,
                None,
                None,
            ],
        });

        let dto = ReadOnlyResponseDto::from(response);

        assert_eq!(dto.command_kind, CommandKindDto::RequestDiagnostics);
        let ReadOnlyResponsePayloadDto::Diagnostics(diagnostics) = dto.payload else {
            panic!("expected diagnostics DTO");
        };
        assert_eq!(diagnostics.details.len(), 1);
        assert_eq!(
            diagnostics.details[0].severity,
            DiagnosticSeverityDto::Warning
        );
        assert_eq!(
            diagnostics.details[0].verification,
            VerificationStatusDto::SourceVerified
        );
    }

    #[test]
    fn session_input_dto_owns_notification_bytes_and_commands() {
        let notification = SessionInputDto::from(SessionInput::Notification {
            channel: GattChannel::from_bytes([0xA1; 16]),
            bytes: &[0xde, 0xad, 0xbe, 0xef],
            monotonic_ms: 42,
        });
        let command =
            SessionInputDto::from(SessionInput::Command(DeviceCommand::SetRawMotorCurrent {
                current_ma: -1_500,
            }));

        assert_eq!(
            notification,
            SessionInputDto::Notification {
                channel: [0xA1; 16],
                bytes: vec![0xde, 0xad, 0xbe, 0xef],
                monotonic_ms: 42,
            }
        );
        assert_eq!(
            command,
            SessionInputDto::Command(DeviceCommandDto::SetRawMotorCurrent { current_ma: -1_500 })
        );
    }

    #[test]
    fn session_output_dto_owns_transport_write_bytes_and_events() {
        let write = SessionOutputDto::from(SessionOutput::Transport(TransportAction::Write {
            channel: GattChannel::from_bytes([0xB2; 16]),
            bytes: WritePayload::try_from_slice(&[1, 2, 3]).expect("payload fits"),
            mode: WriteMode::WithoutResponse,
        }));
        let event = SessionOutputDto::from(SessionOutput::Event(DeviceEvent::LinkUp(LinkInfo {
            monotonic_ms: 7,
            max_write_len: Some(182),
        })));

        assert_eq!(
            write,
            SessionOutputDto::Transport(TransportActionDto::Write {
                channel: [0xB2; 16],
                bytes: vec![1, 2, 3],
                mode: WriteModeDto::WithoutResponse,
            })
        );
        assert_eq!(
            event,
            SessionOutputDto::Event(SessionEventDto::LinkUp {
                monotonic_ms: 7,
                max_write_len: Some(182),
            })
        );
    }
}
