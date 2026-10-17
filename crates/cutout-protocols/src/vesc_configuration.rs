//! Source-pinned complete VESC 7.00 MC and APP configuration images.
//!
//! This codec is deliberately separate from the public read-only telemetry codec and
//! the Refloat package settings schema. It owns validated full-image preservation and
//! emits a single checked field patch without changing any other serialized byte.

use cutout_core::{DeviceSettingValue, SettingId, VerificationStatus};
use thiserror::Error;

use crate::device_settings::{
    SettingAccess, SettingApplyEffect, SettingChoice, SettingCompletionStrategy, SettingControl,
    SettingDescriptor, SettingGroup, SettingUnit,
};

const MC_SIGNATURE: u32 = 1_470_992_211;
const MC_IMAGE_LEN: usize = 488;
const APP_SIGNATURE: u32 = 2_384_027_283;
const APP_IMAGE_LEN: usize = 284;
const MC_GET: u8 = 14;
const MC_SET: u8 = 13;
const APP_GET: u8 = 17;
const APP_SET: u8 = 16;

/// One of the complete VESC 7.00 configuration records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VescConfigurationKind {
    /// Motor controller configuration (`MC_CONFIGURATION`).
    Motor,
    /// Application/controller configuration (`APPCONF`).
    App,
}

impl VescConfigurationKind {
    const fn layout(self) -> (usize, u32) {
        match self {
            Self::Motor => (MC_IMAGE_LEN, MC_SIGNATURE),
            Self::App => (APP_IMAGE_LEN, APP_SIGNATURE),
        }
    }

    pub(crate) const fn read_command_payload(self) -> &'static [u8] {
        match self {
            Self::Motor => &[MC_GET],
            Self::App => &[APP_GET],
        }
    }

    const fn set_command(self) -> u8 {
        match self {
            Self::Motor => MC_SET,
            Self::App => APP_SET,
        }
    }
}

/// Error returned while validating or changing a complete source-pinned config image.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum VescConfigurationError {
    /// The supplied record does not have the complete pinned size.
    #[error("VESC configuration image has the wrong length")]
    Length,
    /// The supplied record carries another firmware/schema signature.
    #[error("VESC configuration signature is unsupported")]
    Signature,
    /// The setting does not belong to this configuration kind.
    #[error("setting is not in this VESC configuration")]
    UnsupportedSetting,
    /// The value is outside the pinned XML domain or cannot round-trip exactly.
    #[error("setting value is outside the pinned VESC 7.00 domain")]
    InvalidValue,
    /// The setting changes transport selection or depends on unverified hardware routing.
    #[error("setting depends on an unverified controller or hardware endpoint")]
    ReadOnly,
    /// The complete image would leave firmware-interpolated thresholds in the wrong order.
    #[error("paired VESC configuration thresholds are inconsistent")]
    CoupledConstraint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WireType {
    F32Auto,
    ScaledI16 { scale: u16 },
    U8,
    U16,
    U32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadOnlyReason {
    Writable,
    /// Changes the identity used to route requests and replies.
    ControllerIdentity,
    /// Can change the active transport endpoint or CAN protocol.
    TransportSelection,
    /// Selects application or input hardware whose endpoints are not in this profile.
    HardwareEndpoint,
    /// Selects an IMU model without capability evidence for attached hardware.
    ImuHardware,
    /// XML defines a multi-option bitfield that the shared value type cannot represent.
    BitfieldValueShape,
}

#[derive(Clone, Copy, Debug)]
struct FieldSpec {
    id: SettingId,
    kind: VescConfigurationKind,
    key: &'static str,
    offset: usize,
    width: usize,
    wire: WireType,
    minimum: i32,
    maximum: i32,
    editor_step: i32,
    precision: u8,
    editor_scale: i32,
    unit: SettingUnit,
    group: SettingGroup,
    read_only: ReadOnlyReason,
    choices: &'static [&'static str],
    boolean: bool,
    bitfield: bool,
}

macro_rules! numeric {
    ($id:ident, $kind:ident, $key:literal, $offset:literal, $width:literal,
     $wire:expr, $min:literal, $max:literal, $step:literal, $precision:literal,
     $editor_scale:literal, $unit:ident, $group:ident, $access:ident) => {
        FieldSpec {
            id: SettingId::$id,
            kind: VescConfigurationKind::$kind,
            key: $key,
            offset: $offset,
            width: $width,
            wire: $wire,
            minimum: $min,
            maximum: $max,
            editor_step: $step,
            precision: $precision,
            editor_scale: $editor_scale,
            unit: SettingUnit::$unit,
            group: SettingGroup::$group,
            read_only: ReadOnlyReason::$access,
            choices: &[],
            boolean: false,
            bitfield: false,
        }
    };
}

macro_rules! choices {
    ($id:ident, $kind:ident, $key:literal, $offset:literal, $options:expr,
     $group:ident, $access:ident) => {
        FieldSpec {
            id: SettingId::$id,
            kind: VescConfigurationKind::$kind,
            key: $key,
            offset: $offset,
            width: 1,
            wire: WireType::U8,
            minimum: 0,
            // The source fields use one-byte choice indices; exact option validity is
            // enforced by the choice table in decoding and encoding.
            maximum: 255,
            editor_step: 1,
            precision: 0,
            editor_scale: 1,
            unit: SettingUnit::Raw,
            group: SettingGroup::$group,
            read_only: ReadOnlyReason::$access,
            choices: $options,
            boolean: false,
            bitfield: false,
        }
    };
}

macro_rules! boolean {
    ($id:ident, $kind:ident, $key:literal, $offset:literal, $group:ident) => {
        FieldSpec {
            id: SettingId::$id,
            kind: VescConfigurationKind::$kind,
            key: $key,
            offset: $offset,
            width: 1,
            wire: WireType::U8,
            minimum: 0,
            maximum: 1,
            editor_step: 1,
            precision: 0,
            editor_scale: 1,
            unit: SettingUnit::Raw,
            group: SettingGroup::$group,
            read_only: ReadOnlyReason::Writable,
            choices: &[],
            boolean: true,
            bitfield: false,
        }
    };
}

macro_rules! bitfield {
    ($id:ident, $kind:ident, $key:literal, $offset:literal, $options:expr, $group:ident) => {
        FieldSpec {
            id: SettingId::$id,
            kind: VescConfigurationKind::$kind,
            key: $key,
            offset: $offset,
            width: 1,
            wire: WireType::U8,
            minimum: 0,
            maximum: 255,
            editor_step: 1,
            precision: 0,
            editor_scale: 1,
            unit: SettingUnit::Raw,
            group: SettingGroup::$group,
            read_only: ReadOnlyReason::BitfieldValueShape,
            choices: $options,
            boolean: false,
            bitfield: true,
        }
    };
}

const VESC_SI_BATTERY_TYPE: &[&str] = &[
    "BATTERY_TYPE_LIION_3_0__4_2",
    "BATTERY_TYPE_LIIRON_2_6__3_6",
    "BATTERY_TYPE_LEAD_ACID",
];
const VESC_BMS_TYPE: &[&str] = &["None", "VESC BMS"];
const VESC_BMS_LIMIT_MODE: &[&str] = &[
    "Overtemp",
    "SOC",
    "VCell Min",
    "VCell Max",
    "Unused",
    "Unused",
    "Unused",
    "Unused",
];
const VESC_BMS_FWD_CAN_MODE: &[&str] = &["Disabled", "USB Only", "Any Interface"];
const VESC_CAN_BAUD_RATE: &[&str] = &[
    "CAN_BAUD_125K",
    "CAN_BAUD_250K",
    "CAN_BAUD_500K",
    "CAN_BAUD_1M",
    "CAN_BAUD_10K",
    "CAN_BAUD_20K",
    "CAN_BAUD_50K",
    "CAN_BAUD_75K",
    "CAN_BAUD_100K",
];
const VESC_CAN_MODE: &[&str] = &["VESC", "UAVCAN", "Comm Bridge", "Unused", "UAVCAN and VESC"];
const VESC_APP_TO_USE: &[&str] = &[
    "No App",
    "PPM",
    "ADC",
    "UART",
    "PPM and UART",
    "ADC and UART",
    "Nunchuk (I2C, Nyko Kama)",
    "NRF",
    "Custom User App",
    "PAS",
    "ADC and PAS",
];
const VESC_KILL_SW_MODE: &[&str] = &["Disabled", "PPM Low", "PPM High", "ADC2 Low", "ADC2 High"];
const VESC_APP_PPM_CONF_CTRL_TYPE: &[&str] = &[
    "Off",
    "Current",
    "Current No Reverse",
    "Current No Reverse With Brake",
    "Duty Cycle",
    "Duty Cycle No Reverse",
    "PID Speed Control",
    "PID Speed Control No Reverse",
    "Current Hyst Reverse With Brake",
    "Current Smart Reverse",
    "PID Position Control: 180°",
    "PID Position Control: 360°",
];
const VESC_APP_PPM_CONF_SAFE_START: &[&str] = &["Disabled", "Regular", "No Fault"];
const VESC_APP_ADC_CONF_CTRL_TYPE: &[&str] = &[
    "Off",
    "Current",
    "Current Reverse Center",
    "Current Reverse Button",
    "Current Reverse ADC2 Brake Button",
    "Current Reverse Button Brake Center",
    "Current No Reverse Brake Center",
    "Current No Reverse Brake Button",
    "Current No Reverse Brake ADC2",
    "Duty Cycle",
    "Duty Cycle Reverse Center",
    "Duty Cycle Reverse Button",
    "PID Speed",
    "PID Speed Reverse Center",
    "PID Speed Reverse Button",
];
const VESC_APP_ADC_CONF_SAFE_START: &[&str] = &["Disabled", "Regular", "No Fault"];
const VESC_APP_CHUK_CONF_CTRL_TYPE: &[&str] = &[
    "Off",
    "Current",
    "Current No Reverse",
    "Current Bidirectional",
];
const VESC_IMU_CONF_TYPE: &[&str] = &[
    "IMU_TYPE_OFF",
    "IMU_TYPE_INTERNAL",
    "IMU_TYPE_EXTERNAL_MPU9X50",
    "IMU_TYPE_EXTERNAL_ICM20948",
    "IMU_TYPE_EXTERNAL_BMI160",
    "IMU_TYPE_EXTERNAL_LSM6DS3",
];
const VESC_IMU_CONF_MODE: &[&str] = &[
    "AHRS_MODE_MADGWICK",
    "AHRS_MODE_MAHONY",
    "AHRS_MODE_MADGWICK_FUSION",
];
const VESC_IMU_CONF_FILTER: &[&str] = &["IMU_FILTER_LOW", "IMU_FILTER_MEDIUM", "IMU_FILTER_HIGH"];

// Offsets include the four-byte schema signature.
const FIELDS: &[FieldSpec] = &[
    numeric!(
        MotorDriveCurrentLimit,
        Motor,
        "l_current_max",
        8,
        4,
        WireType::F32Auto,
        0,
        500_000,
        100,
        2,
        1,
        Amperes,
        Limits,
        Writable
    ),
    numeric!(
        MotorBrakingCurrentLimit,
        Motor,
        "l_current_min",
        12,
        4,
        WireType::F32Auto,
        0,
        500_000,
        100,
        2,
        1,
        Amperes,
        Limits,
        Writable
    ),
    numeric!(
        BatteryCurrentOutLimit,
        Motor,
        "l_in_current_max",
        16,
        4,
        WireType::F32Auto,
        0,
        500_000,
        100,
        2,
        1,
        Amperes,
        Limits,
        Writable
    ),
    numeric!(
        BatteryCurrentInLimit,
        Motor,
        "l_in_current_min",
        20,
        4,
        WireType::F32Auto,
        0,
        500_000,
        100,
        2,
        1,
        Amperes,
        Limits,
        Writable
    ),
    numeric!(
        AbsoluteMotorCurrentLimit,
        Motor,
        "l_abs_current_max",
        28,
        4,
        WireType::F32Auto,
        0,
        500_000,
        100,
        2,
        1,
        Amperes,
        Limits,
        Writable
    ),
    numeric!(
        InputVoltageMinimum,
        Motor,
        "l_min_vin",
        50,
        2,
        WireType::ScaledI16 { scale: 10 },
        0,
        11_000,
        10,
        1,
        1,
        Volts,
        Limits,
        Writable
    ),
    numeric!(
        InputVoltageMaximum,
        Motor,
        "l_max_vin",
        52,
        2,
        WireType::ScaledI16 { scale: 10 },
        0,
        11_000,
        10,
        1,
        1,
        Volts,
        Limits,
        Writable
    ),
    numeric!(
        BatteryCutbackStart,
        Motor,
        "l_battery_cut_start",
        54,
        2,
        WireType::ScaledI16 { scale: 10 },
        0,
        11_000,
        10,
        1,
        1,
        Volts,
        Limits,
        Writable
    ),
    numeric!(
        BatteryCutbackEnd,
        Motor,
        "l_battery_cut_end",
        56,
        2,
        WireType::ScaledI16 { scale: 10 },
        0,
        11_000,
        10,
        1,
        1,
        Volts,
        Limits,
        Writable
    ),
    numeric!(
        RegenerativeBatteryCutbackStart,
        Motor,
        "l_battery_regen_cut_start",
        58,
        2,
        WireType::ScaledI16 { scale: 10 },
        0,
        11_000,
        10,
        1,
        1,
        Volts,
        Limits,
        Writable
    ),
    numeric!(
        RegenerativeBatteryCutbackEnd,
        Motor,
        "l_battery_regen_cut_end",
        60,
        2,
        WireType::ScaledI16 { scale: 10 },
        0,
        11_000,
        10,
        1,
        1,
        Volts,
        Limits,
        Writable
    ),
    numeric!(
        FetTemperatureCutbackStart,
        Motor,
        "l_temp_fet_start",
        63,
        1,
        WireType::U8,
        0,
        190,
        1,
        0,
        1,
        Celsius,
        Limits,
        Writable
    ),
    numeric!(
        FetTemperatureCutbackEnd,
        Motor,
        "l_temp_fet_end",
        64,
        1,
        WireType::U8,
        0,
        190,
        1,
        0,
        1,
        Celsius,
        Limits,
        Writable
    ),
    numeric!(
        MotorTemperatureCutbackStart,
        Motor,
        "l_temp_motor_start",
        65,
        1,
        WireType::U8,
        0,
        190,
        1,
        0,
        1,
        Celsius,
        Limits,
        Writable
    ),
    numeric!(
        MotorTemperatureCutbackEnd,
        Motor,
        "l_temp_motor_end",
        66,
        1,
        WireType::U8,
        0,
        190,
        1,
        0,
        1,
        Celsius,
        Limits,
        Writable
    ),
    numeric!(
        MotorPowerOutLimit,
        Motor,
        "l_watt_max",
        73,
        4,
        WireType::F32Auto,
        0,
        20_000_000,
        10,
        1,
        1,
        Watts,
        Limits,
        Writable
    ),
    numeric!(
        RegenerativeMotorPowerLimit,
        Motor,
        "l_watt_min",
        77,
        4,
        WireType::F32Auto,
        0,
        20_000_000,
        10,
        1,
        1,
        Watts,
        Limits,
        Writable
    ),
    numeric!(
        MotorDriveCurrentLimitScale,
        Motor,
        "l_current_max_scale",
        81,
        2,
        WireType::ScaledI16 { scale: 10_000 },
        0,
        100,
        5,
        2,
        1,
        Scalar,
        Limits,
        Writable
    ),
    numeric!(
        MotorBrakingCurrentLimitScale,
        Motor,
        "l_current_min_scale",
        83,
        2,
        WireType::ScaledI16 { scale: 10_000 },
        0,
        100,
        5,
        2,
        1,
        Scalar,
        Limits,
        Writable
    ),
    numeric!(
        MotorPoleCount,
        Motor,
        "si_motor_poles",
        452,
        1,
        WireType::U8,
        2,
        254,
        2,
        0,
        1,
        Raw,
        Hardware,
        Writable
    ),
    numeric!(
        MotorGearRatio,
        Motor,
        "si_gear_ratio",
        453,
        4,
        WireType::F32Auto,
        100,
        9_999_000,
        100,
        3,
        1,
        Scalar,
        Hardware,
        Writable
    ),
    numeric!(
        WheelDiameter,
        Motor,
        "si_wheel_diameter",
        457,
        4,
        WireType::F32Auto,
        100,
        999_900_000,
        100,
        2,
        1000,
        Millimetres,
        Hardware,
        Writable
    ),
    choices!(
        BatteryChemistry,
        Motor,
        "si_battery_type",
        461,
        VESC_SI_BATTERY_TYPE,
        Battery,
        Writable
    ),
    numeric!(
        BatterySeriesCellCount,
        Motor,
        "si_battery_cells",
        462,
        1,
        WireType::U8,
        1,
        255,
        1,
        0,
        1,
        Raw,
        Battery,
        Writable
    ),
    numeric!(
        BatteryCapacity,
        Motor,
        "si_battery_ah",
        463,
        4,
        WireType::F32Auto,
        0,
        1_000_000,
        100,
        3,
        1,
        AmpHours,
        Battery,
        Writable
    ),
    choices!(
        BmsType,
        Motor,
        "bms.type",
        471,
        VESC_BMS_TYPE,
        Battery,
        Writable
    ),
    bitfield!(
        BmsLimitMode,
        Motor,
        "bms.limit_mode",
        472,
        VESC_BMS_LIMIT_MODE,
        Diagnostics
    ),
    // These ordinary controls project the four documented VESC BMS enable bits.
    // Encoding changes only its own bit and preserves future/unknown mask bits.
    boolean!(
        BmsTemperatureLimitingEnabled,
        Motor,
        "bms.limit_mode.temperature_enabled",
        472,
        Battery
    ),
    boolean!(
        BmsSocLimitingEnabled,
        Motor,
        "bms.limit_mode.soc_enabled",
        472,
        Battery
    ),
    boolean!(
        BmsMinimumCellVoltageLimitingEnabled,
        Motor,
        "bms.limit_mode.vmin_enabled",
        472,
        Battery
    ),
    boolean!(
        BmsMaximumCellVoltageLimitingEnabled,
        Motor,
        "bms.limit_mode.vmax_enabled",
        472,
        Battery
    ),
    numeric!(
        BmsTemperatureLimitStart,
        Motor,
        "bms.t_limit_start",
        473,
        1,
        WireType::U8,
        0,
        99,
        1,
        0,
        1,
        Celsius,
        Battery,
        Writable
    ),
    numeric!(
        BmsTemperatureLimitEnd,
        Motor,
        "bms.t_limit_end",
        474,
        1,
        WireType::U8,
        0,
        99,
        1,
        0,
        1,
        Celsius,
        Battery,
        Writable
    ),
    numeric!(
        BmsSocLimitStart,
        Motor,
        "bms.soc_limit_start",
        475,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        100,
        1,
        2,
        1,
        Scalar,
        Battery,
        Writable
    ),
    numeric!(
        BmsSocLimitEnd,
        Motor,
        "bms.soc_limit_end",
        477,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        100,
        1,
        2,
        1,
        Scalar,
        Battery,
        Writable
    ),
    numeric!(
        BmsCellMinimumVoltageLimitStart,
        Motor,
        "bms.vmin_limit_start",
        479,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        600,
        10,
        2,
        1,
        Volts,
        Battery,
        Writable
    ),
    numeric!(
        BmsCellMinimumVoltageLimitEnd,
        Motor,
        "bms.vmin_limit_end",
        481,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        600,
        10,
        2,
        1,
        Volts,
        Battery,
        Writable
    ),
    numeric!(
        BmsCellMaximumVoltageLimitStart,
        Motor,
        "bms.vmax_limit_start",
        483,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        600,
        10,
        2,
        1,
        Volts,
        Battery,
        Writable
    ),
    numeric!(
        BmsCellMaximumVoltageLimitEnd,
        Motor,
        "bms.vmax_limit_end",
        485,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        600,
        10,
        2,
        1,
        Volts,
        Battery,
        Writable
    ),
    choices!(
        BmsCanForwardMode,
        Motor,
        "bms.fwd_can_mode",
        487,
        VESC_BMS_FWD_CAN_MODE,
        Battery,
        TransportSelection
    ),
    numeric!(
        ControllerId,
        App,
        "controller_id",
        4,
        1,
        WireType::U8,
        0,
        255,
        1,
        0,
        1,
        Raw,
        Hardware,
        ControllerIdentity
    ),
    numeric!(
        CommandTimeout,
        App,
        "timeout_msec",
        5,
        4,
        WireType::U32,
        0,
        30_000_000,
        1,
        0,
        1,
        Milliseconds,
        Limits,
        Writable
    ),
    numeric!(
        CommandTimeoutBrakeCurrent,
        App,
        "timeout_brake_current",
        9,
        4,
        WireType::F32Auto,
        0,
        50_000,
        100,
        2,
        1,
        Amperes,
        Limits,
        Writable
    ),
    numeric!(
        CanStatusRate1,
        App,
        "can_status_rate_1",
        13,
        2,
        WireType::U16,
        0,
        10_000,
        1,
        0,
        1,
        Hertz,
        Hardware,
        Writable
    ),
    numeric!(
        CanStatusRate2,
        App,
        "can_status_rate_2",
        15,
        2,
        WireType::U16,
        0,
        10_000,
        1,
        0,
        1,
        Hertz,
        Hardware,
        Writable
    ),
    choices!(
        CanBaudRate,
        App,
        "can_baud_rate",
        19,
        VESC_CAN_BAUD_RATE,
        Hardware,
        TransportSelection
    ),
    choices!(
        CanMode,
        App,
        "can_mode",
        23,
        VESC_CAN_MODE,
        Hardware,
        TransportSelection
    ),
    choices!(
        ApplicationToUse,
        App,
        "app_to_use",
        33,
        VESC_APP_TO_USE,
        Hardware,
        HardwareEndpoint
    ),
    choices!(
        KillSwitchMode,
        App,
        "kill_sw_mode",
        32,
        VESC_KILL_SW_MODE,
        Hardware,
        HardwareEndpoint
    ),
    choices!(
        PpmControlMode,
        App,
        "app_ppm_conf.ctrl_type",
        34,
        VESC_APP_PPM_CONF_CTRL_TYPE,
        Hardware,
        HardwareEndpoint
    ),
    numeric!(
        PpmHysteresis,
        App,
        "app_ppm_conf.hyst",
        39,
        4,
        WireType::F32Auto,
        0,
        100,
        1,
        0,
        100,
        Percent,
        Hardware,
        Writable
    ),
    numeric!(
        PpmPulseStart,
        App,
        "app_ppm_conf.pulse_start",
        43,
        4,
        WireType::F32Auto,
        0,
        1_000_000,
        1000,
        4,
        1,
        Milliseconds,
        Hardware,
        Writable
    ),
    numeric!(
        PpmPulseEnd,
        App,
        "app_ppm_conf.pulse_end",
        47,
        4,
        WireType::F32Auto,
        0,
        1_000_000,
        1000,
        4,
        1,
        Milliseconds,
        Hardware,
        Writable
    ),
    numeric!(
        PpmPulseCenter,
        App,
        "app_ppm_conf.pulse_center",
        51,
        4,
        WireType::F32Auto,
        0,
        1_000_000,
        1000,
        4,
        1,
        Milliseconds,
        Hardware,
        Writable
    ),
    choices!(
        PpmSafeStart,
        App,
        "app_ppm_conf.safe_start",
        56,
        VESC_APP_PPM_CONF_SAFE_START,
        Hardware,
        Writable
    ),
    numeric!(
        PpmRampTimePositive,
        App,
        "app_ppm_conf.ramp_time_pos",
        66,
        4,
        WireType::F32Auto,
        0,
        100_000,
        10,
        2,
        1,
        Seconds,
        Hardware,
        Writable
    ),
    numeric!(
        PpmRampTimeNegative,
        App,
        "app_ppm_conf.ramp_time_neg",
        70,
        4,
        WireType::F32Auto,
        0,
        100_000,
        10,
        2,
        1,
        Seconds,
        Hardware,
        Writable
    ),
    choices!(
        AdcControlMode,
        App,
        "app_adc_conf.ctrl_type",
        90,
        VESC_APP_ADC_CONF_CTRL_TYPE,
        Hardware,
        HardwareEndpoint
    ),
    numeric!(
        AdcHysteresis,
        App,
        "app_adc_conf.hyst",
        91,
        4,
        WireType::F32Auto,
        0,
        100,
        1,
        0,
        100,
        Percent,
        Hardware,
        Writable
    ),
    numeric!(
        AdcVoltageStart,
        App,
        "app_adc_conf.voltage_start",
        95,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        330,
        1,
        2,
        1,
        Volts,
        Hardware,
        HardwareEndpoint
    ),
    numeric!(
        AdcVoltageEnd,
        App,
        "app_adc_conf.voltage_end",
        97,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        330,
        1,
        2,
        1,
        Volts,
        Hardware,
        HardwareEndpoint
    ),
    numeric!(
        AdcVoltageMinimum,
        App,
        "app_adc_conf.voltage_min",
        99,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        330,
        1,
        2,
        1,
        Volts,
        Hardware,
        HardwareEndpoint
    ),
    numeric!(
        AdcVoltageMaximum,
        App,
        "app_adc_conf.voltage_max",
        101,
        2,
        WireType::ScaledI16 { scale: 1000 },
        0,
        360,
        1,
        2,
        1,
        Volts,
        Hardware,
        HardwareEndpoint
    ),
    choices!(
        AdcSafeStart,
        App,
        "app_adc_conf.safe_start",
        110,
        VESC_APP_ADC_CONF_SAFE_START,
        Hardware,
        Writable
    ),
    numeric!(
        AdcRampTimePositive,
        App,
        "app_adc_conf.ramp_time_pos",
        123,
        4,
        WireType::F32Auto,
        0,
        100_000,
        10,
        2,
        1,
        Seconds,
        Hardware,
        Writable
    ),
    numeric!(
        AdcRampTimeNegative,
        App,
        "app_adc_conf.ramp_time_neg",
        127,
        4,
        WireType::F32Auto,
        0,
        100_000,
        10,
        2,
        1,
        Seconds,
        Hardware,
        Writable
    ),
    choices!(
        NunchukControlMode,
        App,
        "app_chuk_conf.ctrl_type",
        143,
        VESC_APP_CHUK_CONF_CTRL_TYPE,
        Hardware,
        HardwareEndpoint
    ),
    numeric!(
        NunchukHysteresis,
        App,
        "app_chuk_conf.hyst",
        144,
        4,
        WireType::F32Auto,
        0,
        100,
        1,
        0,
        100,
        Percent,
        Hardware,
        Writable
    ),
    numeric!(
        NunchukRampTimePositive,
        App,
        "app_chuk_conf.ramp_time_pos",
        148,
        4,
        WireType::F32Auto,
        0,
        100_000,
        10,
        2,
        1,
        Seconds,
        Hardware,
        Writable
    ),
    numeric!(
        NunchukRampTimeNegative,
        App,
        "app_chuk_conf.ramp_time_neg",
        152,
        4,
        WireType::F32Auto,
        0,
        100_000,
        10,
        2,
        1,
        Seconds,
        Hardware,
        Writable
    ),
    choices!(
        ImuType,
        App,
        "imu_conf.type",
        218,
        VESC_IMU_CONF_TYPE,
        Hardware,
        ImuHardware
    ),
    choices!(
        ImuMode,
        App,
        "imu_conf.mode",
        219,
        VESC_IMU_CONF_MODE,
        Hardware,
        Writable
    ),
    choices!(
        ImuFilter,
        App,
        "imu_conf.filter",
        220,
        VESC_IMU_CONF_FILTER,
        Hardware,
        Writable
    ),
    numeric!(
        ImuSampleRate,
        App,
        "imu_conf.sample_rate_hz",
        229,
        2,
        WireType::U16,
        1,
        10_000,
        10,
        0,
        1,
        Hertz,
        Hardware,
        Writable
    ),
    boolean!(
        ImuUseMagnetometer,
        App,
        "imu_conf.use_magnetometer",
        231,
        Hardware
    ),
];

/// A validated source-pinned MC or APP configuration image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VescConfiguration {
    kind: VescConfigurationKind,
    bytes: Vec<u8>,
}

impl VescConfiguration {
    /// Validates the complete source-pinned image before retaining it byte-for-byte.
    pub(crate) fn decode(
        kind: VescConfigurationKind,
        bytes: &[u8],
    ) -> Result<Self, VescConfigurationError> {
        let (length, signature) = kind.layout();
        if bytes.len() != length {
            return Err(VescConfigurationError::Length);
        }
        let actual = bytes
            .get(..4)
            .and_then(|header| <[u8; 4]>::try_from(header).ok())
            .map(u32::from_be_bytes)
            .ok_or(VescConfigurationError::Length)?;
        if actual != signature {
            return Err(VescConfigurationError::Signature);
        }
        Ok(Self {
            kind,
            bytes: bytes.to_vec(),
        })
    }

    /// Configuration kind selected by the verified reply command.
    pub(crate) const fn kind(&self) -> VescConfigurationKind {
        self.kind
    }

    /// Complete immutable serialized record, including its four-byte signature.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// VESC command payload that requests this complete record.
    pub(crate) fn read_command_payload(&self) -> &'static [u8] {
        self.kind.read_command_payload()
    }

    /// Persistent complete-object set command, including all preserved image bytes.
    pub(crate) fn write_command_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(self.bytes.len() + 1);
        payload.push(self.kind.set_command());
        payload.extend_from_slice(self.bytes());
        payload
    }

    /// Reads one supported typed field from the complete image.
    pub(crate) fn value(&self, id: SettingId) -> Option<DeviceSettingValue> {
        let spec = spec(id)?;
        if spec.kind != self.kind {
            return None;
        }
        if spec.boolean {
            if let Some(bit) = bit_projection(id) {
                let mask = read_unsigned(&self.bytes, spec.offset, spec.width)?;
                return Some(DeviceSettingValue::Boolean(mask & (1 << bit) != 0));
            }
            return match read_unsigned(&self.bytes, spec.offset, spec.width)? {
                0 => Some(DeviceSettingValue::Boolean(false)),
                1 => Some(DeviceSettingValue::Boolean(true)),
                _ => None,
            };
        }
        if spec.bitfield {
            return read_unsigned(&self.bytes, spec.offset, spec.width)
                .and_then(|raw| i32::try_from(raw).ok())
                .map(DeviceSettingValue::Number);
        }
        if !spec.choices.is_empty() {
            let raw = read_unsigned(&self.bytes, spec.offset, spec.width)?;
            return usize::try_from(raw)
                .ok()
                .and_then(|index| spec.choices.get(index))
                .and_then(|_| u16::try_from(raw + 1).ok())
                .map(DeviceSettingValue::Choice);
        }
        let physical = read_physical(&self.bytes, *spec)? * f64::from(wire_polarity(*spec));
        let factor = semantic_factor(*spec)?;
        let scaled = physical * f64::from(factor);
        if !scaled.is_finite() || scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
            return None;
        }
        // The finite/integer bounds above establish that this rounded value fits i32.
        #[allow(clippy::cast_possible_truncation)]
        let value = scaled.round() as i32;
        (spec.minimum..=spec.maximum)
            .contains(&value)
            .then_some(DeviceSettingValue::Number(value))
    }

    /// Returns an image with exactly one checked catalog field changed.
    pub(crate) fn changed(
        &self,
        id: SettingId,
        value: DeviceSettingValue,
    ) -> Result<Self, VescConfigurationError> {
        let spec = spec(id).ok_or(VescConfigurationError::UnsupportedSetting)?;
        if spec.kind != self.kind {
            return Err(VescConfigurationError::UnsupportedSetting);
        }
        if spec.read_only != ReadOnlyReason::Writable {
            return Err(VescConfigurationError::ReadOnly);
        }
        let mut updated = self.clone();
        encode_value(&mut updated.bytes, *spec, value)?;
        validate_coupled_constraints(self.kind, &updated.bytes)?;
        Ok(updated)
    }

    /// Whether two validated images have exactly the same kind and bytes.
    pub(crate) fn is_same_image(&self, other: &Self) -> bool {
        self.kind == other.kind && self.bytes == other.bytes
    }
}

/// Catalog IDs supported by the selected full-object configuration kind.
pub(crate) const fn supported_setting_ids(kind: VescConfigurationKind) -> &'static [SettingId] {
    match kind {
        VescConfigurationKind::Motor => &[
            SettingId::MotorDriveCurrentLimit,
            SettingId::MotorBrakingCurrentLimit,
            SettingId::BatteryCurrentOutLimit,
            SettingId::BatteryCurrentInLimit,
            SettingId::AbsoluteMotorCurrentLimit,
            SettingId::InputVoltageMinimum,
            SettingId::InputVoltageMaximum,
            SettingId::BatteryCutbackStart,
            SettingId::BatteryCutbackEnd,
            SettingId::RegenerativeBatteryCutbackStart,
            SettingId::RegenerativeBatteryCutbackEnd,
            SettingId::FetTemperatureCutbackStart,
            SettingId::FetTemperatureCutbackEnd,
            SettingId::MotorTemperatureCutbackStart,
            SettingId::MotorTemperatureCutbackEnd,
            SettingId::MotorPowerOutLimit,
            SettingId::RegenerativeMotorPowerLimit,
            SettingId::MotorDriveCurrentLimitScale,
            SettingId::MotorBrakingCurrentLimitScale,
            SettingId::MotorPoleCount,
            SettingId::MotorGearRatio,
            SettingId::WheelDiameter,
            SettingId::BatteryChemistry,
            SettingId::BatterySeriesCellCount,
            SettingId::BatteryCapacity,
            SettingId::BmsType,
            SettingId::BmsLimitMode,
            SettingId::BmsTemperatureLimitingEnabled,
            SettingId::BmsSocLimitingEnabled,
            SettingId::BmsMinimumCellVoltageLimitingEnabled,
            SettingId::BmsMaximumCellVoltageLimitingEnabled,
            SettingId::BmsTemperatureLimitStart,
            SettingId::BmsTemperatureLimitEnd,
            SettingId::BmsSocLimitStart,
            SettingId::BmsSocLimitEnd,
            SettingId::BmsCellMinimumVoltageLimitStart,
            SettingId::BmsCellMinimumVoltageLimitEnd,
            SettingId::BmsCellMaximumVoltageLimitStart,
            SettingId::BmsCellMaximumVoltageLimitEnd,
            SettingId::BmsCanForwardMode,
        ],
        VescConfigurationKind::App => &[
            SettingId::ControllerId,
            SettingId::CommandTimeout,
            SettingId::CommandTimeoutBrakeCurrent,
            SettingId::CanStatusRate1,
            SettingId::CanStatusRate2,
            SettingId::CanBaudRate,
            SettingId::CanMode,
            SettingId::ApplicationToUse,
            SettingId::KillSwitchMode,
            SettingId::PpmControlMode,
            SettingId::PpmHysteresis,
            SettingId::PpmPulseStart,
            SettingId::PpmPulseEnd,
            SettingId::PpmPulseCenter,
            SettingId::PpmSafeStart,
            SettingId::PpmRampTimePositive,
            SettingId::PpmRampTimeNegative,
            SettingId::AdcControlMode,
            SettingId::AdcHysteresis,
            SettingId::AdcVoltageStart,
            SettingId::AdcVoltageEnd,
            SettingId::AdcVoltageMinimum,
            SettingId::AdcVoltageMaximum,
            SettingId::AdcSafeStart,
            SettingId::AdcRampTimePositive,
            SettingId::AdcRampTimeNegative,
            SettingId::NunchukControlMode,
            SettingId::NunchukHysteresis,
            SettingId::NunchukRampTimePositive,
            SettingId::NunchukRampTimeNegative,
            SettingId::ImuType,
            SettingId::ImuMode,
            SettingId::ImuFilter,
            SettingId::ImuSampleRate,
            SettingId::ImuUseMagnetometer,
        ],
    }
}

/// Catalog in source order for one VESC configuration image.
pub(crate) fn catalog(kind: VescConfigurationKind) -> Vec<SettingDescriptor> {
    supported_setting_ids(kind)
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(order, id)| descriptor(id, u16::try_from(order).ok()?))
        .collect()
}

/// Checks a proposed value using the field's XML range and exact wire representability.
pub(crate) fn accepts(id: SettingId, value: DeviceSettingValue) -> bool {
    let Some(field) = spec(id) else {
        return false;
    };
    if field.read_only != ReadOnlyReason::Writable {
        return false;
    }
    let (length, _) = field.kind.layout();
    let mut scratch = vec![0; length];
    encode_value(&mut scratch, *field, value).is_ok()
}

/// Maps an ordinary setting to its source-pinned image byte offset.
pub(crate) fn field_id(id: SettingId) -> Option<u16> {
    let field = spec(id)?;
    if let Some(bit) = bit_projection(id) {
        return Some(0xA800 + u16::from(bit));
    }
    let namespace: u16 = match field.kind {
        VescConfigurationKind::Motor => 0xA000,
        VescConfigurationKind::App => 0xB000,
    };
    namespace.checked_add(u16::try_from(field.offset).ok()?)
}

/// Converts a semantic settings-readback value to the typed catalog representation.
/// `raw` is already normalized by the session (decimal precision and choice IDs included);
/// wire conversion is not performed here. Read-only fields remain observable.
pub(crate) fn normalize_raw(id: SettingId, raw: i64) -> Option<DeviceSettingValue> {
    let field = spec(id)?;
    if field.boolean {
        return match raw {
            0 => Some(DeviceSettingValue::Boolean(false)),
            1 => Some(DeviceSettingValue::Boolean(true)),
            _ => None,
        };
    }
    if field.bitfield {
        return u8::try_from(raw)
            .ok()
            .map(|value| DeviceSettingValue::Number(i32::from(value)));
    }
    if !field.choices.is_empty() {
        let choice = u16::try_from(raw).ok()?;
        let index = choice.checked_sub(1)?;
        return (usize::from(index) < field.choices.len())
            .then_some(DeviceSettingValue::Choice(choice));
    }
    let number = i32::try_from(raw).ok()?;
    if !(field.minimum..=field.maximum).contains(&number) || number % wire_step(*field) != 0 {
        return None;
    }
    // Verify the normalized domain exactly round-trips through this field's real wire type,
    // even if the setting is read-only in the current device profile.
    let mut scratch = vec![0; field.kind.layout().0];
    encode_value(&mut scratch, *field, DeviceSettingValue::Number(number)).ok()?;
    Some(DeviceSettingValue::Number(number))
}

/// Configuration kind owning a setting ID.
pub(crate) fn kind_for_setting(id: SettingId) -> Option<VescConfigurationKind> {
    spec(id).map(|field| field.kind)
}

/// Native control shape for a source-pinned ordinary setting.
pub(crate) fn control(id: SettingId) -> Option<SettingControl> {
    let field = spec(id)?;
    if field.boolean {
        return Some(SettingControl::Boolean);
    }
    if field.bitfield {
        return Some(SettingControl::ReadOnly);
    }
    if !field.choices.is_empty() {
        return Some(SettingControl::Choices(
            field
                .choices
                .iter()
                .enumerate()
                .filter_map(|(index, _)| {
                    let id = u16::try_from(index.checked_add(1)?).ok()?;
                    Some(SettingChoice {
                        id,
                        label_key: choice_label_key(field.key, index),
                        writable: field.read_only == ReadOnlyReason::Writable,
                    })
                })
                .collect(),
        ));
    }
    Some(SettingControl::Number {
        minimum: field.minimum,
        maximum: field.maximum,
        step: wire_step(*field),
        editor_step: field.editor_step,
        precision: field.precision,
        unit: field.unit,
        can_disable: false,
    })
}

/// Full native descriptor, including source-backed read-only policy.
pub(crate) fn descriptor(id: SettingId, order: u16) -> Option<SettingDescriptor> {
    let field = spec(id)?;
    let access = if field.read_only == ReadOnlyReason::Writable {
        SettingAccess::Writable
    } else {
        SettingAccess::ReadOnly
    };
    let help_key = match field.read_only {
        ReadOnlyReason::Writable => None,
        ReadOnlyReason::ControllerIdentity => Some("vesc.settings.read_only.controller_identity"),
        ReadOnlyReason::TransportSelection => Some("vesc.settings.read_only.transport_selection"),
        ReadOnlyReason::HardwareEndpoint => Some("vesc.settings.read_only.hardware_endpoint"),
        ReadOnlyReason::ImuHardware => Some("vesc.settings.read_only.imu_hardware"),
        ReadOnlyReason::BitfieldValueShape => Some("vesc.settings.read_only.multiple_selection"),
    };
    Some(SettingDescriptor {
        id,
        label_key: label_key(field.key),
        help_key,
        value_semantics_key: None,
        group: field.group,
        order,
        control: control(id)?,
        access,
        write_verification: VerificationStatus::SourceVerified,
        completion: SettingCompletionStrategy::MatchingReadback,
        apply_effect: SettingApplyEffect::Unspecified,
    })
}

fn label_key(key: &str) -> &'static str {
    match key {
        "bms.limit_mode.temperature_enabled" => {
            "vesc.configuration.mc.bms_limit_temperature_enabled"
        }
        "bms.limit_mode.soc_enabled" => "vesc.configuration.mc.bms_limit_soc_enabled",
        "bms.limit_mode.vmin_enabled" => "vesc.configuration.mc.bms_limit_vmin_enabled",
        "bms.limit_mode.vmax_enabled" => "vesc.configuration.mc.bms_limit_vmax_enabled",
        "l_current_max" => "vesc.configuration.mc.l_current_max",
        "l_current_min" => "vesc.configuration.mc.l_current_min",
        "l_in_current_max" => "vesc.configuration.mc.l_in_current_max",
        "l_in_current_min" => "vesc.configuration.mc.l_in_current_min",
        "l_abs_current_max" => "vesc.configuration.mc.l_abs_current_max",
        "l_min_vin" => "vesc.configuration.mc.l_min_vin",
        "l_max_vin" => "vesc.configuration.mc.l_max_vin",
        "l_battery_cut_start" => "vesc.configuration.mc.l_battery_cut_start",
        "l_battery_cut_end" => "vesc.configuration.mc.l_battery_cut_end",
        "l_battery_regen_cut_start" => "vesc.configuration.mc.l_battery_regen_cut_start",
        "l_battery_regen_cut_end" => "vesc.configuration.mc.l_battery_regen_cut_end",
        "l_temp_fet_start" => "vesc.configuration.mc.l_temp_fet_start",
        "l_temp_fet_end" => "vesc.configuration.mc.l_temp_fet_end",
        "l_temp_motor_start" => "vesc.configuration.mc.l_temp_motor_start",
        "l_temp_motor_end" => "vesc.configuration.mc.l_temp_motor_end",
        "l_watt_max" => "vesc.configuration.mc.l_watt_max",
        "l_watt_min" => "vesc.configuration.mc.l_watt_min",
        "l_current_max_scale" => "vesc.configuration.mc.l_current_max_scale",
        "l_current_min_scale" => "vesc.configuration.mc.l_current_min_scale",
        "si_motor_poles" => "vesc.configuration.mc.si_motor_poles",
        "si_gear_ratio" => "vesc.configuration.mc.si_gear_ratio",
        "si_wheel_diameter" => "vesc.configuration.mc.si_wheel_diameter",
        "si_battery_type" => "vesc.configuration.mc.si_battery_type",
        "si_battery_cells" => "vesc.configuration.mc.si_battery_cells",
        "si_battery_ah" => "vesc.configuration.mc.si_battery_ah",
        "bms.type" => "vesc.configuration.mc.bms_type",
        "bms.limit_mode" => "vesc.configuration.mc.bms_limit_mode",
        "bms.t_limit_start" => "vesc.configuration.mc.bms_t_limit_start",
        "bms.t_limit_end" => "vesc.configuration.mc.bms_t_limit_end",
        "bms.soc_limit_start" => "vesc.configuration.mc.bms_soc_limit_start",
        "bms.soc_limit_end" => "vesc.configuration.mc.bms_soc_limit_end",
        "bms.vmin_limit_start" => "vesc.configuration.mc.bms_vmin_limit_start",
        "bms.vmin_limit_end" => "vesc.configuration.mc.bms_vmin_limit_end",
        "bms.vmax_limit_start" => "vesc.configuration.mc.bms_vmax_limit_start",
        "bms.vmax_limit_end" => "vesc.configuration.mc.bms_vmax_limit_end",
        "bms.fwd_can_mode" => "vesc.configuration.mc.bms_fwd_can_mode",
        "controller_id" => "vesc.configuration.app.controller_id",
        "timeout_msec" => "vesc.configuration.app.timeout_msec",
        "timeout_brake_current" => "vesc.configuration.app.timeout_brake_current",
        "can_status_rate_1" => "vesc.configuration.app.can_status_rate_1",
        "can_status_rate_2" => "vesc.configuration.app.can_status_rate_2",
        "can_baud_rate" => "vesc.configuration.app.can_baud_rate",
        "can_mode" => "vesc.configuration.app.can_mode",
        "app_to_use" => "vesc.configuration.app.app_to_use",
        "kill_sw_mode" => "vesc.configuration.app.kill_sw_mode",
        "app_ppm_conf.ctrl_type" => "vesc.configuration.app.app_ppm_conf_ctrl_type",
        "app_ppm_conf.hyst" => "vesc.configuration.app.app_ppm_conf_hyst",
        "app_ppm_conf.pulse_start" => "vesc.configuration.app.app_ppm_conf_pulse_start",
        "app_ppm_conf.pulse_end" => "vesc.configuration.app.app_ppm_conf_pulse_end",
        "app_ppm_conf.pulse_center" => "vesc.configuration.app.app_ppm_conf_pulse_center",
        "app_ppm_conf.safe_start" => "vesc.configuration.app.app_ppm_conf_safe_start",
        "app_ppm_conf.ramp_time_pos" => "vesc.configuration.app.app_ppm_conf_ramp_time_pos",
        "app_ppm_conf.ramp_time_neg" => "vesc.configuration.app.app_ppm_conf_ramp_time_neg",
        "app_adc_conf.ctrl_type" => "vesc.configuration.app.app_adc_conf_ctrl_type",
        "app_adc_conf.hyst" => "vesc.configuration.app.app_adc_conf_hyst",
        "app_adc_conf.voltage_start" => "vesc.configuration.app.app_adc_conf_voltage_start",
        "app_adc_conf.voltage_end" => "vesc.configuration.app.app_adc_conf_voltage_end",
        "app_adc_conf.voltage_min" => "vesc.configuration.app.app_adc_conf_voltage_min",
        "app_adc_conf.voltage_max" => "vesc.configuration.app.app_adc_conf_voltage_max",
        "app_adc_conf.safe_start" => "vesc.configuration.app.app_adc_conf_safe_start",
        "app_adc_conf.ramp_time_pos" => "vesc.configuration.app.app_adc_conf_ramp_time_pos",
        "app_adc_conf.ramp_time_neg" => "vesc.configuration.app.app_adc_conf_ramp_time_neg",
        "app_chuk_conf.ctrl_type" => "vesc.configuration.app.app_chuk_conf_ctrl_type",
        "app_chuk_conf.hyst" => "vesc.configuration.app.app_chuk_conf_hyst",
        "app_chuk_conf.ramp_time_pos" => "vesc.configuration.app.app_chuk_conf_ramp_time_pos",
        "app_chuk_conf.ramp_time_neg" => "vesc.configuration.app.app_chuk_conf_ramp_time_neg",
        "imu_conf.type" => "vesc.configuration.app.imu_conf_type",
        "imu_conf.mode" => "vesc.configuration.app.imu_conf_mode",
        "imu_conf.filter" => "vesc.configuration.app.imu_conf_filter",
        "imu_conf.sample_rate_hz" => "vesc.configuration.app.imu_conf_sample_rate_hz",
        "imu_conf.use_magnetometer" => "vesc.configuration.app.imu_conf_use_magnetometer",
        _ => "vesc.configuration.unknown",
    }
}

fn choice_label_key(key: &str, index: usize) -> &'static str {
    match (key, index) {
        ("si_battery_type", 0) => "vesc.configuration.choice.mc.si_battery_type.0",
        ("si_battery_type", 1) => "vesc.configuration.choice.mc.si_battery_type.1",
        ("si_battery_type", 2) => "vesc.configuration.choice.mc.si_battery_type.2",
        ("bms.type", 0) => "vesc.configuration.choice.mc.bms_type.0",
        ("bms.type", 1) => "vesc.configuration.choice.mc.bms_type.1",
        ("bms.limit_mode", 0) => "vesc.configuration.choice.mc.bms_limit_mode.0",
        ("bms.limit_mode", 1) => "vesc.configuration.choice.mc.bms_limit_mode.1",
        ("bms.limit_mode", 2) => "vesc.configuration.choice.mc.bms_limit_mode.2",
        ("bms.limit_mode", 3) => "vesc.configuration.choice.mc.bms_limit_mode.3",
        ("bms.limit_mode", 4) => "vesc.configuration.choice.mc.bms_limit_mode.4",
        ("bms.limit_mode", 5) => "vesc.configuration.choice.mc.bms_limit_mode.5",
        ("bms.limit_mode", 6) => "vesc.configuration.choice.mc.bms_limit_mode.6",
        ("bms.limit_mode", 7) => "vesc.configuration.choice.mc.bms_limit_mode.7",
        ("bms.fwd_can_mode", 0) => "vesc.configuration.choice.mc.bms_fwd_can_mode.0",
        ("bms.fwd_can_mode", 1) => "vesc.configuration.choice.mc.bms_fwd_can_mode.1",
        ("bms.fwd_can_mode", 2) => "vesc.configuration.choice.mc.bms_fwd_can_mode.2",
        ("can_baud_rate", 0) => "vesc.configuration.choice.app.can_baud_rate.0",
        ("can_baud_rate", 1) => "vesc.configuration.choice.app.can_baud_rate.1",
        ("can_baud_rate", 2) => "vesc.configuration.choice.app.can_baud_rate.2",
        ("can_baud_rate", 3) => "vesc.configuration.choice.app.can_baud_rate.3",
        ("can_baud_rate", 4) => "vesc.configuration.choice.app.can_baud_rate.4",
        ("can_baud_rate", 5) => "vesc.configuration.choice.app.can_baud_rate.5",
        ("can_baud_rate", 6) => "vesc.configuration.choice.app.can_baud_rate.6",
        ("can_baud_rate", 7) => "vesc.configuration.choice.app.can_baud_rate.7",
        ("can_baud_rate", 8) => "vesc.configuration.choice.app.can_baud_rate.8",
        ("can_mode", 0) => "vesc.configuration.choice.app.can_mode.0",
        ("can_mode", 1) => "vesc.configuration.choice.app.can_mode.1",
        ("can_mode", 2) => "vesc.configuration.choice.app.can_mode.2",
        ("can_mode", 3) => "vesc.configuration.choice.app.can_mode.3",
        ("can_mode", 4) => "vesc.configuration.choice.app.can_mode.4",
        ("app_to_use", 0) => "vesc.configuration.choice.app.app_to_use.0",
        ("app_to_use", 1) => "vesc.configuration.choice.app.app_to_use.1",
        ("app_to_use", 2) => "vesc.configuration.choice.app.app_to_use.2",
        ("app_to_use", 3) => "vesc.configuration.choice.app.app_to_use.3",
        ("app_to_use", 4) => "vesc.configuration.choice.app.app_to_use.4",
        ("app_to_use", 5) => "vesc.configuration.choice.app.app_to_use.5",
        ("app_to_use", 6) => "vesc.configuration.choice.app.app_to_use.6",
        ("app_to_use", 7) => "vesc.configuration.choice.app.app_to_use.7",
        ("app_to_use", 8) => "vesc.configuration.choice.app.app_to_use.8",
        ("app_to_use", 9) => "vesc.configuration.choice.app.app_to_use.9",
        ("app_to_use", 10) => "vesc.configuration.choice.app.app_to_use.10",
        ("kill_sw_mode", 0) => "vesc.configuration.choice.app.kill_sw_mode.0",
        ("kill_sw_mode", 1) => "vesc.configuration.choice.app.kill_sw_mode.1",
        ("kill_sw_mode", 2) => "vesc.configuration.choice.app.kill_sw_mode.2",
        ("kill_sw_mode", 3) => "vesc.configuration.choice.app.kill_sw_mode.3",
        ("kill_sw_mode", 4) => "vesc.configuration.choice.app.kill_sw_mode.4",
        ("app_ppm_conf.ctrl_type", 0) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.0",
        ("app_ppm_conf.ctrl_type", 1) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.1",
        ("app_ppm_conf.ctrl_type", 2) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.2",
        ("app_ppm_conf.ctrl_type", 3) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.3",
        ("app_ppm_conf.ctrl_type", 4) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.4",
        ("app_ppm_conf.ctrl_type", 5) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.5",
        ("app_ppm_conf.ctrl_type", 6) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.6",
        ("app_ppm_conf.ctrl_type", 7) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.7",
        ("app_ppm_conf.ctrl_type", 8) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.8",
        ("app_ppm_conf.ctrl_type", 9) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.9",
        ("app_ppm_conf.ctrl_type", 10) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.10",
        ("app_ppm_conf.ctrl_type", 11) => "vesc.configuration.choice.app.app_ppm_conf_ctrl_type.11",
        ("app_ppm_conf.safe_start", 0) => "vesc.configuration.choice.app.app_ppm_conf_safe_start.0",
        ("app_ppm_conf.safe_start", 1) => "vesc.configuration.choice.app.app_ppm_conf_safe_start.1",
        ("app_ppm_conf.safe_start", 2) => "vesc.configuration.choice.app.app_ppm_conf_safe_start.2",
        ("app_adc_conf.ctrl_type", 0) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.0",
        ("app_adc_conf.ctrl_type", 1) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.1",
        ("app_adc_conf.ctrl_type", 2) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.2",
        ("app_adc_conf.ctrl_type", 3) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.3",
        ("app_adc_conf.ctrl_type", 4) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.4",
        ("app_adc_conf.ctrl_type", 5) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.5",
        ("app_adc_conf.ctrl_type", 6) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.6",
        ("app_adc_conf.ctrl_type", 7) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.7",
        ("app_adc_conf.ctrl_type", 8) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.8",
        ("app_adc_conf.ctrl_type", 9) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.9",
        ("app_adc_conf.ctrl_type", 10) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.10",
        ("app_adc_conf.ctrl_type", 11) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.11",
        ("app_adc_conf.ctrl_type", 12) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.12",
        ("app_adc_conf.ctrl_type", 13) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.13",
        ("app_adc_conf.ctrl_type", 14) => "vesc.configuration.choice.app.app_adc_conf_ctrl_type.14",
        ("app_adc_conf.safe_start", 0) => "vesc.configuration.choice.app.app_adc_conf_safe_start.0",
        ("app_adc_conf.safe_start", 1) => "vesc.configuration.choice.app.app_adc_conf_safe_start.1",
        ("app_adc_conf.safe_start", 2) => "vesc.configuration.choice.app.app_adc_conf_safe_start.2",
        ("app_chuk_conf.ctrl_type", 0) => "vesc.configuration.choice.app.app_chuk_conf_ctrl_type.0",
        ("app_chuk_conf.ctrl_type", 1) => "vesc.configuration.choice.app.app_chuk_conf_ctrl_type.1",
        ("app_chuk_conf.ctrl_type", 2) => "vesc.configuration.choice.app.app_chuk_conf_ctrl_type.2",
        ("app_chuk_conf.ctrl_type", 3) => "vesc.configuration.choice.app.app_chuk_conf_ctrl_type.3",
        ("imu_conf.type", 0) => "vesc.configuration.choice.app.imu_conf_type.0",
        ("imu_conf.type", 1) => "vesc.configuration.choice.app.imu_conf_type.1",
        ("imu_conf.type", 2) => "vesc.configuration.choice.app.imu_conf_type.2",
        ("imu_conf.type", 3) => "vesc.configuration.choice.app.imu_conf_type.3",
        ("imu_conf.type", 4) => "vesc.configuration.choice.app.imu_conf_type.4",
        ("imu_conf.type", 5) => "vesc.configuration.choice.app.imu_conf_type.5",
        ("imu_conf.mode", 0) => "vesc.configuration.choice.app.imu_conf_mode.0",
        ("imu_conf.mode", 1) => "vesc.configuration.choice.app.imu_conf_mode.1",
        ("imu_conf.mode", 2) => "vesc.configuration.choice.app.imu_conf_mode.2",
        ("imu_conf.filter", 0) => "vesc.configuration.choice.app.imu_conf_filter.0",
        ("imu_conf.filter", 1) => "vesc.configuration.choice.app.imu_conf_filter.1",
        ("imu_conf.filter", 2) => "vesc.configuration.choice.app.imu_conf_filter.2",
        _ => "vesc.configuration.choice.unknown",
    }
}

fn spec(id: SettingId) -> Option<&'static FieldSpec> {
    FIELDS.iter().find(|field| field.id == id)
}

fn bit_projection(id: SettingId) -> Option<u8> {
    match id {
        SettingId::BmsTemperatureLimitingEnabled => Some(0),
        SettingId::BmsSocLimitingEnabled => Some(1),
        SettingId::BmsMinimumCellVoltageLimitingEnabled => Some(2),
        SettingId::BmsMaximumCellVoltageLimitingEnabled => Some(3),
        _ => None,
    }
}

fn wire_polarity(spec: FieldSpec) -> i8 {
    match spec.key {
        "l_current_min" | "l_in_current_min" | "l_watt_min" => -1,
        _ => 1,
    }
}

fn wire_step(spec: FieldSpec) -> i32 {
    // The firmware derives pole pairs as motor_poles / 2.0; a physical motor pole
    // count is even, and the VESC Tool XML's two-pole step reflects that geometry.
    if spec.key == "si_motor_poles" { 2 } else { 1 }
}

fn validate_coupled_constraints(
    kind: VescConfigurationKind,
    bytes: &[u8],
) -> Result<(), VescConfigurationError> {
    if kind != VescConfigurationKind::Motor {
        return Ok(());
    }
    // mc_interface.c applies its endpoint branches before interpolation; equal battery
    // cutoff and temperature endpoints select a hard cutoff without reaching utils_map.
    // BMS uses the same branch pattern, gated by VESC BMS type and per-limit mode bits.
    let always_checked = [
        ("l_min_vin", "l_max_vin", true, false),
        ("l_battery_cut_start", "l_battery_cut_end", false, true),
        (
            "l_battery_regen_cut_start",
            "l_battery_regen_cut_end",
            true,
            true,
        ),
        ("l_temp_fet_start", "l_temp_fet_end", true, true),
        ("l_temp_motor_start", "l_temp_motor_end", true, true),
    ];
    for (start_key, end_key, ascending, allow_equal) in always_checked {
        validate_pair(bytes, start_key, end_key, ascending, allow_equal)?;
    }

    let bms_type = read_unsigned(bytes, 471, 1).ok_or(VescConfigurationError::Length)?;
    let bms_limit_mode = read_unsigned(bytes, 472, 1).ok_or(VescConfigurationError::Length)?;
    if bms_type == 1 {
        let bms_pairs = [
            (0, "bms.t_limit_start", "bms.t_limit_end", true),
            (1, "bms.soc_limit_start", "bms.soc_limit_end", false),
            (2, "bms.vmin_limit_start", "bms.vmin_limit_end", false),
            (3, "bms.vmax_limit_start", "bms.vmax_limit_end", true),
        ];
        for (bit, start_key, end_key, ascending) in bms_pairs {
            if bms_limit_mode & (1 << bit) != 0 {
                validate_pair(bytes, start_key, end_key, ascending, true)?;
            }
        }
    }
    Ok(())
}

fn validate_pair(
    bytes: &[u8],
    start_key: &str,
    end_key: &str,
    ascending: bool,
    allow_equal: bool,
) -> Result<(), VescConfigurationError> {
    let start = value_by_key(bytes, start_key).ok_or(VescConfigurationError::CoupledConstraint)?;
    let end = value_by_key(bytes, end_key).ok_or(VescConfigurationError::CoupledConstraint)?;
    let valid = if ascending {
        if allow_equal {
            start <= end
        } else {
            start < end
        }
    } else if allow_equal {
        start >= end
    } else {
        start > end
    };
    if valid {
        Ok(())
    } else {
        Err(VescConfigurationError::CoupledConstraint)
    }
}

fn value_by_key(bytes: &[u8], key: &str) -> Option<f64> {
    let field = FIELDS.iter().find(|field| field.key == key)?;
    Some(read_physical(bytes, *field)? * f64::from(wire_polarity(*field)))
}

fn semantic_factor(spec: FieldSpec) -> Option<i32> {
    10_i32
        .checked_pow(u32::from(spec.precision))?
        .checked_mul(spec.editor_scale)
}

fn read_unsigned(bytes: &[u8], offset: usize, width: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(width)?)?;
    match width {
        1 => slice.first().copied().map(u32::from),
        2 => <[u8; 2]>::try_from(slice)
            .ok()
            .map(u16::from_be_bytes)
            .map(u32::from),
        4 => <[u8; 4]>::try_from(slice).ok().map(u32::from_be_bytes),
        _ => None,
    }
}

fn read_physical(bytes: &[u8], spec: FieldSpec) -> Option<f64> {
    let slice = bytes.get(spec.offset..spec.offset.checked_add(spec.width)?)?;
    match spec.wire {
        WireType::F32Auto => Some(f64::from(decode_float32_auto(slice)?)),
        WireType::ScaledI16 { scale } => {
            let raw = i16::from_be_bytes(<[u8; 2]>::try_from(slice).ok()?);
            Some(f64::from(raw) / f64::from(scale))
        }
        WireType::U8 => slice.first().copied().map(f64::from),
        WireType::U16 => <[u8; 2]>::try_from(slice)
            .ok()
            .map(u16::from_be_bytes)
            .map(f64::from),
        WireType::U32 => <[u8; 4]>::try_from(slice)
            .ok()
            .map(u32::from_be_bytes)
            .map(f64::from),
    }
}

fn encode_value(
    bytes: &mut [u8],
    spec: FieldSpec,
    value: DeviceSettingValue,
) -> Result<(), VescConfigurationError> {
    if spec.boolean {
        let DeviceSettingValue::Boolean(enabled) = value else {
            return Err(VescConfigurationError::InvalidValue);
        };
        if let Some(bit) = bit_projection(spec.id) {
            let current = read_unsigned(bytes, spec.offset, spec.width)
                .ok_or(VescConfigurationError::Length)?;
            let mask = 1_u32 << bit;
            let updated = if enabled {
                current | mask
            } else {
                current & !mask
            };
            return write_unsigned(bytes, spec.offset, spec.width, updated);
        }
        return write_unsigned(bytes, spec.offset, spec.width, u32::from(enabled));
    }
    if spec.bitfield {
        return Err(VescConfigurationError::ReadOnly);
    }
    if !spec.choices.is_empty() {
        let DeviceSettingValue::Choice(choice) = value else {
            return Err(VescConfigurationError::InvalidValue);
        };
        let index = choice
            .checked_sub(1)
            .ok_or(VescConfigurationError::InvalidValue)?;
        if usize::from(index) >= spec.choices.len() {
            return Err(VescConfigurationError::InvalidValue);
        }
        return write_unsigned(bytes, spec.offset, spec.width, u32::from(index));
    }
    let DeviceSettingValue::Number(number) = value else {
        return Err(VescConfigurationError::InvalidValue);
    };
    if !(spec.minimum..=spec.maximum).contains(&number) || number % wire_step(spec) != 0 {
        return Err(VescConfigurationError::InvalidValue);
    }
    let factor = semantic_factor(spec).ok_or(VescConfigurationError::InvalidValue)?;
    let polarity = i64::from(wire_polarity(spec));
    match spec.wire {
        WireType::F32Auto => {
            let physical = f64::from(number) / f64::from(factor) * f64::from(wire_polarity(spec));
            if !physical.is_finite() {
                return Err(VescConfigurationError::InvalidValue);
            }
            // The source scale is bounded by the catalog; keep the explicit cast and
            // reject overflow below because Rust does not provide a checked f64->f32 cast.
            #[allow(clippy::cast_possible_truncation)]
            let value = physical as f32;
            if !value.is_finite() {
                return Err(VescConfigurationError::InvalidValue);
            }
            write_unsigned(bytes, spec.offset, 4, encode_float32_auto(value))?;
        }
        WireType::ScaledI16 { scale } => {
            let numerator = i64::from(number)
                .checked_mul(i64::from(scale))
                .and_then(|value| value.checked_mul(polarity))
                .ok_or(VescConfigurationError::InvalidValue)?;
            let denominator = i64::from(factor);
            if numerator % denominator != 0 {
                return Err(VescConfigurationError::InvalidValue);
            }
            let raw = i16::try_from(numerator / denominator)
                .map_err(|_| VescConfigurationError::InvalidValue)?;
            write_unsigned(
                bytes,
                spec.offset,
                2,
                u32::from(u16::from_be_bytes(raw.to_be_bytes())),
            )?;
        }
        WireType::U8 | WireType::U16 | WireType::U32 => {
            let numerator = i64::from(number)
                .checked_mul(polarity)
                .ok_or(VescConfigurationError::InvalidValue)?;
            let denominator = i64::from(factor);
            if numerator % denominator != 0 {
                return Err(VescConfigurationError::InvalidValue);
            }
            let raw = u32::try_from(numerator / denominator)
                .map_err(|_| VescConfigurationError::InvalidValue)?;
            write_unsigned(bytes, spec.offset, spec.width, raw)?;
        }
    }
    let readback = read_physical(bytes, spec).ok_or(VescConfigurationError::InvalidValue)?
        * f64::from(wire_polarity(spec));
    let round_trip = (readback * f64::from(factor)).round();
    // Both operands are finite, integral semantic values by construction. Numerical
    // equality is intentional here: negative wire polarity can produce signed zero.
    #[allow(clippy::float_cmp)]
    if round_trip != f64::from(number) {
        return Err(VescConfigurationError::InvalidValue);
    }
    Ok(())
}

fn write_unsigned(
    bytes: &mut [u8],
    offset: usize,
    width: usize,
    value: u32,
) -> Result<(), VescConfigurationError> {
    let destination = bytes
        .get_mut(
            offset
                ..offset
                    .checked_add(width)
                    .ok_or(VescConfigurationError::Length)?,
        )
        .ok_or(VescConfigurationError::Length)?;
    match width {
        1 => destination.copy_from_slice(&[
            u8::try_from(value).map_err(|_| VescConfigurationError::InvalidValue)?
        ]),
        2 => destination.copy_from_slice(
            &u16::try_from(value)
                .map_err(|_| VescConfigurationError::InvalidValue)?
                .to_be_bytes(),
        ),
        4 => destination.copy_from_slice(&value.to_be_bytes()),
        _ => return Err(VescConfigurationError::Length),
    }
    Ok(())
}

fn encode_float32_auto(value: f32) -> u32 {
    if value.abs() < 1.5e-38 || value == 0.0 {
        return 0;
    }
    // For finite normal f32 values the firmware's frexpf exponent adjustment
    // produces the same bits as IEEE-754. It flushes magnitudes below 1.5e-38
    // first; zero therefore also intentionally loses a negative-zero sign.
    value.to_bits()
}

fn decode_float32_auto(bytes: &[u8]) -> Option<f32> {
    let raw = u32::from_be_bytes(<[u8; 4]>::try_from(bytes).ok()?);
    let exponent = i32::try_from((raw >> 23) & 0xff).ok()?;
    let mantissa = raw & 0x7f_ffff;
    if exponent == 0 && mantissa != 0 {
        // The pinned firmware encoder flushes subnormals to the all-zero word.
        return None;
    }
    let value = f32::from_bits(raw);
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{DeviceSettingValue, SettingId};

    fn zero_image(kind: VescConfigurationKind) -> Vec<u8> {
        let (len, signature) = kind.layout();
        let mut bytes = vec![0; len];
        bytes[..4].copy_from_slice(&signature.to_be_bytes());
        if kind == VescConfigurationKind::App {
            write_test_u16(&mut bytes, 229, 1); // sample-rate XML minimum
        }
        if kind == VescConfigurationKind::Motor {
            bytes[452] = 8; // even physical pole count
            bytes[462] = 10; // battery cell count is at least one
            write_test_u16(&mut bytes, 50, 200); // input minimum 20 V
            write_test_u16(&mut bytes, 52, 800); // input maximum 80 V
            write_test_u16(&mut bytes, 54, 450); // battery cutback start 45 V
            write_test_u16(&mut bytes, 56, 400); // battery cutback end 40 V
            write_test_u16(&mut bytes, 58, 500); // regenerative cutback start 50 V
            write_test_u16(&mut bytes, 60, 600); // regenerative cutback end 60 V
            bytes[63] = 80;
            bytes[64] = 100;
            bytes[65] = 100;
            bytes[66] = 120;
            bytes[471] = 1; // VESC BMS type
            bytes[472] = 0x0f; // all four supported limit modes enabled
            bytes[473] = 50;
            bytes[474] = 70;
            write_test_u16(&mut bytes, 475, 900); // SOC start 0.9
            write_test_u16(&mut bytes, 477, 200); // SOC end 0.2
            write_test_u16(&mut bytes, 479, 3500); // minimum-cell start 3.5 V
            write_test_u16(&mut bytes, 481, 3000); // minimum-cell end 3.0 V
            write_test_u16(&mut bytes, 483, 4000); // maximum-cell start 4.0 V
            write_test_u16(&mut bytes, 485, 4200); // maximum-cell end 4.2 V
        }
        bytes
    }

    fn write_test_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn framed_motor_reply(image: &[u8]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(image.len() + 1);
        payload.push(MC_GET);
        payload.extend_from_slice(image);

        let mut frame = vec![3];
        frame.extend_from_slice(
            &u16::try_from(payload.len())
                .expect("motor configuration payload fits the long-frame length")
                .to_be_bytes(),
        );
        frame.extend_from_slice(&payload);
        frame.extend_from_slice(&crate::wire_crc::crc16_xmodem(&payload).to_be_bytes());
        frame.push(3);
        frame
    }

    #[test]
    fn accepts_only_source_pinned_signature_and_exact_complete_length() {
        for kind in [VescConfigurationKind::Motor, VescConfigurationKind::App] {
            let bytes = zero_image(kind);
            let decoded = VescConfiguration::decode(kind, &bytes).expect("source layout");
            assert_eq!(decoded.bytes(), bytes);
            let mut truncated = bytes.clone();
            truncated.pop();
            assert!(VescConfiguration::decode(kind, &truncated).is_err());
            let mut wrong_signature = bytes;
            wrong_signature[0] ^= 1;
            assert!(VescConfiguration::decode(kind, &wrong_signature).is_err());
        }
    }

    #[test]
    fn single_field_change_preserves_every_other_image_byte() {
        let mut bytes = zero_image(VescConfigurationKind::Motor);
        for (index, byte) in bytes.iter_mut().enumerate().take(400).skip(200) {
            *byte = u8::try_from(index % 256)
                .expect("modulo 256 fits u8")
                .wrapping_mul(17);
        }
        let before = VescConfiguration::decode(VescConfigurationKind::Motor, &bytes)
            .expect("valid source image");
        let after = before
            .changed(
                SettingId::MotorDriveCurrentLimit,
                DeviceSettingValue::Number(100),
            )
            .expect("one amp at two decimals");
        assert_eq!(after.bytes().len(), before.bytes().len());
        for index in 0..before.bytes().len() {
            if !(8..12).contains(&index) {
                assert_eq!(after.bytes()[index], before.bytes()[index]);
            }
        }
        assert_eq!(&after.bytes()[8..12], &[0x3f, 0x80, 0, 0]);
        assert_eq!(
            after.value(SettingId::MotorDriveCurrentLimit),
            Some(DeviceSettingValue::Number(100))
        );
    }

    #[test]
    fn rejects_out_of_range_and_transport_or_hardware_unsupported_fields() {
        let motor = VescConfiguration::decode(
            VescConfigurationKind::Motor,
            &zero_image(VescConfigurationKind::Motor),
        )
        .expect("valid source image");
        assert!(
            motor
                .changed(
                    SettingId::MotorDriveCurrentLimit,
                    DeviceSettingValue::Number(500_001)
                )
                .is_err()
        );
        let app = VescConfiguration::decode(
            VescConfigurationKind::App,
            &zero_image(VescConfigurationKind::App),
        )
        .expect("valid source image");
        assert!(
            app.changed(SettingId::ControllerId, DeviceSettingValue::Number(1))
                .is_err()
        );
        assert!(
            app.changed(SettingId::ApplicationToUse, DeviceSettingValue::Choice(2))
                .is_err()
        );
        assert!(
            app.changed(SettingId::AdcVoltageStart, DeviceSettingValue::Number(100))
                .is_err()
        );
    }

    #[test]
    fn request_and_write_commands_match_firmware_protocol_ids() {
        let motor = VescConfiguration::decode(
            VescConfigurationKind::Motor,
            &zero_image(VescConfigurationKind::Motor),
        )
        .expect("valid source image");
        assert_eq!(motor.read_command_payload(), &[14]);
        assert_eq!(motor.write_command_payload()[0], 13);
        let app = VescConfiguration::decode(
            VescConfigurationKind::App,
            &zero_image(VescConfigurationKind::App),
        )
        .expect("valid source image");
        assert_eq!(app.read_command_payload(), &[17]);
        assert_eq!(app.write_command_payload()[0], 16);
    }

    #[test]
    fn every_candidate_has_a_descriptor_and_supported_setting_id() {
        for (kind, expected) in [
            (VescConfigurationKind::Motor, 40),
            (VescConfigurationKind::App, 35),
        ] {
            let ids = supported_setting_ids(kind);
            assert_eq!(ids.len(), expected);
            assert_eq!(catalog(kind).len(), expected);
            assert!(ids.iter().all(|id| descriptor(*id, 0).is_some()));
        }
    }

    #[test]
    fn fixed_wire_values_follow_pinned_offsets_and_boolean_shape() {
        let motor = VescConfiguration::decode(
            VescConfigurationKind::Motor,
            &zero_image(VescConfigurationKind::Motor),
        )
        .expect("valid MC image");
        let battery_cells = motor
            .changed(
                SettingId::BatterySeriesCellCount,
                DeviceSettingValue::Number(16),
            )
            .expect("cell count");
        assert_eq!(battery_cells.bytes().get(462), Some(&16));
        let app = VescConfiguration::decode(
            VescConfigurationKind::App,
            &zero_image(VescConfigurationKind::App),
        )
        .expect("valid APP image");
        let magnetometer = app
            .changed(
                SettingId::ImuUseMagnetometer,
                DeviceSettingValue::Boolean(true),
            )
            .expect("boolean field");
        assert_eq!(magnetometer.bytes().get(231), Some(&1));
        assert_eq!(
            magnetometer.value(SettingId::ImuUseMagnetometer),
            Some(DeviceSettingValue::Boolean(true))
        );
        assert_eq!(
            control(SettingId::BmsLimitMode),
            Some(SettingControl::ReadOnly)
        );
    }

    #[test]
    fn bms_limit_projection_toggles_only_its_bit_and_preserves_unknown_bits() {
        let projections = [
            (SettingId::BmsTemperatureLimitingEnabled, 0_u8),
            (SettingId::BmsSocLimitingEnabled, 1),
            (SettingId::BmsMinimumCellVoltageLimitingEnabled, 2),
            (SettingId::BmsMaximumCellVoltageLimitingEnabled, 3),
        ];
        let mut bytes = zero_image(VescConfigurationKind::Motor);
        bytes[472] = 0x90; // retain an unused known bit and a future bit 7
        let image = VescConfiguration::decode(VescConfigurationKind::Motor, &bytes)
            .expect("valid motor image");
        for (id, bit) in projections {
            assert_eq!(field_id(id), Some(0xA800 + u16::from(bit)));
            assert_eq!(image.value(id), Some(DeviceSettingValue::Boolean(false)));
            assert_eq!(
                image
                    .changed(id, DeviceSettingValue::Boolean(true))
                    .expect("enable one BMS limit")
                    .bytes()[472],
                0x90 | (1 << bit)
            );
            let enabled = image
                .changed(id, DeviceSettingValue::Boolean(true))
                .expect("enable one BMS limit");
            assert_eq!(enabled.value(id), Some(DeviceSettingValue::Boolean(true)));
            assert_eq!(enabled.bytes()[..472], image.bytes()[..472]);
            assert_eq!(enabled.bytes()[473..], image.bytes()[473..]);
            assert_eq!(
                enabled
                    .changed(id, DeviceSettingValue::Boolean(false))
                    .expect("disable same BMS limit")
                    .bytes()[472],
                0x90
            );
            assert_eq!(
                normalize_raw(id, 0),
                Some(DeviceSettingValue::Boolean(false))
            );
            assert_eq!(
                normalize_raw(id, 1),
                Some(DeviceSettingValue::Boolean(true))
            );
            assert_eq!(normalize_raw(id, 2), None);
        }
    }

    #[test]
    fn bms_projection_enabling_activates_only_its_source_coupled_pair() {
        let mut bytes = zero_image(VescConfigurationKind::Motor);
        bytes[472] = 0x80; // future bit retained; all four modes disabled
        write_test_u16(&mut bytes, 475, 200); // invalid SOC order: start < end
        write_test_u16(&mut bytes, 477, 900);
        let invalid_inactive = VescConfiguration::decode(VescConfigurationKind::Motor, &bytes)
            .expect("stored inactive thresholds remain readable");
        assert!(matches!(
            invalid_inactive.changed(
                SettingId::BmsSocLimitingEnabled,
                DeviceSettingValue::Boolean(true)
            ),
            Err(VescConfigurationError::CoupledConstraint)
        ));
        let soc_enabled = invalid_inactive
            .changed(
                SettingId::BmsSocLimitingEnabled,
                DeviceSettingValue::Boolean(false),
            )
            .expect("disabling an inactive bit preserves unrelated invalid thresholds");
        assert_eq!(soc_enabled.bytes()[472], 0x80);

        // With the invalid pair active, changing it to disable the bit repairs the active
        // constraint in the same single-field operation and keeps future bits untouched.
        bytes[472] |= 1 << 1;
        let active_invalid = VescConfiguration::decode(VescConfigurationKind::Motor, &bytes)
            .expect("readable existing invalid state");
        assert!(matches!(
            active_invalid.changed(
                SettingId::BmsTemperatureLimitingEnabled,
                DeviceSettingValue::Boolean(true)
            ),
            Err(VescConfigurationError::CoupledConstraint)
        ));
        assert_eq!(
            active_invalid
                .changed(
                    SettingId::BmsSocLimitingEnabled,
                    DeviceSettingValue::Boolean(false)
                )
                .expect("disabling invalid active pair")
                .bytes()[472],
            0x80
        );
    }

    #[test]
    fn current_and_power_in_limits_are_positive_magnitudes_with_wire_sign_preserved() {
        let mut bytes = zero_image(VescConfigurationKind::Motor);
        bytes[12..16].copy_from_slice(&encode_float32_auto(-3.25_f32).to_be_bytes());
        bytes[20..24].copy_from_slice(&encode_float32_auto(-7.5_f32).to_be_bytes());
        bytes[77..81].copy_from_slice(&encode_float32_auto(-120.0_f32).to_be_bytes());
        let before =
            VescConfiguration::decode(VescConfigurationKind::Motor, &bytes).expect("pinned image");
        assert_eq!(
            before.value(SettingId::MotorBrakingCurrentLimit),
            Some(DeviceSettingValue::Number(325))
        );
        assert_eq!(
            before.value(SettingId::BatteryCurrentInLimit),
            Some(DeviceSettingValue::Number(750))
        );
        assert_eq!(
            before.value(SettingId::RegenerativeMotorPowerLimit),
            Some(DeviceSettingValue::Number(1200))
        );
        let after = before
            .changed(
                SettingId::MotorBrakingCurrentLimit,
                DeviceSettingValue::Number(400),
            )
            .expect("positive magnitude");
        let zero_regeneration = before
            .changed(
                SettingId::BatteryCurrentInLimit,
                DeviceSettingValue::Number(0),
            )
            .expect("zero magnitude is representable with either wire sign");
        assert_eq!(
            zero_regeneration.value(SettingId::BatteryCurrentInLimit),
            Some(DeviceSettingValue::Number(0))
        );
        assert_eq!(
            &after.bytes()[12..16],
            &encode_float32_auto(-4.0_f32).to_be_bytes()
        );
        for (index, (after_byte, before_byte)) in after.bytes().iter().zip(&bytes).enumerate() {
            if !(12..16).contains(&index) {
                assert_eq!(after_byte, before_byte);
            }
        }
    }

    #[test]
    fn unknown_boolean_wire_values_are_not_coerced_to_true() {
        let mut bytes = zero_image(VescConfigurationKind::App);
        bytes[231] = 2;
        let image = VescConfiguration::decode(VescConfigurationKind::App, &bytes)
            .expect("image signature and length");
        assert_eq!(image.value(SettingId::ImuUseMagnetometer), None);
    }

    #[test]
    fn pinned_c_generated_default_images_decode_as_complete_fixed_records() {
        let motor = include_bytes!("../tests/fixtures/vesc-settings/vesc-7.00-motor-default.bin");
        let app = include_bytes!("../tests/fixtures/vesc-settings/vesc-7.00-app-default.bin");
        let motor = VescConfiguration::decode(VescConfigurationKind::Motor, motor)
            .expect("firmware MC default");
        let app = VescConfiguration::decode(VescConfigurationKind::App, app)
            .expect("firmware APP default");
        assert_eq!(motor.bytes().len(), MC_IMAGE_LEN);
        assert_eq!(app.bytes().len(), APP_IMAGE_LEN);
        assert_eq!(
            u32::from_be_bytes(motor.bytes()[..4].try_into().expect("four-byte signature")),
            MC_SIGNATURE,
        );
        assert_eq!(
            u32::from_be_bytes(app.bytes()[..4].try_into().expect("four-byte signature")),
            APP_SIGNATURE,
        );
        assert_eq!(motor.write_command_payload().len(), MC_IMAGE_LEN + 1);
        assert_eq!(app.write_command_payload().len(), APP_IMAGE_LEN + 1);
    }

    #[test]
    fn coupled_firmware_interpolation_thresholds_require_source_directions() {
        let motor = VescConfiguration::decode(
            VescConfigurationKind::Motor,
            &zero_image(VescConfigurationKind::Motor),
        )
        .expect("valid coupled baseline");
        assert_eq!(
            motor.value(SettingId::BatteryCutbackEnd),
            Some(DeviceSettingValue::Number(400)),
            "the fixture's 40.0 V endpoint is represented in tenths of a volt",
        );
        assert_eq!(
            motor.value(SettingId::FetTemperatureCutbackEnd),
            Some(DeviceSettingValue::Number(100)),
        );
        let invalid = [
            (SettingId::InputVoltageMinimum, 8001),
            // Start must remain above the end. Equal endpoints are intentionally
            // accepted because the firmware selects a hard cutoff in its branches.
            // Battery voltage semantic values are tenths of a volt: 390 is 39.0 V,
            // below this fixture's 400 (40.0 V) endpoint.
            (SettingId::BatteryCutbackStart, 390),
            (SettingId::RegenerativeBatteryCutbackStart, 610),
            (SettingId::FetTemperatureCutbackStart, 101),
            (SettingId::MotorTemperatureCutbackEnd, 99),
            (SettingId::BmsTemperatureLimitStart, 71),
            (SettingId::BmsSocLimitStart, 19),
            (SettingId::BmsCellMinimumVoltageLimitStart, 299),
            (SettingId::BmsCellMaximumVoltageLimitStart, 421),
        ];
        for (id, value) in invalid {
            assert_eq!(
                motor.changed(id, DeviceSettingValue::Number(value)),
                Err(VescConfigurationError::CoupledConstraint),
                "{id:?} must preserve its firmware interpolation direction",
            );
        }
    }

    #[test]
    fn invalid_device_baseline_is_readable_but_cannot_be_resubmitted_by_unrelated_edit() {
        let mut bytes = zero_image(VescConfigurationKind::Motor);
        // Reverse the valid battery cutoff ordering without invalidating the signature.
        write_test_u16(&mut bytes, 54, 350);
        write_test_u16(&mut bytes, 56, 400);
        let baseline = VescConfiguration::decode(VescConfigurationKind::Motor, &bytes)
            .expect("read path validates image signature and size, not policy");
        assert!(baseline.value(SettingId::MotorDriveCurrentLimit).is_some());
        assert_eq!(
            baseline.changed(
                SettingId::MotorDriveCurrentLimit,
                DeviceSettingValue::Number(100)
            ),
            Err(VescConfigurationError::CoupledConstraint),
        );
        let repaired = baseline
            .changed(
                SettingId::BatteryCutbackStart,
                DeviceSettingValue::Number(450),
            )
            .expect("a single edit can restore the firmware ordering");
        assert!(
            repaired
                .changed(
                    SettingId::MotorDriveCurrentLimit,
                    DeviceSettingValue::Number(100)
                )
                .is_ok()
        );
    }

    #[test]
    fn motor_pole_count_is_even_to_match_firmware_pole_pair_geometry() {
        let motor = VescConfiguration::decode(
            VescConfigurationKind::Motor,
            &zero_image(VescConfigurationKind::Motor),
        )
        .expect("valid coupled baseline");
        assert!(
            motor
                .changed(SettingId::MotorPoleCount, DeviceSettingValue::Number(8))
                .is_ok()
        );
        assert_eq!(
            motor.changed(SettingId::MotorPoleCount, DeviceSettingValue::Number(7)),
            Err(VescConfigurationError::InvalidValue),
        );
        assert!(accepts(
            SettingId::MotorPoleCount,
            DeviceSettingValue::Number(8)
        ));
        assert!(!accepts(
            SettingId::MotorPoleCount,
            DeviceSettingValue::Number(7)
        ));
        assert!(matches!(
            control(SettingId::MotorPoleCount),
            Some(SettingControl::Number { step: 2, .. })
        ));
    }

    #[test]
    fn motor_speed_geometry_rejects_zero_and_accepts_positive_catalog_values() {
        let fixture = include_bytes!("../tests/fixtures/vesc-settings/vesc-7.00-motor-default.bin");
        let motor = VescConfiguration::decode(VescConfigurationKind::Motor, fixture)
            .expect("pinned source motor configuration");
        for id in [SettingId::MotorGearRatio, SettingId::WheelDiameter] {
            assert!(matches!(
                control(id),
                Some(SettingControl::Number { minimum: 100, .. })
            ));
            assert!(!accepts(id, DeviceSettingValue::Number(0)));
            assert_eq!(normalize_raw(id, 0), None);
            assert_eq!(
                motor.changed(id, DeviceSettingValue::Number(0)),
                Err(VescConfigurationError::InvalidValue)
            );

            for value in [100, 1_000] {
                let semantic = DeviceSettingValue::Number(value);
                assert!(accepts(id, semantic), "{id:?}={value}");
                assert!(motor.changed(id, semantic).is_ok(), "{id:?}={value}");
                assert_eq!(normalize_raw(id, i64::from(value)), Some(semantic));
            }
        }

        let changed = motor
            .changed(SettingId::MotorGearRatio, DeviceSettingValue::Number(1_000))
            .expect("positive gear ratio")
            .changed(SettingId::WheelDiameter, DeviceSettingValue::Number(1_000))
            .expect("positive wheel diameter");
        let frame = framed_motor_reply(changed.bytes());
        let reply = crate::VescReadOnlyCodec::decode_reply(&frame)
            .expect("the complete changed MC image decodes as a command-14 reply");
        let crate::VescReadOnlyReply::MotorConfig(config) = reply else {
            panic!("expected decoded motor configuration reply");
        };
        assert!(config.speed_geometry.gear_ratio_denominator.as_milli() > 0);
        assert!(config.speed_geometry.wheel_circumference.as_millimetres() > 0);
    }

    #[test]
    fn equal_cutoff_thresholds_are_valid_when_firmware_branch_guards_make_hard_cutoff() {
        let motor = VescConfiguration::decode(
            VescConfigurationKind::Motor,
            &zero_image(VescConfigurationKind::Motor),
        )
        .expect("valid coupled baseline");
        assert!(
            motor
                .changed(
                    SettingId::BatteryCutbackStart,
                    DeviceSettingValue::Number(400)
                )
                .is_ok()
        );
        assert!(
            motor
                .changed(
                    SettingId::RegenerativeBatteryCutbackEnd,
                    DeviceSettingValue::Number(500)
                )
                .is_ok()
        );
        assert!(
            motor
                .changed(
                    SettingId::FetTemperatureCutbackEnd,
                    DeviceSettingValue::Number(80)
                )
                .is_ok()
        );
        assert!(
            motor
                .changed(
                    SettingId::BmsTemperatureLimitEnd,
                    DeviceSettingValue::Number(50)
                )
                .is_ok()
        );
        assert!(
            motor
                .changed(SettingId::BmsSocLimitEnd, DeviceSettingValue::Number(90))
                .is_ok()
        );
        assert!(
            motor
                .changed(
                    SettingId::BmsCellMinimumVoltageLimitEnd,
                    DeviceSettingValue::Number(350)
                )
                .is_ok()
        );
        assert!(
            motor
                .changed(
                    SettingId::BmsCellMaximumVoltageLimitEnd,
                    DeviceSettingValue::Number(400)
                )
                .is_ok()
        );
    }

    #[test]
    fn disabled_bms_threshold_pairs_do_not_block_unrelated_edits() {
        let mut bytes = zero_image(VescConfigurationKind::Motor);
        // BMS disabled: firmware cannot receive usable VESC BMS telemetry.
        bytes[471] = 0;
        write_test_u16(&mut bytes, 475, 100);
        write_test_u16(&mut bytes, 477, 900);
        let baseline = VescConfiguration::decode(VescConfigurationKind::Motor, &bytes)
            .expect("signature and length are valid regardless of unused limits");
        assert!(
            baseline
                .changed(
                    SettingId::MotorDriveCurrentLimit,
                    DeviceSettingValue::Number(100)
                )
                .is_ok()
        );

        // VESC BMS enabled but the SOC limiter bit is disabled; other active bits remain valid.
        bytes[471] = 1;
        bytes[472] &= !(1 << 1);
        let baseline = VescConfiguration::decode(VescConfigurationKind::Motor, &bytes)
            .expect("signature and length are valid regardless of inactive SOC limits");
        assert!(
            baseline
                .changed(
                    SettingId::MotorDriveCurrentLimit,
                    DeviceSettingValue::Number(100)
                )
                .is_ok()
        );
    }

    #[test]
    fn scaled_i16_values_round_trip_exactly_over_each_catalog_domain() {
        for field in FIELDS
            .iter()
            .filter(|field| matches!(field.wire, WireType::ScaledI16 { .. }))
        {
            let (_, signature) = field.kind.layout();
            let mut bytes = vec![0; field.kind.layout().0];
            bytes[..4].copy_from_slice(&signature.to_be_bytes());
            for number in field.minimum..=field.maximum {
                let expected_representable = {
                    let WireType::ScaledI16 { scale } = field.wire else {
                        unreachable!()
                    };
                    let Some(factor) = semantic_factor(*field) else {
                        panic!("factor")
                    };
                    let numerator =
                        i64::from(number) * i64::from(scale) * i64::from(wire_polarity(*field));
                    let denominator = i64::from(factor);
                    numerator % denominator == 0 && i16::try_from(numerator / denominator).is_ok()
                };
                let actual = encode_value(&mut bytes, *field, DeviceSettingValue::Number(number));
                assert_eq!(
                    actual.is_ok(),
                    expected_representable,
                    "{}={number}",
                    field.key
                );
                if actual.is_ok() {
                    let raw = read_physical(&bytes, *field).expect("encoded value")
                        * f64::from(wire_polarity(*field));
                    let semantic =
                        (raw * f64::from(semantic_factor(*field).expect("factor"))).round();
                    let semantic = if semantic == 0.0 { 0.0 } else { semantic };
                    assert_eq!(
                        semantic.to_bits(),
                        f64::from(number).to_bits(),
                        "{}={number}",
                        field.key
                    );
                }
            }
        }
    }

    #[test]
    fn semantic_readback_normalizer_preserves_typed_values_and_namespaced_identity() {
        assert_eq!(
            field_id(SettingId::MotorDriveCurrentLimit),
            Some(0xA000 + 8),
        );
        assert_eq!(field_id(SettingId::CommandTimeout), Some(0xB000 + 5));
        assert_ne!(
            field_id(SettingId::MotorDriveCurrentLimit),
            field_id(SettingId::CommandTimeout)
        );
        assert_eq!(
            normalize_raw(SettingId::MotorDriveCurrentLimit, 123),
            Some(DeviceSettingValue::Number(123))
        );
        assert_eq!(
            normalize_raw(SettingId::ApplicationToUse, 2),
            Some(DeviceSettingValue::Choice(2))
        );
        assert_eq!(normalize_raw(SettingId::ApplicationToUse, 12), None);
        assert_eq!(
            normalize_raw(SettingId::ControllerId, 44),
            Some(DeviceSettingValue::Number(44))
        );
        assert_eq!(
            normalize_raw(SettingId::ImuUseMagnetometer, 1),
            Some(DeviceSettingValue::Boolean(true))
        );
        assert_eq!(normalize_raw(SettingId::ImuUseMagnetometer, 2), None);
        assert_eq!(
            normalize_raw(SettingId::BmsLimitMode, 0x81),
            Some(DeviceSettingValue::Number(0x81))
        );
    }

    #[test]
    fn firmware_float32_auto_matches_source_exponent_layout_for_positive_and_negative_values() {
        assert_eq!(encode_float32_auto(1.0), 0x3f80_0000);
        assert_eq!(encode_float32_auto(0.5), 0x3f00_0000);
        assert_eq!(encode_float32_auto(-3.25), (-3.25_f32).to_bits());
        assert_eq!(encode_float32_auto(-0.0), 0);
        assert_eq!(encode_float32_auto(1.0e-39), 0);
        for value in [0.5_f32, 1.0, -3.25, 120.0, -0.0] {
            let raw = encode_float32_auto(value).to_be_bytes();
            let decoded = decode_float32_auto(&raw).expect("finite source encoding");
            let expected = if value == 0.0 { 0.0 } else { value };
            assert_eq!(decoded.to_bits(), expected.to_bits());
        }
        assert_eq!(decode_float32_auto(&0x0000_0001_u32.to_be_bytes()), None);
        assert_eq!(decode_float32_auto(&0x7f80_0000_u32.to_be_bytes()), None);
        assert_eq!(decode_float32_auto(&0x7fc0_0000_u32.to_be_bytes()), None);
    }
}
