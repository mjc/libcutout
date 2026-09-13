use cutout_core::{
    InvalidPwmDutyAlarmThreshold, PhoneAlarmActions, PhoneAlarmDeliveryRequest, PhoneAlarmEvent,
    PhoneAlarmManagerError, PhoneAlarmPreferences, RideStopReason, RideWarning,
};

use crate::{MobileVescRideStopReasonDto, MobileVescRideWarningDto};

/// Per-device Rust-owned phone alarm preferences.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneAlarmPreferencesDto {
    /// Platform-scoped identity these preferences belong to.
    pub device_identity: String,
    /// Whether phone-generated ride alarms are enabled.
    pub enabled: bool,
    /// Consumed PWM duty threshold, from 1 through 100 percent.
    pub pwm_duty_percent: u8,
    /// Complementary unused PWM headroom, from 0 through 99 percent.
    pub pwm_headroom_percent: u8,
}

/// Native delivery capabilities supplied to the Rust session owner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneAlarmDeliveryCapabilityDto {
    /// Whether native notification settings currently permit scheduling.
    pub can_schedule: bool,
    /// Whether scheduled alarms should request a sound.
    pub plays_sound: bool,
}

/// Normalized alarm payload returned for native delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneAlarmEventDto {
    /// PWM duty reached the configured threshold.
    PwmDuty {
        /// Consumed duty in percent.
        duty_percent: u8,
        /// Complementary unused headroom in percent.
        headroom_percent: u8,
    },
    /// A typed VESC controller warning is active.
    ControllerWarning {
        /// Active warning.
        warning: MobileVescRideWarningDto,
    },
    /// A typed VESC stop reason is active.
    ControllerStop {
        /// Active stop reason.
        reason: MobileVescRideStopReasonDto,
    },
}

/// One Rust-reserved native delivery request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneAlarmDeliveryRequestDto {
    /// Monotonic Rust-owned request identity used for completion and cancellation.
    pub id: u64,
    /// Normalized event to deliver.
    pub event: MobilePhoneAlarmEventDto,
    /// Whether current native capability permits sound for this delivery.
    pub plays_sound: bool,
}

/// Native effects from one Rust-owned phone alarm transition.
#[derive(Clone, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneAlarmActionsDto {
    /// Deliveries native code should schedule.
    pub schedule: Vec<MobilePhoneAlarmDeliveryRequestDto>,
    /// Previously scheduled request identities native code should cancel.
    pub cancel_request_ids: Vec<u64>,
}

/// Stable errors from phone alarm settings owned by the session state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobilePhoneAlarmError {
    /// No device is selected.
    #[error("no active device")]
    NoActiveDevice,
    /// The selected platform identity is invalid.
    #[error("invalid device identity")]
    InvalidDeviceIdentity,
    /// Consumed duty must be from 1 through 100 percent.
    #[error("invalid PWM duty threshold")]
    InvalidPwmDutyThreshold,
    /// Remaining headroom must be from 0 through 99 percent.
    #[error("invalid PWM headroom threshold")]
    InvalidPwmHeadroomThreshold,
    /// The bounded in-memory preference store is full.
    #[error("too many phone alarm preference devices")]
    TooManyDevices,
    /// The active device changed before a settings write completed.
    #[error("active phone alarm device changed")]
    DeviceIdentityChanged,
    /// Durable preference storage failed.
    #[error("phone alarm preference storage failed")]
    StorageFailure,
}

impl MobilePhoneAlarmPreferencesDto {
    pub(crate) fn from_core(device_identity: String, preferences: PhoneAlarmPreferences) -> Self {
        Self {
            device_identity,
            enabled: preferences.enabled(),
            pwm_duty_percent: preferences.duty_percent(),
            pwm_headroom_percent: preferences.headroom_percent(),
        }
    }
}

impl From<PhoneAlarmManagerError> for MobilePhoneAlarmError {
    fn from(error: PhoneAlarmManagerError) -> Self {
        match error {
            PhoneAlarmManagerError::NoActiveDevice => Self::NoActiveDevice,
            PhoneAlarmManagerError::InvalidDeviceIdentity => Self::InvalidDeviceIdentity,
            PhoneAlarmManagerError::InvalidPwmDutyThreshold(_) => Self::InvalidPwmDutyThreshold,
            PhoneAlarmManagerError::InvalidPwmHeadroomThreshold(_) => {
                Self::InvalidPwmHeadroomThreshold
            }
            PhoneAlarmManagerError::TooManyDevices => Self::TooManyDevices,
            PhoneAlarmManagerError::DeviceIdentityChanged => Self::DeviceIdentityChanged,
        }
    }
}

impl From<InvalidPwmDutyAlarmThreshold> for MobilePhoneAlarmError {
    fn from(_: InvalidPwmDutyAlarmThreshold) -> Self {
        Self::InvalidPwmDutyThreshold
    }
}

impl MobilePhoneAlarmActionsDto {
    pub(crate) fn from_core(actions: PhoneAlarmActions, plays_sound: bool) -> Self {
        let (schedule, cancel_request_ids) = actions.into_parts();
        Self {
            schedule: schedule
                .into_iter()
                .map(|request| MobilePhoneAlarmDeliveryRequestDto::from_core(request, plays_sound))
                .collect(),
            cancel_request_ids,
        }
    }

    pub(crate) fn cancellations(cancel_request_ids: Vec<u64>) -> Self {
        Self {
            schedule: Vec::new(),
            cancel_request_ids,
        }
    }
}

impl MobilePhoneAlarmDeliveryRequestDto {
    fn from_core(request: PhoneAlarmDeliveryRequest, plays_sound: bool) -> Self {
        Self {
            id: request.id(),
            event: request.event().into(),
            plays_sound,
        }
    }
}

impl From<PhoneAlarmEvent> for MobilePhoneAlarmEventDto {
    fn from(event: PhoneAlarmEvent) -> Self {
        match event {
            PhoneAlarmEvent::PwmDuty {
                duty_percent,
                headroom_percent,
            } => Self::PwmDuty {
                duty_percent,
                headroom_percent,
            },
            PhoneAlarmEvent::ControllerWarning(warning) => Self::ControllerWarning {
                warning: warning.into(),
            },
            PhoneAlarmEvent::ControllerStop(reason) => Self::ControllerStop {
                reason: reason.into(),
            },
        }
    }
}

impl From<MobileVescRideWarningDto> for RideWarning {
    fn from(warning: MobileVescRideWarningDto) -> Self {
        match warning {
            MobileVescRideWarningDto::None => Self::None,
            MobileVescRideWarningDto::LowVoltage => Self::LowVoltage,
            MobileVescRideWarningDto::HighVoltage => Self::HighVoltage,
            MobileVescRideWarningDto::MosfetTemperature => Self::MosfetTemperature,
            MobileVescRideWarningDto::MotorTemperature => Self::MotorTemperature,
            MobileVescRideWarningDto::Current => Self::Current,
            MobileVescRideWarningDto::DutyPushback => Self::DutyPushback,
            MobileVescRideWarningDto::SpeedPushback => Self::SpeedPushback,
            MobileVescRideWarningDto::TemperaturePushback => Self::TemperaturePushback,
            MobileVescRideWarningDto::Wheelslip => Self::Wheelslip,
            MobileVescRideWarningDto::Sensors => Self::Sensors,
            MobileVescRideWarningDto::LowBattery => Self::LowBattery,
            MobileVescRideWarningDto::Error => Self::Error,
            MobileVescRideWarningDto::BmsConnection => Self::BmsConnection,
            MobileVescRideWarningDto::Unknown => Self::Unknown,
        }
    }
}

impl From<RideWarning> for MobileVescRideWarningDto {
    fn from(warning: RideWarning) -> Self {
        match warning {
            RideWarning::None => Self::None,
            RideWarning::LowVoltage => Self::LowVoltage,
            RideWarning::HighVoltage => Self::HighVoltage,
            RideWarning::MosfetTemperature => Self::MosfetTemperature,
            RideWarning::MotorTemperature => Self::MotorTemperature,
            RideWarning::Current => Self::Current,
            RideWarning::DutyPushback => Self::DutyPushback,
            RideWarning::SpeedPushback => Self::SpeedPushback,
            RideWarning::TemperaturePushback => Self::TemperaturePushback,
            RideWarning::Wheelslip => Self::Wheelslip,
            RideWarning::Sensors => Self::Sensors,
            RideWarning::LowBattery => Self::LowBattery,
            RideWarning::Error => Self::Error,
            RideWarning::BmsConnection => Self::BmsConnection,
            RideWarning::Unknown => Self::Unknown,
        }
    }
}

impl From<MobileVescRideStopReasonDto> for RideStopReason {
    fn from(reason: MobileVescRideStopReasonDto) -> Self {
        match reason {
            MobileVescRideStopReasonDto::None => Self::None,
            MobileVescRideStopReasonDto::Pitch => Self::Pitch,
            MobileVescRideStopReasonDto::Roll => Self::Roll,
            MobileVescRideStopReasonDto::SwitchHalf => Self::SwitchHalf,
            MobileVescRideStopReasonDto::SwitchFull => Self::SwitchFull,
            MobileVescRideStopReasonDto::Reverse => Self::Reverse,
            MobileVescRideStopReasonDto::QuickStop => Self::QuickStop,
        }
    }
}

impl From<RideStopReason> for MobileVescRideStopReasonDto {
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
