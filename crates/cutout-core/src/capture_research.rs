//! User-reported settings evidence retained independently of decoded telemetry.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum named fields in a single worksheet snapshot.
pub const MAX_CAPTURE_SETTINGS_VALUES: usize = 64;

/// Maximum worksheet snapshots retained for one capture.
pub const MAX_CAPTURE_SETTINGS_OBSERVATIONS: usize = 1024;

/// Maximum capture-relative settings observation time (24 hours).
pub const MAX_CAPTURE_SETTINGS_OFFSET_MILLISECONDS: u64 = 24 * 60 * 60 * 1_000;

/// How a worksheet profile was selected; neither choice upgrades device support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CaptureResearchProfileSelection {
    /// The person selected a research profile.
    Manual,
    /// The existing resolver supplied the profile identity.
    Resolved,
}

/// Profile context supplied with a reported settings snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub struct CaptureResearchProfile {
    /// Stable protocol/model profile identity, preserved even when unknown locally.
    pub profile_id: String,
    /// PEV category, separate from protocol and model identity.
    pub pev_category: Option<String>,
    /// Independently supplied model description.
    pub model: Option<String>,
    /// Independently supplied firmware description.
    pub firmware: Option<String>,
    /// Origin of this profile selection, not hardware verification.
    pub selection: CaptureResearchProfileSelection,
}

/// Where the person observed the values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CaptureObservationSource {
    /// The wheel's own display.
    WheelDisplay,
    /// A manufacturer application.
    ManufacturerApp,
    /// Another source, optionally described in the note.
    Other,
}

/// Original user entry, without applying writable-setting limits.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
pub enum CaptureObservedValue {
    /// Explicitly unobserved; different from a reported zero or disabled value.
    Unobserved,
    /// Explicit Off, separate from numeric zero and a false boolean.
    Off,
    /// A reported boolean.
    Boolean(bool),
    /// Original reported text, including unusual numeric values.
    Entered(String),
}

/// One named setting or measurement in a user-reported snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub struct CaptureSettingObservationValue {
    /// Stable semantic catalog slug; unknown imported identities remain evidence.
    pub setting_id: String,
    /// Original entry with explicit unobserved and Off states.
    pub value: CaptureObservedValue,
    /// Original unit, or None when the unit was not established.
    pub unit: Option<String>,
    /// Optional context for this value.
    pub note: Option<String>,
}

/// A bounded, independently entered settings snapshot on a capture timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub struct CaptureSettingsObservation {
    /// Canonical identity of the capture where these values were reported.
    pub capture_id: String,
    /// Profile context without promoting support or verification.
    pub profile: CaptureResearchProfile,
    /// Capture-relative milliseconds when the person saw these values.
    pub observed_at_ms: u64,
    /// Capture-relative milliseconds when the snapshot was entered.
    pub recorded_at_ms: u64,
    /// Source of the person's observations, distinct from decoder readback.
    pub source: CaptureObservationSource,
    /// Reported values in worksheet order.
    pub values: Vec<CaptureSettingObservationValue>,
    /// Optional snapshot context.
    pub note: Option<String>,
}

/// Structural admission failure; writable-setting ranges are deliberately absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum CaptureSettingsObservationError {
    /// A required identity or entered value was blank.
    #[error("capture settings observation contains a blank value")]
    Blank,
    /// A bounded field or collection was too large.
    #[error("capture settings observation exceeds its storage limit")]
    TooLarge,
    /// A setting was supplied more than once in one snapshot.
    #[error("capture settings observation repeats a setting")]
    DuplicateSetting,
    /// The observation was placed after its entry time.
    #[error("capture settings observation is later than its entry time")]
    FutureObservation,
    /// The capture identity was not a UUID.
    #[error("capture settings observation has an invalid capture identity")]
    InvalidCaptureId,
    /// The capture-relative entry time exceeds the supported 24-hour span.
    #[error("capture settings observation exceeds the capture time limit")]
    TimeOutOfRange,
}

impl CaptureSettingsObservation {
    /// Checks storage bounds and evidence identities without interpreting write ranges.
    ///
    /// # Errors
    /// Returns the violated structural bound or identity invariant.
    pub fn validate(&self) -> Result<(), CaptureSettingsObservationError> {
        if uuid::Uuid::parse_str(&self.capture_id)
            .map_or(true, |id| id.to_string() != self.capture_id)
        {
            return Err(CaptureSettingsObservationError::InvalidCaptureId);
        }
        bounded_text(&self.profile.profile_id, 128, true)?;
        for text in [
            &self.profile.pev_category,
            &self.profile.model,
            &self.profile.firmware,
        ] {
            if let Some(text) = text {
                bounded_text(text, 128, true)?;
            }
        }
        if self.recorded_at_ms > MAX_CAPTURE_SETTINGS_OFFSET_MILLISECONDS {
            return Err(CaptureSettingsObservationError::TimeOutOfRange);
        }
        if self.observed_at_ms > self.recorded_at_ms {
            return Err(CaptureSettingsObservationError::FutureObservation);
        }
        if self.values.is_empty() {
            return Err(CaptureSettingsObservationError::Blank);
        }
        if self.values.len() > MAX_CAPTURE_SETTINGS_VALUES {
            return Err(CaptureSettingsObservationError::TooLarge);
        }
        let mut seen = std::collections::BTreeSet::new();
        for value in &self.values {
            bounded_text(&value.setting_id, 128, true)?;
            if !seen.insert(&value.setting_id) {
                return Err(CaptureSettingsObservationError::DuplicateSetting);
            }
            if let CaptureObservedValue::Entered(text) = &value.value {
                bounded_text(text, 256, true)?;
            }
            if let Some(unit) = &value.unit {
                bounded_text(unit, 32, true)?;
            }
            if let Some(note) = &value.note {
                bounded_text(note, 1024, false)?;
            }
        }
        if let Some(note) = &self.note {
            bounded_text(note, 1024, false)?;
        }
        Ok(())
    }
}

fn bounded_text(
    text: &str,
    max: usize,
    required: bool,
) -> Result<(), CaptureSettingsObservationError> {
    if required && text.trim().is_empty() {
        return Err(CaptureSettingsObservationError::Blank);
    }
    if text.len() > max {
        return Err(CaptureSettingsObservationError::TooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn observation() -> CaptureSettingsObservation {
        CaptureSettingsObservation {
            capture_id: "6b20c1f8-c544-49e5-bc20-247647de00b5".into(),
            profile: CaptureResearchProfile {
                profile_id: "nosfet_aero".into(),
                pev_category: Some("euc".into()),
                model: None,
                firmware: Some("unknown".into()),
                selection: CaptureResearchProfileSelection::Manual,
            },
            observed_at_ms: 120,
            recorded_at_ms: 900,
            source: CaptureObservationSource::ManufacturerApp,
            values: vec![CaptureSettingObservationValue {
                setting_id: "pwm_tiltback".into(),
                value: CaptureObservedValue::Entered("125.00".into()),
                unit: Some("%".into()),
                note: None,
            }],
            note: None,
        }
    }

    #[test]
    fn unusual_and_unknown_reported_values_are_preserved_without_write_limits() {
        let mut value = observation();
        value.values[0].setting_id = "future_setting".into();
        assert_eq!(value.validate(), Ok(()));
        assert_eq!(
            value.values[0].value,
            CaptureObservedValue::Entered("125.00".into())
        );
        for state in [
            CaptureObservedValue::Unobserved,
            CaptureObservedValue::Off,
            CaptureObservedValue::Boolean(false),
            CaptureObservedValue::Entered("0".into()),
        ] {
            value.values[0].value = state;
            assert_eq!(value.validate(), Ok(()));
        }
    }

    #[test]
    fn snapshot_time_is_bounded_to_twenty_four_hours() {
        let mut value = observation();
        value.recorded_at_ms = MAX_CAPTURE_SETTINGS_OFFSET_MILLISECONDS;
        value.observed_at_ms = value.recorded_at_ms;
        assert_eq!(value.validate(), Ok(()));
        value.recorded_at_ms += 1;
        assert_eq!(
            value.validate(),
            Err(CaptureSettingsObservationError::TimeOutOfRange)
        );
    }

    #[test]
    fn snapshots_reject_duplicate_identity_future_time_and_unbounded_text() {
        let mut value = observation();
        value.values.push(value.values[0].clone());
        assert_eq!(
            value.validate(),
            Err(CaptureSettingsObservationError::DuplicateSetting)
        );
        value.values.pop();
        value.observed_at_ms = 901;
        assert_eq!(
            value.validate(),
            Err(CaptureSettingsObservationError::FutureObservation)
        );
        value.observed_at_ms = 0;
        value.values[0].value = CaptureObservedValue::Entered("x".repeat(257));
        assert_eq!(
            value.validate(),
            Err(CaptureSettingsObservationError::TooLarge)
        );
    }
}
