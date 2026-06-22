use crate::{
    BatteryInfo, BatteryPageKind, BatteryPageMetadata, BatteryPagePayload, CommandKind,
    DiagnosticDetail, DiagnosticReadback, DiagnosticSeverity, FirmwareInfo, Measured,
    RawFieldValue, ReadOnlyResponse, SettingsEntry, SettingsReadback, ValueQuality, ValueSource,
    VerificationStatus,
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

#[cfg(test)]
mod tests {
    use crate::{
        BatteryInfo, BatteryPageMetadata, BatteryPagePayload, DiagnosticDetail, DiagnosticReadback,
        DiagnosticSeverity, FirmwareInfo, Measured, RawFieldValue, ReadOnlyResponse, SettingsEntry,
        SettingsReadback, ValueQuality, ValueSource, VerificationStatus,
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
}
