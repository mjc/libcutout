//! Mechanical mobile projection of capture-linked research evidence.
use crate::{
    MobileCaptureSubmissionSourceDto, MobileSettingControlDto, MobileSettingGroupDto,
    RideDatabaseHandle,
};
use cutout_core::{
    CaptureObservationSource, CaptureObservedValue, CaptureResearchProfile,
    CaptureResearchProfileSelection, CaptureSettingObservationValue, CaptureSettingsObservation,
};

/// How the research profile was chosen; neither option verifies hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureResearchProfileSelectionDto {
    #[doc = "Selected by the user."]
    Manual,
    #[doc = "Supplied by the existing resolver."]
    Resolved,
}
/// Device context retained with each observation.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureResearchProfileDto {
    #[doc = "Stable research profile identity."]
    pub profile_id: String,
    #[doc = "Vehicle category independent of protocol."]
    pub pev_category: Option<String>,
    #[doc = "Reported model context."]
    pub model: Option<String>,
    #[doc = "Reported firmware context."]
    pub firmware: Option<String>,
    #[doc = "How the profile was selected."]
    pub selection: MobileCaptureResearchProfileSelectionDto,
}
/// Independent source of the entered values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureObservationSourceDto {
    #[doc = "Wheel display."]
    WheelDisplay,
    #[doc = "Manufacturer application."]
    ManufacturerApp,
    #[doc = "Another independently observed source."]
    Other,
}
/// Original observation; absence, Off and zero are distinct.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureObservedValueDto {
    #[doc = "Explicitly unobserved."]
    Unobserved,
    #[doc = "Explicit disabled state."]
    Off,
    #[doc = "Reported boolean."]
    Boolean {
        #[doc = "Original boolean."]
        value: bool,
    },
    #[doc = "Original entered text."]
    Entered {
        #[doc = "Original text."]
        value: String,
    },
}
/// One named value in the worksheet.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSettingObservationValueDto {
    #[doc = "Stable catalog field identity."]
    pub setting_id: String,
    #[doc = "Original or decoded observation value."]
    pub value: MobileCaptureObservedValueDto,
    #[doc = "Original unit, absent when unknown."]
    pub unit: Option<String>,
    #[doc = "Optional observation context."]
    pub note: Option<String>,
}
/// One admitted snapshot on the capture timeline.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSettingsObservationDto {
    #[doc = "Canonical capture identity."]
    pub capture_id: String,
    #[doc = "Selected research profile."]
    pub profile: MobileCaptureResearchProfileDto,
    #[doc = "Capture time when the value was observed."]
    pub observed_at_ms: u64,
    #[doc = "Capture time when entry was recorded."]
    pub recorded_at_ms: u64,
    #[doc = "Independent source of this observation."]
    pub source: MobileCaptureObservationSourceDto,
    #[doc = "Independently entered named values."]
    pub values: Vec<MobileCaptureSettingObservationValueDto>,
    #[doc = "Optional observation context."]
    pub note: Option<String>,
}
/// Native edits; Rust supplies capture identity and the recorded time.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSettingsDraftDto {
    #[doc = "Selected research profile."]
    pub profile: MobileCaptureResearchProfileDto,
    #[doc = "Capture time when the value was observed."]
    pub observed_at_ms: Option<u64>,
    #[doc = "Independent source of this observation."]
    pub source: MobileCaptureObservationSourceDto,
    #[doc = "Independently entered named values."]
    pub values: Vec<MobileCaptureSettingObservationValueDto>,
    #[doc = "Optional observation context."]
    pub note: Option<String>,
}
/// Named field from the protocol-owned worksheet catalog.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureResearchFieldDto {
    #[doc = "Stable catalog field identity."]
    pub id: String,
    #[doc = "Shared localization key."]
    pub label_key: String,
    #[doc = "Semantic presentation group."]
    pub group: MobileSettingGroupDto,
    #[doc = "Readable shape without write authorization."]
    pub control: MobileSettingControlDto,
    #[doc = "Meaning or applicability needs investigation."]
    pub research_only: bool,
    #[doc = "Unit for an independent measurement."]
    pub unit_hint: Option<String>,
}
/// Applicable worksheet fields; no write operation is exposed.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureResearchCatalogDto {
    #[doc = "Stable research profile identity."]
    pub profile_id: String,
    #[doc = "Reported model context."]
    pub model: String,
    #[doc = "Vehicle category independent of protocol."]
    pub pev_category: String,
    #[doc = "Protocol family of the selected research profile."]
    pub protocol: crate::MobileProtocolFamilyDto,
    #[doc = "Applicable named observation fields."]
    pub fields: Vec<MobileCaptureResearchFieldDto>,
}
/// Observation could not be accepted or read.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileCaptureResearchError {
    /// Draft or readback source is invalid.
    #[error("{reason}")]
    InvalidObservation {
        #[doc = "Actionable domain reason."]
        reason: String,
    },
    /// The capture no longer owns a recording writer.
    #[error("Capture is not recording")]
    NotRecording,
    /// Writer admission did not retain the snapshot.
    #[error("The writer queue did not accept these values")]
    AdmissionLost,
    /// Durability could not be confirmed.
    #[error("Could not save settings values")]
    WriterFailed,
}
impl From<MobileCaptureResearchProfileDto> for CaptureResearchProfile {
    fn from(v: MobileCaptureResearchProfileDto) -> Self {
        Self {
            profile_id: v.profile_id,
            pev_category: v.pev_category,
            model: v.model,
            firmware: v.firmware,
            selection: match v.selection {
                MobileCaptureResearchProfileSelectionDto::Manual => {
                    CaptureResearchProfileSelection::Manual
                }
                MobileCaptureResearchProfileSelectionDto::Resolved => {
                    CaptureResearchProfileSelection::Resolved
                }
            },
        }
    }
}
impl From<CaptureResearchProfile> for MobileCaptureResearchProfileDto {
    fn from(v: CaptureResearchProfile) -> Self {
        Self {
            profile_id: v.profile_id,
            pev_category: v.pev_category,
            model: v.model,
            firmware: v.firmware,
            selection: match v.selection {
                CaptureResearchProfileSelection::Manual => {
                    MobileCaptureResearchProfileSelectionDto::Manual
                }
                CaptureResearchProfileSelection::Resolved => {
                    MobileCaptureResearchProfileSelectionDto::Resolved
                }
            },
        }
    }
}
impl From<MobileCaptureObservationSourceDto> for CaptureObservationSource {
    fn from(v: MobileCaptureObservationSourceDto) -> Self {
        match v {
            MobileCaptureObservationSourceDto::WheelDisplay => Self::WheelDisplay,
            MobileCaptureObservationSourceDto::ManufacturerApp => Self::ManufacturerApp,
            MobileCaptureObservationSourceDto::Other => Self::Other,
        }
    }
}
impl From<CaptureObservationSource> for MobileCaptureObservationSourceDto {
    fn from(v: CaptureObservationSource) -> Self {
        match v {
            CaptureObservationSource::WheelDisplay => Self::WheelDisplay,
            CaptureObservationSource::ManufacturerApp => Self::ManufacturerApp,
            CaptureObservationSource::Other => Self::Other,
        }
    }
}
impl From<MobileCaptureSettingObservationValueDto> for CaptureSettingObservationValue {
    fn from(v: MobileCaptureSettingObservationValueDto) -> Self {
        Self {
            setting_id: v.setting_id,
            value: match v.value {
                MobileCaptureObservedValueDto::Unobserved => CaptureObservedValue::Unobserved,
                MobileCaptureObservedValueDto::Off => CaptureObservedValue::Off,
                MobileCaptureObservedValueDto::Boolean { value } => {
                    CaptureObservedValue::Boolean(value)
                }
                MobileCaptureObservedValueDto::Entered { value } => {
                    CaptureObservedValue::Entered(value)
                }
            },
            unit: v.unit,
            note: v.note,
        }
    }
}
impl From<CaptureSettingObservationValue> for MobileCaptureSettingObservationValueDto {
    fn from(v: CaptureSettingObservationValue) -> Self {
        Self {
            setting_id: v.setting_id,
            value: match v.value {
                CaptureObservedValue::Unobserved => MobileCaptureObservedValueDto::Unobserved,
                CaptureObservedValue::Off => MobileCaptureObservedValueDto::Off,
                CaptureObservedValue::Boolean(value) => {
                    MobileCaptureObservedValueDto::Boolean { value }
                }
                CaptureObservedValue::Entered(value) => {
                    MobileCaptureObservedValueDto::Entered { value }
                }
            },
            unit: v.unit,
            note: v.note,
        }
    }
}
impl From<CaptureSettingsObservation> for MobileCaptureSettingsObservationDto {
    fn from(v: CaptureSettingsObservation) -> Self {
        Self {
            capture_id: v.capture_id,
            profile: v.profile.into(),
            observed_at_ms: v.observed_at_ms,
            recorded_at_ms: v.recorded_at_ms,
            source: v.source.into(),
            values: v.values.into_iter().map(Into::into).collect(),
            note: v.note,
        }
    }
}

/// Returns the existing protocol catalog for a manually chosen research profile.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI exports require owned strings"
)]
#[uniffi::export]
pub fn capture_research_catalog(profile_id: String) -> Option<MobileCaptureResearchCatalogDto> {
    cutout_protocols::capture_research_catalog(&profile_id).map(Into::into)
}

/// Available research profiles; selection does not verify hardware support.
#[must_use]
#[uniffi::export]
pub fn capture_research_catalogs() -> Vec<MobileCaptureResearchCatalogDto> {
    cutout_protocols::capture_research_catalogs()
        .into_iter()
        .map(Into::into)
        .collect()
}

impl From<cutout_protocols::CaptureResearchCatalog> for MobileCaptureResearchCatalogDto {
    fn from(catalog: cutout_protocols::CaptureResearchCatalog) -> Self {
        Self {
            profile_id: catalog.profile_id.into(),
            model: catalog.model.into(),
            pev_category: catalog.pev_category.into(),
            protocol: cutout_core::ProtocolFamilyDto::from(catalog.protocol).into(),
            fields: catalog
                .fields
                .into_iter()
                .map(|field| MobileCaptureResearchFieldDto {
                    id: field.id.into(),
                    label_key: field.label_key.into(),
                    group: field.group.into(),
                    control: field.control.into(),
                    research_only: field.research_only,
                    unit_hint: field.unit_hint.map(Into::into),
                })
                .collect(),
        }
    }
}

#[uniffi::export]
impl RideDatabaseHandle {
    /// Reads evidence from the canonical retained capture; call off the UI thread.
    ///
    /// # Errors
    /// Returns an invalid source or storage failure.
    pub fn capture_settings_observations(
        &self,
        source: MobileCaptureSubmissionSourceDto,
    ) -> Result<Vec<MobileCaptureSettingsObservationDto>, MobileCaptureResearchError> {
        let source = source
            .try_into()
            .map_err(|error: crate::MobileCaptureSubmissionError| {
                MobileCaptureResearchError::InvalidObservation {
                    reason: error.to_string(),
                }
            })?;
        self.inner
            .capture_settings_observations(source)
            .map(|values| values.into_iter().map(Into::into).collect())
            .map_err(|error| MobileCaptureResearchError::InvalidObservation {
                reason: error.to_string(),
            })
    }
}

impl From<MobileCaptureSettingsObservationDto> for CaptureSettingsObservation {
    fn from(value: MobileCaptureSettingsObservationDto) -> Self {
        Self {
            capture_id: value.capture_id,
            profile: value.profile.into(),
            observed_at_ms: value.observed_at_ms,
            recorded_at_ms: value.recorded_at_ms,
            source: value.source.into(),
            values: value.values.into_iter().map(Into::into).collect(),
            note: value.note,
        }
    }
}

/// Field identities whose reported values fall outside the known writable range.
#[must_use]
#[uniffi::export]
pub fn capture_research_unusual_fields(
    observation: MobileCaptureSettingsObservationDto,
) -> Vec<String> {
    cutout_protocols::validate_capture_research_observation(&observation.into()).unwrap_or_default()
}

/// One passively decoded field and its original capture-relative packet time.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureResearchDecodedValueDto {
    /// Stable catalog setting identity.
    #[doc = "Stable catalog field identity."]
    pub setting_id: String,
    /// No decoded value when the captured protocol explicitly reported unknown.
    #[doc = "Original or decoded observation value."]
    pub value: Option<crate::MobileSettingValueDto>,
    /// Time of the packet that supplied this value.
    #[doc = "Capture time when the value was observed."]
    pub observed_at_ms: u64,
}

/// One independently requested observation cutoff on a capture timeline.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureResearchReadbackRequestDto {
    /// Explicit research decoder profile.
    pub profile_id: String,
    /// Include only packets observed at or before this capture-relative time.
    pub at_ms: u64,
}

/// Decoded values for one request, in the same order as the input requests.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureResearchReadbackDto {
    /// Research profile supplied by the request.
    pub profile_id: String,
    /// Capture-relative cutoff supplied by the request.
    pub at_ms: u64,
    /// Actual packet values and times; unavailable fields remain absent.
    pub values: Vec<MobileCaptureResearchDecodedValueDto>,
}

pub(crate) fn readback_batch(
    requests: Vec<MobileCaptureResearchReadbackRequestDto>,
    read: impl FnOnce(
        &mut dyn FnMut(cutout_core::PevcapRecord),
    ) -> Result<(), MobileCaptureResearchError>,
) -> Result<Vec<MobileCaptureResearchReadbackDto>, MobileCaptureResearchError> {
    if requests.len() > cutout_core::MAX_CAPTURE_SETTINGS_OBSERVATIONS
        || requests
            .iter()
            .any(|request| request.at_ms > cutout_core::MAX_CAPTURE_SETTINGS_OFFSET_MILLISECONDS)
    {
        return Err(MobileCaptureResearchError::InvalidObservation {
            reason: "Settings readback exceeds the capture limits".into(),
        });
    }
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let mut cutoffs_by_profile = std::collections::BTreeMap::<String, Vec<u64>>::new();
    for request in &requests {
        cutoffs_by_profile
            .entry(request.profile_id.clone())
            .or_default()
            .push(request.at_ms);
    }
    let mut readbacks: std::collections::BTreeMap<_, _> = cutoffs_by_profile
        .into_iter()
        .filter_map(|(profile_id, cutoffs)| {
            cutout_protocols::CaptureResearchReadbackBatch::new(&profile_id, &cutoffs)
                .map(|readback| (profile_id, readback))
        })
        .collect();
    // Preserve source validation even when every requested profile is unknown.
    read(&mut |record| {
        for readback in readbacks.values_mut() {
            readback.observe(&record);
        }
    })?;
    Ok(requests
        .into_iter()
        .map(|request| {
            let values = readbacks
                .get(&request.profile_id)
                .and_then(|readback| readback.values_at(request.at_ms))
                .unwrap_or_default()
                .iter()
                .map(|value| MobileCaptureResearchDecodedValueDto {
                    setting_id: value.setting_id.into(),
                    value: value.value.map(Into::into),
                    observed_at_ms: value.observed_at_ms,
                })
                .collect();
            MobileCaptureResearchReadbackDto {
                profile_id: request.profile_id,
                at_ms: request.at_ms,
                values,
            }
        })
        .collect())
}

#[uniffi::export]
impl RideDatabaseHandle {
    /// Replays retained input bytes without issuing any wheel commands.
    /// Unknown research profiles return unavailable values after source validation.
    ///
    /// # Errors
    /// Returns an invalid source, resource bound or storage failure.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports require owned strings"
    )]
    pub fn capture_research_readback(
        &self,
        source: MobileCaptureSubmissionSourceDto,
        profile_id: String,
        at_ms: u64,
    ) -> Result<Vec<MobileCaptureResearchDecodedValueDto>, MobileCaptureResearchError> {
        self.capture_research_readback_batch(
            source,
            vec![MobileCaptureResearchReadbackRequestDto { profile_id, at_ms }],
        )
        .map(|results| {
            results
                .into_iter()
                .next()
                .map_or_else(Vec::new, |result| result.values)
        })
    }

    /// Reads all displayed observation cutoffs in one retained transport scan.
    /// Unknown profiles keep their request position with unavailable values.
    ///
    /// # Errors
    /// Returns an invalid source, resource bound or storage failure.
    pub fn capture_research_readback_batch(
        &self,
        source: MobileCaptureSubmissionSourceDto,
        requests: Vec<MobileCaptureResearchReadbackRequestDto>,
    ) -> Result<Vec<MobileCaptureResearchReadbackDto>, MobileCaptureResearchError> {
        let source = source
            .try_into()
            .map_err(|error: crate::MobileCaptureSubmissionError| {
                MobileCaptureResearchError::InvalidObservation {
                    reason: error.to_string(),
                }
            })?;
        readback_batch(requests, |observe| {
            self.inner
                .visit_capture_transport_records(source, observe)
                .map_err(|error| MobileCaptureResearchError::InvalidObservation {
                    reason: error.to_string(),
                })
        })
    }
}

/// Compares independent values only when both meanings and units are established.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI exports require owned strings"
)]
#[uniffi::export]
pub fn capture_research_values_differ(
    profile_id: String,
    reported: MobileCaptureSettingObservationValueDto,
    decoded: crate::MobileSettingValueDto,
) -> Option<bool> {
    cutout_protocols::capture_research_values_differ(&profile_id, &reported.into(), decoded.into())
}

#[cfg(test)]
mod tests;
