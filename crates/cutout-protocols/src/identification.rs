use std::borrow::Borrow;

use cutout_core::{GattFingerprint, ModelRegistryEntry, ProtocolFamily};

use crate::{
    BegodeBanner, DeviceFamily, ProtocolFamilyClassification, classify_begode_ascii_banner,
};

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
pub struct StagedIdentityInput<'a, GattEvidence = &'a [GattFingerprint]> {
    /// BLE advertised name, when present.
    pub advertised_name: Option<&'a str>,

    /// Host-observed GATT fingerprints.
    pub gatt: GattEvidence,

    /// Passive stream-family classification from notification bytes.
    pub stream_family: ProtocolFamilyClassification,

    /// Most recent parsed model banner text.
    pub banner_model: Option<&'a str>,
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

/// Model evidence parsed from untrusted identity bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParsedModelBanner<'a> {
    /// Model name text extracted from a recognized identity banner.
    pub model: &'a str,
}

/// Parser for model identity bytes emitted by a protocol family.
#[derive(Clone, Copy, Debug)]
pub struct IdentityParser {
    parse_model_banner: for<'a> fn(&'a [u8]) -> Option<ParsedModelBanner<'a>>,
}

impl IdentityParser {
    /// Creates a parser from a model-banner parser function.
    #[must_use]
    pub const fn new(
        parse_model_banner: for<'a> fn(&'a [u8]) -> Option<ParsedModelBanner<'a>>,
    ) -> Self {
        Self { parse_model_banner }
    }

    /// Parses untrusted identity bytes as model banner evidence.
    #[must_use]
    pub fn parse_model_banner(self, bytes: &[u8]) -> Option<ParsedModelBanner<'_>> {
        (self.parse_model_banner)(bytes)
    }
}

const IDENTITY_PARSERS: [IdentityParser; 1] = [IdentityParser::new(parse_begode_model_banner)];

/// Iterates known identity parsers and returns the first model banner they recognize.
#[must_use]
pub fn parse_model_banner(bytes: &[u8]) -> Option<ParsedModelBanner<'_>> {
    IDENTITY_PARSERS
        .into_iter()
        .find_map(|parser| parser.parse_model_banner(bytes))
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
    input: &StagedIdentityInput<'_, impl Clone + IntoIterator<Item: Borrow<GattFingerprint>>>,
    registry: &[&'static ModelRegistryEntry],
) -> StagedIdentityResolution {
    let Some(expected_family) = protocol_family_from_classification(input.stream_family) else {
        return hints_only_resolution(input, registry);
    };

    registry
        .iter()
        .copied()
        .filter(|entry| entry.protocol_family == expected_family)
        .map(|entry| family_resolution(input, entry))
        .max_by_key(|resolution| resolution.confidence)
        .unwrap_or(StagedIdentityResolution::NO_MATCH)
}

/// Identifies the best known model from the crate-owned compile-time registry.
#[must_use]
pub fn identify_known_model(
    input: &StagedIdentityInput<'_, impl Clone + IntoIterator<Item: Borrow<GattFingerprint>>>,
) -> StagedIdentityResolution {
    identify_model(input, &crate::MODEL_REGISTRY)
}

fn hints_only_resolution(
    input: &StagedIdentityInput<'_, impl Clone + IntoIterator<Item: Borrow<GattFingerprint>>>,
    registry: &[&'static ModelRegistryEntry],
) -> StagedIdentityResolution {
    registry
        .iter()
        .copied()
        .map(|entry| StagedIdentityResolution {
            model: None,
            confidence: IdentityConfidence::HintsOnly,
            evidence: candidate_hints(input, entry),
        })
        .filter(|resolution| resolution.evidence != IdentityEvidence::empty())
        .max_by_key(|resolution| resolution.confidence)
        .unwrap_or(StagedIdentityResolution::NO_MATCH)
}

fn family_resolution<GattEvidence>(
    input: &StagedIdentityInput<'_, GattEvidence>,
    entry: &'static ModelRegistryEntry,
) -> StagedIdentityResolution
where
    GattEvidence: Clone + IntoIterator,
    GattEvidence::Item: Borrow<GattFingerprint>,
{
    input
        .banner_model
        .filter(|name| model_name_matches(name, entry))
        .map_or_else(
            || StagedIdentityResolution {
                model: None,
                confidence: IdentityConfidence::FamilyOnly,
                evidence: candidate_hints(input, entry).with_passive_family_match(),
            },
            |_| StagedIdentityResolution {
                model: Some(entry),
                confidence: IdentityConfidence::Model,
                evidence: candidate_hints(input, entry)
                    .with_passive_family_match()
                    .with_banner_model_match(),
            },
        )
}

fn candidate_hints<GattEvidence>(
    input: &StagedIdentityInput<'_, GattEvidence>,
    entry: &ModelRegistryEntry,
) -> IdentityEvidence
where
    GattEvidence: Clone + IntoIterator,
    GattEvidence::Item: Borrow<GattFingerprint>,
{
    [
        input
            .advertised_name
            .is_some_and(|name| model_name_matches(name, entry))
            .then_some(IdentityEvidence::with_advertised_name_hint as fn(IdentityEvidence) -> _),
        gatt_matches(input.gatt.clone(), entry.gatt)
            .then_some(IdentityEvidence::with_gatt_hint as fn(IdentityEvidence) -> _),
    ]
    .into_iter()
    .flatten()
    .fold(IdentityEvidence::empty(), |evidence, with_hint| {
        with_hint(evidence)
    })
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

fn model_name_matches(name: &str, entry: &ModelRegistryEntry) -> bool {
    contains_ascii_ignore_case(name, entry.model)
        || entry
            .advertised_name_hints
            .iter()
            .copied()
            .any(|hint| contains_ascii_ignore_case(name, hint))
}

fn parse_begode_model_banner(bytes: &[u8]) -> Option<ParsedModelBanner<'_>> {
    classify_begode_ascii_banner(bytes)
        .banner()
        .and_then(|banner| match banner {
            BegodeBanner::ModelName(model) => Some(ParsedModelBanner { model }),
            BegodeBanner::Firmware { .. } | BegodeBanner::Imu(_) => None,
        })
}

fn gatt_matches<GattEvidence>(observed: GattEvidence, expected: &[GattFingerprint]) -> bool
where
    GattEvidence: IntoIterator,
    GattEvidence::Item: Borrow<GattFingerprint>,
{
    observed.into_iter().any(|observed| {
        let observed = observed.borrow();
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
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_SERVICE_CHANNEL, DeviceFamily,
        IdentityConfidence, IdentityEvidence, ProtocolFamilyClassification, StagedIdentityInput,
        identify_known_model, identify_model, parse_model_banner,
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
            &StagedIdentityInput {
                advertised_name: Some("Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Pending,
                banner_model: None,
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
            &StagedIdentityInput {
                advertised_name: None,
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner_model: None,
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_passive_family_match());
    }

    #[test]
    fn begode_family_magic_and_name_banner_resolve_falcon() {
        let input = StagedIdentityInput {
            advertised_name: Some("Begode_Falcon"),
            gatt: &BEGODE_GATT,
            stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
            banner_model: Some("Falcon"),
        };
        let resolution = identify_model(&input, &[&BEGODE_FALCON_REGISTRY_ENTRY]);
        let known_resolution = identify_known_model(&input);

        assert_eq!(known_resolution, resolution);
        assert_eq!(resolution.confidence, IdentityConfidence::Model);
        assert_eq!(resolution.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert!(resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn known_model_identification_uses_protocol_owned_registry() {
        let resolution = identify_known_model(&StagedIdentityInput {
            advertised_name: Some("Begode_Falcon"),
            gatt: &BEGODE_GATT,
            stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
            banner_model: Some("Falcon"),
        });

        assert_eq!(resolution.confidence, IdentityConfidence::Model);
        assert_eq!(resolution.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert!(resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn identity_accepts_streamed_gatt_evidence_without_a_slice() {
        let resolution = identify_known_model(&StagedIdentityInput {
            advertised_name: None,
            gatt: BEGODE_GATT.iter().copied(),
            stream_family: ProtocolFamilyClassification::Pending,
            banner_model: None,
        });

        assert_eq!(resolution.confidence, IdentityConfidence::HintsOnly);
        assert!(resolution.evidence.has_gatt_hint());
    }

    #[test]
    fn identity_parsers_find_model_banner_without_transport_knowing_family_type() {
        assert_eq!(
            parse_model_banner(b"NAME=Falcon"),
            Some(super::ParsedModelBanner { model: "Falcon" })
        );
        assert_eq!(parse_model_banner(&[0x55, 0xaa, 0x20, 0x20]), None);
    }

    #[test]
    fn conflicting_stream_family_rejects_advertised_name_match() {
        let resolution = identify_model(
            &StagedIdentityInput {
                advertised_name: Some("Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero),
                banner_model: Some("Falcon"),
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
            &StagedIdentityInput {
                advertised_name: Some("Begode_Master"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner_model: Some("Master"),
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_passive_family_match());
        assert!(!resolution.evidence.has_banner_model_match());
    }
}
