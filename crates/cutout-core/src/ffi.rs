use crate::{
    Angle, BatteryCurrent, BatteryInfo, BatteryLevel, BatteryPageKind, BatteryPageMetadata,
    BatteryPagePayload, BatteryReadback, BatteryReadbackAvailability, BmsPackCurrents, ChargeMode,
    CommandKind, ControlRefusal, ControlRefusalReason, DeviceCommand, DeviceEvent,
    DiagnosticDetail, DiagnosticError, DiagnosticErrorKind, DiagnosticReadback, DiagnosticSeverity,
    Distance, DutyCycle, FaultCode, FaultHistoryAvailability, FaultHistoryEntry,
    FaultHistoryReadback, FirmwareInfo, FootpadContactState, FootpadTelemetry,
    IgnoredNotificationEvidence, IgnoredNotificationReason, LightState, Measured,
    MonotonicTimestamp, NotificationByteLen, NotificationEvidence, NotificationIngestOutcome,
    ParserDiagnosticCount, ParserDiagnostics, ParserDroppedBytes, ParserError, ParserFrameLen,
    ParserGapEvidence, PayloadBodyLen, PhaseCurrent, Power, ProtocolFamily, ProtocolTag,
    RawFieldValue, RawTelemetryReadback, ReadOnlyResponse, ReservedPayloadEvidence,
    RideOperatingState, RideStopReason, RideWarning, SafetyClass, SemanticEventCount, SessionInput,
    SessionOutput, SettingsEntry, SettingsReadback, SettingsReadbackAvailability, Speed,
    TelemetryDelta, TelemetrySnapshot, Temperature, TransportAction, TransportWriteLimit,
    ValueQuality, ValueSource, VerificationStatus, Voltage, WriteMode,
};

/// UniFFI-ready owned read-only output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOnlyOutput {
    /// Command kind associated with this response.
    pub command_kind: CommandKindDto,

    /// Owned response payload.
    pub payload: ReadOnlyOutputPayload,
}

impl From<ReadOnlyResponse> for ReadOnlyOutput {
    fn from(response: ReadOnlyResponse) -> Self {
        Self {
            command_kind: response.command_kind().into(),
            payload: response.into(),
        }
    }
}

/// UniFFI-ready monotonic timestamp in milliseconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonotonicMillisDto {
    /// Milliseconds on the host monotonic clock.
    pub milliseconds: u64,
}

impl MonotonicMillisDto {
    fn from_core(value: MonotonicTimestamp) -> Self {
        Self {
            milliseconds: value.get(),
        }
    }

    fn into_core(self) -> MonotonicTimestamp {
        MonotonicTimestamp::new(self.milliseconds)
    }
}

/// UniFFI-ready notification payload length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationByteLenDto {
    /// Length in bytes.
    pub bytes: usize,
}

impl NotificationByteLenDto {
    fn from_core(value: NotificationByteLen) -> Self {
        Self {
            bytes: value.as_bytes(),
        }
    }
}

/// UniFFI-ready protocol payload body length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadBodyLenDto {
    /// Length in bytes.
    pub bytes: usize,
}

impl PayloadBodyLenDto {
    fn from_core(value: PayloadBodyLen) -> Self {
        Self {
            bytes: value.as_bytes(),
        }
    }
}

/// UniFFI-ready semantic event count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticEventCountDto {
    /// Count of emitted semantic events.
    pub count: usize,
}

impl SemanticEventCountDto {
    fn from_core(value: SemanticEventCount) -> Self {
        Self {
            count: value.as_events(),
        }
    }
}

/// UniFFI-ready dropped parser byte count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserDroppedBytesDto {
    /// Count of dropped bytes.
    pub bytes: u64,
}

impl ParserDroppedBytesDto {
    fn from_core(value: ParserDroppedBytes) -> Self {
        Self {
            bytes: value.as_bytes(),
        }
    }
}

/// UniFFI-ready parser diagnostic event count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserDiagnosticCountDto {
    /// Count of parser diagnostic events.
    pub count: u64,
}

impl ParserDiagnosticCountDto {
    fn from_core(value: ParserDiagnosticCount) -> Self {
        Self {
            count: value.as_events(),
        }
    }
}

/// UniFFI-ready maximum transport write payload length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportWriteLimitDto {
    /// Length in bytes.
    pub bytes: u16,
}

impl TransportWriteLimitDto {
    fn from_core(value: TransportWriteLimit) -> Self {
        Self { bytes: value.get() }
    }

    fn into_core(self) -> TransportWriteLimit {
        TransportWriteLimit::from_bytes(self.bytes)
    }
}

/// UniFFI-ready parser frame length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserFrameLenDto {
    /// Length in bytes.
    pub bytes: usize,
}

impl ParserFrameLenDto {
    fn from_core(value: ParserFrameLen) -> Self {
        Self {
            bytes: value.as_bytes(),
        }
    }
}

/// UniFFI-ready owned read-only output payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadOnlyOutputPayload {
    /// Firmware or protocol version response.
    Firmware(FirmwareInfoDto),

    /// Battery or BMS response.
    Battery(BatteryReadbackDto),

    /// Settings readback response.
    Settings(SettingsReadbackDto),

    /// Fault-history readback response.
    FaultHistory(FaultHistoryReadbackDto),

    /// Diagnostic readback response.
    Diagnostics(DiagnosticReadbackDto),

    /// Protocol-native raw telemetry response.
    RawTelemetry(RawTelemetryReadbackDto),
}

impl From<ReadOnlyResponse> for ReadOnlyOutputPayload {
    fn from(response: ReadOnlyResponse) -> Self {
        match response {
            ReadOnlyResponse::Firmware(firmware) => Self::Firmware(firmware.into()),
            ReadOnlyResponse::Battery(battery) => Self::Battery(battery.into()),
            ReadOnlyResponse::Settings(settings) => Self::Settings(settings.into()),
            ReadOnlyResponse::FaultHistory(fault_history) => {
                Self::FaultHistory(fault_history.into())
            }
            ReadOnlyResponse::Diagnostics(diagnostics) => Self::Diagnostics(diagnostics.into()),
            ReadOnlyResponse::RawTelemetry(raw) => Self::RawTelemetry(raw.into()),
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

    /// Request historical fault information.
    RequestFaultHistory,

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
            CommandKind::RequestFaultHistory => Self::RequestFaultHistory,
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

    /// Request historical fault information.
    RequestFaultHistory,

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights(LightStateDto),

    /// Sound a device horn or alert.
    SoundHorn,

    /// Set raw motor current.
    SetRawMotorCurrent {
        /// Target motor/phase current in milliamps.
        current: i32,
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
            DeviceCommand::RequestFaultHistory => Self::RequestFaultHistory,
            DeviceCommand::RequestSettings => Self::RequestSettings,
            DeviceCommand::SetLights(state) => Self::SetLights(state.into()),
            DeviceCommand::SoundHorn => Self::SoundHorn,
            DeviceCommand::SetRawMotorCurrent { current } => Self::SetRawMotorCurrent {
                current: current.as_milliamps(),
            },
        }
    }
}

impl From<DeviceCommandDto> for DeviceCommand {
    fn from(command: DeviceCommandDto) -> Self {
        match command {
            DeviceCommandDto::RequestIdentity => Self::RequestIdentity,
            DeviceCommandDto::RequestTelemetry => Self::RequestTelemetry,
            DeviceCommandDto::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            DeviceCommandDto::RequestBatteryInfo => Self::RequestBatteryInfo,
            DeviceCommandDto::RequestDiagnostics => Self::RequestDiagnostics,
            DeviceCommandDto::RequestFaultHistory => Self::RequestFaultHistory,
            DeviceCommandDto::RequestSettings => Self::RequestSettings,
            DeviceCommandDto::SetLights(state) => Self::SetLights(state.into()),
            DeviceCommandDto::SoundHorn => Self::SoundHorn,
            DeviceCommandDto::SetRawMotorCurrent { current } => Self::SetRawMotorCurrent {
                current: PhaseCurrent::from_milliamps(current),
            },
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

impl From<LightStateDto> for LightState {
    fn from(state: LightStateDto) -> Self {
        match state {
            LightStateDto::Off => Self::Off,
            LightStateDto::On => Self::On,
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

/// UniFFI-ready charging state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargeModeDto {
    /// The device reports that charging is active.
    Charging,

    /// The device reports that charging is not active.
    NotCharging,
}

impl From<ChargeMode> for ChargeModeDto {
    fn from(mode: ChargeMode) -> Self {
        match mode {
            ChargeMode::Charging => Self::Charging,
            ChargeMode::NotCharging => Self::NotCharging,
        }
    }
}

/// UniFFI-ready ride operating state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideOperatingStateDto {
    /// No live evidence has established whether the vehicle is parked, riding, or charging.
    Unknown,

    /// Explicit telemetry indicates the vehicle is parked.
    Parked,

    /// Live telemetry indicates the vehicle is stationary without explicit parked/off evidence.
    Standing,

    /// Telemetry indicates the vehicle is moving or balancing under ride context.
    Riding,

    /// Telemetry indicates charger-connected/charging state.
    Charging,
}

impl From<RideOperatingState> for RideOperatingStateDto {
    fn from(state: RideOperatingState) -> Self {
        match state {
            RideOperatingState::Unknown => Self::Unknown,
            RideOperatingState::Parked => Self::Parked,
            RideOperatingState::Standing => Self::Standing,
            RideOperatingState::Riding => Self::Riding,
            RideOperatingState::Charging => Self::Charging,
        }
    }
}

/// UniFFI-ready protocol-decoded ride warning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideWarningDto {
    /// No ride warning is active.
    None,

    /// Controller input voltage is below its configured warning threshold.
    LowVoltage,
    /// Controller input voltage is above its configured warning threshold.
    HighVoltage,
    /// Controller MOSFET temperature reached its warning threshold.
    MosfetTemperature,
    /// Motor temperature reached its warning threshold.
    MotorTemperature,
    /// Motor current reached its configured warning threshold.
    Current,

    /// The controller is applying duty-based pushback.
    DutyPushback,
    /// The controller is applying temperature-based pushback.
    TemperaturePushback,
    /// The controller reports active wheel slip.
    Wheelslip,
    /// The controller reports a sensor warning.
    Sensors,
    /// The package reports a low battery warning.
    LowBattery,
    /// The package reports an error warning.
    Error,
}

/// UniFFI-ready reason that the controller stopped balancing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideStopReasonDto {
    /// No stop condition is active.
    None,
    /// Board pitch exceeded the allowed range.
    Pitch,
    /// Board roll exceeded the allowed range.
    Roll,
    /// One half of the footpad switch caused the stop.
    SwitchHalf,
    /// The full footpad switch caused the stop.
    SwitchFull,
    /// Reverse-stop logic caused the stop.
    Reverse,
    /// Quick-stop logic caused the stop.
    QuickStop,
}

impl From<RideStopReason> for RideStopReasonDto {
    fn from(reason: RideStopReason) -> Self {
        match reason {
            RideStopReason::None => Self::None,
            RideStopReason::Pitch => Self::Pitch,
            RideStopReason::Roll => Self::Roll,
            RideStopReason::SwitchHalf => Self::SwitchHalf,
            RideStopReason::SwitchFull => Self::SwitchFull,
            RideStopReason::Reverse => Self::Reverse,
            RideStopReason::QuickStop => Self::QuickStop,
        }
    }
}

impl From<RideWarning> for RideWarningDto {
    fn from(warning: RideWarning) -> Self {
        match warning {
            RideWarning::None => Self::None,
            RideWarning::LowVoltage => Self::LowVoltage,
            RideWarning::HighVoltage => Self::HighVoltage,
            RideWarning::MosfetTemperature => Self::MosfetTemperature,
            RideWarning::MotorTemperature => Self::MotorTemperature,
            RideWarning::Current => Self::Current,
            RideWarning::DutyPushback => Self::DutyPushback,
            RideWarning::TemperaturePushback => Self::TemperaturePushback,
            RideWarning::Wheelslip => Self::Wheelslip,
            RideWarning::Sensors => Self::Sensors,
            RideWarning::LowBattery => Self::LowBattery,
            RideWarning::Error => Self::Error,
        }
    }
}

macro_rules! measured_reading_dto {
    ($name:ident, $quantity:ty, $raw:ty, $map:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name {
            /// Fixed-unit value.
            pub value: $raw,

            /// Value source.
            pub source: ValueSourceDto,

            /// Value quality.
            pub quality: ValueQualityDto,

            /// Value verification status.
            pub verification: VerificationStatusDto,
        }

        impl From<Measured<$quantity>> for $name {
            fn from(measured: Measured<$quantity>) -> Self {
                let measured = measured.map_value($map);
                Self {
                    value: measured.value,
                    source: measured.source.into(),
                    quality: measured.quality.into(),
                    verification: measured.verification.into(),
                }
            }
        }
    };
}

measured_reading_dto!(
    SpeedReadingDto,
    Speed,
    i32,
    Speed::as_millimetres_per_second,
    "UniFFI-ready speed reading with provenance."
);
measured_reading_dto!(
    VoltageReadingDto,
    Voltage,
    i32,
    Voltage::as_millivolts,
    "UniFFI-ready voltage reading with provenance."
);
measured_reading_dto!(
    BatteryCurrentReadingDto,
    BatteryCurrent,
    i32,
    BatteryCurrent::as_milliamps,
    "UniFFI-ready battery current reading with provenance."
);
measured_reading_dto!(
    PhaseCurrentReadingDto,
    PhaseCurrent,
    i32,
    PhaseCurrent::as_milliamps,
    "UniFFI-ready phase current reading with provenance."
);
measured_reading_dto!(
    PowerReadingDto,
    Power,
    i64,
    Power::as_milliwatts,
    "UniFFI-ready power reading with provenance."
);
measured_reading_dto!(
    TemperatureReadingDto,
    Temperature,
    i32,
    Temperature::as_millicelsius,
    "UniFFI-ready temperature reading with provenance."
);
measured_reading_dto!(
    DistanceReadingDto,
    Distance,
    u64,
    Distance::as_millimetres,
    "UniFFI-ready distance reading with provenance."
);
measured_reading_dto!(
    AngleReadingDto,
    Angle,
    i32,
    Angle::as_millidegrees,
    "UniFFI-ready angle reading with provenance."
);
measured_reading_dto!(
    BatteryLevelReadingDto,
    BatteryLevel,
    u8,
    BatteryLevel::as_percent,
    "UniFFI-ready battery level reading with provenance."
);
measured_reading_dto!(
    DutyCycleReadingDto,
    DutyCycle,
    i16,
    DutyCycle::as_permille,
    "UniFFI-ready duty-cycle reading with provenance."
);

/// UniFFI-ready firmware or protocol version component with provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionComponentDto {
    /// Version component value.
    pub value: u16,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<Measured<u16>> for VersionComponentDto {
    fn from(measured: Measured<u16>) -> Self {
        Self {
            value: measured.value,
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

macro_rules! measured_reading_from_i32 {
    ($name:ident) => {
        impl From<Measured<i32>> for $name {
            fn from(measured: Measured<i32>) -> Self {
                Self {
                    value: measured.value,
                    source: measured.source.into(),
                    quality: measured.quality.into(),
                    verification: measured.verification.into(),
                }
            }
        }
    };
}

measured_reading_from_i32!(SpeedReadingDto);
measured_reading_from_i32!(VoltageReadingDto);
measured_reading_from_i32!(BatteryCurrentReadingDto);
measured_reading_from_i32!(PhaseCurrentReadingDto);
measured_reading_from_i32!(TemperatureReadingDto);
measured_reading_from_i32!(AngleReadingDto);

impl From<Measured<i32>> for VersionComponentDto {
    fn from(measured: Measured<i32>) -> Self {
        Self {
            value: u16::try_from(measured.value.clamp(0, i32::from(u16::MAX))).unwrap_or_default(),
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

/// UniFFI-ready charging-state reading with provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargeModeReadingDto {
    /// Charging state value.
    pub value: ChargeModeDto,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<Measured<ChargeMode>> for ChargeModeReadingDto {
    fn from(measured: Measured<ChargeMode>) -> Self {
        Self {
            value: measured.value.into(),
            source: measured.source.into(),
            quality: measured.quality.into(),
            verification: measured.verification.into(),
        }
    }
}

impl BatteryCurrentReadingDto {
    fn from_bms_pack_current(current: BatteryCurrent, currents: BmsPackCurrents) -> Self {
        Self {
            value: current.as_milliamps(),
            source: currents.source.into(),
            quality: currents.quality.into(),
            verification: currents.verification.into(),
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

/// UniFFI-ready interpreted BMS status page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsStatusPage {
    /// Source BMS status page id.
    pub id: BmsStatusPageId,

    /// Battery page kind.
    pub kind: BatteryPageKindDto,

    /// Page interpretation verification.
    pub verification: VerificationStatusDto,
}

impl From<BatteryPageMetadata> for BmsStatusPage {
    fn from(page: BatteryPageMetadata) -> Self {
        Self {
            id: page.into(),
            kind: page.kind.into(),
            verification: page.verification.into(),
        }
    }
}

/// UniFFI-ready BMS status page id.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsStatusPageId {
    /// Protocol namespace that scopes the selector, when available.
    pub namespace: Option<BmsStatusPageNamespace>,

    /// BMS page selector.
    pub selector: u8,
}

impl From<BatteryPageMetadata> for BmsStatusPageId {
    fn from(page: BatteryPageMetadata) -> Self {
        Self {
            namespace: page.tag.map(BmsStatusPageNamespace::from_core),
            selector: page.selector.get(),
        }
    }
}

/// UniFFI-ready BMS status page namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsStatusPageNamespace {
    /// Raw protocol namespace value.
    pub value: u16,
}

impl BmsStatusPageNamespace {
    fn from_core(tag: ProtocolTag) -> Self {
        Self { value: tag.get() }
    }

    /// Converts this status page namespace back into the core protocol tag type.
    #[must_use]
    pub fn into_core(self) -> ProtocolTag {
        ProtocolTag::new(self.value)
    }
}

/// UniFFI-ready battery or BMS readback availability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatteryReadbackAvailabilityDto {
    /// Battery or BMS data was reported by the device.
    Available,

    /// Battery or BMS data is expected but was not reported.
    Unavailable,

    /// Battery or BMS data is not supported for this device/profile.
    Unsupported,
}

impl From<BatteryReadbackAvailability> for BatteryReadbackAvailabilityDto {
    fn from(availability: BatteryReadbackAvailability) -> Self {
        match availability {
            BatteryReadbackAvailability::Available => Self::Available,
            BatteryReadbackAvailability::Unavailable => Self::Unavailable,
            BatteryReadbackAvailability::Unsupported => Self::Unsupported,
        }
    }
}

/// UniFFI-ready battery or BMS readback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatteryReadbackDto {
    /// Whether battery or BMS data is available for display.
    pub availability: BatteryReadbackAvailabilityDto,

    /// Battery or BMS page, when available.
    pub page: Option<BatteryInfoDto>,
}

impl From<BatteryReadback> for BatteryReadbackDto {
    fn from(readback: BatteryReadback) -> Self {
        Self {
            availability: readback.availability().into(),
            page: readback.page().cloned().map(Into::into),
        }
    }
}

/// UniFFI-ready battery or BMS response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatteryInfoDto {
    /// Page metadata for this battery response.
    pub page: BmsStatusPage,

    /// Pack or input voltage in millivolts.
    pub voltage: Option<VoltageReadingDto>,

    /// Pack or battery current in milliamps.
    pub current: Option<BatteryCurrentReadingDto>,

    /// First page-specific BMS pack current in milliamps.
    pub bms_pack_current_0: Option<BatteryCurrentReadingDto>,

    /// Second page-specific BMS pack current in milliamps.
    pub bms_pack_current_1: Option<BatteryCurrentReadingDto>,

    /// Battery level reported by the device.
    pub level_reported: Option<BatteryLevelReadingDto>,

    /// Battery level estimated by Cutout.
    pub level_estimated: Option<BatteryLevelReadingDto>,

    /// Battery or BMS temperature in millicelsius.
    pub temperature: Option<TemperatureReadingDto>,

    /// Page-specific BMS temperature values in millicelsius.
    pub temperatures: Vec<Option<TemperatureReadingDto>>,

    /// Page-specific cell or cell-group voltage values in millivolts.
    pub cell_voltages: Vec<VoltageReadingDto>,

    /// Raw battery or BMS state field.
    pub raw_state: Option<RawFieldValueDto>,
}

impl From<BatteryPagePayload> for BatteryInfoDto {
    fn from(payload: BatteryPagePayload) -> Self {
        let battery = payload.battery();
        let temperatures = payload
            .temperatures()
            .into_iter()
            .map(|measured| measured.map(Into::into))
            .collect();
        let cell_voltages = match &payload {
            BatteryPagePayload::CellVoltage(page) => page
                .cell_voltages
                .iter()
                .copied()
                .map(Measured::reported)
                .map(Into::into)
                .collect(),
            BatteryPagePayload::Temperature(_) | BatteryPagePayload::Raw(_) => Vec::new(),
        };
        Self::from_payload_parts(
            payload.page(),
            battery,
            payload.bms_pack_currents(),
            temperatures,
            cell_voltages,
        )
    }
}

impl BatteryInfoDto {
    fn from_payload_parts(
        page: BatteryPageMetadata,
        battery: BatteryInfo,
        bms_pack_currents: Option<BmsPackCurrents>,
        temperatures: Vec<Option<TemperatureReadingDto>>,
        cell_voltages: Vec<VoltageReadingDto>,
    ) -> Self {
        Self {
            page: page.into(),
            voltage: battery.voltage.map(Into::into),
            current: battery.current.map(Into::into),
            bms_pack_current_0: bms_pack_currents.map(|currents| {
                BatteryCurrentReadingDto::from_bms_pack_current(currents.current_0(), currents)
            }),
            bms_pack_current_1: bms_pack_currents.map(|currents| {
                BatteryCurrentReadingDto::from_bms_pack_current(currents.current_1(), currents)
            }),
            level_reported: battery.level_reported.map(Into::into),
            level_estimated: battery.level_estimated.map(Into::into),
            temperature: battery.temperature.map(Into::into),
            temperatures,
            cell_voltages,
            raw_state: battery.raw_state.map(Into::into),
        }
    }
}

/// UniFFI-ready firmware or protocol version response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareInfoDto {
    /// Protocol version, when reported.
    pub protocol_version: Option<VersionComponentDto>,

    /// Firmware major version, when reported.
    pub firmware_major: Option<VersionComponentDto>,

    /// Firmware minor version, when reported.
    pub firmware_minor: Option<VersionComponentDto>,

    /// Firmware patch version, when reported.
    pub firmware_patch: Option<VersionComponentDto>,

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
    /// Whether the requested settings readback is available for display.
    pub availability: SettingsReadbackAvailabilityDto,

    /// Present settings entries.
    pub entries: Vec<SettingsEntryDto>,
}

/// UniFFI-ready settings readback availability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsReadbackAvailabilityDto {
    /// Settings were reported by the device.
    Available,

    /// Settings are expected for this device/profile but were not reported.
    Unavailable,

    /// Settings are not supported for this device/profile.
    Unsupported,
}

impl From<SettingsReadbackAvailability> for SettingsReadbackAvailabilityDto {
    fn from(availability: SettingsReadbackAvailability) -> Self {
        match availability {
            SettingsReadbackAvailability::Available => Self::Available,
            SettingsReadbackAvailability::Unavailable => Self::Unavailable,
            SettingsReadbackAvailability::Unsupported => Self::Unsupported,
        }
    }
}

impl From<SettingsReadback> for SettingsReadbackDto {
    fn from(settings: SettingsReadback) -> Self {
        Self {
            availability: settings.availability().into(),
            entries: settings
                .entries()
                .into_iter()
                .flatten()
                .map(Into::into)
                .collect(),
        }
    }
}

/// UniFFI-ready fault-history readback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultHistoryReadbackDto {
    /// Whether the requested fault-history readback is available for display.
    pub availability: FaultHistoryAvailabilityDto,

    /// Last reported fault, if any.
    pub last_fault: Option<FaultHistoryEntryDto>,

    /// Distance since the last fault, if reported separately.
    pub since_distance: Option<DistanceReadingDto>,
}

impl From<FaultHistoryReadback> for FaultHistoryReadbackDto {
    fn from(readback: FaultHistoryReadback) -> Self {
        Self {
            availability: readback.availability().into(),
            last_fault: readback.last_fault().map(Into::into),
            since_distance: readback.since_distance().map(Into::into),
        }
    }
}

/// UniFFI-ready fault-history availability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultHistoryAvailabilityDto {
    /// Fault history was reported by the device.
    Available,

    /// Fault history is expected for this device/profile but was not reported.
    Unavailable,

    /// Fault history is not supported for this device/profile.
    Unsupported,
}

impl From<FaultHistoryAvailability> for FaultHistoryAvailabilityDto {
    fn from(availability: FaultHistoryAvailability) -> Self {
        match availability {
            FaultHistoryAvailability::Available => Self::Available,
            FaultHistoryAvailability::Unavailable => Self::Unavailable,
            FaultHistoryAvailability::Unsupported => Self::Unsupported,
        }
    }
}

/// UniFFI-ready last-fault entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultHistoryEntryDto {
    /// Protocol-specific fault code without proven semantic mapping.
    pub code: FaultCodeDto,

    /// Value source.
    pub source: ValueSourceDto,

    /// Value quality.
    pub quality: ValueQualityDto,

    /// Value verification status.
    pub verification: VerificationStatusDto,
}

impl From<FaultHistoryEntry> for FaultHistoryEntryDto {
    fn from(entry: FaultHistoryEntry) -> Self {
        Self {
            code: entry.code.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

/// UniFFI-ready protocol-specific fault code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultCodeDto {
    /// Raw protocol field/value pair for an unknown fault code.
    pub raw: RawFieldValueDto,
}

impl From<FaultCode> for FaultCodeDto {
    fn from(code: FaultCode) -> Self {
        Self {
            raw: code.raw.into(),
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

/// UniFFI-ready raw telemetry readback DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawTelemetryReadbackDto {
    /// Present raw telemetry fields.
    pub fields: Vec<RawFieldValueDto>,
    /// Present protocol-native floating fields, stored as exact IEEE-754 bits.
    pub float_fields: Vec<RawFloatFieldValueDto>,
}

/// UniFFI-ready protocol-native floating field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawFloatFieldValueDto {
    /// Protocol-family field identifier.
    pub id: u16,
    /// Exact IEEE-754 bits.
    pub value_bits: u32,
}

impl From<RawTelemetryReadback> for RawTelemetryReadbackDto {
    fn from(raw: RawTelemetryReadback) -> Self {
        Self {
            fields: raw.fields.into_iter().map(Into::into).collect(),
            float_fields: raw
                .float_fields
                .into_iter()
                .map(|field| RawFloatFieldValueDto {
                    id: field.id,
                    value_bits: field.value_bits,
                })
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
        monotonic_ms: MonotonicMillisDto,

        /// Maximum write payload length reported by the host, when known.
        max_write_len: Option<TransportWriteLimitDto>,
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
        monotonic_ms: MonotonicMillisDto,
    },

    /// Timer tick supplied by the host.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicMillisDto,
    },

    /// Command requested by the host application.
    Command(DeviceCommandDto),
}

impl From<SessionInput<'_>> for SessionInputDto {
    fn from(input: SessionInput<'_>) -> Self {
        match input {
            SessionInput::LinkUp(link) => Self::LinkUp {
                monotonic_ms: MonotonicMillisDto::from_core(link.monotonic_ms),
                max_write_len: link.max_write_len.map(TransportWriteLimitDto::from_core),
            },
            SessionInput::LinkDown => Self::LinkDown,
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => Self::Notification {
                channel: channel.as_bytes(),
                bytes: bytes.to_vec(),
                monotonic_ms: MonotonicMillisDto::from_core(monotonic_ms),
            },
            SessionInput::Tick { monotonic_ms } => Self::Tick {
                monotonic_ms: MonotonicMillisDto::from_core(monotonic_ms),
            },
            SessionInput::Command(command) => Self::Command(command.into()),
        }
    }
}

impl SessionInputDto {
    /// Borrows this owned DTO as a core session input for immediate reactor use.
    #[must_use]
    pub fn as_session_input(&self) -> SessionInput<'_> {
        match self {
            Self::LinkUp {
                monotonic_ms,
                max_write_len,
            } => SessionInput::LinkUp(crate::LinkInfo {
                monotonic_ms: (*monotonic_ms).into_core(),
                max_write_len: max_write_len.map(TransportWriteLimitDto::into_core),
            }),
            Self::LinkDown => SessionInput::LinkDown,
            Self::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => SessionInput::Notification {
                channel: crate::GattChannel::from_bytes(*channel),
                bytes: bytes.as_slice(),
                monotonic_ms: (*monotonic_ms).into_core(),
            },
            Self::Tick { monotonic_ms } => SessionInput::Tick {
                monotonic_ms: (*monotonic_ms).into_core(),
            },
            Self::Command(command) => SessionInput::Command((*command).into()),
        }
    }
}

/// UniFFI-ready session output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionOutputDto {
    /// Transport action to execute outside the protocol engine.
    Transport(TransportActionDto),

    /// Read-only response emitted by a protocol session.
    ReadOnly(ReadOnlyOutput),

    /// Semantic event to report to the application.
    Event(SessionEventDto),

    /// Parser-level notification ingest outcome.
    NotificationIngest(NotificationIngestOutcomeDto),
}

impl From<SessionOutput> for SessionOutputDto {
    fn from(output: SessionOutput) -> Self {
        match output {
            SessionOutput::Transport(action) => Self::Transport(action.into()),
            SessionOutput::Event(DeviceEvent::ReadOnlyResponse(response)) => {
                Self::ReadOnly(response.into())
            }
            SessionOutput::Event(event) => match SessionEventDto::from_event(event) {
                SessionEventProjection::Event(event) => Self::Event(event),
                SessionEventProjection::ReadOnly(response) => Self::ReadOnly(response.into()),
            },
            SessionOutput::NotificationIngest(outcome) => Self::NotificationIngest(outcome.into()),
        }
    }
}

/// UniFFI-ready notification ingest outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationIngestOutcomeDto {
    /// Notification produced semantic protocol events.
    SemanticEvents {
        /// Notification evidence shared by ingest outcomes.
        notification: NotificationEvidenceDto,

        /// Number of semantic events emitted from this notification.
        event_count: SemanticEventCountDto,
    },

    /// Notification bytes are a valid partial frame.
    BufferedFragment(NotificationEvidenceDto),

    /// Notification was recognized but failed parser validation.
    ParserDiagnostic {
        /// Notification evidence shared by ingest outcomes.
        notification: NotificationEvidenceDto,

        /// Parser error observed while ingesting the notification.
        error: ParserErrorDto,
    },

    /// Notification carried a known reserved/opaque payload.
    KnownReserved {
        /// Notification evidence shared by ingest outcomes.
        notification: NotificationEvidenceDto,

        /// Reserved payload evidence.
        payload: ReservedPayloadEvidenceDto,
    },

    /// Notification reached a known parser gap.
    ParserGap {
        /// Notification evidence shared by ingest outcomes.
        notification: NotificationEvidenceDto,

        /// Parser gap evidence.
        gap: ParserGapEvidenceDto,
    },

    /// Notification was explicitly ignored.
    Ignored {
        /// Ignored-notification evidence.
        evidence: IgnoredNotificationEvidenceDto,

        /// Reason the notification was ignored.
        reason: IgnoredNotificationReasonDto,
    },
}

impl From<NotificationIngestOutcome> for NotificationIngestOutcomeDto {
    fn from(outcome: NotificationIngestOutcome) -> Self {
        match outcome {
            NotificationIngestOutcome::SemanticEvents {
                notification,
                event_count,
            } => Self::SemanticEvents {
                notification: notification.into(),
                event_count: SemanticEventCountDto::from_core(event_count),
            },
            NotificationIngestOutcome::BufferedFragment(notification) => {
                Self::BufferedFragment(notification.into())
            }
            NotificationIngestOutcome::ParserDiagnostic {
                notification,
                error,
            } => Self::ParserDiagnostic {
                notification: notification.into(),
                error: error.into(),
            },
            NotificationIngestOutcome::KnownReserved {
                notification,
                payload,
            } => Self::KnownReserved {
                notification: notification.into(),
                payload: payload.into(),
            },
            NotificationIngestOutcome::ParserGap { notification, gap } => Self::ParserGap {
                notification: notification.into(),
                gap: gap.into(),
            },
            NotificationIngestOutcome::Ignored { evidence, reason } => Self::Ignored {
                evidence: evidence.into(),
                reason: reason.into(),
            },
        }
    }
}

/// UniFFI-ready notification evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationEvidenceDto {
    /// Protocol family that accepted or classified the notification.
    pub family: ProtocolFamilyDto,

    /// GATT channel UUID bytes.
    pub channel: [u8; 16],

    /// Notification payload length.
    pub len: NotificationByteLenDto,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MonotonicMillisDto,
}

impl From<NotificationEvidence> for NotificationEvidenceDto {
    fn from(evidence: NotificationEvidence) -> Self {
        Self {
            family: evidence.family.into(),
            channel: evidence.channel.as_bytes(),
            len: NotificationByteLenDto::from_core(evidence.len),
            monotonic_ms: MonotonicMillisDto::from_core(evidence.monotonic_ms),
        }
    }
}

/// UniFFI-ready ignored notification evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IgnoredNotificationEvidenceDto {
    /// Protocol family when classification got that far.
    pub family: Option<ProtocolFamilyDto>,

    /// GATT channel UUID bytes.
    pub channel: [u8; 16],

    /// Notification payload length.
    pub len: NotificationByteLenDto,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MonotonicMillisDto,

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,
}

impl From<IgnoredNotificationEvidence> for IgnoredNotificationEvidenceDto {
    fn from(evidence: IgnoredNotificationEvidence) -> Self {
        Self {
            family: evidence.family.map(Into::into),
            channel: evidence.channel.as_bytes(),
            len: NotificationByteLenDto::from_core(evidence.len),
            monotonic_ms: MonotonicMillisDto::from_core(evidence.monotonic_ms),
            retained_payload: evidence.retained_payload.as_slice().to_vec(),
        }
    }
}

/// UniFFI-ready ignored notification reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoredNotificationReasonDto {
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

impl From<IgnoredNotificationReason> for IgnoredNotificationReasonDto {
    fn from(reason: IgnoredNotificationReason) -> Self {
        match reason {
            IgnoredNotificationReason::WrongChannel => Self::WrongChannel,
            IgnoredNotificationReason::UnsupportedFamily => Self::UnsupportedFamily,
            IgnoredNotificationReason::UnsupportedChannel => Self::UnsupportedChannel,
            IgnoredNotificationReason::AcceptedButUnmapped => Self::AcceptedButUnmapped,
            IgnoredNotificationReason::SeekingFrameBoundary => Self::SeekingFrameBoundary,
            IgnoredNotificationReason::IntentionallyDropped => Self::IntentionallyDropped,
        }
    }
}

/// UniFFI-ready protocol family identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolFamilyDto {
    /// Veteran/LeaperKim/NOSFET frame family.
    VeteranLeaperkimNosfet,

    /// Begode/Gotway frame family.
    BegodeGotway,

    /// VESC UART/CAN-derived family.
    Vesc,
}

impl From<ProtocolFamily> for ProtocolFamilyDto {
    fn from(family: ProtocolFamily) -> Self {
        match family {
            ProtocolFamily::VeteranLeaperkimNosfet => Self::VeteranLeaperkimNosfet,
            ProtocolFamily::BegodeGotway => Self::BegodeGotway,
            ProtocolFamily::Vesc => Self::Vesc,
        }
    }
}

/// UniFFI-ready parser error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParserErrorDto {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame {
        /// Claimed or observed frame length.
        claimed: ParserFrameLenDto,

        /// Configured maximum accepted frame length.
        max: ParserFrameLenDto,
    },

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before the expected data arrived.
    Timeout {
        /// Elapsed monotonic milliseconds.
        elapsed_ms: MonotonicMillisDto,

        /// Timeout threshold in monotonic milliseconds.
        timeout_ms: MonotonicMillisDto,
    },

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

impl From<ParserError> for ParserErrorDto {
    fn from(error: ParserError) -> Self {
        match error {
            ParserError::OversizedFrame { claimed, max } => Self::OversizedFrame {
                claimed: ParserFrameLenDto::from_core(claimed),
                max: ParserFrameLenDto::from_core(max),
            },
            ParserError::BadChecksum => Self::BadChecksum,
            ParserError::MalformedFrame => Self::MalformedFrame,
            ParserError::Timeout {
                elapsed_ms,
                timeout_ms,
            } => Self::Timeout {
                elapsed_ms: MonotonicMillisDto::from_core(elapsed_ms),
                timeout_ms: MonotonicMillisDto::from_core(timeout_ms),
            },
            ParserError::UnmatchedReply => Self::UnmatchedReply,
        }
    }
}

/// UniFFI-ready reserved payload evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReservedPayloadEvidenceDto {
    /// Selector byte when the family has one.
    pub selector: Option<u8>,

    /// Tag byte when the family has one.
    pub tag: Option<u16>,

    /// Reserved payload body length.
    pub body_len: PayloadBodyLenDto,

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,

    /// Evidence verification status.
    pub verification: VerificationStatusDto,
}

impl From<ReservedPayloadEvidence> for ReservedPayloadEvidenceDto {
    fn from(evidence: ReservedPayloadEvidence) -> Self {
        Self {
            selector: evidence
                .classifier
                .selector_value()
                .map(super::ProtocolSelector::get),
            tag: evidence.classifier.tag_value().map(ProtocolTag::get),
            body_len: PayloadBodyLenDto::from_core(evidence.body_len),
            retained_payload: evidence.retained_payload.as_slice().to_vec(),
            verification: evidence.verification.into(),
        }
    }
}

/// UniFFI-ready parser gap evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParserGapEvidenceDto {
    /// Selector byte when the family has one.
    pub selector: Option<u8>,

    /// Tag byte when the family has one.
    pub tag: Option<u16>,

    /// Unparsed body length.
    pub body_len: PayloadBodyLenDto,

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,
}

impl From<ParserGapEvidence> for ParserGapEvidenceDto {
    fn from(evidence: ParserGapEvidence) -> Self {
        Self {
            selector: evidence
                .classifier
                .selector_value()
                .map(super::ProtocolSelector::get),
            tag: evidence.classifier.tag_value().map(ProtocolTag::get),
            body_len: PayloadBodyLenDto::from_core(evidence.body_len),
            retained_payload: evidence.retained_payload.as_slice().to_vec(),
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
        monotonic_ms: MonotonicMillisDto,

        /// Maximum write payload length reported by the host, when known.
        max_write_len: Option<TransportWriteLimitDto>,
    },

    /// Link-down event accepted by the session.
    LinkDown,

    /// Tick event accepted by the session.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicMillisDto,
    },

    /// Telemetry update emitted by a protocol session.
    Telemetry(TelemetryDeltaDto),

    /// Control command refused before transport writes.
    ControlRefusal(ControlRefusalDto),

    /// Parser diagnostics emitted by a protocol session.
    Diagnostics(ParserDiagnosticsDto),

    /// Detailed parser diagnostic error emitted by a protocol session.
    DiagnosticError(DiagnosticErrorDto),
}

enum SessionEventProjection {
    Event(SessionEventDto),
    ReadOnly(ReadOnlyResponse),
}

impl SessionEventDto {
    fn from_event(event: DeviceEvent) -> SessionEventProjection {
        match event {
            DeviceEvent::LinkUp(link) => SessionEventProjection::Event(Self::LinkUp {
                monotonic_ms: MonotonicMillisDto::from_core(link.monotonic_ms),
                max_write_len: link.max_write_len.map(TransportWriteLimitDto::from_core),
            }),
            DeviceEvent::LinkDown => SessionEventProjection::Event(Self::LinkDown),
            DeviceEvent::Tick { monotonic_ms } => SessionEventProjection::Event(Self::Tick {
                monotonic_ms: MonotonicMillisDto::from_core(monotonic_ms),
            }),
            DeviceEvent::Telemetry(delta) => {
                SessionEventProjection::Event(Self::Telemetry(delta.into()))
            }
            DeviceEvent::ControlRefusal(refusal) => {
                SessionEventProjection::Event(Self::ControlRefusal(refusal.into()))
            }
            DeviceEvent::Diagnostics(diagnostics) => {
                SessionEventProjection::Event(Self::Diagnostics(diagnostics.into()))
            }
            DeviceEvent::DiagnosticError(error) => {
                SessionEventProjection::Event(Self::DiagnosticError(error.into()))
            }
            DeviceEvent::ReadOnlyResponse(response) => SessionEventProjection::ReadOnly(response),
        }
    }
}

/// UniFFI-ready telemetry delta.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryDeltaDto {
    /// Host monotonic timestamp for this update.
    pub at_ms: MonotonicMillisDto,

    /// Reported or calculated speed in millimeters per second.
    pub speed: Option<SpeedReadingDto>,

    /// Reported or measured input voltage in millivolts.
    pub voltage: Option<VoltageReadingDto>,

    /// Battery/input current in milliamps.
    pub battery_current: Option<BatteryCurrentReadingDto>,

    /// Device charging state decoded from protocol-specific status fields.
    pub charge_mode: Option<ChargeModeReadingDto>,

    /// Ride operating state decoded from protocol-specific status fields.
    pub operating_state: Option<RideOperatingStateDto>,

    /// Motor/phase current in milliamps.
    pub motor_current: Option<PhaseCurrentReadingDto>,

    /// Electrical power in milliwatts.
    pub power: Option<PowerReadingDto>,

    /// Controller temperature in millicelsius.
    pub controller_temperature: Option<TemperatureReadingDto>,

    /// Motor temperature in millicelsius.
    pub motor_temperature: Option<TemperatureReadingDto>,

    /// Battery temperature in millicelsius.
    pub battery_temperature: Option<TemperatureReadingDto>,

    /// PWM duty in permille.
    pub pwm: Option<DutyCycleReadingDto>,

    /// Total or trip distance in millimeters.
    pub distance: Option<DistanceReadingDto>,

    /// Pitch in millidegrees.
    pub pitch: Option<AngleReadingDto>,

    /// Balance-loop target angle in millidegrees.
    pub balance_angle: Option<AngleReadingDto>,

    /// Roll in millidegrees.
    pub roll: Option<AngleReadingDto>,

    /// Footpad/sensor state.
    pub footpad: Option<FootpadTelemetryDto>,

    /// Battery level reported by the device.
    pub battery_level_reported: Option<BatteryLevelReadingDto>,

    /// Battery level estimated by Cutout.
    pub battery_level_estimated: Option<BatteryLevelReadingDto>,
}

impl From<TelemetryDelta> for TelemetryDeltaDto {
    fn from(delta: TelemetryDelta) -> Self {
        Self {
            at_ms: MonotonicMillisDto::from_core(delta.at_ms),
            speed: delta.speed.map(Into::into),
            voltage: delta.voltage.map(Into::into),
            battery_current: delta.battery_current.map(Into::into),
            charge_mode: delta.charge_mode.map(Into::into),
            operating_state: delta.operating_state.map(Into::into),
            motor_current: delta.motor_current.map(Into::into),
            power: delta.power.map(Into::into),
            controller_temperature: delta.controller_temperature.map(Into::into),
            motor_temperature: delta.motor_temperature.map(Into::into),
            battery_temperature: delta.battery_temperature.map(Into::into),
            pwm: delta.pwm.map(Into::into),
            distance: delta.distance.map(Into::into),
            pitch: delta.pitch.map(Into::into),
            balance_angle: delta.balance_angle.map(Into::into),
            roll: delta.roll.map(Into::into),
            footpad: delta.footpad.map(Into::into),
            battery_level_reported: delta.battery_level_reported.map(Into::into),
            battery_level_estimated: delta.battery_level_estimated.map(Into::into),
        }
    }
}

/// UniFFI-ready footpad telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FootpadContactStateDto {
    /// Neither footpad contact is active.
    None,

    /// Only the left footpad contact is active.
    Left,

    /// Only the right footpad contact is active.
    Right,

    /// Both footpad contacts are active.
    Both,
}

impl From<FootpadContactState> for FootpadContactStateDto {
    fn from(state: FootpadContactState) -> Self {
        match state {
            FootpadContactState::None => Self::None,
            FootpadContactState::Left => Self::Left,
            FootpadContactState::Right => Self::Right,
            FootpadContactState::Both => Self::Both,
        }
    }
}

/// UniFFI-ready footpad telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FootpadTelemetryDto {
    /// Protocol-specific footpad state bitfield/nibble.
    pub state: u8,

    /// Semantically decoded contact state when the protocol defines one.
    pub contact_state: Option<FootpadContactStateDto>,

    /// First footpad ADC reading in protocol units, scaled by 1000.
    pub adc1_milliunits: Option<i32>,

    /// Second footpad ADC reading in protocol units, scaled by 1000.
    pub adc2_milliunits: Option<i32>,
}

impl From<FootpadTelemetry> for FootpadTelemetryDto {
    fn from(footpad: FootpadTelemetry) -> Self {
        Self {
            state: footpad.state,
            contact_state: footpad.contact_state.map(Into::into),
            adc1_milliunits: footpad.adc1_milliunits,
            adc2_milliunits: footpad.adc2_milliunits,
        }
    }
}

/// UniFFI-ready telemetry snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetrySnapshotDto {
    /// Host monotonic timestamp for the latest update, when known.
    pub at_ms: Option<MonotonicMillisDto>,

    /// Reported or calculated speed in millimeters per second.
    pub speed: Option<SpeedReadingDto>,

    /// Reported or measured input voltage in millivolts.
    pub voltage: Option<VoltageReadingDto>,

    /// Battery/input current in milliamps.
    pub battery_current: Option<BatteryCurrentReadingDto>,

    /// Device charging state decoded from protocol-specific status fields.
    pub charge_mode: Option<ChargeModeReadingDto>,

    /// Ride operating state decoded from protocol-specific status fields.
    pub operating_state: Option<RideOperatingStateDto>,

    /// Protocol-decoded ride warning.
    pub ride_warning: Option<RideWarningDto>,

    /// Protocol-decoded reason the controller stopped balancing.
    pub ride_stop_reason: Option<RideStopReasonDto>,

    /// Motor/phase current in milliamps.
    pub motor_current: Option<PhaseCurrentReadingDto>,

    /// Electrical power in milliwatts.
    pub power: Option<PowerReadingDto>,

    /// Controller temperature in millicelsius.
    pub controller_temperature: Option<TemperatureReadingDto>,

    /// Motor temperature in millicelsius.
    pub motor_temperature: Option<TemperatureReadingDto>,

    /// Battery temperature in millicelsius.
    pub battery_temperature: Option<TemperatureReadingDto>,

    /// PWM duty in permille.
    pub pwm: Option<DutyCycleReadingDto>,

    /// Total or trip distance in millimeters.
    pub distance: Option<DistanceReadingDto>,

    /// Pitch in millidegrees.
    pub pitch: Option<AngleReadingDto>,

    /// Balance-loop target angle in millidegrees.
    pub balance_angle: Option<AngleReadingDto>,

    /// Roll in millidegrees.
    pub roll: Option<AngleReadingDto>,

    /// Footpad/sensor state.
    pub footpad: Option<FootpadTelemetryDto>,

    /// Battery level reported by the device.
    pub battery_level_reported: Option<BatteryLevelReadingDto>,

    /// Battery level estimated by Cutout.
    pub battery_level_estimated: Option<BatteryLevelReadingDto>,
}

impl From<TelemetrySnapshot> for TelemetrySnapshotDto {
    fn from(snapshot: TelemetrySnapshot) -> Self {
        Self {
            at_ms: snapshot.at_ms.map(MonotonicMillisDto::from_core),
            speed: snapshot.speed.map(Into::into),
            voltage: snapshot.voltage.map(Into::into),
            battery_current: snapshot.battery_current.map(Into::into),
            charge_mode: snapshot.charge_mode.map(Into::into),
            operating_state: snapshot.operating_state.map(Into::into),
            ride_warning: snapshot.ride_warning.map(Into::into),
            ride_stop_reason: snapshot.ride_stop_reason.map(Into::into),
            motor_current: snapshot.motor_current.map(Into::into),
            power: snapshot.power.map(Into::into),
            controller_temperature: snapshot.controller_temperature.map(Into::into),
            motor_temperature: snapshot.motor_temperature.map(Into::into),
            battery_temperature: snapshot.battery_temperature.map(Into::into),
            pwm: snapshot.pwm.map(Into::into),
            distance: snapshot.distance.map(Into::into),
            pitch: snapshot.pitch.map(Into::into),
            balance_angle: snapshot.balance_angle.map(Into::into),
            roll: snapshot.roll.map(Into::into),
            footpad: snapshot.footpad.map(Into::into),
            battery_level_reported: snapshot.battery_level_reported.map(Into::into),
            battery_level_estimated: snapshot.battery_level_estimated.map(Into::into),
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
    pub dropped_bytes: ParserDroppedBytesDto,

    /// Parser resynchronization attempts.
    pub resyncs: ParserDiagnosticCountDto,

    /// Frames rejected because their checksum did not match.
    pub bad_checksums: ParserDiagnosticCountDto,

    /// Parser deadlines that elapsed before expected data arrived.
    pub timeouts: ParserDiagnosticCountDto,

    /// Frames rejected because they exceeded parser limits.
    pub oversized_frames: ParserDiagnosticCountDto,

    /// Frames rejected because their structure was invalid.
    pub malformed_frames: ParserDiagnosticCountDto,

    /// Replies that could not be matched to an in-flight request.
    pub unmatched_replies: ParserDiagnosticCountDto,
}

impl From<ParserDiagnostics> for ParserDiagnosticsDto {
    fn from(diagnostics: ParserDiagnostics) -> Self {
        Self {
            dropped_bytes: ParserDroppedBytesDto::from_core(diagnostics.dropped_bytes),
            resyncs: ParserDiagnosticCountDto::from_core(diagnostics.resyncs),
            bad_checksums: ParserDiagnosticCountDto::from_core(diagnostics.bad_checksums),
            timeouts: ParserDiagnosticCountDto::from_core(diagnostics.timeouts),
            oversized_frames: ParserDiagnosticCountDto::from_core(diagnostics.oversized_frames),
            malformed_frames: ParserDiagnosticCountDto::from_core(diagnostics.malformed_frames),
            unmatched_replies: ParserDiagnosticCountDto::from_core(diagnostics.unmatched_replies),
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
    pub claimed_len: Option<ParserFrameLenDto>,

    /// Configured maximum frame length for oversized-frame errors.
    pub max_len: Option<ParserFrameLenDto>,

    /// Elapsed monotonic milliseconds for timeout errors.
    pub elapsed_ms: Option<MonotonicMillisDto>,

    /// Timeout threshold in monotonic milliseconds for timeout errors.
    pub timeout_ms: Option<MonotonicMillisDto>,
}

impl From<DiagnosticError> for DiagnosticErrorDto {
    fn from(error: DiagnosticError) -> Self {
        Self {
            kind: error.kind.into(),
            claimed_len: error.claimed_len.map(ParserFrameLenDto::from_core),
            max_len: error.max_len.map(ParserFrameLenDto::from_core),
            elapsed_ms: error.elapsed_ms.map(MonotonicMillisDto::from_core),
            timeout_ms: error.timeout_ms.map(MonotonicMillisDto::from_core),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        BatteryCurrent, BatteryInfo, BatteryLevel, BatteryPageMetadata, BatteryPagePayload,
        BatteryReadback, DeviceCommand, DeviceEvent, DiagnosticDetail, DiagnosticReadback,
        DiagnosticSeverity, Distance, DutyCycle, FirmwareInfo, GattChannel, LinkInfo, Measured,
        ProtocolSelector, RawFieldValue, ReadOnlyResponse, RideOperatingState, SessionInput,
        SessionOutput, SettingsEntry, SettingsReadback, Speed, TelemetrySnapshot, Temperature,
        TransportAction, ValueQuality, ValueSource, VerificationStatus, Voltage, WriteMode,
        WritePayload,
    };

    use super::*;

    const fn ms(value: u64) -> MonotonicMillisDto {
        MonotonicMillisDto {
            milliseconds: value,
        }
    }

    const fn write_len(value: u16) -> TransportWriteLimit {
        TransportWriteLimit::from_bytes(value)
    }

    const fn write_len_dto(value: u16) -> TransportWriteLimitDto {
        TransportWriteLimitDto { bytes: value }
    }

    #[test]
    fn read_only_battery_output_preserves_page_and_unknown_values() {
        let response = ReadOnlyResponse::Battery(BatteryReadback::available(
            BatteryPagePayload::raw(
                BatteryPageMetadata::raw(
                    ProtocolSelector::new(8),
                    VerificationStatus::SourceVerified,
                ),
                BatteryInfo {
                    voltage: Some(Measured::reported(Voltage::from_millivolts(80_000))),
                    current: None,
                    level_reported: Some(Measured::reported(BatteryLevel::from_percent(72))),
                    level_estimated: None,
                    temperature: Some(Measured::reported(Temperature::from_millicelsius(25_000))),
                    raw_state: Some(RawFieldValue::new(0x0008, 0x55aa)),
                },
            )
            .with_bms_pack_currents(BmsPackCurrents::reported(
                BatteryCurrent::from_milliamps(-1_230),
                BatteryCurrent::from_milliamps(450),
            )),
        ));

        let output = ReadOnlyOutput::from(response);

        assert_eq!(output.command_kind, CommandKindDto::RequestBatteryInfo);
        let ReadOnlyOutputPayload::Battery(readback) = output.payload else {
            panic!("expected battery DTO");
        };
        assert_eq!(
            readback.availability,
            BatteryReadbackAvailabilityDto::Available
        );
        let battery = readback.page.expect("battery page");
        assert_eq!(battery.page.id.selector, 8);
        assert_eq!(battery.page.kind, BatteryPageKindDto::Raw);
        assert_eq!(
            battery.page.verification,
            VerificationStatusDto::SourceVerified
        );
        assert_eq!(battery.voltage.expect("voltage").value, 80_000);
        assert_eq!(battery.current, None);
        assert_eq!(
            battery.bms_pack_current_0.expect("first BMS current").value,
            -1_230
        );
        assert_eq!(
            battery
                .bms_pack_current_1
                .expect("second BMS current")
                .value,
            450
        );
        assert_eq!(battery.level_reported.expect("level").value, 72);
        assert_eq!(battery.level_estimated, None);
        assert_eq!(battery.temperature.expect("temperature").value, 25_000);
        assert_eq!(
            battery.raw_state,
            Some(RawFieldValueDto {
                id: 0x0008,
                value: 0x55aa
            })
        );
    }

    #[test]
    fn read_only_battery_output_preserves_unsupported_availability() {
        let output =
            ReadOnlyOutput::from(ReadOnlyResponse::Battery(BatteryReadback::unsupported()));

        assert_eq!(output.command_kind, CommandKindDto::RequestBatteryInfo);
        let ReadOnlyOutputPayload::Battery(readback) = output.payload else {
            panic!("expected battery DTO");
        };
        assert_eq!(
            readback.availability,
            BatteryReadbackAvailabilityDto::Unsupported
        );
        assert_eq!(readback.page, None);
    }

    #[test]
    fn read_only_firmware_output_preserves_optional_fields() {
        let response = ReadOnlyResponse::Firmware(FirmwareInfo {
            protocol_version: Some(Measured::reported(2)),
            firmware_major: Some(Measured::reported(43)),
            firmware_minor: None,
            firmware_patch: Some(Measured::reported(7)),
            build_id: Some(RawFieldValue::new(0x002a, 99)),
        });

        let output = ReadOnlyOutput::from(response);

        assert_eq!(output.command_kind, CommandKindDto::RequestFirmwareInfo);
        let ReadOnlyOutputPayload::Firmware(firmware) = output.payload else {
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
    fn read_only_settings_output_owns_present_entries_only() {
        let response = ReadOnlyResponse::Settings(SettingsReadback::available([
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
        ]));

        let output = ReadOnlyOutput::from(response);

        assert_eq!(output.command_kind, CommandKindDto::RequestSettings);
        let ReadOnlyOutputPayload::Settings(settings) = output.payload else {
            panic!("expected settings DTO");
        };
        assert_eq!(
            settings.availability,
            SettingsReadbackAvailabilityDto::Available
        );
        assert_eq!(settings.entries.len(), 2);
        assert_eq!(settings.entries[0].field.id, 0x0014);
        assert_eq!(settings.entries[0].source, ValueSourceDto::Reported);
        assert_eq!(settings.entries[1].field.value, 45);
        assert_eq!(settings.entries[1].quality, ValueQualityDto::Inferred);
    }

    #[test]
    fn fault_history_output_preserves_structured_unknown_code_and_distance() {
        let readback = FaultHistoryReadback::fault_since(
            FaultHistoryEntry::reported_unknown(FaultCode::unknown(RawFieldValue::new(0x0040, 1))),
            Some(Measured::reported(Distance::from_millimetres(61_456_941))),
        );

        let output = ReadOnlyOutput::from(ReadOnlyResponse::FaultHistory(readback));

        assert_eq!(output.command_kind, CommandKindDto::RequestFaultHistory);
        let ReadOnlyOutputPayload::FaultHistory(dto) = output.payload else {
            panic!("expected fault-history DTO");
        };
        assert_eq!(dto.availability, FaultHistoryAvailabilityDto::Available);
        assert_eq!(
            dto.last_fault.expect("fault").code,
            FaultCodeDto {
                raw: RawFieldValueDto {
                    id: 0x0040,
                    value: 1
                }
            }
        );
        assert_eq!(dto.since_distance.expect("distance").value, 61_456_941);
    }

    #[test]
    fn read_only_diagnostics_output_owns_present_details_only() {
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

        let output = ReadOnlyOutput::from(response);

        assert_eq!(output.command_kind, CommandKindDto::RequestDiagnostics);
        let ReadOnlyOutputPayload::Diagnostics(diagnostics) = output.payload else {
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
    fn read_only_raw_telemetry_output_owns_present_fields_only() {
        let response = ReadOnlyResponse::RawTelemetry(RawTelemetryReadback {
            fields: [
                RawFieldValue::new(0x8001, 989),
                RawFieldValue::new(0x8002, -21_973),
            ]
            .into_iter()
            .collect(),
            float_fields: arrayvec::ArrayVec::new(),
        });

        let output = ReadOnlyOutput::from(response);

        assert_eq!(output.command_kind, CommandKindDto::RequestTelemetry);
        let ReadOnlyOutputPayload::RawTelemetry(raw) = output.payload else {
            panic!("expected raw telemetry DTO");
        };
        assert_eq!(raw.fields.len(), 2);
        assert_eq!(raw.fields[0].id, 0x8001);
        assert_eq!(raw.fields[0].value, 989);
        assert_eq!(raw.fields[1].id, 0x8002);
        assert_eq!(raw.fields[1].value, -21_973);
    }

    #[test]
    fn session_input_dto_owns_notification_bytes_and_commands() {
        let notification = SessionInputDto::from(SessionInput::Notification {
            channel: GattChannel::from_bytes([0xA1; 16]),
            bytes: &[0xde, 0xad, 0xbe, 0xef],
            monotonic_ms: MonotonicTimestamp::new(42),
        });
        let command =
            SessionInputDto::from(SessionInput::Command(DeviceCommand::SetRawMotorCurrent {
                current: PhaseCurrent::from_milliamps(-1_500),
            }));

        assert_eq!(
            notification,
            SessionInputDto::Notification {
                channel: [0xA1; 16],
                bytes: vec![0xde, 0xad, 0xbe, 0xef],
                monotonic_ms: ms(42),
            }
        );
        assert_eq!(
            command,
            SessionInputDto::Command(DeviceCommandDto::SetRawMotorCurrent { current: -1_500 })
        );
    }

    #[test]
    fn session_input_dto_borrows_core_notification_input() {
        let dto = SessionInputDto::Notification {
            channel: [0xA1; 16],
            bytes: vec![0xde, 0xad, 0xbe, 0xef],
            monotonic_ms: ms(42),
        };

        assert_eq!(
            dto.as_session_input(),
            SessionInput::Notification {
                channel: GattChannel::from_bytes([0xA1; 16]),
                bytes: &[0xde, 0xad, 0xbe, 0xef],
                monotonic_ms: MonotonicTimestamp::new(42),
            }
        );
    }

    #[test]
    fn session_input_dto_maps_commands_back_to_core() {
        assert_eq!(
            SessionInputDto::Command(DeviceCommandDto::SetLights(LightStateDto::On))
                .as_session_input(),
            SessionInput::Command(DeviceCommand::SetLights(LightState::On))
        );
        assert_eq!(
            SessionInputDto::Command(DeviceCommandDto::SetRawMotorCurrent { current: -1_500 })
                .as_session_input(),
            SessionInput::Command(DeviceCommand::SetRawMotorCurrent {
                current: PhaseCurrent::from_milliamps(-1_500),
            })
        );
    }

    #[test]
    fn session_output_owns_transport_write_bytes_and_events() {
        let write = SessionOutputDto::from(SessionOutput::Transport(TransportAction::Write {
            channel: GattChannel::from_bytes([0xB2; 16]),
            bytes: WritePayload::try_from_slice(&[1, 2, 3]).expect("payload fits"),
            mode: WriteMode::WithoutResponse,
        }));
        let event = SessionOutputDto::from(SessionOutput::Event(DeviceEvent::LinkUp(LinkInfo {
            monotonic_ms: MonotonicTimestamp::new(7),
            max_write_len: Some(write_len(182)),
        })));
        let readback = SessionOutputDto::from(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
            ReadOnlyResponse::Settings(SettingsReadback::available([
                Some(SettingsEntry {
                    field: RawFieldValue::new(0x0014, 30),
                    source: ValueSource::Reported,
                    quality: ValueQuality::Known,
                    verification: VerificationStatus::HardwareVerified,
                }),
                None,
                None,
                None,
            ])),
        )));

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
                monotonic_ms: ms(7),
                max_write_len: Some(write_len_dto(182)),
            })
        );
        let SessionOutputDto::ReadOnly(response) = readback else {
            panic!("read-only responses must not be nested under event outputs");
        };
        assert_eq!(response.command_kind, CommandKindDto::RequestSettings);
        let ReadOnlyOutputPayload::Settings(settings) = response.payload else {
            panic!("expected settings readback");
        };
        assert_eq!(
            settings.availability,
            SettingsReadbackAvailabilityDto::Available
        );
        assert_eq!(settings.entries[0].field.id, 0x0014);
        assert_eq!(settings.entries[0].field.value, 30);
    }

    #[test]
    fn telemetry_snapshot_dto_preserves_optional_fields() {
        let snapshot = TelemetrySnapshot {
            at_ms: Some(MonotonicTimestamp::new(42)),
            speed: Some(Measured::reported(Speed::from_millimetres_per_second(
                1_200,
            ))),
            voltage: Some(Measured::reported(Voltage::from_millivolts(84_000))),
            battery_current: None,
            charge_mode: Some(Measured::reported(ChargeMode::Charging)),
            operating_state: Some(RideOperatingState::Charging),
            ride_warning: Some(RideWarning::DutyPushback),
            ride_stop_reason: Some(RideStopReason::Pitch),
            motor_current: Some(Measured::reported(PhaseCurrent::from_milliamps(-1_500))),
            power: None,
            controller_temperature: Some(Measured::reported(Temperature::from_millicelsius(
                31_000,
            ))),
            motor_temperature: None,
            battery_temperature: None,
            pwm: Some(Measured::reported(DutyCycle::from_permille(250))),
            distance: Some(Measured::reported(Distance::from_millimetres(12_345))),
            pitch: None,
            balance_angle: None,
            roll: None,
            footpad: Some(FootpadTelemetry {
                state: 2,
                contact_state: None,
                adc1_milliunits: Some(1_250),
                adc2_milliunits: None,
            }),
            battery_level_reported: None,
            battery_level_estimated: Some(Measured::estimated(BatteryLevel::from_percent(80))),
        };

        let dto = TelemetrySnapshotDto::from(snapshot);

        assert_eq!(dto.at_ms, Some(ms(42)));
        assert_eq!(dto.speed.expect("speed").value, 1_200);
        assert_eq!(dto.voltage.expect("voltage").value, 84_000);
        assert_eq!(dto.battery_current, None);
        assert_eq!(
            dto.charge_mode.expect("charge mode").value,
            ChargeModeDto::Charging
        );
        assert_eq!(
            dto.operating_state.expect("operating state"),
            RideOperatingStateDto::Charging
        );
        assert_eq!(
            dto.ride_warning.expect("ride warning"),
            RideWarningDto::DutyPushback
        );
        assert_eq!(
            dto.ride_stop_reason.expect("ride stop reason"),
            RideStopReasonDto::Pitch
        );
        assert_eq!(dto.motor_current.expect("current").value, -1_500);
        assert_eq!(
            dto.footpad,
            Some(FootpadTelemetryDto {
                state: 2,
                contact_state: None,
                adc1_milliunits: Some(1_250),
                adc2_milliunits: None,
            })
        );
        assert_eq!(dto.battery_level_estimated.expect("level").value, 80);
    }
}
