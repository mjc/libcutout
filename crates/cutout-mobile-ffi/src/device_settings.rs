//! Native settings records and conversions; validation and lifecycle live in Rust owners.

use cutout_core::{DeviceSettingSnapshot, DeviceSettingValue, SettingCommandStatus, SettingId};
use cutout_protocols::{
    DeviceSettingRequestError, DeviceSettingsSnapshot, SettingAccess, SettingControl,
    SettingDescriptor, SettingGroup, SettingUnit, SettingsRequestError,
};

use crate::{
    CutoutSessionStateHandle, MobileConnectionAttemptSnapshotDto, MobileConnectionAttemptTokenDto,
    MobileControlRefusalReasonDto, MobileDeviceSessionStepDto, MobileSettingValueSourceDto,
    MobileValueQualityDto, MobileValueSourceDto, MobileVerificationStatusDto,
};

macro_rules! setting_enum {
    ($mobile:ident, $core:ident, $description:literal, [$($variant:ident),+ $(,)?]) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
        pub enum $mobile { $(#[doc = "Corresponding shared domain variant."] $variant),+ }
        impl From<$core> for $mobile {
            fn from(value: $core) -> Self { match value { $($core::$variant => Self::$variant),+ } }
        }
        impl From<$mobile> for $core {
            fn from(value: $mobile) -> Self { match value { $($mobile::$variant => Self::$variant),+ } }
        }
    };
}

pub(super) use setting_enum;

setting_enum!(
    MobileSettingIdDto,
    SettingId,
    "Stable semantic setting identity.",
    [
        Headlight,
        HighBeam,
        TiltbackSpeed,
        PwmTiltback,
        PedalHardness,
        DisplayBrightness,
        DisplayUnits,
        BeeperVolumePercent,
        DynamicAssist,
        PedalDipCompensation,
        LateralTiltLimit,
        VoltageCorrection,
        ChargeLimitDiagnostic,
        HighSpeedMode,
        LowBatteryMode,
        TransportMode,
        SpeedAlarmThreshold,
        PedalAngle,
        RidingPreset,
        BrakeOverpressureAlarm,
        PedalMode,
        RollAngleMode,
        SpeedAlarmMode,
        MaximumSpeed,
        BeeperVolumeLevel,
        LightingPattern,
        AccelerationAssist,
        Taillight,
        AutoShutdownRemaining,
        ChargeMode,
        PowerOffDelay
    ]
);
setting_enum!(
    MobileSettingUnitDto,
    SettingUnit,
    "Physical unit and fixed-point interpretation.",
    [
        Percent,
        PwmDutyPercent,
        KilometresPerHour,
        Degrees,
        Level,
        Seconds,
        Minutes
    ]
);
setting_enum!(
    MobileSettingGroupDto,
    SettingGroup,
    "Semantic section for native rendering.",
    [Limits, Riding, Interface, Modes, Diagnostics]
);
setting_enum!(
    MobileSettingAccessDto,
    SettingAccess,
    "Static submission availability resolved by the protocol profile.",
    [Writable, Unverified, ReadOnly]
);
setting_enum!(
    MobileSettingStatusDto,
    SettingCommandStatus,
    "Shared settings request lifecycle.",
    [
        Idle,
        WaitingForConfirmation,
        SentWithoutConfirmation,
        TimedOut,
        Confirmed,
        Refused,
        Failed
    ]
);

/// Semantic setting value; absence and explicit disabled state remain distinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSettingValueDto {
    /// Explicit on/off.
    Boolean { value: bool },
    /// Quantity scaled by the descriptor's decimal precision.
    Number { value: i32 },
    /// Opaque semantic option identifier.
    Choice { id: u16 },
    /// Explicitly disabled functionality.
    Disabled,
}

impl From<DeviceSettingValue> for MobileSettingValueDto {
    fn from(value: DeviceSettingValue) -> Self {
        match value {
            DeviceSettingValue::Boolean(value) => Self::Boolean { value },
            DeviceSettingValue::Number(value) => Self::Number { value },
            DeviceSettingValue::Choice(id) => Self::Choice { id },
            DeviceSettingValue::Disabled => Self::Disabled,
        }
    }
}

impl From<MobileSettingValueDto> for DeviceSettingValue {
    fn from(value: MobileSettingValueDto) -> Self {
        match value {
            MobileSettingValueDto::Boolean { value } => Self::Boolean(value),
            MobileSettingValueDto::Number { value } => Self::Number(value),
            MobileSettingValueDto::Choice { id } => Self::Choice(id),
            MobileSettingValueDto::Disabled => Self::Disabled,
        }
    }
}

/// One readable setting option and its write availability.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingChoiceDto {
    /// Stable semantic option identity.
    pub id: u16,
    /// Shared localization key.
    pub label_key: String,
    /// Whether the protocol permits choosing this value.
    pub writable: bool,
}

/// Native-neutral control shape with Rust-owned limits.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSettingControlDto {
    /// Explicit on/off control.
    Boolean,
    /// Bounded fixed-point quantity.
    Number {
        minimum: i32,
        maximum: i32,
        step: i32,
        precision: u8,
        unit: MobileSettingUnitDto,
        can_disable: bool,
    },
    /// Closed readable option set.
    Choices {
        choices: Vec<MobileSettingChoiceDto>,
    },
    /// Diagnostic observation with no submission.
    ReadOnly,
}

impl From<SettingControl> for MobileSettingControlDto {
    fn from(value: SettingControl) -> Self {
        match value {
            SettingControl::Boolean => Self::Boolean,
            SettingControl::Number {
                minimum,
                maximum,
                step,
                precision,
                unit,
                can_disable,
            } => Self::Number {
                minimum,
                maximum,
                step,
                precision,
                unit: unit.into(),
                can_disable,
            },
            SettingControl::Choices(choices) => Self::Choices {
                choices: choices
                    .into_iter()
                    .map(|choice| MobileSettingChoiceDto {
                        id: choice.id,
                        label_key: choice.label_key.into(),
                        writable: choice.writable,
                    })
                    .collect(),
            },
            SettingControl::ReadOnly => Self::ReadOnly,
        }
    }
}

/// One semantic setting definition for generic mobile rendering.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingDescriptorDto {
    /// Stable setting identity.
    pub id: MobileSettingIdDto,
    /// Shared localization key.
    pub label_key: String,
    /// Optional Rust-selected explanation for this control.
    pub help_key: Option<String>,
    /// Optional Rust-selected explanation of units or value meaning.
    pub value_semantics_key: Option<String>,
    /// Semantic presentation group.
    pub group: MobileSettingGroupDto,
    /// Stable order within the catalog.
    pub order: u16,
    /// Control shape and valid values.
    pub control: MobileSettingControlDto,
    /// Static submission availability.
    pub access: MobileSettingAccessDto,
    /// Write evidence, independent of availability and observed values.
    pub write_verification: MobileVerificationStatusDto,
    /// Whether actual readback can confirm an accepted request.
    pub confirmation_supported: bool,
}

impl From<SettingDescriptor> for MobileSettingDescriptorDto {
    fn from(value: SettingDescriptor) -> Self {
        Self {
            id: value.id.into(),
            label_key: value.label_key.into(),
            help_key: value.help_key.map(Into::into),
            value_semantics_key: value.value_semantics_key.map(Into::into),
            group: value.group.into(),
            order: value.order,
            control: value.control.into(),
            access: value.access.into(),
            write_verification: value.write_verification.into(),
            confirmation_supported: value.confirmation_supported,
        }
    }
}

/// Original protocol measurement evidence, without upgrading its confidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingEvidenceDto {
    /// Measurement source.
    pub source: MobileValueSourceDto,
    /// Measurement quality.
    pub quality: MobileValueQualityDto,
    /// Verification of the reported value.
    pub verification: MobileVerificationStatusDto,
}

/// An observed value and request lifecycle, kept separate.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingSnapshotDto {
    /// Stable setting identity.
    pub id: MobileSettingIdDto,
    /// Current observation, absent when unknown.
    pub current: Option<MobileSettingValueDto>,
    /// Origin of the current observation.
    pub current_source: Option<MobileSettingValueSourceDto>,
    /// Original measured evidence when supplied by the protocol.
    pub evidence: Option<MobileSettingEvidenceDto>,
    /// Most recent protocol submission, distinct from current readback.
    pub requested: Option<MobileSettingValueDto>,
    /// Shared request lifecycle.
    pub status: MobileSettingStatusDto,
    /// Observation age at the snapshot timestamp.
    pub age_ms: Option<u64>,
    /// Protocol guard refusal, if any.
    pub refusal: Option<MobileControlRefusalReasonDto>,
}

impl From<DeviceSettingSnapshot> for MobileSettingSnapshotDto {
    fn from(value: DeviceSettingSnapshot) -> Self {
        Self {
            id: value.id.into(),
            current: value.current.map(|current| current.value.into()),
            current_source: value.current.map(|current| current.source.into()),
            evidence: value.measured.map(|measured| MobileSettingEvidenceDto {
                source: measured.source.into(),
                quality: measured.quality.into(),
                verification: measured.verification.into(),
            }),
            requested: value.requested.map(Into::into),
            status: value.status.into(),
            age_ms: value.age.map(|age| age.as_milliseconds()),
            refusal: value
                .refusal
                .map(|reason| cutout_core::ControlRefusalReasonDto::from(reason).into()),
        }
    }
}

/// Descriptor list paired with the connection that selected it.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingsDescriptorSnapshotDto {
    /// Immutable admission and identity token.
    pub connection: MobileConnectionAttemptSnapshotDto,
    /// Whether this attempt authorizes source-backed validation writes.
    pub validation_authorized: bool,
    /// Protocol-selected controls.
    pub descriptors: Vec<MobileSettingDescriptorDto>,
}

/// Atomic settings publication from the shared session owner.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceSettingsSnapshotDto {
    /// Immutable admission and identity token.
    pub connection: MobileConnectionAttemptSnapshotDto,
    /// Latest accepted monotonic input time.
    pub at_ms: u64,
    /// Shared observed/requested state.
    pub settings: Vec<MobileSettingSnapshotDto>,
}

impl From<DeviceSettingsSnapshot> for MobileDeviceSettingsSnapshotDto {
    fn from(value: DeviceSettingsSnapshot) -> Self {
        Self {
            connection: (&value.connection).into(),
            at_ms: value.at.get(),
            settings: value.settings.into_iter().map(Into::into).collect(),
        }
    }
}

/// Complete generic controls publication from one connection-owner observation.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceControlsSnapshotDto {
    /// Attempt and readiness associated with every row.
    pub connection: MobileConnectionAttemptSnapshotDto,
    /// Whether this attempt authorizes source-backed validation writes.
    pub validation_authorized: bool,
    /// Owner timestamp used to calculate ages.
    pub at_ms: u64,
    /// Optional protocol-selected battery basis, preserving its original evidence.
    pub default_charge_profile: Option<crate::MobileChargeProfileDto>,
    /// Semantic controls available in this presentation mode.
    pub setting_descriptors: Vec<MobileSettingDescriptorDto>,
    /// Observed and requested setting state.
    pub settings: Vec<MobileSettingSnapshotDto>,
    /// Semantic actions available in this presentation mode.
    pub action_descriptors: Vec<crate::MobileDeviceActionDescriptorDto>,
    /// Procedure and momentary-action lifecycle state.
    pub actions: Vec<crate::MobileDeviceActionSnapshotDto>,
}

/// Semantic submission refusal before protocol execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileDeviceSettingRequestError {
    /// Current connection no longer permits this request.
    #[error("connection unavailable")]
    ConnectionUnavailable,
    /// The selected device has no such control.
    #[error("setting unavailable")]
    Unavailable,
    /// This is a diagnostic observation.
    #[error("setting is read-only")]
    ReadOnly,
    /// This write requires explicit validation mode.
    #[error("setting write is unverified")]
    Unverified,
    /// The descriptor rejected this value.
    #[error("invalid setting value")]
    InvalidValue,
}

impl From<DeviceSettingRequestError> for MobileDeviceSettingRequestError {
    fn from(value: DeviceSettingRequestError) -> Self {
        match value {
            DeviceSettingRequestError::ConnectionUnavailable => Self::ConnectionUnavailable,
            DeviceSettingRequestError::Profile(SettingsRequestError::Unavailable) => {
                Self::Unavailable
            }
            DeviceSettingRequestError::Profile(SettingsRequestError::ReadOnly) => Self::ReadOnly,
            DeviceSettingRequestError::Profile(SettingsRequestError::Unverified) => {
                Self::Unverified
            }
            DeviceSettingRequestError::Profile(SettingsRequestError::InvalidValue) => {
                Self::InvalidValue
            }
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Reads the complete Tune presentation under the existing session lock.
    #[must_use]
    pub fn device_controls_snapshot(&self) -> MobileDeviceControlsSnapshotDto {
        let inner = self.lock_inner();
        let settings: MobileDeviceSettingsSnapshotDto = inner.settings_snapshot().into();
        let actions = inner.actions_snapshot();
        let default_charge_profile =
            inner
                .default_charge_profile()
                .map(|profile| crate::MobileChargeProfileDto {
                    session_id: settings.connection.generation,
                    profile_id: profile.identity.get(),
                    capacity_milliamp_hours: profile.usable_capacity.as_milliamp_hours(),
                    capacity_source: profile.usable_capacity.source.into(),
                    verification: profile.usable_capacity.verification.into(),
                    charge_flow_verification: profile.charge_flow_verification.into(),
                });
        MobileDeviceControlsSnapshotDto {
            connection: settings.connection,
            validation_authorized: inner.validation_authorized(),
            default_charge_profile,
            at_ms: settings.at_ms,
            setting_descriptors: inner
                .settings_descriptors()
                .into_iter()
                .map(Into::into)
                .collect(),
            settings: settings.settings,
            action_descriptors: inner
                .action_descriptors()
                .into_iter()
                .map(Into::into)
                .collect(),
            actions: actions.actions.into_iter().map(Into::into).collect(),
        }
    }

    /// Projects semantic controls and their producing connection under one lock.
    #[must_use]
    pub fn settings_descriptors(&self) -> MobileSettingsDescriptorSnapshotDto {
        let inner = self.lock_inner();
        MobileSettingsDescriptorSnapshotDto {
            connection: inner.session_state().connection.snapshot().into(),
            validation_authorized: inner.validation_authorized(),
            descriptors: inner
                .settings_descriptors()
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }

    /// Projects observed/requested values and their producing connection atomically.
    #[must_use]
    pub fn settings_snapshot(&self) -> MobileDeviceSettingsSnapshotDto {
        self.lock_inner().settings_snapshot().into()
    }

    /// Changes validation authorization for exactly one current connection attempt.
    pub fn authorize_device_controls(&self, token: MobileConnectionAttemptTokenDto) -> bool {
        let token = token.into();
        self.lock_inner().authorize_validation(&token)
    }

    /// Revokes validation authorization for exactly one current connection attempt.
    pub fn revoke_device_controls(&self, token: MobileConnectionAttemptTokenDto) -> bool {
        let token = token.into();
        self.lock_inner().revoke_validation(&token)
    }

    /// Submits one semantic value through the protocol owner.
    pub fn submit_setting(
        &self,
        token: MobileConnectionAttemptTokenDto,
        id: MobileSettingIdDto,
        value: MobileSettingValueDto,
        monotonic_ms: u64,
    ) -> Result<MobileDeviceSessionStepDto, MobileDeviceSettingRequestError> {
        self.lock_inner()
            .submit_setting(
                &token.into(),
                id.into(),
                value.into(),
                cutout_core::MonotonicTimestamp::new(monotonic_ms),
            )
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_charge_profile_follows_verified_protocol_and_attempt() {
        use crate::{
            Distance, MobileChargeCapacitySourceDto, MobileChargeProfileDto,
            MobileVerificationStatusDto, VescBatteryCellModel, VescBatteryType, VescBoardProfile,
        };
        let handle = CutoutSessionStateHandle::new();
        let charge = MobileChargeProfileDto {
            session_id: 999,
            profile_id: 15_002,
            capacity_milliamp_hours: 6_000,
            capacity_source: MobileChargeCapacitySourceDto::ProtocolProfile,
            verification: MobileVerificationStatusDto::SourceVerified,
            charge_flow_verification: MobileVerificationStatusDto::HardwareVerified,
        };
        let profile = VescBoardProfile {
            motor_pole_pairs: 15,
            gear_ratio_denominator: 1,
            wheel_circumference: Distance { value: 2_100 },
            battery_type: VescBatteryType::LiIon,
            battery_cells: 15,
            battery_parallel_cells: 2,
            battery_cell_model: VescBatteryCellModel::SonyVtc6,
            charge_profile: Some(charge),
            reports_battery_current: true,
        };
        let vesc_reply = vec![
            2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104,
            0, 38, 208, 3,
        ];
        let first = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        assert!(handle.configure_connection_vesc_profile(first.clone(), profile));
        assert!(
            handle
                .device_controls_snapshot()
                .default_charge_profile
                .is_none()
        );
        handle.connection_link_established(first.clone());
        handle.observe_connection_notification(first.clone(), vesc_reply.clone());
        handle.resolve_device_session(first.clone(), false, 1);
        let selected = handle
            .device_controls_snapshot()
            .default_charge_profile
            .unwrap();
        assert_eq!(
            selected,
            MobileChargeProfileDto {
                session_id: first.generation,
                ..charge
            }
        );

        let second = handle
            .begin_connection_attempt("B".into(), 2)
            .token
            .unwrap();
        assert!(handle.configure_connection_vesc_profile(second.clone(), profile));
        handle.connection_link_established(second.clone());
        let mut aero_frame = vec![0_u8; 42];
        aero_frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        aero_frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        handle.observe_connection_notification(second.clone(), aero_frame);
        handle.resolve_device_session(second.clone(), false, 3);
        let selected = handle
            .device_controls_snapshot()
            .default_charge_profile
            .unwrap();
        assert_eq!(
            selected.profile_id, 43,
            "saved VESC capacity must not overwrite an EUC profile"
        );
        assert_eq!(selected.session_id, second.generation);

        let third = handle
            .begin_connection_attempt("C".into(), 4)
            .token
            .unwrap();
        handle.connection_link_established(third.clone());
        handle.observe_connection_notification(third.clone(), vesc_reply);
        handle.resolve_device_session(third, false, 5);
        assert!(
            handle
                .device_controls_snapshot()
                .default_charge_profile
                .is_none()
        );
    }

    #[test]
    fn generic_settings_boundary_keeps_identity_refusals_and_requests_distinct() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        handle.observe_connection_notification(token.clone(), frame.clone());
        handle.resolve_device_session(token.clone(), false, 1);
        let _ = handle.lock_inner().ingest(
            &token.clone().into(),
            &cutout_core::SessionInputDto::LinkUp {
                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                max_write_len: None,
            },
        );
        let _ = handle.lock_inner().ingest(
            &token.clone().into(),
            &cutout_core::SessionInputDto::Notification {
                channel: cutout_protocols::VETERAN_DATA_CHANNEL.as_bytes(),
                bytes: frame,
                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
            },
        );
        let descriptors = handle.settings_descriptors();
        assert_eq!(descriptors.connection.token, Some(token.clone()));
        assert!(!descriptors.validation_authorized);
        assert!(
            descriptors
                .descriptors
                .iter()
                .any(|descriptor| descriptor.id == MobileSettingIdDto::HighBeam)
        );
        handle
            .submit_setting(
                token.clone(),
                MobileSettingIdDto::HighBeam,
                MobileSettingValueDto::Boolean { value: true },
                2,
            )
            .unwrap();
        let before = handle.settings_snapshot();
        let headlight = before
            .settings
            .iter()
            .find(|setting| setting.id == MobileSettingIdDto::HighBeam)
            .unwrap();
        assert_eq!(
            headlight.requested,
            Some(MobileSettingValueDto::Boolean { value: true })
        );
        assert_eq!(headlight.current, None);
        assert_eq!(
            headlight.status,
            MobileSettingStatusDto::SentWithoutConfirmation
        );
        assert_eq!(
            handle
                .submit_setting(
                    token.clone(),
                    MobileSettingIdDto::HighBeam,
                    MobileSettingValueDto::Number { value: 80 },
                    3
                )
                .unwrap_err(),
            MobileDeviceSettingRequestError::InvalidValue
        );
        assert_eq!(handle.settings_snapshot(), before);
        let writable: Vec<_> = descriptors
            .descriptors
            .iter()
            .filter(|descriptor| descriptor.access == MobileSettingAccessDto::Writable)
            .collect();
        assert_eq!(writable.len(), 18);
        for descriptor in writable {
            assert_eq!(
                descriptor.write_verification,
                MobileVerificationStatusDto::Unverified
            );
            let value = match &descriptor.control {
                MobileSettingControlDto::Boolean => MobileSettingValueDto::Boolean { value: true },
                MobileSettingControlDto::Number { minimum, .. } => {
                    MobileSettingValueDto::Number { value: *minimum }
                }
                MobileSettingControlDto::Choices { choices } => {
                    MobileSettingValueDto::Choice { id: choices[0].id }
                }
                MobileSettingControlDto::ReadOnly => unreachable!(),
            };
            let step = handle
                .submit_setting(token.clone(), descriptor.id, value, 3)
                .unwrap();
            assert!(step.result.error.is_none(), "{:?}", descriptor.id);
            let settings = handle.settings_snapshot();
            let setting = settings
                .settings
                .iter()
                .find(|item| item.id == descriptor.id)
                .unwrap();
            assert_eq!(setting.requested, Some(value));
            assert_eq!(
                setting.status,
                if descriptor.confirmation_supported {
                    MobileSettingStatusDto::WaitingForConfirmation
                } else {
                    MobileSettingStatusDto::SentWithoutConfirmation
                }
            );
        }
        assert!(!handle.settings_descriptors().validation_authorized);
        assert!(handle.authorize_device_controls(token.clone()));
        assert!(handle.settings_descriptors().validation_authorized);
        let next = handle.begin_connection_attempt("B".into(), 4);
        assert_eq!(
            handle
                .submit_setting(
                    token,
                    MobileSettingIdDto::HighBeam,
                    MobileSettingValueDto::Boolean { value: false },
                    5
                )
                .unwrap_err(),
            MobileDeviceSettingRequestError::ConnectionUnavailable
        );
        assert_eq!(handle.settings_descriptors().connection, next);
        assert!(!handle.settings_descriptors().validation_authorized);
        assert!(handle.settings_descriptors().descriptors.is_empty());
        assert!(handle.settings_snapshot().settings.is_empty());
    }

    #[test]
    fn generic_settings_projection_preserves_disabled_and_measured_evidence() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        let mut inner = handle.lock_inner();
        inner.session_state_mut().settings.observe_measured(
            SettingId::PwmTiltback,
            cutout_core::Measured {
                value: DeviceSettingValue::Disabled,
                source: cutout_core::ValueSource::Reported,
                quality: cutout_core::ValueQuality::Known,
                verification: cutout_core::VerificationStatus::SourceVerified,
            },
            cutout_core::MonotonicTimestamp::new(0),
        );
        drop(inner);
        let result = handle.settings_snapshot();
        assert_eq!(result.connection.token, Some(token));
        let pwm = result
            .settings
            .iter()
            .find(|setting| setting.id == MobileSettingIdDto::PwmTiltback)
            .unwrap();
        assert_eq!(pwm.current, Some(MobileSettingValueDto::Disabled));
        assert_eq!(
            pwm.current_source,
            Some(MobileSettingValueSourceDto::LiveReadback)
        );
        assert_eq!(pwm.age_ms, Some(0));
        assert_eq!(
            pwm.evidence,
            Some(MobileSettingEvidenceDto {
                source: cutout_core::ValueSource::Reported.into(),
                quality: cutout_core::ValueQuality::Known.into(),
                verification: cutout_core::VerificationStatus::SourceVerified.into(),
            })
        );
    }
}
