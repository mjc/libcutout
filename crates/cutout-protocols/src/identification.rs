use cutout_core::{GattFingerprint, ModelRegistryEntry, ProtocolFamily};

use crate::{BegodeBanner, DeviceFamily, ProtocolFamilyClassification};

/// Confidence level for staged model identification.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum IdentityConfidence {
    /// No registry candidate matched the supplied evidence.
    NoMatch,

    /// Only weak advertisement or GATT hints matched.
    HintsOnly,

    /// Passive wire evidence identified a protocol family, but not a model.
    FamilyOnly,

    /// Passive family evidence and model-specific identity evidence agreed.
    Model,
}

/// Bitset of evidence that contributed to a staged identity decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IdentityEvidence(u8);

impl IdentityEvidence {
    const ADVERTISED_NAME_HINT: u8 = 1 << 0;
    const GATT_HINT: u8 = 1 << 1;
    const PASSIVE_FAMILY_MATCH: u8 = 1 << 2;
    const BANNER_MODEL_MATCH: u8 = 1 << 3;

    /// Empty evidence set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns whether an advertised-name hint matched.
    #[must_use]
    pub const fn has_advertised_name_hint(self) -> bool {
        self.0 & Self::ADVERTISED_NAME_HINT != 0
    }

    /// Returns whether a GATT fingerprint hint matched.
    #[must_use]
    pub const fn has_gatt_hint(self) -> bool {
        self.0 & Self::GATT_HINT != 0
    }

    /// Returns whether passive stream family evidence matched.
    #[must_use]
    pub const fn has_passive_family_match(self) -> bool {
        self.0 & Self::PASSIVE_FAMILY_MATCH != 0
    }

    /// Returns whether a model-name banner matched.
    #[must_use]
    pub const fn has_banner_model_match(self) -> bool {
        self.0 & Self::BANNER_MODEL_MATCH != 0
    }

    const fn with_advertised_name_hint(self) -> Self {
        Self(self.0 | Self::ADVERTISED_NAME_HINT)
    }

    const fn with_gatt_hint(self) -> Self {
        Self(self.0 | Self::GATT_HINT)
    }

    const fn with_passive_family_match(self) -> Self {
        Self(self.0 | Self::PASSIVE_FAMILY_MATCH)
    }

    const fn with_banner_model_match(self) -> Self {
        Self(self.0 | Self::BANNER_MODEL_MATCH)
    }
}

/// Borrowed evidence collected during staged identity detection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagedIdentityInput<'a> {
    /// BLE advertised name, when present.
    pub advertised_name: Option<&'a str>,

    /// Host-observed GATT fingerprints.
    pub gatt: &'a [GattFingerprint],

    /// Passive stream-family classification from notification bytes.
    pub stream_family: ProtocolFamilyClassification,

    /// Most recent parsed identity/config banner.
    pub banner: Option<BegodeBanner<'a>>,
}

/// Result of staged identity detection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagedIdentityResolution {
    /// Resolved model entry when model confidence is high enough.
    pub model: Option<&'static ModelRegistryEntry>,

    /// Confidence level for the decision.
    pub confidence: IdentityConfidence,

    /// Evidence that contributed to this decision.
    pub evidence: IdentityEvidence,
}

impl StagedIdentityResolution {
    const NO_MATCH: Self = Self {
        model: None,
        confidence: IdentityConfidence::NoMatch,
        evidence: IdentityEvidence::empty(),
    };
}

/// Identifies the best registry model candidate from staged, non-actuating evidence.
#[must_use]
pub fn identify_model(
    input: StagedIdentityInput<'_>,
    registry: &[&'static ModelRegistryEntry],
) -> StagedIdentityResolution {
    let Some(expected_family) = protocol_family_from_classification(input.stream_family) else {
        return hints_only_resolution(input, registry);
    };

    let mut best = StagedIdentityResolution::NO_MATCH;

    for entry in registry {
        if entry.protocol_family != expected_family {
            continue;
        }

        let mut evidence = candidate_hints(input, entry).with_passive_family_match();
        if has_matching_model_banner(input.banner, entry) {
            evidence = evidence.with_banner_model_match();
            return StagedIdentityResolution {
                model: Some(*entry),
                confidence: IdentityConfidence::Model,
                evidence,
            };
        }

        best = max_resolution(
            best,
            StagedIdentityResolution {
                model: None,
                confidence: IdentityConfidence::FamilyOnly,
                evidence,
            },
        );
    }

    best
}

fn hints_only_resolution(
    input: StagedIdentityInput<'_>,
    registry: &[&'static ModelRegistryEntry],
) -> StagedIdentityResolution {
    let mut best = StagedIdentityResolution::NO_MATCH;

    for entry in registry {
        let evidence = candidate_hints(input, entry);
        if evidence != IdentityEvidence::empty() {
            best = max_resolution(
                best,
                StagedIdentityResolution {
                    model: None,
                    confidence: IdentityConfidence::HintsOnly,
                    evidence,
                },
            );
        }
    }

    best
}

fn candidate_hints(input: StagedIdentityInput<'_>, entry: &ModelRegistryEntry) -> IdentityEvidence {
    let mut evidence = IdentityEvidence::empty();

    if input
        .advertised_name
        .is_some_and(|name| model_name_matches(name, entry))
    {
        evidence = evidence.with_advertised_name_hint();
    }
    if gatt_matches(input.gatt, entry.gatt) {
        evidence = evidence.with_gatt_hint();
    }

    evidence
}

fn max_resolution(
    current: StagedIdentityResolution,
    candidate: StagedIdentityResolution,
) -> StagedIdentityResolution {
    if candidate.confidence > current.confidence {
        candidate
    } else {
        current
    }
}

const fn protocol_family_from_classification(
    classification: ProtocolFamilyClassification,
) -> Option<ProtocolFamily> {
    match classification {
        ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero) => {
            Some(ProtocolFamily::VeteranLeaperkimNosfet)
        }
        ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon) => {
            Some(ProtocolFamily::BegodeGotway)
        }
        ProtocolFamilyClassification::Pending | ProtocolFamilyClassification::Unknown => None,
    }
}

fn has_matching_model_banner(banner: Option<BegodeBanner<'_>>, entry: &ModelRegistryEntry) -> bool {
    matches!(banner, Some(BegodeBanner::ModelName(name)) if model_name_matches(name, entry))
}

fn model_name_matches(name: &str, entry: &ModelRegistryEntry) -> bool {
    contains_ascii_ignore_case(name, entry.model)
        || entry
            .advertised_name_hints
            .iter()
            .copied()
            .any(|hint| contains_ascii_ignore_case(name, hint))
}

fn gatt_matches(observed: &[GattFingerprint], expected: &[GattFingerprint]) -> bool {
    observed.iter().any(|observed| {
        expected.iter().any(|expected| {
            observed.service == expected.service
                && observed.characteristic == expected.characteristic
                && roles_include(observed, expected)
        })
    })
}

fn roles_include(observed: &GattFingerprint, expected: &GattFingerprint) -> bool {
    (!expected.roles.supports_read() || observed.roles.supports_read())
        && (!expected.roles.supports_write() || observed.roles.supports_write())
        && (!expected.roles.supports_write_without_response()
            || observed.roles.supports_write_without_response())
        && (!expected.roles.supports_notify() || observed.roles.supports_notify())
        && (!expected.roles.supports_indicate() || observed.roles.supports_indicate())
}

fn contains_ascii_ignore_case(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();

    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|window| ascii_eq_ignore_case(window, needle))
}

fn ascii_eq_ignore_case(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

#[cfg(test)]
mod tests {
    use cutout_core::{GattFingerprint, GattRoles, VerificationStatus};

    use crate::{
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_SERVICE_CHANNEL, BegodeBanner,
        DeviceFamily, IdentityConfidence, IdentityEvidence, ProtocolFamilyClassification,
        StagedIdentityInput, identify_model,
    };

    const BEGODE_GATT: [GattFingerprint; 1] = [GattFingerprint {
        service: BEGODE_SERVICE_CHANNEL,
        characteristic: BEGODE_DATA_CHANNEL,
        roles: GattRoles::empty()
            .with_write_without_response()
            .with_notify(),
        verification: VerificationStatus::HardwareVerified,
    }];

    #[test]
    fn advertised_name_and_shared_gatt_are_hints_only() {
        let resolution = identify_model(
            StagedIdentityInput {
                advertised_name: Some("Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Pending,
                banner: None,
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::HintsOnly);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_advertised_name_hint());
        assert!(resolution.evidence.has_gatt_hint());
    }

    #[test]
    fn begode_family_magic_without_model_evidence_does_not_resolve_falcon() {
        let resolution = identify_model(
            StagedIdentityInput {
                advertised_name: None,
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner: None,
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_passive_family_match());
    }

    #[test]
    fn begode_family_magic_and_name_banner_resolve_falcon() {
        let resolution = identify_model(
            StagedIdentityInput {
                advertised_name: Some("Begode_Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner: Some(BegodeBanner::ModelName("Falcon")),
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::Model);
        assert_eq!(resolution.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert!(resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn conflicting_stream_family_rejects_advertised_name_match() {
        let resolution = identify_model(
            StagedIdentityInput {
                advertised_name: Some("Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero),
                banner: Some(BegodeBanner::ModelName("Falcon")),
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::NoMatch);
        assert_eq!(resolution.model, None);
        assert_eq!(resolution.evidence, IdentityEvidence::empty());
    }

    #[test]
    fn different_name_banner_keeps_begode_resolution_at_family_level() {
        let resolution = identify_model(
            StagedIdentityInput {
                advertised_name: Some("Begode_Master"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner: Some(BegodeBanner::ModelName("Master")),
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_passive_family_match());
        assert!(!resolution.evidence.has_banner_model_match());
    }
}
