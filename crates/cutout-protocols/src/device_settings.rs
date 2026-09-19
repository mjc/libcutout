//! Shared device settings definitions and semantic command validation.

mod aero;
mod falcon;
mod readback;
pub use readback::SettingObservation;

use cutout_core::{
    Capabilities, Capacity, CapacitySource, ChargeProfile, ChargeProfileIdentity, CommandKind,
    DeviceCommand, DeviceSettingValue, ProtocolFamily, SettingId, SettingsEntry,
    UsablePackCapacity, VerificationStatus,
};

pub use cutout_core::SettingCompletionStrategy;

/// Meaning of a fixed-point numeric value before native display-unit conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingUnit {
    /// Ordinary percentage.
    Percent,
    /// Used PWM duty: 80 means 80% duty and 20% remaining headroom.
    PwmDutyPercent,
    /// Speed in kilometres per hour.
    KilometresPerHour,
    /// Angle in degrees.
    Degrees,
    /// Device-defined ordinal level, with no invented physical units.
    Level,
    /// Duration in seconds.
    Seconds,
    /// Duration in minutes.
    Minutes,
}

/// Semantic grouping and stable ordering independent of device names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingGroup {
    /// Alarms and operating limits.
    Limits,
    /// Pedal response and assistance.
    Riding,
    /// Lights, sound, and wheel display.
    Interface,
    /// Special operating modes.
    Modes,
    /// Values whose physical meaning is not established.
    Diagnostics,
}

/// A readable option may be unavailable for writing on the current protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingChoice {
    /// Stable semantic option identifier, not a wire byte.
    pub id: u16,
    /// Shared localization catalog key.
    pub label_key: &'static str,
    /// Whether a submission can select this option.
    pub writable: bool,
}

/// Native-neutral control definition. Unknown current values are separate state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingControl {
    /// Explicit on/off selection.
    Boolean,
    /// A bounded fixed-point numeric quantity.
    Number {
        /// Inclusive minimum in fixed-point units.
        minimum: i32,
        /// Inclusive maximum in fixed-point units.
        maximum: i32,
        /// Increment in fixed-point units.
        step: i32,
        /// Decimal places in the physical value.
        precision: u8,
        /// Physical or ordinal quantity.
        unit: SettingUnit,
        /// A separate disabled selection is supported.
        can_disable: bool,
    },
    /// Closed readable option set with individual write availability.
    Choices(Vec<SettingChoice>),
    /// Diagnostic value with no permitted submission.
    ReadOnly,
}

impl SettingControl {
    fn accepts(&self, value: DeviceSettingValue) -> bool {
        match (self, value) {
            (Self::Boolean, DeviceSettingValue::Boolean(_)) => true,
            (
                Self::Number {
                    minimum,
                    maximum,
                    step,
                    ..
                },
                DeviceSettingValue::Number(value),
            ) => (*minimum..=*maximum).contains(&value) && (value - minimum) % step == 0,
            (Self::Number { can_disable, .. }, DeviceSettingValue::Disabled) => *can_disable,
            (Self::Choices(choices), DeviceSettingValue::Choice(id)) => choices
                .iter()
                .any(|choice| choice.id == id && choice.writable),
            _ => false,
        }
    }
}

/// Static write availability. The session also checks live conditions at execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingAccess {
    /// The profile permits this write; live session guards still apply.
    Writable,
    /// A candidate encoder exists but has not been verified.
    Unverified,
    /// Physical meaning or write semantics are unresolved.
    ReadOnly,
}

/// Shared setting definition consumed by every native settings renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingDescriptor {
    /// Stable semantic identity.
    pub id: SettingId,
    /// Shared localization catalog key for the label.
    pub label_key: &'static str,
    /// Optional Rust-selected explanation for this control.
    pub help_key: Option<&'static str>,
    /// Optional Rust-selected explanation of units or value meaning.
    pub value_semantics_key: Option<&'static str>,
    /// Semantic section.
    pub group: SettingGroup,
    /// Stable display order within the catalog.
    pub order: u16,
    /// Control and allowed value shape.
    pub control: SettingControl,
    /// Static availability in the selected validation mode.
    pub access: SettingAccess,
    /// Write evidence, independent of production availability and readback evidence.
    pub write_verification: VerificationStatus,
    /// Typed completion rule for an accepted write.
    pub completion: SettingCompletionStrategy,
}

/// Per-dialect definition shared by the descriptor and completion paths.
///
/// A binding is deliberately smaller than a descriptor: profile capability and
/// write verification are selected by the connection, while the dialect owns
/// the semantic value shape and the evidence required to finish a write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SettingBinding {
    pub control: SettingControl,
    pub observation: SettingObservationBinding,
    pub write_access: SettingWriteAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingObservationBinding {
    None,
    ObservedField(u16),
    MatchingField(u16),
    ReadOnlyField(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingWriteAccess {
    Writable,
    ReadOnly,
}

impl SettingBinding {
    fn new(id: SettingId, control: SettingControl, observation: SettingObservationBinding) -> Self {
        let write_access = match id {
            SettingId::ChargeLimitDiagnostic
            | SettingId::AutoShutdownRemaining
            | SettingId::ChargeMode
            | SettingId::PowerOffDelay
            | SettingId::LightingPattern => SettingWriteAccess::ReadOnly,
            _ => SettingWriteAccess::Writable,
        };
        Self {
            control,
            observation,
            write_access,
        }
    }

    fn completion(&self) -> SettingCompletionStrategy {
        if matches!(
            self.observation,
            SettingObservationBinding::MatchingField(_)
        ) {
            SettingCompletionStrategy::MatchingReadback
        } else {
            SettingCompletionStrategy::SubmissionOnly
        }
    }
}

impl SettingObservationBinding {
    fn field(self) -> Option<u16> {
        match self {
            Self::None => None,
            Self::ObservedField(field)
            | Self::MatchingField(field)
            | Self::ReadOnlyField(field) => Some(field),
        }
    }
}

/// A semantic request rejected before it reaches the live session checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SettingsRequestError {
    /// No setting exists in this profile for the requested identifier.
    #[error("setting unavailable")]
    Unavailable,
    /// The descriptor is diagnostic-only, even in validation mode.
    #[error("setting is read-only")]
    ReadOnly,
    /// The candidate write requires explicit validation mode.
    #[error("setting write is unverified")]
    Unverified,
    /// Wrong value shape, out-of-range number, or non-writable choice.
    #[error("invalid setting value")]
    InvalidValue,
}

/// Capability selection over shared settings and actions, without client model switches.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeviceControlProfile {
    settings_adapter: SettingsAdapter,
    pub(crate) available: Capabilities,
    pub(crate) verified: Capabilities,
    pub(crate) production: Capabilities,
    available_settings: &'static [SettingId],
    verified_settings: &'static [SettingId],
    readable: &'static [SettingId],
    default_charge_profile: Option<ChargeProfile>,
}

/// Protocol-owned settings semantics selected with a verified device profile.
///
/// Raw command capabilities alone cannot select a settings adapter: different
/// protocols can reuse command kinds while assigning different value ranges and
/// wire encodings to the same semantic setting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SettingsAdapter {
    #[default]
    None,
    Aero,
    Falcon,
}

impl SettingsAdapter {
    fn binding(self, id: SettingId) -> Option<SettingBinding> {
        match self {
            Self::None => None,
            Self::Aero => aero::binding(id),
            Self::Falcon => falcon::binding(id),
        }
    }

    fn checked_request(self, id: SettingId, value: DeviceSettingValue) -> Option<DeviceCommand> {
        match self {
            Self::None => None,
            Self::Aero => {
                let command = DeviceCommand::SetSetting { id, value };
                (crate::request_encoder::NosfetDialect::encode(command).is_some()
                    || crate::request_encoder::NosfetDialect::encode_settings_sequence(command)
                        .is_some())
                .then_some(command)
            }
            Self::Falcon => {
                let command = DeviceCommand::SetSetting { id, value };
                (crate::request_encoder::FalconDialect::encode(command).is_some()
                    || crate::request_encoder::FalconDialect::encode_settings_sequence(command)
                        .is_some())
                .then_some(command)
            }
        }
    }

    fn normalize_readback(self, entry: SettingsEntry, observations: &mut Vec<SettingObservation>) {
        match self {
            Self::None => {}
            Self::Aero => aero::normalize_readback(entry, observations),
            Self::Falcon => falcon::normalize_readback(entry, observations),
        }
    }

    fn protocol(self) -> Option<ProtocolFamily> {
        match self {
            Self::None => None,
            Self::Aero => Some(ProtocolFamily::VeteranLeaperkimNosfet),
            Self::Falcon => Some(ProtocolFamily::BegodeGotway),
        }
    }
}

impl DeviceControlProfile {
    /// Selects available encoders and verified writes.
    #[must_use]
    pub const fn new(available: Capabilities, verified: Capabilities) -> Self {
        Self {
            settings_adapter: SettingsAdapter::None,
            available,
            verified,
            production: verified,
            available_settings: &[],
            verified_settings: &[],
            readable: &[],
            default_charge_profile: None,
        }
    }

    /// Selects the verified protocol's settings semantics.
    #[must_use]
    const fn with_settings_adapter(mut self, settings_adapter: SettingsAdapter) -> Self {
        self.settings_adapter = settings_adapter;
        self
    }

    /// Admits source-backed commands without promoting their hardware evidence.
    #[must_use]
    const fn with_production_commands(mut self, production: Capabilities) -> Self {
        self.production = production;
        self
    }

    #[must_use]
    const fn with_setting_capabilities(
        mut self,
        available: &'static [SettingId],
        verified: &'static [SettingId],
    ) -> Self {
        self.available_settings = available;
        self.verified_settings = verified;
        self
    }

    /// Adds protocol-specific passive observations that have no write command.
    #[must_use]
    pub const fn with_readable_settings(mut self, readable: &'static [SettingId]) -> Self {
        self.readable = readable;
        self
    }

    /// Adds the protocol-selected default charge-estimation basis.
    #[must_use]
    pub const fn with_default_charge_profile(mut self, profile: ChargeProfile) -> Self {
        self.default_charge_profile = Some(profile);
        self
    }

    /// Returns the protocol-selected default charge-estimation basis.
    #[must_use]
    pub const fn default_charge_profile(self) -> Option<ChargeProfile> {
        self.default_charge_profile
    }

    /// Returns supported controls, retaining diagnostic-only and unverified definitions.
    #[must_use]
    pub fn descriptors(self, validation_mode: bool) -> Vec<SettingDescriptor> {
        if self.settings_adapter == SettingsAdapter::None {
            return Vec::new();
        }
        let adapter = self.settings_adapter;
        CATALOG
            .iter()
            .filter_map(|&(id, label_key, group, order)| {
                if !self.readable.contains(&id) && !self.available_settings.contains(&id) {
                    return None;
                }
                let binding = adapter.binding(id)?;
                let completion = binding.completion();
                let mut control = binding.control;
                let writable = self.available_settings.contains(&id)
                    && (self
                        .production
                        .supports_command_kind(CommandKind::SetSetting)
                        || validation_mode
                        || self.verified_settings.contains(&id));
                if let SettingControl::Number { can_disable, .. } = &mut control {
                    *can_disable = id == SettingId::PwmTiltback && writable;
                }
                let access = if binding.write_access == SettingWriteAccess::ReadOnly {
                    SettingAccess::ReadOnly
                } else if writable {
                    SettingAccess::Writable
                } else {
                    SettingAccess::Unverified
                };
                Some(SettingDescriptor {
                    id,
                    label_key,
                    help_key: setting_help_key(id),
                    value_semantics_key: value_semantics_key(id),
                    group,
                    order,
                    control,
                    access,
                    write_verification: if self.verified_settings.contains(&id) {
                        VerificationStatus::HardwareVerified
                    } else {
                        VerificationStatus::Unverified
                    },
                    completion,
                })
            })
            .collect()
    }

    /// Resolves a semantic request into an existing checked domain command.
    ///
    /// # Errors
    /// Rejects unavailable, read-only, unverified, or invalid values. This does not
    /// authorize transport: the session must revalidate identity and live conditions.
    pub fn command(
        self,
        id: SettingId,
        value: DeviceSettingValue,
        validation_mode: bool,
    ) -> Result<DeviceCommand, SettingsRequestError> {
        let descriptor = self
            .descriptors(validation_mode)
            .into_iter()
            .find(|entry| entry.id == id)
            .ok_or(SettingsRequestError::Unavailable)?;
        match descriptor.access {
            SettingAccess::ReadOnly => return Err(SettingsRequestError::ReadOnly),
            SettingAccess::Unverified => return Err(SettingsRequestError::Unverified),
            SettingAccess::Writable => {}
        }
        if !descriptor.control.accepts(value) {
            return Err(SettingsRequestError::InvalidValue);
        }
        self.settings_adapter
            .checked_request(id, value)
            .ok_or(SettingsRequestError::InvalidValue)
    }
}

/// Aero controls retain explicit per-command verification; charge raw stays diagnostic-only.
#[must_use]
pub const fn aero_control_profile() -> DeviceControlProfile {
    let available = Capabilities::from_supported_commands([
        CommandKind::SetSetting,
        CommandKind::SoundHorn,
        CommandKind::ResetTripMeter,
        CommandKind::GyroCalibration,
    ]);
    DeviceControlProfile::new(
        available,
        Capabilities::from_supported_commands([CommandKind::SetSetting, CommandKind::SoundHorn]),
    )
    .with_production_commands(available)
    .with_settings_adapter(SettingsAdapter::Aero)
    .with_setting_capabilities(
        &[
            SettingId::HighBeam,
            SettingId::TiltbackSpeed,
            SettingId::PwmTiltback,
            SettingId::RidingPreset,
            SettingId::BrakeOverpressureAlarm,
            SettingId::PedalHardness,
            SettingId::DisplayBrightness,
            SettingId::BeeperVolumePercent,
            SettingId::DynamicAssist,
            SettingId::PedalDipCompensation,
            SettingId::LateralTiltLimit,
            SettingId::VoltageCorrection,
            SettingId::ChargeLimitDiagnostic,
            SettingId::DisplayUnits,
            SettingId::HighSpeedMode,
            SettingId::LowBatteryMode,
            SettingId::TransportMode,
            SettingId::SpeedAlarmThreshold,
            SettingId::PedalAngle,
        ],
        &[],
    )
    .with_readable_settings(&[SettingId::AutoShutdownRemaining, SettingId::ChargeMode])
    .with_default_charge_profile(ChargeProfile::new(
        ChargeProfileIdentity::new(43),
        UsablePackCapacity::new(
            Capacity::from_milliamp_hours(10_000),
            CapacitySource::ProtocolProfile,
            VerificationStatus::SourceVerified,
        ),
        VerificationStatus::Unverified,
    ))
}

/// Falcon writes with no readable acknowledgement remain explicitly unconfirmed.
#[must_use]
pub const fn falcon_control_profile() -> DeviceControlProfile {
    let available = Capabilities::from_supported_commands([CommandKind::SetSetting]);
    DeviceControlProfile::new(available, available)
        .with_settings_adapter(SettingsAdapter::Falcon)
        .with_setting_capabilities(
            &[
                SettingId::Headlight,
                SettingId::PedalMode,
                SettingId::RollAngleMode,
                SettingId::SpeedAlarmMode,
                SettingId::MaximumSpeed,
                SettingId::BeeperVolumeLevel,
                SettingId::LightingPattern,
            ],
            &[
                SettingId::Headlight,
                SettingId::PedalMode,
                SettingId::RollAngleMode,
                SettingId::SpeedAlarmMode,
                SettingId::MaximumSpeed,
                SettingId::BeeperVolumeLevel,
                SettingId::LightingPattern,
            ],
        )
        .with_readable_settings(&[SettingId::PowerOffDelay])
        .with_default_charge_profile(ChargeProfile::new(
            ChargeProfileIdentity::new(44),
            UsablePackCapacity::new(
                Capacity::from_milliamp_hours(10_000),
                CapacitySource::ProtocolProfile,
                VerificationStatus::SourceVerified,
            ),
            VerificationStatus::Unverified,
        ))
}

const CATALOG: &[(SettingId, &str, SettingGroup, u16)] = &[
    (
        SettingId::Headlight,
        "settings.headlight.label",
        SettingGroup::Interface,
        0,
    ),
    (
        SettingId::HighBeam,
        "settings.high_beam.label",
        SettingGroup::Interface,
        1,
    ),
    (
        SettingId::TiltbackSpeed,
        "settings.tiltback_speed.label",
        SettingGroup::Limits,
        2,
    ),
    (
        SettingId::PwmTiltback,
        "settings.pwm_tiltback.label",
        SettingGroup::Limits,
        3,
    ),
    (
        SettingId::PedalHardness,
        "settings.pedal_hardness.label",
        SettingGroup::Riding,
        4,
    ),
    (
        SettingId::DisplayBrightness,
        "settings.display_brightness.label",
        SettingGroup::Interface,
        5,
    ),
    (
        SettingId::DisplayUnits,
        "settings.display_units.label",
        SettingGroup::Interface,
        6,
    ),
    (
        SettingId::BeeperVolumePercent,
        "settings.beeper_volume_percent.label",
        SettingGroup::Interface,
        7,
    ),
    (
        SettingId::DynamicAssist,
        "settings.dynamic_assist.label",
        SettingGroup::Riding,
        8,
    ),
    (
        SettingId::PedalDipCompensation,
        "settings.pedal_dip_compensation.label",
        SettingGroup::Riding,
        9,
    ),
    (
        SettingId::LateralTiltLimit,
        "settings.lateral_tilt_limit.label",
        SettingGroup::Limits,
        10,
    ),
    (
        SettingId::VoltageCorrection,
        "settings.voltage_correction.label",
        SettingGroup::Modes,
        11,
    ),
    (
        SettingId::ChargeLimitDiagnostic,
        "settings.charge_limit_diagnostic.label",
        SettingGroup::Diagnostics,
        12,
    ),
    (
        SettingId::HighSpeedMode,
        "settings.high_speed_mode.label",
        SettingGroup::Modes,
        13,
    ),
    (
        SettingId::LowBatteryMode,
        "settings.low_battery_mode.label",
        SettingGroup::Modes,
        14,
    ),
    (
        SettingId::TransportMode,
        "settings.transport_mode.label",
        SettingGroup::Modes,
        15,
    ),
    (
        SettingId::SpeedAlarmThreshold,
        "settings.speed_alarm_threshold.label",
        SettingGroup::Limits,
        16,
    ),
    (
        SettingId::PedalAngle,
        "settings.pedal_angle.label",
        SettingGroup::Riding,
        17,
    ),
    (
        SettingId::RidingPreset,
        "settings.riding_preset.label",
        SettingGroup::Riding,
        18,
    ),
    (
        SettingId::BrakeOverpressureAlarm,
        "settings.brake_overpressure_alarm.label",
        SettingGroup::Limits,
        19,
    ),
    (
        SettingId::PedalMode,
        "settings.pedal_mode.label",
        SettingGroup::Riding,
        20,
    ),
    (
        SettingId::RollAngleMode,
        "settings.roll_angle_mode.label",
        SettingGroup::Riding,
        21,
    ),
    (
        SettingId::SpeedAlarmMode,
        "settings.speed_alarm_mode.label",
        SettingGroup::Limits,
        22,
    ),
    (
        SettingId::MaximumSpeed,
        "settings.maximum_speed.label",
        SettingGroup::Limits,
        23,
    ),
    (
        SettingId::BeeperVolumeLevel,
        "settings.beeper_volume_level.label",
        SettingGroup::Interface,
        24,
    ),
    (
        SettingId::LightingPattern,
        "settings.lighting_pattern.label",
        SettingGroup::Interface,
        25,
    ),
    (
        SettingId::AccelerationAssist,
        "settings.acceleration_assist.label",
        SettingGroup::Riding,
        26,
    ),
    (
        SettingId::Taillight,
        "settings.taillight.label",
        SettingGroup::Interface,
        27,
    ),
    (
        SettingId::AutoShutdownRemaining,
        "settings.auto_shutdown_remaining.label",
        SettingGroup::Diagnostics,
        28,
    ),
    (
        SettingId::PowerOffDelay,
        "settings.power_off_delay.label",
        SettingGroup::Modes,
        29,
    ),
    (
        SettingId::ChargeMode,
        "settings.charge_mode.label",
        SettingGroup::Diagnostics,
        30,
    ),
];

const fn setting_help_key(id: SettingId) -> Option<&'static str> {
    match id {
        SettingId::PwmTiltback => Some("settings.pwm_tiltback.help"),
        SettingId::ChargeLimitDiagnostic => Some("settings.charge_limit_diagnostic.help"),
        SettingId::LightingPattern => Some("settings.lighting_pattern.help"),
        _ => None,
    }
}

const fn value_semantics_key(id: SettingId) -> Option<&'static str> {
    match id {
        SettingId::PwmTiltback => Some("settings.pwm_tiltback.semantics"),
        SettingId::TiltbackSpeed | SettingId::SpeedAlarmThreshold | SettingId::MaximumSpeed => {
            Some("controls.speed_steps")
        }
        _ => None,
    }
}

fn number(minimum: i32, maximum: i32, precision: u8, unit: SettingUnit) -> SettingControl {
    SettingControl::Number {
        minimum,
        maximum,
        step: 1,
        precision,
        unit,
        can_disable: false,
    }
}

fn speed_control(minimum: i32, maximum: i32) -> SettingControl {
    SettingControl::Number {
        minimum: minimum * 10,
        maximum: maximum * 10,
        step: 10,
        precision: 1,
        unit: SettingUnit::KilometresPerHour,
        can_disable: false,
    }
}

fn choices(entries: &[(u16, &'static str, bool)]) -> SettingControl {
    SettingControl::Choices(
        entries
            .iter()
            .map(|&(id, label_key, writable)| SettingChoice {
                id,
                label_key,
                writable,
            })
            .collect(),
    )
}

fn control_value(control: SettingControl, raw: i64) -> Option<DeviceSettingValue> {
    match control {
        SettingControl::Boolean => match raw {
            0 => Some(DeviceSettingValue::Boolean(false)),
            1 => Some(DeviceSettingValue::Boolean(true)),
            _ => None,
        },
        SettingControl::Choices(choices) => {
            let value = u16::try_from(raw).ok()?;
            choices
                .iter()
                .any(|choice| choice.id == value)
                .then_some(DeviceSettingValue::Choice(value))
        }
        SettingControl::Number {
            minimum, maximum, ..
        } => {
            let value = i32::try_from(raw).ok()?;
            (minimum..=maximum)
                .contains(&value)
                .then_some(DeviceSettingValue::Number(value))
        }
        SettingControl::ReadOnly => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{
        Capabilities, CapacitySource, CommandKind, DeviceCommand, DeviceSettingValue, SettingId,
        VerificationStatus,
    };

    #[test]
    fn euc_profiles_publish_their_typed_default_charge_basis() {
        let aero = aero_control_profile()
            .default_charge_profile()
            .expect("Aero charge profile");
        assert_eq!(aero.identity.get(), 43);
        assert_eq!(aero.usable_capacity.as_milliamp_hours(), 10_000);
        assert_eq!(aero.usable_capacity.source, CapacitySource::ProtocolProfile);
        assert_eq!(
            aero.usable_capacity.verification,
            VerificationStatus::SourceVerified
        );
        assert_eq!(
            aero.charge_flow_verification,
            VerificationStatus::Unverified
        );

        let falcon = falcon_control_profile()
            .default_charge_profile()
            .expect("Falcon charge profile");
        assert_eq!(falcon.identity.get(), 44);
        assert_eq!(falcon.usable_capacity.as_milliamp_hours(), 10_000);
        assert_eq!(
            DeviceControlProfile::default().default_charge_profile(),
            None
        );
    }

    #[test]
    fn aero_profile_exposes_one_descriptor_per_physical_control() {
        let descriptors = aero_control_profile().descriptors(true);
        assert!(
            descriptors
                .iter()
                .any(|item| item.id == SettingId::HighBeam)
        );
        assert!(
            descriptors
                .iter()
                .any(|item| item.id == SettingId::RidingPreset)
        );
        assert!(
            !descriptors
                .iter()
                .any(|item| item.id == SettingId::Headlight)
        );
        assert!(
            !descriptors
                .iter()
                .any(|item| item.id == SettingId::PedalMode)
        );
    }

    #[test]
    fn completion_strategy_is_adapter_owned_and_explicit() {
        let available = Capabilities::from_supported_commands([CommandKind::SetLights]);
        let unknown = DeviceControlProfile::new(available, available);
        assert!(unknown.descriptors(false).is_empty());
        let aero = aero_control_profile().descriptors(true);
        for id in [
            SettingId::HighBeam,
            SettingId::PedalAngle,
            SettingId::RidingPreset,
            SettingId::ChargeLimitDiagnostic,
        ] {
            assert!(
                !aero
                    .iter()
                    .find(|entry| entry.id == id)
                    .unwrap()
                    .completion
                    .supports_readback(),
                "{id:?}"
            );
        }
        for id in [
            SettingId::PwmTiltback,
            SettingId::SpeedAlarmThreshold,
            SettingId::LateralTiltLimit,
            SettingId::BrakeOverpressureAlarm,
            SettingId::BeeperVolumePercent,
            SettingId::HighSpeedMode,
            SettingId::LowBatteryMode,
        ] {
            assert!(
                aero.iter()
                    .find(|entry| entry.id == id)
                    .unwrap()
                    .completion
                    .supports_readback(),
                "{id:?} has device readback; an unconfirmed write must not disable it"
            );
        }
        assert_eq!(
            aero.iter()
                .find(|entry| entry.id == SettingId::HighBeam)
                .unwrap()
                .write_verification,
            VerificationStatus::Unverified
        );
    }

    #[test]
    fn matching_completion_requires_a_typed_observation_binding() {
        for (adapter, profile) in [
            (SettingsAdapter::Aero, aero_control_profile()),
            (SettingsAdapter::Falcon, falcon_control_profile()),
        ] {
            for descriptor in profile.descriptors(true) {
                let binding = adapter.binding(descriptor.id).unwrap();
                assert_eq!(descriptor.completion, binding.completion());
                if descriptor.completion.supports_readback() {
                    assert!(
                        matches!(
                            binding.observation,
                            SettingObservationBinding::MatchingField(_)
                        ),
                        "matching readback without a typed observation: {:?}",
                        descriptor.id
                    );
                }
            }
        }
    }

    #[test]
    fn generic_capabilities_do_not_inherit_an_unrelated_settings_adapter() {
        let capabilities = Capabilities::from_supported_commands([CommandKind::SetLights]);
        let profile = DeviceControlProfile::new(capabilities, capabilities);
        assert!(profile.descriptors(false).is_empty());
        assert_eq!(
            profile
                .command(SettingId::PwmTiltback, DeviceSettingValue::Disabled, false)
                .unwrap_err(),
            SettingsRequestError::Unavailable
        );
    }

    #[test]
    fn protocol_adapters_do_not_share_vendor_setting_contracts() {
        let aero = aero_control_profile();
        let falcon = falcon_control_profile();

        assert!(
            aero.descriptors(true)
                .iter()
                .all(|descriptor| descriptor.id != SettingId::MaximumSpeed)
        );
        assert_eq!(
            aero.command(
                SettingId::MaximumSpeed,
                DeviceSettingValue::Number(480),
                true
            ),
            Err(SettingsRequestError::Unavailable)
        );
        assert_eq!(
            falcon
                .command(SettingId::PwmTiltback, DeviceSettingValue::Disabled, true)
                .unwrap_err(),
            SettingsRequestError::Unavailable
        );
        assert_eq!(
            aero.command(SettingId::PwmTiltback, DeviceSettingValue::Disabled, true)
                .unwrap(),
            DeviceCommand::SetSetting {
                id: SettingId::PwmTiltback,
                value: DeviceSettingValue::Disabled,
            }
        );
    }

    #[test]
    fn readable_alarm_options_do_not_grant_unsupported_writes() {
        let profile = falcon_control_profile();
        let descriptors = profile.descriptors(false);
        let alarm = descriptors
            .iter()
            .find(|entry| entry.id == SettingId::SpeedAlarmMode)
            .unwrap();
        let SettingControl::Choices(choices) = &alarm.control else {
            panic!("not choices")
        };
        assert_eq!(choices.len(), 4);
        assert_eq!(choices.iter().filter(|choice| choice.writable).count(), 2);
        assert_eq!(
            profile.command(
                SettingId::SpeedAlarmMode,
                DeviceSettingValue::Choice(2),
                false
            ),
            Err(SettingsRequestError::InvalidValue)
        );
    }

    #[test]
    fn uncertain_charge_meaning_cannot_be_enabled_by_validation_mode() {
        let profile = aero_control_profile();
        let descriptors = profile.descriptors(true);
        let charge = descriptors
            .iter()
            .find(|entry| entry.id == SettingId::ChargeLimitDiagnostic)
            .unwrap();
        assert_eq!(charge.access, SettingAccess::ReadOnly);
        assert_eq!(charge.control, SettingControl::ReadOnly);
        assert_eq!(
            profile.command(
                SettingId::ChargeLimitDiagnostic,
                DeviceSettingValue::Number(46),
                true
            ),
            Err(SettingsRequestError::ReadOnly)
        );
        assert!(
            profile
                .command(
                    SettingId::PwmTiltback,
                    DeviceSettingValue::Number(80),
                    false
                )
                .is_ok()
        );
        assert!(
            profile
                .command(SettingId::PwmTiltback, DeviceSettingValue::Number(80), true)
                .is_ok()
        );
    }

    #[test]
    fn ordinary_aero_catalog_encodes_every_supported_value_without_promoting_evidence() {
        let profile = aero_control_profile();
        let expected = [
            SettingId::HighBeam,
            SettingId::TiltbackSpeed,
            SettingId::PwmTiltback,
            SettingId::RidingPreset,
            SettingId::BrakeOverpressureAlarm,
            SettingId::PedalHardness,
            SettingId::DisplayBrightness,
            SettingId::BeeperVolumePercent,
            SettingId::DynamicAssist,
            SettingId::PedalDipCompensation,
            SettingId::LateralTiltLimit,
            SettingId::VoltageCorrection,
            SettingId::DisplayUnits,
            SettingId::HighSpeedMode,
            SettingId::LowBatteryMode,
            SettingId::TransportMode,
            SettingId::SpeedAlarmThreshold,
            SettingId::PedalAngle,
        ];
        let descriptors = profile.descriptors(false);
        assert_eq!(descriptors, profile.descriptors(true));
        assert_eq!(
            descriptors
                .iter()
                .filter(|item| item.access == SettingAccess::Writable)
                .count(),
            expected.len()
        );
        for id in expected {
            let descriptor = descriptors.iter().find(|item| item.id == id).unwrap();
            assert_eq!(descriptor.access, SettingAccess::Writable, "{id:?}");
            assert_eq!(
                descriptor.write_verification,
                VerificationStatus::Unverified,
                "{id:?}"
            );
            let values: Vec<_> = match &descriptor.control {
                SettingControl::Boolean => vec![
                    DeviceSettingValue::Boolean(false),
                    DeviceSettingValue::Boolean(true),
                ],
                SettingControl::Choices(choices) => choices
                    .iter()
                    .filter(|choice| choice.writable)
                    .map(|choice| DeviceSettingValue::Choice(choice.id))
                    .collect(),
                SettingControl::Number {
                    minimum,
                    maximum,
                    step,
                    can_disable,
                    ..
                } => {
                    let mut values: Vec<_> = (*minimum..=*maximum)
                        .step_by(*step as usize)
                        .map(DeviceSettingValue::Number)
                        .collect();
                    if *can_disable {
                        values.push(DeviceSettingValue::Disabled);
                    }
                    values
                }
                SettingControl::ReadOnly => panic!("ordinary setting is read-only: {id:?}"),
            };
            assert!(!values.is_empty());
            for value in values {
                let command = profile.command(id, value, false).unwrap();
                assert_eq!(
                    command.safety_class(),
                    cutout_core::SafetyClass::StationaryOnly
                );
                for mode in [
                    crate::VeteranCommandMode::Binary,
                    crate::VeteranCommandMode::Ascii,
                ] {
                    if let Some(encoded) = crate::NosfetDialect::encode_in_mode(command, mode) {
                        assert!(!encoded.payload.as_slice().is_empty());
                        assert_eq!(encoded.mode, cutout_core::WriteMode::WithoutResponse);
                    } else {
                        let sequence = crate::NosfetDialect::encode_settings_sequence(command)
                            .unwrap_or_else(|| {
                                panic!("missing encoder: {id:?} {value:?} {mode:?}")
                            });
                        assert!(!sequence.steps.is_empty());
                        assert!(
                            sequence.steps.iter().all(|step| {
                                step.mode == cutout_core::WriteMode::WithoutResponse
                            })
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn falcon_unencoded_toggles_cannot_acquire_write_authority() {
        let profile = falcon_control_profile();
        for validation_mode in [false, true] {
            for id in [SettingId::Taillight, SettingId::AccelerationAssist] {
                assert!(
                    profile
                        .descriptors(validation_mode)
                        .iter()
                        .all(|descriptor| descriptor.id != id)
                );
                for value in [false, true] {
                    assert_eq!(
                        profile.command(id, DeviceSettingValue::Boolean(value), validation_mode),
                        Err(SettingsRequestError::Unavailable)
                    );
                }
            }
        }
    }

    #[test]
    fn every_falcon_writable_descriptor_has_a_checked_wire_binding() {
        let profile = falcon_control_profile();
        for descriptor in profile.descriptors(false) {
            if descriptor.access != SettingAccess::Writable {
                continue;
            }
            let values: Vec<_> = match descriptor.control {
                SettingControl::Boolean => vec![
                    DeviceSettingValue::Boolean(false),
                    DeviceSettingValue::Boolean(true),
                ],
                SettingControl::Choices(choices) => choices
                    .into_iter()
                    .filter(|choice| choice.writable)
                    .map(|choice| DeviceSettingValue::Choice(choice.id))
                    .collect(),
                SettingControl::Number {
                    minimum,
                    maximum,
                    step,
                    can_disable,
                    ..
                } => {
                    let mut values = vec![DeviceSettingValue::Number(minimum)];
                    if maximum != minimum {
                        values.push(DeviceSettingValue::Number(maximum));
                    }
                    if can_disable {
                        values.push(DeviceSettingValue::Disabled);
                    }
                    assert!(step > 0);
                    values
                }
                SettingControl::ReadOnly => panic!("writable descriptor is read-only"),
            };
            assert!(!values.is_empty(), "{:?}", descriptor.id);
            for value in values {
                let command = profile
                    .command(descriptor.id, value, false)
                    .unwrap_or_else(|error| panic!("{:?} {value:?}: {error}", descriptor.id));
                assert!(
                    crate::FalconDialect::encode(command).is_some()
                        || crate::FalconDialect::encode_settings_sequence(command).is_some(),
                    "missing Falcon encoder for {:?} {:?}",
                    descriptor.id,
                    value
                );
            }
        }
    }

    #[test]
    fn numeric_descriptors_and_checked_commands_agree_on_every_value() {
        for profile in [aero_control_profile(), falcon_control_profile()] {
            for descriptor in profile.descriptors(true) {
                let SettingControl::Number {
                    minimum,
                    maximum,
                    step,
                    ..
                } = descriptor.control
                else {
                    continue;
                };
                for value in -256..=256 {
                    let permitted = descriptor.access == SettingAccess::Writable
                        && (minimum..=maximum).contains(&value)
                        && (value - minimum) % step == 0;
                    assert_eq!(
                        profile
                            .command(descriptor.id, DeviceSettingValue::Number(value), true)
                            .is_ok(),
                        permitted,
                        "{:?}: {value}",
                        descriptor.id
                    );
                }
            }
        }
    }
    #[test]
    fn descriptors_select_help_and_value_semantics_in_rust() {
        let aero = aero_control_profile().descriptors(true);
        let pwm = aero
            .iter()
            .find(|item| item.id == SettingId::PwmTiltback)
            .unwrap();
        assert_eq!(pwm.help_key, Some("settings.pwm_tiltback.help"));
        assert_eq!(
            pwm.value_semantics_key,
            Some("settings.pwm_tiltback.semantics")
        );
        let speed = aero
            .iter()
            .find(|item| item.id == SettingId::TiltbackSpeed)
            .unwrap();
        assert_eq!(speed.value_semantics_key, Some("controls.speed_steps"));
        let raw_charge = aero
            .iter()
            .find(|item| item.id == SettingId::ChargeLimitDiagnostic)
            .unwrap();
        assert_eq!(
            raw_charge.help_key,
            Some("settings.charge_limit_diagnostic.help")
        );
    }

    #[test]
    fn unverified_begode_pattern_values_are_observable_but_read_only() {
        let profile = falcon_control_profile();
        let pattern = profile
            .descriptors(false)
            .into_iter()
            .find(|item| item.id == SettingId::LightingPattern)
            .unwrap();
        assert_eq!(pattern.access, SettingAccess::ReadOnly);
        let SettingControl::Choices(choices) = pattern.control else {
            panic!("pattern must retain its raw choices");
        };
        assert!(choices.iter().all(|choice| !choice.writable));
        assert_eq!(
            profile.command(
                SettingId::LightingPattern,
                DeviceSettingValue::Choice(0),
                true
            ),
            Err(SettingsRequestError::ReadOnly)
        );
    }
}
