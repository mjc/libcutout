//! Source-pinned Refloat 1.3.0 custom-configuration settings codec.
//! All decoded values and writes use the complete 282-byte serializer image from
//! Refloat source revision 58b3ccb0ba9ab026e86959460e69fb0721cf1f31.

use crate::{
    SettingAccess, SettingApplyEffect, SettingChoice, SettingControl, SettingDescriptor,
    SettingGroup, SettingUnit,
};
use cutout_core::{DeviceSettingValue, SettingCompletionStrategy, SettingId, VerificationStatus};

pub(crate) const REFLOAT_CONFIG_LEN: usize = 282;
pub(crate) const REFLOAT_CONFIG_SIGNATURE: u32 = 421_162_011;
const SIGNATURE: [u8; 4] = REFLOAT_CONFIG_SIGNATURE.to_be_bytes();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RefloatValueKind {
    Number,
    Boolean,
    Choice,
}

#[derive(Clone, Copy, Debug)]
struct RefloatFieldSpec {
    id: SettingId,
    field_id: u16,
    offset: usize,
    width: u8,
    signed: bool,
    semantic_per_wire: i32,
    minimum: i32,
    maximum: i32,
    wire_step: i32,
    editor_step: i32,
    precision: u8,
    unit: SettingUnit,
    kind: RefloatValueKind,
    choices: &'static [&'static str],
    group: SettingGroup,
    access: SettingAccess,
    apply_effect: SettingApplyEffect,
    order: u16,
    label_key: &'static str,
    help_key: Option<&'static str>,
    value_semantics_key: Option<&'static str>,
}

const CHOICES_REMOTE: &[&str] = &["None", "UART", "PPM"];
const CHOICES_TRANSITION: &[&str] = &["Fade", "Fade Out Fade In", "Cipher", "Mono Cipher"];
const CHOICES_EFFECT: &[&str] = &[
    "Solid",
    "Fade",
    "Pulse",
    "Strobe",
    "Knight Rider",
    "Felony",
    "Rainbow Cycle",
    "Rainbow Fade",
    "Rainbow Roll",
];
const CHOICES_PALETTE: &[&str] = &[
    "Black",
    "White (full)",
    "White (rgb)",
    "White (single)",
    "Red",
    "Ferrari",
    "Flame",
    "Coral",
    "Sunset",
    "Sunrise",
    "Gold",
    "Orange",
    "Yellow",
    "Banana",
    "Lime",
    "Acid",
    "Sage",
    "Green",
    "Mint",
    "Tiffany",
    "Cyan",
    "Steel",
    "Sky",
    "Azure",
    "Sapphire",
    "Blue",
    "Violet",
    "Amethyst",
    "Magenta",
    "Pink",
    "Fuchsia",
    "Lavender",
];
const CHOICES_HW_MODE: &[&str] = &["Off", "Internal", "External", "Internal and External"];
const CHOICES_PIN: &[&str] = &["PPM/Servo pin", "Dedicated LED pin", "JetFleet F6 v1 pin"];
const CHOICES_PIN_CONFIG: &[&str] = &["Pullup to 5V", "No Pullup"];
const CHOICES_LED_ORDER: &[&str] = &["Not Present", "1st", "2nd", "3rd"];
const CHOICES_COLOR_ORDER: &[&str] = &["GRB", "GRBW", "RGB", "WRGB"];

// 91 source-selected General settings. The 80 ride-feel/tune fields remain opaque.
const FIELDS: &[RefloatFieldSpec] = &[
    RefloatFieldSpec {
        id: SettingId::TiltbackSpeed,
        field_id: 0x9006,
        offset: 83,
        width: 1,
        signed: false,
        semantic_per_wire: 10,
        minimum: 50,
        maximum: 1000,
        wire_step: 10,
        editor_step: 50,
        precision: 1,
        unit: SettingUnit::KilometresPerHour,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 0,
        label_key: "settings.refloat.tiltback_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PwmTiltback,
        field_id: 0x9005,
        offset: 68,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 99,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::PwmDutyPercent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 1,
        label_key: "settings.refloat.pwm_tiltback.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackDutyAngle,
        field_id: 0x9102,
        offset: 64,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3000,
        wire_step: 1,
        editor_step: 100,
        precision: 2,
        unit: SettingUnit::Degrees,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 2,
        label_key: "settings.refloat.pushback_duty_angle.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackDutySpeed,
        field_id: 0x9103,
        offset: 66,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3000,
        wire_step: 1,
        editor_step: 50,
        precision: 2,
        unit: SettingUnit::DegreesPerSecond,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 3,
        label_key: "settings.refloat.pushback_duty_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackReturnSpeed,
        field_id: 0x9104,
        offset: 84,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1000,
        wire_step: 1,
        editor_step: 50,
        precision: 2,
        unit: SettingUnit::DegreesPerSecond,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 4,
        label_key: "settings.refloat.pushback_return_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackDutyBeepEnabled,
        field_id: 0x9105,
        offset: 70,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 5,
        label_key: "settings.refloat.pushback_duty_beep_enabled.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackPersistentFatalError,
        field_id: 0x9106,
        offset: 86,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 6,
        label_key: "settings.refloat.pushback_persistent_fatal_error.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackHighVoltageAngle,
        field_id: 0x9107,
        offset: 71,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3000,
        wire_step: 1,
        editor_step: 100,
        precision: 2,
        unit: SettingUnit::Degrees,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 7,
        label_key: "settings.refloat.pushback_high_voltage_angle.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackHighVoltageSpeed,
        field_id: 0x9108,
        offset: 73,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3000,
        wire_step: 1,
        editor_step: 50,
        precision: 2,
        unit: SettingUnit::DegreesPerSecond,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 8,
        label_key: "settings.refloat.pushback_high_voltage_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackLowVoltageAngle,
        field_id: 0x9109,
        offset: 77,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3000,
        wire_step: 1,
        editor_step: 100,
        precision: 2,
        unit: SettingUnit::Degrees,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 9,
        label_key: "settings.refloat.pushback_low_voltage_angle.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackLowVoltageSpeed,
        field_id: 0x910a,
        offset: 79,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3000,
        wire_step: 1,
        editor_step: 50,
        precision: 2,
        unit: SettingUnit::DegreesPerSecond,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 10,
        label_key: "settings.refloat.pushback_low_voltage_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsDutyFrequency,
        field_id: 0x910b,
        offset: 250,
        width: 2,
        signed: false,
        semantic_per_wire: 1,
        minimum: 300,
        maximum: 1000,
        wire_step: 1,
        editor_step: 20,
        precision: 0,
        unit: SettingUnit::Hertz,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 11,
        label_key: "settings.refloat.haptics_duty_frequency.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsDutyStrength,
        field_id: 0x910c,
        offset: 252,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 120,
        wire_step: 1,
        editor_step: 5,
        precision: 1,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 12,
        label_key: "settings.refloat.haptics_duty_strength.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsErrorFrequency,
        field_id: 0x910d,
        offset: 254,
        width: 2,
        signed: false,
        semantic_per_wire: 1,
        minimum: 300,
        maximum: 1000,
        wire_step: 1,
        editor_step: 20,
        precision: 0,
        unit: SettingUnit::Hertz,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 13,
        label_key: "settings.refloat.haptics_error_frequency.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsErrorStrength,
        field_id: 0x910e,
        offset: 256,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 120,
        wire_step: 1,
        editor_step: 5,
        precision: 1,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 14,
        label_key: "settings.refloat.haptics_error_strength.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsVibrateFrequency,
        field_id: 0x910f,
        offset: 258,
        width: 2,
        signed: false,
        semantic_per_wire: 1,
        minimum: 10,
        maximum: 200,
        wire_step: 1,
        editor_step: 5,
        precision: 0,
        unit: SettingUnit::Hertz,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 15,
        label_key: "settings.refloat.haptics_vibrate_frequency.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsVibrateStrength,
        field_id: 0x9110,
        offset: 260,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 250,
        wire_step: 1,
        editor_step: 20,
        precision: 1,
        unit: SettingUnit::Amperes,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 16,
        label_key: "settings.refloat.haptics_vibrate_strength.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsDutySolidOffset,
        field_id: 0x9111,
        offset: 262,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 100,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 17,
        label_key: "settings.refloat.haptics_duty_solid_offset.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsCurrentThreshold,
        field_id: 0x9112,
        offset: 264,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 18,
        label_key: "settings.refloat.haptics_current_threshold.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsMinStrength,
        field_id: 0x9113,
        offset: 266,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 19,
        label_key: "settings.refloat.haptics_min_strength.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsMaxStrengthSpeed,
        field_id: 0x9114,
        offset: 268,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 10,
        maximum: 100,
        wire_step: 1,
        editor_step: 5,
        precision: 0,
        unit: SettingUnit::KilometresPerHour,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 20,
        label_key: "settings.refloat.haptics_max_strength_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HapticsStrengthCurvature,
        field_id: 0x9115,
        offset: 269,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1000,
        wire_step: 1,
        editor_step: 100,
        precision: 3,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Haptics,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 21,
        label_key: "settings.refloat.haptics_strength_curvature.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::RemoteReceiverType,
        field_id: 0x9116,
        offset: 99,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 2,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_REMOTE,
        group: SettingGroup::Remote,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 22,
        label_key: "settings.refloat.remote_receiver_type.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.receiver_reboot_and_beeper_conflict.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::RemoteInputDeadband,
        field_id: 0x9117,
        offset: 103,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 5000,
        wire_step: 1,
        editor_step: 100,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Remote,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 23,
        label_key: "settings.refloat.remote_input_deadband.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::LightingOn,
        field_id: 0x9118,
        offset: 180,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Unspecified,
        order: 24,
        label_key: "settings.refloat.lighting_on.label",
        help_key: Some("settings.refloat.lighting_startup_preference.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHeadlightsOn,
        field_id: 0x9119,
        offset: 181,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Unspecified,
        order: 25,
        label_key: "settings.refloat.lighting_headlights_on.label",
        help_key: Some("settings.refloat.lighting_startup_preference.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHeadlightsTransition,
        field_id: 0x911a,
        offset: 182,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_TRANSITION,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 26,
        label_key: "settings.refloat.lighting_headlights_transition.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingDirectionTransition,
        field_id: 0x911b,
        offset: 183,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_TRANSITION,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 27,
        label_key: "settings.refloat.lighting_direction_transition.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingLightsOffWhenLifted,
        field_id: 0x911c,
        offset: 184,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 28,
        label_key: "settings.refloat.lighting_lights_off_when_lifted.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusOnFrontWhenLifted,
        field_id: 0x911d,
        offset: 185,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 29,
        label_key: "settings.refloat.lighting_status_on_front_when_lifted.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingFrontMode,
        field_id: 0x911e,
        offset: 186,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 8,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_EFFECT,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 30,
        label_key: "settings.refloat.lighting_front_mode.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingFrontBrightness,
        field_id: 0x911f,
        offset: 187,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 31,
        label_key: "settings.refloat.lighting_front_brightness.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingFrontColor1,
        field_id: 0x9120,
        offset: 189,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 32,
        label_key: "settings.refloat.lighting_front_color1.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingFrontColor2,
        field_id: 0x9121,
        offset: 190,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 33,
        label_key: "settings.refloat.lighting_front_color2.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingFrontSpeed,
        field_id: 0x9122,
        offset: 191,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 15000,
        wire_step: 1,
        editor_step: 500,
        precision: 3,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 34,
        label_key: "settings.refloat.lighting_front_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingRearMode,
        field_id: 0x9123,
        offset: 193,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 8,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_EFFECT,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 35,
        label_key: "settings.refloat.lighting_rear_mode.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingRearBrightness,
        field_id: 0x9124,
        offset: 194,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 36,
        label_key: "settings.refloat.lighting_rear_brightness.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingRearColor1,
        field_id: 0x9125,
        offset: 196,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 37,
        label_key: "settings.refloat.lighting_rear_color1.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingRearColor2,
        field_id: 0x9126,
        offset: 197,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 38,
        label_key: "settings.refloat.lighting_rear_color2.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingRearSpeed,
        field_id: 0x9127,
        offset: 198,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 15000,
        wire_step: 1,
        editor_step: 500,
        precision: 3,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 39,
        label_key: "settings.refloat.lighting_rear_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHeadlightsMode,
        field_id: 0x9128,
        offset: 200,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 8,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_EFFECT,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 40,
        label_key: "settings.refloat.lighting_headlights_mode.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHeadlightsBrightness,
        field_id: 0x9129,
        offset: 201,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 41,
        label_key: "settings.refloat.lighting_headlights_brightness.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHeadlightsColor1,
        field_id: 0x912a,
        offset: 203,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 42,
        label_key: "settings.refloat.lighting_headlights_color1.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHeadlightsColor2,
        field_id: 0x912b,
        offset: 204,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 43,
        label_key: "settings.refloat.lighting_headlights_color2.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHeadlightsSpeed,
        field_id: 0x912c,
        offset: 205,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 15000,
        wire_step: 1,
        editor_step: 500,
        precision: 3,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 44,
        label_key: "settings.refloat.lighting_headlights_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingTaillightsMode,
        field_id: 0x912d,
        offset: 207,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 8,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_EFFECT,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 45,
        label_key: "settings.refloat.lighting_taillights_mode.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingTaillightsBrightness,
        field_id: 0x912e,
        offset: 208,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 46,
        label_key: "settings.refloat.lighting_taillights_brightness.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingTaillightsColor1,
        field_id: 0x912f,
        offset: 210,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 47,
        label_key: "settings.refloat.lighting_taillights_color1.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingTaillightsColor2,
        field_id: 0x9130,
        offset: 211,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 48,
        label_key: "settings.refloat.lighting_taillights_color2.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingTaillightsSpeed,
        field_id: 0x9131,
        offset: 212,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 15000,
        wire_step: 1,
        editor_step: 500,
        precision: 3,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 49,
        label_key: "settings.refloat.lighting_taillights_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusBrightnessHeadlightsOff,
        field_id: 0x9132,
        offset: 214,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 50,
        label_key: "settings.refloat.lighting_status_brightness_headlights_off.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusBrightnessHeadlightsOn,
        field_id: 0x9133,
        offset: 216,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 51,
        label_key: "settings.refloat.lighting_status_brightness_headlights_on.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusShowSensorsWhileRunning,
        field_id: 0x9134,
        offset: 218,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 52,
        label_key: "settings.refloat.lighting_status_show_sensors_while_running.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusMotorUtilizationThreshold,
        field_id: 0x9135,
        offset: 219,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 1500,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 53,
        label_key: "settings.refloat.lighting_status_motor_utilization_threshold.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusRedBarPercentage,
        field_id: 0x9136,
        offset: 221,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 5000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 54,
        label_key: "settings.refloat.lighting_status_red_bar_percentage.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusIdleTimeout,
        field_id: 0x9137,
        offset: 223,
        width: 2,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 300,
        wire_step: 1,
        editor_step: 10,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 55,
        label_key: "settings.refloat.lighting_status_idle_timeout.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusIdleMode,
        field_id: 0x9138,
        offset: 225,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 8,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_EFFECT,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 56,
        label_key: "settings.refloat.lighting_status_idle_mode.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusIdleBrightness,
        field_id: 0x9139,
        offset: 226,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 10000,
        wire_step: 1,
        editor_step: 500,
        precision: 2,
        unit: SettingUnit::Percent,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 57,
        label_key: "settings.refloat.lighting_status_idle_brightness.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusIdleColor1,
        field_id: 0x913a,
        offset: 228,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 58,
        label_key: "settings.refloat.lighting_status_idle_color1.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusIdleColor2,
        field_id: 0x913b,
        offset: 229,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 31,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PALETTE,
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 59,
        label_key: "settings.refloat.lighting_status_idle_color2.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingStatusIdleSpeed,
        field_id: 0x913c,
        offset: 230,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 15000,
        wire_step: 1,
        editor_step: 500,
        precision: 3,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Lighting,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 60,
        label_key: "settings.refloat.lighting_status_idle_speed.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::PackageDisabled,
        field_id: 0x913d,
        offset: 249,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::Unspecified,
        order: 61,
        label_key: "settings.refloat.package_disabled.label",
        help_key: Some("settings.refloat.package_disable_read_only.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::PushbackHv,
        field_id: 0x913e,
        offset: 75,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 15000,
        wire_step: 1,
        editor_step: 5,
        precision: 2,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::Unspecified,
        order: 62,
        label_key: "settings.refloat.pushback_hv.label",
        help_key: Some("settings.refloat.voltage_cell_or_pack_basis.help"),
        value_semantics_key: Some("settings.refloat.voltage_cell_or_pack_basis.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::PushbackLv,
        field_id: 0x913f,
        offset: 81,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 15000,
        wire_step: 1,
        editor_step: 5,
        precision: 2,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Limits,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::Unspecified,
        order: 63,
        label_key: "settings.refloat.pushback_lv.label",
        help_key: Some("settings.refloat.voltage_cell_or_pack_basis.help"),
        value_semantics_key: Some("settings.refloat.voltage_cell_or_pack_basis.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::SensorsAdcchannel1,
        field_id: 0x9140,
        offset: 44,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3300,
        wire_step: 1,
        editor_step: 100,
        precision: 3,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::Unspecified,
        order: 64,
        label_key: "settings.refloat.sensors_adcchannel1.label",
        help_key: Some("settings.refloat.footpad_adc_hardware.help"),
        value_semantics_key: Some("settings.refloat.footpad_adc_hardware.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::SensorsAdcchannel2,
        field_id: 0x9141,
        offset: 46,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3300,
        wire_step: 1,
        editor_step: 100,
        precision: 3,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::Unspecified,
        order: 65,
        label_key: "settings.refloat.sensors_adcchannel2.label",
        help_key: Some("settings.refloat.footpad_adc_hardware.help"),
        value_semantics_key: Some("settings.refloat.footpad_adc_hardware.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::SensorsSwapFootpadAdcs,
        field_id: 0x9142,
        offset: 247,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::Unspecified,
        order: 66,
        label_key: "settings.refloat.sensors_swap_footpad_adcs.label",
        help_key: Some("settings.refloat.footpad_adc_hardware.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::SensorsFootFaultBeepEnabled,
        field_id: 0x9143,
        offset: 48,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 67,
        label_key: "settings.refloat.sensors_foot_fault_beep_enabled.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::HardwareBeeperEnabled,
        field_id: 0x9144,
        offset: 248,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 68,
        label_key: "settings.refloat.hardware_beeper_enabled.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: None,
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareMode,
        field_id: 0x9145,
        offset: 232,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_HW_MODE,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 69,
        label_key: "settings.refloat.lighting_hardware_mode.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwarePin,
        field_id: 0x9146,
        offset: 233,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 2,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PIN,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 70,
        label_key: "settings.refloat.lighting_hardware_pin.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwarePinConfig,
        field_id: 0x9147,
        offset: 234,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_PIN_CONFIG,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 71,
        label_key: "settings.refloat.lighting_hardware_pin_config.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareStatusOrder,
        field_id: 0x9148,
        offset: 235,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_LED_ORDER,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 72,
        label_key: "settings.refloat.lighting_hardware_status_order.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareStatusCount,
        field_id: 0x9149,
        offset: 236,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 30,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 73,
        label_key: "settings.refloat.lighting_hardware_status_count.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareStatusColorOrder,
        field_id: 0x914a,
        offset: 237,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_COLOR_ORDER,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 74,
        label_key: "settings.refloat.lighting_hardware_status_color_order.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareStatusReverse,
        field_id: 0x914b,
        offset: 238,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 75,
        label_key: "settings.refloat.lighting_hardware_status_reverse.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareFrontOrder,
        field_id: 0x914c,
        offset: 239,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_LED_ORDER,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 76,
        label_key: "settings.refloat.lighting_hardware_front_order.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareFrontCount,
        field_id: 0x914d,
        offset: 240,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 30,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 77,
        label_key: "settings.refloat.lighting_hardware_front_count.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareFrontColorOrder,
        field_id: 0x914e,
        offset: 241,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_COLOR_ORDER,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 78,
        label_key: "settings.refloat.lighting_hardware_front_color_order.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareFrontReverse,
        field_id: 0x914f,
        offset: 242,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 79,
        label_key: "settings.refloat.lighting_hardware_front_reverse.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareRearOrder,
        field_id: 0x9150,
        offset: 243,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_LED_ORDER,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 80,
        label_key: "settings.refloat.lighting_hardware_rear_order.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareRearCount,
        field_id: 0x9151,
        offset: 244,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 30,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 81,
        label_key: "settings.refloat.lighting_hardware_rear_count.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareRearColorOrder,
        field_id: 0x9152,
        offset: 245,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 3,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Choice,
        choices: CHOICES_COLOR_ORDER,
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 82,
        label_key: "settings.refloat.lighting_hardware_rear_color_order.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::LightingHardwareRearReverse,
        field_id: 0x9153,
        offset: 246,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Hardware,
        access: SettingAccess::ReadOnly,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 83,
        label_key: "settings.refloat.lighting_hardware_rear_reverse.label",
        help_key: Some("settings.refloat.hardware_wiring_read_only.help"),
        value_semantics_key: Some("settings.refloat.lighting_hardware_applicability.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::BmsEnabled,
        field_id: 0x9154,
        offset: 271,
        width: 1,
        signed: false,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 1,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Scalar,
        kind: RefloatValueKind::Boolean,
        choices: &[],
        group: SettingGroup::Battery,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::RestartRequired,
        order: 84,
        label_key: "settings.refloat.bms_enabled.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.bms_alert_thresholds.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::BmsCellLvThreshold,
        field_id: 0x9155,
        offset: 272,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 2500,
        maximum: 4500,
        wire_step: 1,
        editor_step: 100,
        precision: 3,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Battery,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 85,
        label_key: "settings.refloat.bms_cell_lv_threshold.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.bms_alert_thresholds.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::BmsCellHvThreshold,
        field_id: 0x9156,
        offset: 274,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 2500,
        maximum: 4500,
        wire_step: 1,
        editor_step: 100,
        precision: 3,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Battery,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 86,
        label_key: "settings.refloat.bms_cell_hv_threshold.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.bms_alert_thresholds.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::BmsCellBalanceThreshold,
        field_id: 0x9157,
        offset: 276,
        width: 2,
        signed: true,
        semantic_per_wire: 1,
        minimum: 10,
        maximum: 1000,
        wire_step: 1,
        editor_step: 50,
        precision: 3,
        unit: SettingUnit::Volts,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Battery,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 87,
        label_key: "settings.refloat.bms_cell_balance_threshold.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.bms_alert_thresholds.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::BmsCellHtThreshold,
        field_id: 0x9158,
        offset: 278,
        width: 1,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 60,
        wire_step: 1,
        editor_step: 5,
        precision: 0,
        unit: SettingUnit::Celsius,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Battery,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 88,
        label_key: "settings.refloat.bms_cell_ht_threshold.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.bms_alert_thresholds.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::BmsCellLtThreshold,
        field_id: 0x9159,
        offset: 279,
        width: 1,
        signed: true,
        semantic_per_wire: 1,
        minimum: -20,
        maximum: 20,
        wire_step: 1,
        editor_step: 1,
        precision: 0,
        unit: SettingUnit::Celsius,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Battery,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 89,
        label_key: "settings.refloat.bms_cell_lt_threshold.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.bms_alert_thresholds.semantics"),
    },
    RefloatFieldSpec {
        id: SettingId::BmsBmsHtThreshold,
        field_id: 0x915a,
        offset: 280,
        width: 1,
        signed: true,
        semantic_per_wire: 1,
        minimum: 0,
        maximum: 80,
        wire_step: 1,
        editor_step: 5,
        precision: 0,
        unit: SettingUnit::Celsius,
        kind: RefloatValueKind::Number,
        choices: &[],
        group: SettingGroup::Battery,
        access: SettingAccess::Writable,
        apply_effect: SettingApplyEffect::Immediate,
        order: 90,
        label_key: "settings.refloat.bms_bms_ht_threshold.label",
        help_key: Some("settings.refloat.full_config_persistent_stationary.help"),
        value_semantics_key: Some("settings.refloat.bms_alert_thresholds.semantics"),
    },
];

pub(crate) const fn supported_setting_ids() -> &'static [SettingId] {
    &SUPPORTED_IDS
}
const SUPPORTED_IDS: [SettingId; 91] = [
    SettingId::TiltbackSpeed,
    SettingId::PwmTiltback,
    SettingId::PushbackDutyAngle,
    SettingId::PushbackDutySpeed,
    SettingId::PushbackReturnSpeed,
    SettingId::PushbackDutyBeepEnabled,
    SettingId::PushbackPersistentFatalError,
    SettingId::PushbackHighVoltageAngle,
    SettingId::PushbackHighVoltageSpeed,
    SettingId::PushbackLowVoltageAngle,
    SettingId::PushbackLowVoltageSpeed,
    SettingId::HapticsDutyFrequency,
    SettingId::HapticsDutyStrength,
    SettingId::HapticsErrorFrequency,
    SettingId::HapticsErrorStrength,
    SettingId::HapticsVibrateFrequency,
    SettingId::HapticsVibrateStrength,
    SettingId::HapticsDutySolidOffset,
    SettingId::HapticsCurrentThreshold,
    SettingId::HapticsMinStrength,
    SettingId::HapticsMaxStrengthSpeed,
    SettingId::HapticsStrengthCurvature,
    SettingId::RemoteReceiverType,
    SettingId::RemoteInputDeadband,
    SettingId::LightingOn,
    SettingId::LightingHeadlightsOn,
    SettingId::LightingHeadlightsTransition,
    SettingId::LightingDirectionTransition,
    SettingId::LightingLightsOffWhenLifted,
    SettingId::LightingStatusOnFrontWhenLifted,
    SettingId::LightingFrontMode,
    SettingId::LightingFrontBrightness,
    SettingId::LightingFrontColor1,
    SettingId::LightingFrontColor2,
    SettingId::LightingFrontSpeed,
    SettingId::LightingRearMode,
    SettingId::LightingRearBrightness,
    SettingId::LightingRearColor1,
    SettingId::LightingRearColor2,
    SettingId::LightingRearSpeed,
    SettingId::LightingHeadlightsMode,
    SettingId::LightingHeadlightsBrightness,
    SettingId::LightingHeadlightsColor1,
    SettingId::LightingHeadlightsColor2,
    SettingId::LightingHeadlightsSpeed,
    SettingId::LightingTaillightsMode,
    SettingId::LightingTaillightsBrightness,
    SettingId::LightingTaillightsColor1,
    SettingId::LightingTaillightsColor2,
    SettingId::LightingTaillightsSpeed,
    SettingId::LightingStatusBrightnessHeadlightsOff,
    SettingId::LightingStatusBrightnessHeadlightsOn,
    SettingId::LightingStatusShowSensorsWhileRunning,
    SettingId::LightingStatusMotorUtilizationThreshold,
    SettingId::LightingStatusRedBarPercentage,
    SettingId::LightingStatusIdleTimeout,
    SettingId::LightingStatusIdleMode,
    SettingId::LightingStatusIdleBrightness,
    SettingId::LightingStatusIdleColor1,
    SettingId::LightingStatusIdleColor2,
    SettingId::LightingStatusIdleSpeed,
    SettingId::PackageDisabled,
    SettingId::PushbackHv,
    SettingId::PushbackLv,
    SettingId::SensorsAdcchannel1,
    SettingId::SensorsAdcchannel2,
    SettingId::SensorsSwapFootpadAdcs,
    SettingId::SensorsFootFaultBeepEnabled,
    SettingId::HardwareBeeperEnabled,
    SettingId::LightingHardwareMode,
    SettingId::LightingHardwarePin,
    SettingId::LightingHardwarePinConfig,
    SettingId::LightingHardwareStatusOrder,
    SettingId::LightingHardwareStatusCount,
    SettingId::LightingHardwareStatusColorOrder,
    SettingId::LightingHardwareStatusReverse,
    SettingId::LightingHardwareFrontOrder,
    SettingId::LightingHardwareFrontCount,
    SettingId::LightingHardwareFrontColorOrder,
    SettingId::LightingHardwareFrontReverse,
    SettingId::LightingHardwareRearOrder,
    SettingId::LightingHardwareRearCount,
    SettingId::LightingHardwareRearColorOrder,
    SettingId::LightingHardwareRearReverse,
    SettingId::BmsEnabled,
    SettingId::BmsCellLvThreshold,
    SettingId::BmsCellHvThreshold,
    SettingId::BmsCellBalanceThreshold,
    SettingId::BmsCellHtThreshold,
    SettingId::BmsCellLtThreshold,
    SettingId::BmsBmsHtThreshold,
];

fn spec(id: SettingId) -> Option<&'static RefloatFieldSpec> {
    FIELDS.iter().find(|field| field.id == id)
}

pub(crate) fn field_id(id: SettingId) -> Option<u16> {
    spec(id).map(|field| field.field_id)
}
pub(crate) fn access(id: SettingId) -> SettingAccess {
    spec(id).map_or(SettingAccess::ReadOnly, |field| field.access)
}
pub(crate) fn apply_effect(id: SettingId) -> SettingApplyEffect {
    spec(id).map_or(SettingApplyEffect::Unspecified, |field| field.apply_effect)
}
/// Applicability of lighting settings under the configured Refloat LED mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RefloatSettingApplicability {
    /// The current LED mode uses this setting.
    Applicable,
    /// The setting is valid but LED output is disabled.
    ConfiguredButInactive,
    /// The selected external controller ignores this setting.
    ExternalControllerIgnoresSetting,
    /// The LED mode is unknown or unsupported.
    UnknownHardware,
    /// The setting is independent of LED mode.
    NotLighting,
}

/// Projects the observed package's LED routing mode without exposing wire data.
pub(crate) fn lighting_mode(config: &RefloatSettings) -> crate::RefloatLightingMode {
    match spec(SettingId::LightingHardwareMode).and_then(|field| read_raw(&config.bytes, field)) {
        Some(0) => crate::RefloatLightingMode::Off,
        Some(1) => crate::RefloatLightingMode::Internal,
        Some(2) => crate::RefloatLightingMode::External,
        Some(3) => crate::RefloatLightingMode::InternalAndExternal,
        _ => crate::RefloatLightingMode::Unknown,
    }
}

/// Reports mode-dependent LED applicability from the complete observed config.
pub(crate) fn applicability(
    id: SettingId,
    mode: crate::RefloatLightingMode,
) -> RefloatSettingApplicability {
    let Some(field) = spec(id) else {
        return RefloatSettingApplicability::UnknownHardware;
    };
    if field.group != SettingGroup::Lighting {
        return RefloatSettingApplicability::NotLighting;
    }
    match mode {
        crate::RefloatLightingMode::Off => RefloatSettingApplicability::ConfiguredButInactive,
        crate::RefloatLightingMode::Internal | crate::RefloatLightingMode::InternalAndExternal => {
            RefloatSettingApplicability::Applicable
        }
        crate::RefloatLightingMode::External => match id {
            SettingId::LightingOn
            | SettingId::LightingHeadlightsOn
            | SettingId::LightingFrontBrightness
            | SettingId::LightingHeadlightsBrightness
            | SettingId::LightingStatusBrightnessHeadlightsOff
            | SettingId::LightingStatusBrightnessHeadlightsOn => {
                RefloatSettingApplicability::Applicable
            }
            _ => RefloatSettingApplicability::ExternalControllerIgnoresSetting,
        },
        crate::RefloatLightingMode::Unknown => RefloatSettingApplicability::UnknownHardware,
    }
}

pub(crate) fn control(id: SettingId) -> Option<SettingControl> {
    let field = spec(id)?;
    Some(match field.kind {
        RefloatValueKind::Boolean => SettingControl::Boolean,
        RefloatValueKind::Choice => SettingControl::Choices(
            field
                .choices
                .iter()
                .enumerate()
                .filter_map(|(index, _)| {
                    Some(SettingChoice {
                        id: u16::try_from(index).ok()?.checked_add(1)?,
                        label_key: choice_label_key(field.id, index),
                        writable: field.access == SettingAccess::Writable,
                    })
                })
                .collect(),
        ),
        RefloatValueKind::Number => SettingControl::Number {
            minimum: field.minimum,
            maximum: field.maximum,
            step: field.wire_step,
            editor_step: field.editor_step,
            precision: field.precision,
            unit: field.unit,
            can_disable: match field.id {
                SettingId::PwmTiltback | SettingId::TiltbackSpeed => true,
                _ => false,
            },
        },
    })
}

/// Normalizes a shared readback entry. Readback values are already semantic
/// integers, unlike serializer-native values decoded by [`RefloatSettings::value`].
pub(crate) fn normalize_raw(id: SettingId, raw: i64) -> Option<DeviceSettingValue> {
    let field = spec(id)?;
    let raw = i32::try_from(raw).ok()?;
    match field.kind {
        RefloatValueKind::Boolean => match raw {
            0 => Some(DeviceSettingValue::Boolean(false)),
            1 => Some(DeviceSettingValue::Boolean(true)),
            _ => None,
        },
        RefloatValueKind::Choice => {
            if raw <= 0 || usize::try_from(raw).ok()? > field.choices.len() {
                return None;
            }
            Some(DeviceSettingValue::Choice(u16::try_from(raw).ok()?))
        }
        RefloatValueKind::Number if id == SettingId::PwmTiltback => {
            if raw == 100 {
                Some(DeviceSettingValue::Disabled)
            } else if accepts(id, DeviceSettingValue::Number(raw)) {
                Some(DeviceSettingValue::Number(raw))
            } else {
                None
            }
        }
        RefloatValueKind::Number if id == SettingId::TiltbackSpeed => {
            if raw == 0 {
                Some(DeviceSettingValue::Disabled)
            } else if accepts(id, DeviceSettingValue::Number(raw)) {
                Some(DeviceSettingValue::Number(raw))
            } else {
                None
            }
        }
        RefloatValueKind::Number if accepts(id, DeviceSettingValue::Number(raw)) => {
            Some(DeviceSettingValue::Number(raw))
        }
        RefloatValueKind::Number => None,
    }
}

fn decode_wire_value(id: SettingId, raw: i32) -> Option<DeviceSettingValue> {
    let field = spec(id)?;
    match field.kind {
        RefloatValueKind::Boolean => match raw {
            0 => Some(DeviceSettingValue::Boolean(false)),
            1 => Some(DeviceSettingValue::Boolean(true)),
            _ => None,
        },
        RefloatValueKind::Choice => {
            let id = u16::try_from(raw).ok()?.checked_add(1)?;
            if usize::from(id) <= field.choices.len() {
                Some(DeviceSettingValue::Choice(id))
            } else {
                None
            }
        }
        RefloatValueKind::Number if id == SettingId::PwmTiltback => {
            if raw == 1000 {
                Some(DeviceSettingValue::Disabled)
            } else if (0..=990).contains(&raw) && raw % 10 == 0 {
                Some(DeviceSettingValue::Number(raw / 10))
            } else {
                None
            }
        }
        RefloatValueKind::Number if id == SettingId::TiltbackSpeed => {
            if raw == 0 {
                Some(DeviceSettingValue::Disabled)
            } else {
                let value = raw.checked_mul(field.semantic_per_wire)?;
                if accepts(id, DeviceSettingValue::Number(value)) {
                    Some(DeviceSettingValue::Number(value))
                } else {
                    None
                }
            }
        }
        RefloatValueKind::Number => {
            let value = raw.checked_mul(field.semantic_per_wire)?;
            if accepts(id, DeviceSettingValue::Number(value)) {
                Some(DeviceSettingValue::Number(value))
            } else {
                None
            }
        }
    }
}

const REMOTE_CHOICES: &[&str] = &[
    "settings.refloat.choice.receiver.none",
    "settings.refloat.choice.receiver.uart",
    "settings.refloat.choice.receiver.ppm",
];
const TRANSITION_CHOICES: &[&str] = &[
    "settings.refloat.choice.transition.fade",
    "settings.refloat.choice.transition.fade_out_in",
    "settings.refloat.choice.transition.cipher",
    "settings.refloat.choice.transition.mono_cipher",
];
const LIGHTING_EFFECT_CHOICES: &[&str] = &[
    "settings.refloat.choice.effect.solid",
    "settings.refloat.choice.effect.fade",
    "settings.refloat.choice.effect.pulse",
    "settings.refloat.choice.effect.strobe",
    "settings.refloat.choice.effect.knight_rider",
    "settings.refloat.choice.effect.felony",
    "settings.refloat.choice.effect.rainbow_cycle",
    "settings.refloat.choice.effect.rainbow_fade",
    "settings.refloat.choice.effect.rainbow_roll",
];
const PALETTE_CHOICES: &[&str] = &[
    "settings.refloat.choice.palette.black",
    "settings.refloat.choice.palette.white_full",
    "settings.refloat.choice.palette.white_rgb",
    "settings.refloat.choice.palette.white_single",
    "settings.refloat.choice.palette.red",
    "settings.refloat.choice.palette.ferrari",
    "settings.refloat.choice.palette.flame",
    "settings.refloat.choice.palette.coral",
    "settings.refloat.choice.palette.sunset",
    "settings.refloat.choice.palette.sunrise",
    "settings.refloat.choice.palette.gold",
    "settings.refloat.choice.palette.orange",
    "settings.refloat.choice.palette.yellow",
    "settings.refloat.choice.palette.banana",
    "settings.refloat.choice.palette.lime",
    "settings.refloat.choice.palette.acid",
    "settings.refloat.choice.palette.sage",
    "settings.refloat.choice.palette.green",
    "settings.refloat.choice.palette.mint",
    "settings.refloat.choice.palette.tiffany",
    "settings.refloat.choice.palette.cyan",
    "settings.refloat.choice.palette.steel",
    "settings.refloat.choice.palette.sky",
    "settings.refloat.choice.palette.azure",
    "settings.refloat.choice.palette.sapphire",
    "settings.refloat.choice.palette.blue",
    "settings.refloat.choice.palette.violet",
    "settings.refloat.choice.palette.amethyst",
    "settings.refloat.choice.palette.magenta",
    "settings.refloat.choice.palette.pink",
    "settings.refloat.choice.palette.fuchsia",
    "settings.refloat.choice.palette.lavender",
];
const HARDWARE_MODE_CHOICES: &[&str] = &[
    "settings.refloat.choice.hardware_mode.off",
    "settings.refloat.choice.hardware_mode.internal",
    "settings.refloat.choice.hardware_mode.external",
    "settings.refloat.choice.hardware_mode.internal_and_external",
];
const PIN_CHOICES: &[&str] = &[
    "settings.refloat.choice.hardware_pin.servo",
    "settings.refloat.choice.hardware_pin.dedicated_led",
    "settings.refloat.choice.hardware_pin.jetfleet_f6_v1",
];
const PIN_CONFIG_CHOICES: &[&str] = &[
    "settings.refloat.choice.pin_config.pullup_5v",
    "settings.refloat.choice.pin_config.no_pullup",
];
const LED_ORDER_CHOICES: &[&str] = &[
    "settings.refloat.choice.led_order.not_present",
    "settings.refloat.choice.led_order.first",
    "settings.refloat.choice.led_order.second",
    "settings.refloat.choice.led_order.third",
];
const COLOR_ORDER_CHOICES: &[&str] = &[
    "settings.refloat.choice.color_order.grb",
    "settings.refloat.choice.color_order.grbw",
    "settings.refloat.choice.color_order.rgb",
    "settings.refloat.choice.color_order.wrgb",
];

fn choice_label_key(id: SettingId, index: usize) -> &'static str {
    // Shared semantic choice groups keep repeated palettes/effects consistent.
    let choices = match id {
        SettingId::RemoteReceiverType => REMOTE_CHOICES,
        SettingId::LightingHeadlightsTransition | SettingId::LightingDirectionTransition => {
            TRANSITION_CHOICES
        }
        SettingId::LightingFrontMode
        | SettingId::LightingRearMode
        | SettingId::LightingHeadlightsMode
        | SettingId::LightingTaillightsMode
        | SettingId::LightingStatusIdleMode => LIGHTING_EFFECT_CHOICES,
        SettingId::LightingFrontColor1
        | SettingId::LightingFrontColor2
        | SettingId::LightingRearColor1
        | SettingId::LightingRearColor2
        | SettingId::LightingHeadlightsColor1
        | SettingId::LightingHeadlightsColor2
        | SettingId::LightingTaillightsColor1
        | SettingId::LightingTaillightsColor2
        | SettingId::LightingStatusIdleColor1
        | SettingId::LightingStatusIdleColor2 => PALETTE_CHOICES,
        SettingId::LightingHardwareMode => HARDWARE_MODE_CHOICES,
        SettingId::LightingHardwarePin => PIN_CHOICES,
        SettingId::LightingHardwarePinConfig => PIN_CONFIG_CHOICES,
        SettingId::LightingHardwareStatusOrder
        | SettingId::LightingHardwareFrontOrder
        | SettingId::LightingHardwareRearOrder => LED_ORDER_CHOICES,
        SettingId::LightingHardwareStatusColorOrder
        | SettingId::LightingHardwareFrontColorOrder
        | SettingId::LightingHardwareRearColorOrder => COLOR_ORDER_CHOICES,
        _ => &[],
    };
    choices
        .get(index)
        .copied()
        .unwrap_or("settings.refloat.choice.unknown")
}

pub(crate) fn descriptor(id: SettingId) -> Option<SettingDescriptor> {
    let field = spec(id)?;
    Some(SettingDescriptor {
        id,
        label_key: field.label_key,
        help_key: field.help_key,
        value_semantics_key: field.value_semantics_key,
        group: field.group,
        order: field.order,
        control: control(id)?,
        access: field.access,
        write_verification: VerificationStatus::SourceVerified,
        completion: SettingCompletionStrategy::MatchingReadback,
        apply_effect: apply_effect(id),
    })
}

pub(crate) fn accepts(id: SettingId, value: DeviceSettingValue) -> bool {
    control(id).is_some_and(|control| control.accepts_value(value))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum RefloatSettingsError {
    #[error("unsupported Refloat custom-config length")]
    UnsupportedLength,
    #[error("unsupported Refloat custom-config signature")]
    UnsupportedSignature,
    #[error("unsupported Refloat setting")]
    UnsupportedSetting,
    #[error("invalid Refloat setting value")]
    InvalidValue,
    #[error("inconsistent Refloat settings combination")]
    InvalidCombination,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RefloatSettings {
    bytes: [u8; REFLOAT_CONFIG_LEN],
}

impl RefloatSettings {
    pub(crate) fn is_same_except_default(&self, other: &Self) -> bool {
        self.bytes()
            .iter()
            .zip(other.bytes().iter())
            .enumerate()
            .all(|(i, (a, b))| a == b || (i == REFLOAT_CONFIG_LEN - 1 && *a == 1 && *b == 0))
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, RefloatSettingsError> {
        if bytes.len() != REFLOAT_CONFIG_LEN {
            return Err(RefloatSettingsError::UnsupportedLength);
        }
        if bytes.get(..4) != Some(SIGNATURE.as_slice()) {
            return Err(RefloatSettingsError::UnsupportedSignature);
        }
        let mut image = [0; REFLOAT_CONFIG_LEN];
        image.copy_from_slice(bytes);
        Ok(Self { bytes: image })
    }
    pub(crate) const fn bytes(&self) -> &[u8; REFLOAT_CONFIG_LEN] {
        &self.bytes
    }
    pub(crate) fn value(&self, id: SettingId) -> Option<DeviceSettingValue> {
        let raw = read_raw(&self.bytes, spec(id)?)?;
        decode_wire_value(id, raw)
    }
    pub(crate) fn changed(
        &self,
        id: SettingId,
        value: DeviceSettingValue,
    ) -> Result<Self, RefloatSettingsError> {
        let f = spec(id).ok_or(RefloatSettingsError::UnsupportedSetting)?;
        if !accepts(id, value) {
            return Err(RefloatSettingsError::InvalidValue);
        }
        let raw = match (f.kind, value) {
            (RefloatValueKind::Boolean, DeviceSettingValue::Boolean(v)) => i32::from(v),
            (RefloatValueKind::Choice, DeviceSettingValue::Choice(v)) => i32::from(v - 1),
            (RefloatValueKind::Number, DeviceSettingValue::Number(v))
                if id == SettingId::PwmTiltback =>
            {
                v.checked_mul(10)
                    .ok_or(RefloatSettingsError::InvalidValue)?
            }
            (RefloatValueKind::Number, DeviceSettingValue::Number(v))
                if id == SettingId::TiltbackSpeed =>
            {
                v / 10
            }
            (RefloatValueKind::Number, DeviceSettingValue::Number(v)) => v / f.semantic_per_wire,
            (RefloatValueKind::Number, DeviceSettingValue::Disabled)
                if id == SettingId::PwmTiltback =>
            {
                1000
            }
            (RefloatValueKind::Number, DeviceSettingValue::Disabled)
                if id == SettingId::TiltbackSpeed =>
            {
                0
            }
            _ => return Err(RefloatSettingsError::InvalidValue),
        };
        let mut updated = self.clone();
        write_raw(&mut updated.bytes, f, raw)?;
        updated.validate_couplings()?;
        Ok(updated)
    }
    fn validate_couplings(&self) -> Result<(), RefloatSettingsError> {
        let raw = |id| spec(id).and_then(|f| read_raw(&self.bytes, f));
        if raw(SettingId::RemoteReceiverType) == Some(2)
            && raw(SettingId::HardwareBeeperEnabled) == Some(1)
        {
            return Err(RefloatSettingsError::InvalidCombination);
        }
        if let (Some(low), Some(high)) = (
            raw(SettingId::BmsCellLtThreshold),
            raw(SettingId::BmsCellHtThreshold),
        ) && low > high
        {
            return Err(RefloatSettingsError::InvalidCombination);
        }
        if let (Some(low), Some(high)) = (
            raw(SettingId::BmsCellLvThreshold),
            raw(SettingId::BmsCellHvThreshold),
        ) && low >= high
        {
            return Err(RefloatSettingsError::InvalidCombination);
        }
        Ok(())
    }
    pub(crate) fn write_command_payload(&self) -> [u8; REFLOAT_CONFIG_LEN + 2] {
        let mut p = [0; REFLOAT_CONFIG_LEN + 2];
        p[0] = 95;
        p[1] = 0;
        p[2..].copy_from_slice(&self.bytes);
        p
    }
}

fn read_raw(bytes: &[u8], f: &RefloatFieldSpec) -> Option<i32> {
    match f.width {
        1 => {
            let b = *bytes.get(f.offset)?;
            Some(if f.signed {
                i32::from(i8::from_be_bytes([b]))
            } else {
                i32::from(b)
            })
        }
        2 => {
            let b: [u8; 2] = bytes
                .get(f.offset..f.offset.checked_add(2)?)?
                .try_into()
                .ok()?;
            Some(if f.signed {
                i32::from(i16::from_be_bytes(b))
            } else {
                i32::from(u16::from_be_bytes(b))
            })
        }
        _ => None,
    }
}
fn write_raw(
    bytes: &mut [u8],
    f: &RefloatFieldSpec,
    value: i32,
) -> Result<(), RefloatSettingsError> {
    match (f.width, f.signed) {
        (1, false) => {
            *bytes
                .get_mut(f.offset)
                .ok_or(RefloatSettingsError::InvalidValue)? =
                u8::try_from(value).map_err(|_| RefloatSettingsError::InvalidValue)?;
        }
        (1, true) => {
            *bytes
                .get_mut(f.offset)
                .ok_or(RefloatSettingsError::InvalidValue)? = i8::try_from(value)
                .map_err(|_| RefloatSettingsError::InvalidValue)?
                .cast_unsigned();
        }
        (2, false) => {
            let b = u16::try_from(value)
                .map_err(|_| RefloatSettingsError::InvalidValue)?
                .to_be_bytes();
            bytes
                .get_mut(f.offset..f.offset + 2)
                .ok_or(RefloatSettingsError::InvalidValue)?
                .copy_from_slice(&b);
        }
        (2, true) => {
            let b = i16::try_from(value)
                .map_err(|_| RefloatSettingsError::InvalidValue)?
                .to_be_bytes();
            bytes
                .get_mut(f.offset..f.offset + 2)
                .ok_or(RefloatSettingsError::InvalidValue)?
                .copy_from_slice(&b);
        }
        _ => return Err(RefloatSettingsError::InvalidValue),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Independently serialized by Refloat 1.3.0's pinned C implementation.
    const DEFAULT_CONFIG: &[u8; REFLOAT_CONFIG_LEN] =
        include_bytes!("../tests/fixtures/refloat-settings/refloat-1.3.0-default.bin");

    #[test]
    fn independent_default_fixture_decodes_the_complete_supported_catalog() {
        assert_eq!(&DEFAULT_CONFIG[..4], &SIGNATURE);
        assert_eq!(supported_setting_ids().len(), 91);
        let config = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        for id in supported_setting_ids() {
            assert!(field_id(*id).is_some(), "missing field id for {id:?}");
            assert!(descriptor(*id).is_some(), "missing descriptor for {id:?}");
            assert!(
                config.value(*id).is_some(),
                "unrecognized default for {id:?}"
            );
        }
        assert_eq!(
            config.value(SettingId::PwmTiltback),
            Some(DeviceSettingValue::Number(80))
        );
        assert_eq!(
            config.value(SettingId::TiltbackSpeed),
            Some(DeviceSettingValue::Disabled)
        );
    }

    #[test]
    fn same_value_roundtrip_preserves_every_field_in_the_complete_image() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        for id in supported_setting_ids() {
            let value = original.value(*id).expect("default field decodes");
            let changed = original
                .changed(*id, value)
                .expect("same field value is encodable");
            assert_eq!(
                changed.bytes(),
                original.bytes(),
                "same-value rewrite for {id:?}"
            );
        }
    }

    #[test]
    fn one_field_changes_preserve_every_other_byte_including_deferred_tunes() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let id = SettingId::HapticsDutyFrequency;
        let field = spec(id).expect("haptic catalog field");
        let changed = original
            .changed(id, DeviceSettingValue::Number(520))
            .expect("valid frequency");
        assert_eq!(changed.value(id), Some(DeviceSettingValue::Number(520)));
        for offset in 0..REFLOAT_CONFIG_LEN {
            if !(field.offset..field.offset + usize::from(field.width)).contains(&offset) {
                assert_eq!(
                    changed.bytes()[offset],
                    original.bytes()[offset],
                    "unexpected byte change at {offset}"
                );
            }
        }
        // These offsets contain deferred ride-feel fields; they stay opaque.
        for offset in [4, 6, 42, 43, 50, 52, 54, 56, 58, 60, 62] {
            assert_eq!(changed.bytes()[offset], original.bytes()[offset]);
        }
    }

    #[test]
    fn reads_ignore_editor_step_but_writes_require_exact_wire_representability() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let speed = spec(SettingId::TiltbackSpeed).expect("speed field");
        let mut bytes = *DEFAULT_CONFIG;
        write_raw(&mut bytes, speed, 53).expect("in-bounds raw speed");
        let observed = RefloatSettings::decode(&bytes).expect("valid complete config");
        assert_eq!(
            observed.value(SettingId::TiltbackSpeed),
            Some(DeviceSettingValue::Number(530))
        );
        assert!(accepts(
            SettingId::TiltbackSpeed,
            DeviceSettingValue::Number(530)
        ));
        assert!(!accepts(
            SettingId::TiltbackSpeed,
            DeviceSettingValue::Number(531)
        ));
        assert!(!accepts(
            SettingId::TiltbackSpeed,
            DeviceSettingValue::Number(525)
        ));
        assert_eq!(
            original.value(SettingId::TiltbackSpeed),
            Some(DeviceSettingValue::Disabled)
        );
    }

    #[test]
    fn serializer_quantum_and_editor_increment_are_separate() {
        let offset = spec(SettingId::HapticsDutySolidOffset).expect("solid offset field");
        assert_eq!(offset.wire_step, 1);
        assert_eq!(offset.editor_step, 100);
        assert!(accepts(
            SettingId::HapticsDutySolidOffset,
            DeviceSettingValue::Number(101),
        ));
        let control = control(SettingId::HapticsDutySolidOffset).expect("numeric control");
        match control {
            SettingControl::Number {
                step,
                editor_step,
                precision,
                ..
            } => {
                assert_eq!(step, 1);
                assert_eq!(editor_step, 100);
                assert_eq!(precision, 2);
            }
            _ => panic!("expected fixed-point number control"),
        }
    }

    #[test]
    fn semantic_readback_normalizes_all_catalog_fields() {
        let config = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        for id in supported_setting_ids() {
            let field = spec(*id).expect("catalog field");
            let raw_wire = read_raw(config.bytes(), field).expect("field bytes");
            let value = decode_wire_value(*id, raw_wire).expect("wire value decodes");
            let raw_semantic = match value {
                DeviceSettingValue::Number(value) => i64::from(value),
                DeviceSettingValue::Boolean(value) => i64::from(value),
                DeviceSettingValue::Choice(value) => i64::from(value),
                DeviceSettingValue::Disabled if *id == SettingId::PwmTiltback => 100,
                DeviceSettingValue::Disabled if *id == SettingId::TiltbackSpeed => 0,
                DeviceSettingValue::Disabled => panic!("unexpected disabled field {id:?}"),
            };
            assert_eq!(
                normalize_raw(*id, raw_semantic),
                Some(value),
                "semantic normalization for {id:?}"
            );
        }
        assert_eq!(
            normalize_raw(SettingId::PwmTiltback, 80),
            Some(DeviceSettingValue::Number(80))
        );
        assert_eq!(
            normalize_raw(SettingId::PwmTiltback, 100),
            Some(DeviceSettingValue::Disabled)
        );
        assert_eq!(
            normalize_raw(SettingId::TiltbackSpeed, 50),
            Some(DeviceSettingValue::Number(50))
        );
        assert_eq!(
            normalize_raw(SettingId::TiltbackSpeed, 0),
            Some(DeviceSettingValue::Disabled)
        );
        assert_eq!(normalize_raw(SettingId::TiltbackSpeed, 40), None);
        assert_eq!(normalize_raw(SettingId::TiltbackSpeed, 55), None);
        assert_eq!(normalize_raw(SettingId::RemoteReceiverType, 0), None);
        assert_eq!(normalize_raw(SettingId::RemoteReceiverType, 4), None);
    }

    #[test]
    fn shared_control_and_codec_acceptance_have_the_same_value_domain() {
        let config = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        for id in supported_setting_ids() {
            let value = config.value(*id).expect("fixture semantic value");
            let control = control(*id).expect("shared control");
            assert_eq!(
                accepts(*id, value),
                control.accepts_value(value),
                "value-domain parity for {id:?}"
            );
        }

        // A read-only choice remains decodable and semantically valid without
        // being advertised as writable by its descriptor.
        let id = SettingId::LightingHardwareMode;
        assert_eq!(access(id), SettingAccess::ReadOnly);
        assert!(accepts(id, DeviceSettingValue::Choice(1)));
        assert!(descriptor(id).is_some_and(|row| row.access == SettingAccess::ReadOnly));
    }

    #[test]
    fn pwm_keeps_whole_percent_precision_and_explicit_disabled_sentinel() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let changed = original
            .changed(SettingId::PwmTiltback, DeviceSettingValue::Number(81))
            .expect("whole percent");
        assert_eq!(
            read_raw(changed.bytes(), spec(SettingId::PwmTiltback).unwrap()),
            Some(810)
        );
        assert_eq!(
            changed.value(SettingId::PwmTiltback),
            Some(DeviceSettingValue::Number(81))
        );
        assert_eq!(
            original
                .changed(SettingId::PwmTiltback, DeviceSettingValue::Disabled)
                .unwrap()
                .value(SettingId::PwmTiltback),
            Some(DeviceSettingValue::Disabled)
        );
        assert!(!accepts(
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(100)
        ));
    }

    #[test]
    fn enums_are_opaque_one_based_choices_and_unknown_bytes_stay_unknown() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let id = SettingId::RemoteReceiverType;
        match original.value(id) {
            Some(DeviceSettingValue::Choice(choice)) => assert!(choice > 0),
            other => panic!("expected known receiver choice, got {other:?}"),
        }
        assert!(!accepts(id, DeviceSettingValue::Choice(0)));
        assert!(!accepts(id, DeviceSettingValue::Choice(u16::MAX)));
        let mut bytes = *DEFAULT_CONFIG;
        let field = spec(id).expect("remote field");
        write_raw(&mut bytes, field, 255).expect("one-byte raw enum");
        assert_eq!(RefloatSettings::decode(&bytes).unwrap().value(id), None);

        let boolean_id = SettingId::LightingOn;
        let mut bytes = *DEFAULT_CONFIG;
        write_raw(
            &mut bytes,
            spec(boolean_id).expect("lighting boolean field"),
            2,
        )
        .expect("one-byte raw boolean");
        assert_eq!(
            RefloatSettings::decode(&bytes).unwrap().value(boolean_id),
            None
        );
        assert_eq!(normalize_raw(boolean_id, 2), None);
    }

    #[test]
    fn signed_low_temperature_roundtrips_as_signed_int8() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let changed = original
            .changed(
                SettingId::BmsCellLtThreshold,
                DeviceSettingValue::Number(-10),
            )
            .expect("valid low temperature");
        assert_eq!(
            read_raw(
                changed.bytes(),
                spec(SettingId::BmsCellLtThreshold).unwrap()
            ),
            Some(-10)
        );
        assert_eq!(
            changed.value(SettingId::BmsCellLtThreshold),
            Some(DeviceSettingValue::Number(-10))
        );
    }

    #[test]
    fn coupled_bms_and_ppm_beeper_values_are_validated_without_mutating_other_fields() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        assert_eq!(
            original.changed(
                SettingId::BmsCellLvThreshold,
                DeviceSettingValue::Number(4_400)
            ),
            Err(RefloatSettingsError::InvalidCombination)
        );
        let beeper_on = original
            .changed(
                SettingId::HardwareBeeperEnabled,
                DeviceSettingValue::Boolean(true),
            )
            .expect("beeper enabled under default UART receiver");
        assert_eq!(
            beeper_on.changed(SettingId::RemoteReceiverType, DeviceSettingValue::Choice(3)),
            Err(RefloatSettingsError::InvalidCombination)
        );
        assert_eq!(original.bytes(), DEFAULT_CONFIG);
    }

    #[test]
    fn hardware_and_package_disable_fields_remain_readable_but_read_only() {
        for id in [
            SettingId::PackageDisabled,
            SettingId::LightingHardwarePin,
            SettingId::SensorsSwapFootpadAdcs,
            SettingId::SensorsAdcchannel1,
            SettingId::PushbackHv,
            SettingId::PushbackLv,
        ] {
            assert_eq!(access(id), SettingAccess::ReadOnly);
            assert!(descriptor(id).is_some());
        }
        assert_eq!(
            descriptor(SettingId::PushbackHv).and_then(|row| row.value_semantics_key),
            Some("settings.refloat.voltage_cell_or_pack_basis.semantics")
        );
        assert_eq!(
            descriptor(SettingId::SensorsAdcchannel1).and_then(|row| row.value_semantics_key),
            Some("settings.refloat.footpad_adc_hardware.semantics")
        );
        assert_eq!(
            descriptor(SettingId::SensorsAdcchannel1).and_then(|row| row.help_key),
            Some("settings.refloat.footpad_adc_hardware.help")
        );
        for id in [SettingId::LightingOn, SettingId::LightingHeadlightsOn] {
            assert_eq!(apply_effect(id), SettingApplyEffect::Unspecified);
            assert_eq!(
                descriptor(id).and_then(|row| row.help_key),
                Some("settings.refloat.lighting_startup_preference.help")
            );
        }
        assert_eq!(
            descriptor(SettingId::PackageDisabled).and_then(|row| row.help_key),
            Some("settings.refloat.package_disable_read_only.help")
        );
        assert_eq!(
            apply_effect(SettingId::LightingFrontBrightness),
            SettingApplyEffect::Immediate
        );
        assert_eq!(
            descriptor(SettingId::LightingFrontBrightness).and_then(|row| row.help_key),
            Some("settings.refloat.full_config_persistent_stationary.help")
        );
        assert_eq!(
            apply_effect(SettingId::BmsEnabled),
            SettingApplyEffect::RestartRequired
        );
        let config = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        assert_eq!(
            applicability(SettingId::LightingFrontMode, lighting_mode(&config)),
            RefloatSettingApplicability::ConfiguredButInactive
        );
        assert_eq!(
            applicability(SettingId::LightingHardwareMode, lighting_mode(&config)),
            RefloatSettingApplicability::NotLighting
        );
        let external = config
            .changed(
                SettingId::LightingHardwareMode,
                DeviceSettingValue::Choice(3),
            )
            .expect("external mode");
        assert_eq!(
            applicability(SettingId::LightingFrontBrightness, lighting_mode(&external)),
            RefloatSettingApplicability::Applicable
        );
        assert_eq!(
            applicability(SettingId::LightingFrontMode, lighting_mode(&external)),
            RefloatSettingApplicability::ExternalControllerIgnoresSetting
        );
        let mut unknown_bytes = *DEFAULT_CONFIG;
        write_raw(
            &mut unknown_bytes,
            spec(SettingId::LightingHardwareMode).expect("lighting mode field"),
            4,
        )
        .expect("preserve unknown mode for readback classification");
        let unknown = RefloatSettings::decode(&unknown_bytes).expect("complete image");
        assert_eq!(lighting_mode(&unknown), crate::RefloatLightingMode::Unknown);
        assert_eq!(
            applicability(SettingId::LightingFrontMode, lighting_mode(&unknown)),
            RefloatSettingApplicability::UnknownHardware
        );
    }

    #[test]
    fn decoder_and_command_keep_signature_size_and_only_clear_default_marker_on_save() {
        let original = RefloatSettings::decode(DEFAULT_CONFIG).expect("pinned default image");
        let mut saved_bytes = *DEFAULT_CONFIG;
        saved_bytes[REFLOAT_CONFIG_LEN - 1] = 0;
        let saved = RefloatSettings::decode(&saved_bytes).expect("valid image");
        assert!(original.is_same_except_default(&saved));
        saved_bytes[20] ^= 1;
        assert!(!original.is_same_except_default(&RefloatSettings::decode(&saved_bytes).unwrap()));
        assert_eq!(
            RefloatSettings::decode(&DEFAULT_CONFIG[..REFLOAT_CONFIG_LEN - 1]),
            Err(RefloatSettingsError::UnsupportedLength)
        );
        let mut invalid = *DEFAULT_CONFIG;
        invalid[0] ^= 0xff;
        assert_eq!(
            RefloatSettings::decode(&invalid),
            Err(RefloatSettingsError::UnsupportedSignature)
        );
        let payload = original.write_command_payload();
        assert_eq!(payload.get(..2), Some([95_u8, 0].as_slice()));
        assert_eq!(payload.get(2..), Some(original.bytes().as_slice()));
    }
}
