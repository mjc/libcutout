//! Passive field catalog and readback for independently reported capture values.

use std::collections::BTreeMap;

use cutout_core::{
    CaptureObservedValue, CaptureSettingsObservation, DeviceEvent, DeviceSettingValue,
    PevcapDirection, PevcapRecord, ProtocolFamily, ProtocolSession, ReadOnlyResponse, SessionInput,
    SessionOutput, SettingId,
};

use super::{
    CATALOG, DeviceControlProfile, SettingControl, SettingGroup, SettingUnit, aero_control_profile,
    falcon_control_profile, number,
};

/// A named observation field. This grants no permission to change a setting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureResearchField {
    /// Stable identity preserved in the capture.
    pub id: &'static str,
    /// Semantic setting, or an independent displayed measurement.
    pub setting_id: Option<SettingId>,
    /// Existing localized setting name.
    pub label_key: &'static str,
    /// Existing presentation group.
    pub group: SettingGroup,
    /// Readable value shape, independent of write availability.
    pub control: SettingControl,
    /// Unit for an independent measurement without a writable control.
    pub unit_hint: Option<&'static str>,
    /// Meaning or model applicability still needs investigation.
    pub research_only: bool,
}

/// Explicitly selected research context, never resolved device identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureResearchCatalog {
    /// Stable profile selection.
    pub profile_id: &'static str,
    /// Human-readable model name.
    pub model: &'static str,
    /// Physical vehicle category, independent of the wire protocol.
    pub pev_category: &'static str,
    /// Decoder used only for this selected research context.
    pub protocol: ProtocolFamily,
    /// Named fields from the existing bindings.
    pub fields: Vec<CaptureResearchField>,
}

/// Available explicit research contexts. Selection never verifies connected hardware.
#[must_use]
pub fn capture_research_catalogs() -> Vec<CaptureResearchCatalog> {
    ["nosfet_aero", "begode_falcon"]
        .into_iter()
        .filter_map(capture_research_catalog)
        .collect()
}

fn profile(id: &str) -> Option<DeviceControlProfile> {
    match id {
        "nosfet_aero" => Some(aero_control_profile()),
        "begode_falcon" => Some(falcon_control_profile()),
        _ => None,
    }
}

/// Enumerates readable names without exposing a command or copying the settings catalog.
#[must_use]
pub fn capture_research_catalog(profile_id: &str) -> Option<CaptureResearchCatalog> {
    let profile = profile(profile_id)?;
    let (profile_id, model, protocol) = match profile_id {
        "nosfet_aero" => (
            "nosfet_aero",
            "NOSFET Aero",
            ProtocolFamily::VeteranLeaperkimNosfet,
        ),
        "begode_falcon" => (
            "begode_falcon",
            "Begode Falcon",
            ProtocolFamily::BegodeGotway,
        ),
        _ => return None,
    };
    let visible = profile.descriptors(false);
    let mut fields: Vec<_> = CATALOG
        .iter()
        .filter_map(|&(id, label_key, group, _)| {
            let binding = profile.settings_adapter.binding(id)?;
            let mut control = binding.control;
            if let SettingControl::Number { can_disable, .. } = &mut control {
                *can_disable = id == SettingId::PwmTiltback;
            }
            Some(CaptureResearchField {
                id: capture_research_setting_id(id)?,
                setting_id: Some(id),
                label_key,
                group,
                control,
                unit_hint: None,
                research_only: group == SettingGroup::Diagnostics
                    || !visible.iter().any(|entry| entry.id == id),
            })
        })
        .collect();
    fields.push(CaptureResearchField {
        id: "displayed_pack_voltage",
        setting_id: None,
        label_key: "capture.research.displayed_pack_voltage",
        group: SettingGroup::Diagnostics,
        control: number(0, 1_000_000, 3, SettingUnit::Volts),
        unit_hint: Some("V"),
        research_only: false,
    });
    Some(CaptureResearchCatalog {
        profile_id,
        model,
        pev_category: "euc",
        protocol,
        fields,
    })
}

/// Stable identity for a named capture research field, independent of display text.
/// Settings outside the existing capture field vocabulary return `None`.
#[must_use]
pub const fn capture_research_setting_id(id: SettingId) -> Option<&'static str> {
    Some(match id {
        SettingId::Headlight => "headlight",
        SettingId::HighBeam => "high_beam",
        SettingId::TiltbackSpeed => "tiltback_speed",
        SettingId::PwmTiltback => "pwm_tiltback",
        SettingId::PedalHardness => "pedal_hardness",
        SettingId::DisplayBrightness => "display_brightness",
        SettingId::DisplayUnits => "display_units",
        SettingId::BeeperVolumePercent => "beeper_volume_percent",
        SettingId::DynamicAssist => "dynamic_assist",
        SettingId::PedalDipCompensation => "pedal_dip_compensation",
        SettingId::LateralTiltLimit => "lateral_tilt_limit",
        SettingId::VoltageCorrection => "voltage_correction",
        SettingId::ChargeLimitDiagnostic => "charge_limit_diagnostic",
        SettingId::HighSpeedMode => "high_speed_mode",
        SettingId::LowBatteryMode => "low_battery_mode",
        SettingId::TransportMode => "transport_mode",
        SettingId::SpeedAlarmThreshold => "speed_alarm_threshold",
        SettingId::PedalAngle => "pedal_angle",
        SettingId::RidingPreset => "riding_preset",
        SettingId::BrakeOverpressureAlarm => "brake_overpressure_alarm",
        SettingId::PedalMode => "pedal_mode",
        SettingId::RollAngleMode => "roll_angle_mode",
        SettingId::SpeedAlarmMode => "speed_alarm_mode",
        SettingId::MaximumSpeed => "maximum_speed",
        SettingId::BeeperVolumeLevel => "beeper_volume_level",
        SettingId::LightingPattern => "lighting_pattern",
        SettingId::AccelerationAssist => "acceleration_assist",
        SettingId::Taillight => "taillight",
        SettingId::AutoShutdownRemaining => "auto_shutdown_remaining",
        SettingId::PowerOffDelay => "power_off_delay",
        SettingId::ChargeMode => "charge_mode",
        _ => return None,
    })
}

/// A native draft is structurally invalid; import still preserves unknown future fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CaptureResearchValueError {
    /// The selected native research profile is not known.
    #[error("Choose a device profile")]
    UnknownProfile,
    /// The named field is not in this profile.
    #[error("Setting does not belong to this profile")]
    UnknownField,
    /// A numeric entry is non-finite or not numeric.
    #[error("Enter a number, Off, or leave the value blank")]
    InvalidNumber,
}

/// Validates observation syntax and returns unusual field identities without clamping them.
///
/// # Errors
/// Rejects an unknown native field/profile or non-finite numeric text.
pub fn validate_capture_research_observation(
    observation: &CaptureSettingsObservation,
) -> Result<Vec<String>, CaptureResearchValueError> {
    let catalog = capture_research_catalog(&observation.profile.profile_id)
        .ok_or(CaptureResearchValueError::UnknownProfile)?;
    let mut unusual = Vec::new();
    for value in &observation.values {
        let field = catalog
            .fields
            .iter()
            .find(|field| field.id == value.setting_id)
            .ok_or(CaptureResearchValueError::UnknownField)?;
        if let CaptureObservedValue::Entered(text) = &value.value
            && let SettingControl::Number {
                minimum,
                maximum,
                precision,
                unit,
                ..
            } = field.control
        {
            let number = text
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .ok_or(CaptureResearchValueError::InvalidNumber)?;
            let Some(factor) = reported_unit_factor(unit, value.unit.as_deref()) else {
                continue;
            };
            let scaled = number * factor * 10_f64.powi(i32::from(precision));
            if scaled < f64::from(minimum) || scaled > f64::from(maximum) {
                unusual.push(value.setting_id.clone());
            }
        }
    }
    Ok(unusual)
}

/// A decoded value with the time of the packet which supplied it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureResearchDecodedValue {
    /// Stable setting or measurement identity.
    pub setting_id: &'static str,
    /// Decoded value, or explicit unknown from a present packet field.
    pub value: Option<DeviceSettingValue>,
    /// Capture-relative time of the packet.
    pub observed_at_ms: u64,
}

/// Replays input notifications only. Outbound captured writes are never executed.
#[derive(Clone, Debug)]
pub struct CaptureResearchReadback {
    profile: DeviceControlProfile,
    session: crate::RegisteredEucSession,
    decoder_active: bool,
    through_ms: u64,
    earliest_input_at_ms: Option<u64>,
    boundary_at_ms: Option<u64>,
    values: Vec<CaptureResearchDecodedValue>,
}

impl CaptureResearchReadback {
    /// Selects a decoder for the user's research profile without resolving device identity.
    #[must_use]
    pub fn new(profile_id: &str, through_ms: u64) -> Option<Self> {
        let profile = profile(profile_id)?;
        let session = match profile_id {
            "nosfet_aero" => crate::registry::nosfet_aero_session(),
            "begode_falcon" => crate::registry::begode_falcon_session(),
            _ => return None,
        };
        Some(Self {
            profile,
            session,
            decoder_active: false,
            through_ms,
            earliest_input_at_ms: None,
            boundary_at_ms: None,
            values: Vec::new(),
        })
    }

    /// Consumes one retained transport observation without emitting any transport effects.
    pub fn observe(&mut self, record: &PevcapRecord) {
        let at_ms = record.monotonic_ms.get();
        if at_ms > self.through_ms {
            return;
        }
        let mut output = Vec::new();
        match record.direction {
            PevcapDirection::LinkUp => {
                if !self.record_boundary(at_ms) {
                    return;
                }
                self.activate_decoder(record, &mut output);
            }
            PevcapDirection::LinkDown => {
                if !self.record_boundary(at_ms) {
                    return;
                }
                self.session.handle(SessionInput::LinkDown, &mut output);
                self.decoder_active = false;
            }
            PevcapDirection::Inbound => {
                if self.boundary_at_ms.is_some_and(|boundary| at_ms < boundary) {
                    return;
                }
                self.earliest_input_at_ms =
                    Some(self.earliest_input_at_ms.unwrap_or(at_ms).min(at_ms));
                // A capture can begin during an existing connection. Prime only the
                // passive decoder; transport effects are discarded below and this
                // does not add a link event to the retained evidence.
                if !self.decoder_active {
                    self.activate_decoder(record, &mut output);
                }
                self.session.handle(
                    SessionInput::Notification {
                        channel: record.characteristic,
                        bytes: &record.bytes,
                        monotonic_ms: record.monotonic_ms,
                    },
                    &mut output,
                );
            }
            PevcapDirection::Outbound => return,
        }
        for event in output {
            if let SessionOutput::Event(DeviceEvent::Telemetry(delta)) = &event
                && let Some(voltage) = delta.voltage
            {
                self.retain(CaptureResearchDecodedValue {
                    setting_id: "displayed_pack_voltage",
                    value: Some(DeviceSettingValue::Number(voltage.value.as_millivolts())),
                    observed_at_ms: record.monotonic_ms.get(),
                });
            }
            if let SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                ReadOnlyResponse::Settings(readback),
            )) = event
            {
                for observation in self.profile.normalize_capture_readback(&readback, true) {
                    let Some(id) = capture_research_setting_id(observation.id) else {
                        continue;
                    };
                    let value = CaptureResearchDecodedValue {
                        setting_id: id,
                        value: observation.value.map(|value| value.value),
                        observed_at_ms: record.monotonic_ms.get(),
                    };
                    self.retain(value);
                }
            }
        }
    }

    /// Last observations at or before the requested time. Missing fields stay absent.
    #[must_use]
    pub fn values(&self) -> &[CaptureResearchDecodedValue] {
        &self.values
    }

    fn record_boundary(&mut self, at_ms: u64) -> bool {
        if self.boundary_at_ms.is_some_and(|boundary| at_ms < boundary) {
            return false;
        }
        self.boundary_at_ms = Some(at_ms);
        // Preserve newer framing only when none of its input predates this boundary.
        // Otherwise values may depend on bytes or decoder context from both epochs.
        if self
            .earliest_input_at_ms
            .is_some_and(|earliest| at_ms < earliest)
        {
            return false;
        }
        self.values.clear();
        self.earliest_input_at_ms = None;
        true
    }

    fn activate_decoder(&mut self, record: &PevcapRecord, output: &mut Vec<SessionOutput>) {
        self.session.handle(
            SessionInput::LinkUp(cutout_core::LinkInfo {
                monotonic_ms: record.monotonic_ms,
                max_write_len: record.link_max_write_len,
            }),
            output,
        );
        self.decoder_active = true;
    }

    fn retain(&mut self, value: CaptureResearchDecodedValue) {
        match self
            .values
            .iter_mut()
            .find(|entry| entry.setting_id == value.setting_id)
        {
            Some(entry) if value.observed_at_ms >= entry.observed_at_ms => *entry = value,
            Some(_) => {}
            None => self.values.push(value),
        }
    }
}

/// Shares passive decoder prefixes across requested cutoffs for one research profile.
///
/// Ordered packets update one decoder state. Delayed packets split only the cutoff
/// groups which require different packet histories, retaining each cutoff's framing
/// and connection context without buffering or reordering captured transport records.
#[derive(Debug)]
pub struct CaptureResearchReadbackBatch {
    cutoffs: Vec<u64>,
    groups: BTreeMap<u64, CaptureResearchReadbackGroup>,
    #[cfg(test)]
    decoder_updates: usize,
}

#[derive(Debug)]
struct CaptureResearchReadbackGroup {
    minimum_cutoff: u64,
    readback: CaptureResearchReadback,
}

impl CaptureResearchReadbackBatch {
    /// Selects a passive decoder for bounded capture observation cutoffs.
    ///
    /// Returns `None` for an unknown profile, empty requests or existing capture
    /// count/time bounds being exceeded. Duplicate cutoffs share their result.
    #[must_use]
    pub fn new(profile_id: &str, cutoffs: &[u64]) -> Option<Self> {
        if cutoffs.len() > cutout_core::MAX_CAPTURE_SETTINGS_OBSERVATIONS
            || cutoffs
                .iter()
                .any(|at| *at > cutout_core::MAX_CAPTURE_SETTINGS_OFFSET_MILLISECONDS)
        {
            return None;
        }
        let mut cutoffs = cutoffs.to_vec();
        cutoffs.sort_unstable();
        cutoffs.dedup();
        let minimum_cutoff = *cutoffs.first()?;
        let maximum_cutoff = *cutoffs.last()?;
        let readback = CaptureResearchReadback::new(profile_id, maximum_cutoff)?;
        Some(Self {
            cutoffs,
            groups: BTreeMap::from([(
                maximum_cutoff,
                CaptureResearchReadbackGroup {
                    minimum_cutoff,
                    readback,
                },
            )]),
            #[cfg(test)]
            decoder_updates: 0,
        })
    }

    /// Consumes one retained record once per distinct eligible decoder prefix.
    pub fn observe(&mut self, record: &PevcapRecord) {
        if record.direction == PevcapDirection::Outbound {
            return;
        }
        let at_ms = record.monotonic_ms.get();
        self.split_at(at_ms);
        for (_, group) in self.groups.range_mut(at_ms..) {
            group.readback.observe(record);
            #[cfg(test)]
            {
                self.decoder_updates += 1;
            }
        }
    }

    /// Returns a requested cutoff's values, preserving original packet times.
    #[must_use]
    pub fn values_at(&self, at_ms: u64) -> Option<&[CaptureResearchDecodedValue]> {
        self.cutoffs.binary_search(&at_ms).ok()?;
        self.groups
            .range(at_ms..)
            .next()
            .map(|(_, group)| group.readback.values())
    }

    fn split_at(&mut self, at_ms: u64) {
        let Some((_, group)) = self.groups.range_mut(at_ms..).next() else {
            return;
        };
        if group.minimum_cutoff >= at_ms {
            return;
        }
        let split = self.cutoffs.partition_point(|cutoff| *cutoff < at_ms);
        let Some((&lower_maximum, &upper_minimum)) = self
            .cutoffs
            .get(..split)
            .and_then(<[u64]>::last)
            .zip(self.cutoffs.get(split))
        else {
            return;
        };
        let lower_group = CaptureResearchReadbackGroup {
            minimum_cutoff: group.minimum_cutoff,
            readback: group.readback.clone(),
        };
        group.minimum_cutoff = upper_minimum;
        self.groups.insert(lower_maximum, lower_group);
    }
}

/// Compares independent evidence only when its original unit and value shape are known.
#[must_use]
pub fn capture_research_values_differ(
    profile_id: &str,
    entered: &cutout_core::CaptureSettingObservationValue,
    decoded: DeviceSettingValue,
) -> Option<bool> {
    match (&entered.value, decoded) {
        (CaptureObservedValue::Off, DeviceSettingValue::Disabled) => Some(false),
        (CaptureObservedValue::Off, DeviceSettingValue::Number(_))
        | (CaptureObservedValue::Entered(_), DeviceSettingValue::Disabled) => {
            if let CaptureObservedValue::Entered(text) = &entered.value {
                text.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|number| number.is_finite())?;
            }
            let catalog = capture_research_catalog(profile_id)?;
            let field = catalog
                .fields
                .iter()
                .find(|field| field.id == entered.setting_id)?;
            match field.control {
                SettingControl::Number {
                    can_disable: true, ..
                } => Some(true),
                _ => None,
            }
        }
        (CaptureObservedValue::Boolean(reported), DeviceSettingValue::Boolean(value)) => {
            Some(*reported != value)
        }
        (CaptureObservedValue::Entered(text), DeviceSettingValue::Choice(value)) => {
            text.parse::<u16>().ok().map(|reported| reported != value)
        }
        (CaptureObservedValue::Entered(text), DeviceSettingValue::Number(value)) => {
            let catalog = capture_research_catalog(profile_id)?;
            let field = catalog
                .fields
                .iter()
                .find(|field| field.id == entered.setting_id)?;
            let SettingControl::Number {
                precision, unit, ..
            } = field.control
            else {
                return None;
            };
            let factor = reported_unit_factor(unit, entered.unit.as_deref())?;
            let text = text.trim();
            let reported = text
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())?;
            let decimals = text
                .split_once('.')
                .map_or(0, |(_, fractional)| fractional.len())
                .min(12);
            let decimals = i32::try_from(decimals).ok()?;
            let tolerance = (0.5 * 10_f64.powi(-decimals) * factor)
                .max(0.5 * 10_f64.powi(-i32::from(precision)));
            Some(
                (reported * factor - f64::from(value) / 10_f64.powi(i32::from(precision))).abs()
                    > tolerance,
            )
        }
        _ => None,
    }
}

fn reported_unit_factor(unit: SettingUnit, reported: Option<&str>) -> Option<f64> {
    match (unit, reported) {
        (SettingUnit::KilometresPerHour, Some("mph")) => Some(1.609_344),
        (SettingUnit::KilometresPerHour, Some("km/h"))
        | (SettingUnit::Percent | SettingUnit::PwmDutyPercent, Some("%"))
        | (SettingUnit::Degrees, Some("°"))
        | (SettingUnit::Level, Some("level"))
        | (SettingUnit::Seconds, Some("s"))
        | (SettingUnit::Minutes, Some("min"))
        | (SettingUnit::Volts, Some("V")) => Some(1.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
