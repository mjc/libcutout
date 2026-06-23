use std::borrow::Borrow;

use cutout_core::{GattFingerprint, ModelRegistryEntry, ProtocolFamily};

use crate::{
    BegodeBanner, BegodeBannerParse, DeviceFamily, ProtocolFamilyClassification,
    classify_begode_ascii_banner,
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

/// Defensive staged identity outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedIdentityOutcome {
    /// A concrete registry model matched the evidence.
    Matched,

    /// No registry candidate matched the supplied evidence.
    NoMatch,

    /// Only weak advertisement or GATT hints matched.
    HintsOnly,

    /// Passive wire evidence identified a protocol family, but not a model.
    FamilyOnly,

    /// More than one registry candidate matched equally.
    Ambiguous,

    /// Evidence contradicted a registry candidate or family claim.
    Conflict,

    /// Identity evidence was syntactically malformed.
    Malformed,
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

    /// Most recent parsed model banner evidence.
    pub banner_model: IdentityBannerEvidence<'a>,
}

/// Result of staged identity detection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagedIdentityResolution {
    /// Resolved model entry when model confidence is high enough.
    pub model: Option<&'static ModelRegistryEntry>,

    /// Defensive typed outcome for the decision.
    pub outcome: StagedIdentityOutcome,

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

/// Parsed identity banner evidence from untrusted bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityBannerEvidence<'a> {
    /// No identity banner was present.
    Missing,

    /// A recognized model-name banner was present.
    Model(ParsedModelBanner<'a>),

    /// The bytes looked like identity evidence but were malformed.
    Malformed,
}

impl<'a> IdentityBannerEvidence<'a> {
    /// Creates model-name banner evidence.
    #[must_use]
    pub const fn model(model: &'a str) -> Self {
        Self::Model(ParsedModelBanner { model })
    }
}

/// Parser for model identity bytes emitted by a protocol family.
#[derive(Clone, Copy, Debug)]
pub struct IdentityParser {
    parse_model_banner: for<'a> fn(&'a [u8]) -> IdentityBannerEvidence<'a>,
}

impl IdentityParser {
    /// Creates a parser from a model-banner parser function.
    #[must_use]
    pub const fn new(
        parse_model_banner: for<'a> fn(&'a [u8]) -> IdentityBannerEvidence<'a>,
    ) -> Self {
        Self { parse_model_banner }
    }

    /// Parses untrusted identity bytes as model banner evidence.
    #[must_use]
    pub fn parse_model_banner(self, bytes: &[u8]) -> IdentityBannerEvidence<'_> {
        (self.parse_model_banner)(bytes)
    }
}

const IDENTITY_PARSERS: [IdentityParser; 1] = [IdentityParser::new(parse_begode_model_banner)];

/// Iterates known identity parsers and returns the first identity evidence they recognize.
#[must_use]
pub fn parse_model_banner(bytes: &[u8]) -> IdentityBannerEvidence<'_> {
    IDENTITY_PARSERS
        .into_iter()
        .map(|parser| parser.parse_model_banner(bytes))
        .find(|evidence| !matches!(evidence, IdentityBannerEvidence::Missing))
        .unwrap_or(IdentityBannerEvidence::Missing)
}

impl StagedIdentityResolution {
    const NO_MATCH: Self = Self {
        model: None,
        outcome: StagedIdentityOutcome::NoMatch,
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

    best_resolution(
        registry
            .iter()
            .copied()
            .filter(|entry| entry.protocol_family == expected_family)
            .map(|entry| family_resolution(input, entry)),
    )
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
            outcome: StagedIdentityOutcome::HintsOnly,
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
    let evidence = candidate_hints(input, entry).with_passive_family_match();
    match input.banner_model {
        IdentityBannerEvidence::Model(ParsedModelBanner { model })
            if model_name_matches(model, entry) =>
        {
            StagedIdentityResolution {
                model: Some(entry),
                outcome: StagedIdentityOutcome::Matched,
                confidence: IdentityConfidence::Model,
                evidence: evidence.with_banner_model_match(),
            }
        }
        IdentityBannerEvidence::Model(_) => StagedIdentityResolution {
            model: None,
            outcome: StagedIdentityOutcome::Conflict,
            confidence: IdentityConfidence::NoMatch,
            evidence,
        },
        IdentityBannerEvidence::Malformed => StagedIdentityResolution {
            model: None,
            outcome: StagedIdentityOutcome::Malformed,
            confidence: IdentityConfidence::NoMatch,
            evidence,
        },
        IdentityBannerEvidence::Missing => StagedIdentityResolution {
            model: None,
            outcome: StagedIdentityOutcome::FamilyOnly,
            confidence: IdentityConfidence::FamilyOnly,
            evidence,
        },
    }
}

impl<'a> From<ParsedModelBanner<'a>> for IdentityBannerEvidence<'a> {
    fn from(banner: ParsedModelBanner<'a>) -> Self {
        Self::Model(banner)
    }
}

fn best_resolution(
    resolutions: impl IntoIterator<Item = StagedIdentityResolution>,
) -> StagedIdentityResolution {
    let mut best = StagedIdentityResolution::NO_MATCH;
    let mut ambiguous = false;
    for resolution in resolutions {
        match resolution_rank(resolution).cmp(&resolution_rank(best)) {
            core::cmp::Ordering::Greater => {
                best = resolution;
                ambiguous = false;
            }
            core::cmp::Ordering::Equal if resolution_rank(resolution) > 0 => {
                ambiguous = true;
            }
            core::cmp::Ordering::Equal | core::cmp::Ordering::Less => {}
        }
    }
    if ambiguous {
        StagedIdentityResolution {
            model: None,
            outcome: StagedIdentityOutcome::Ambiguous,
            confidence: best.confidence,
            evidence: best.evidence,
        }
    } else {
        best
    }
}

const fn resolution_rank(resolution: StagedIdentityResolution) -> u8 {
    match resolution.outcome {
        StagedIdentityOutcome::NoMatch => 0,
        StagedIdentityOutcome::Malformed => 1,
        StagedIdentityOutcome::Conflict => 2,
        StagedIdentityOutcome::HintsOnly => 3,
        StagedIdentityOutcome::FamilyOnly => 4,
        StagedIdentityOutcome::Matched => 5,
        StagedIdentityOutcome::Ambiguous => 6,
    }
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

fn parse_begode_model_banner(bytes: &[u8]) -> IdentityBannerEvidence<'_> {
    match classify_begode_ascii_banner(bytes) {
        BegodeBannerParse::Banner(BegodeBanner::ModelName(model)) => {
            IdentityBannerEvidence::model(model)
        }
        BegodeBannerParse::NonAscii if bytes.starts_with(b"NAME") => {
            IdentityBannerEvidence::Malformed
        }
        BegodeBannerParse::Banner(BegodeBanner::Firmware { .. } | BegodeBanner::Imu(_))
        | BegodeBannerParse::Empty
        | BegodeBannerParse::BinaryFrame
        | BegodeBannerParse::NonAscii
        | BegodeBannerParse::UnknownText => IdentityBannerEvidence::Missing,
    }
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
    use cutout_core::{
        Capabilities, GattFingerprint, GattRoles, ModelRegistryEntry, ProtocolFamily,
        VerificationStatus,
    };

    use crate::{
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_SERVICE_CHANNEL, DeviceFamily,
        IdentityBannerEvidence, IdentityConfidence, IdentityEvidence, ProtocolFamilyClassification,
        StagedIdentityInput, StagedIdentityOutcome, identify_known_model, identify_model,
        parse_model_banner,
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
                banner_model: IdentityBannerEvidence::Missing,
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::HintsOnly);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::HintsOnly);
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
                banner_model: IdentityBannerEvidence::Missing,
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::FamilyOnly);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_passive_family_match());
    }

    #[test]
    fn begode_family_magic_and_name_banner_resolve_falcon() {
        let input = StagedIdentityInput {
            advertised_name: Some("Begode_Falcon"),
            gatt: &BEGODE_GATT,
            stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
            banner_model: IdentityBannerEvidence::model("Falcon"),
        };
        let resolution = identify_model(&input, &[&BEGODE_FALCON_REGISTRY_ENTRY]);
        let known_resolution = identify_known_model(&input);

        assert_eq!(known_resolution, resolution);
        assert_eq!(resolution.confidence, IdentityConfidence::Model);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::Matched);
        assert_eq!(resolution.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert!(resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn known_model_identification_uses_protocol_owned_registry() {
        let resolution = identify_known_model(&StagedIdentityInput {
            advertised_name: Some("Begode_Falcon"),
            gatt: &BEGODE_GATT,
            stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
            banner_model: IdentityBannerEvidence::model("Falcon"),
        });

        assert_eq!(resolution.confidence, IdentityConfidence::Model);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::Matched);
        assert_eq!(resolution.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert!(resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn identity_accepts_streamed_gatt_evidence_without_a_slice() {
        let resolution = identify_known_model(&StagedIdentityInput {
            advertised_name: None,
            gatt: BEGODE_GATT.iter().copied(),
            stream_family: ProtocolFamilyClassification::Pending,
            banner_model: IdentityBannerEvidence::Missing,
        });

        assert_eq!(resolution.confidence, IdentityConfidence::HintsOnly);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::HintsOnly);
        assert!(resolution.evidence.has_gatt_hint());
    }

    #[test]
    fn identity_parsers_find_model_banner_without_transport_knowing_family_type() {
        assert_eq!(
            parse_model_banner(b"NAME=Falcon"),
            IdentityBannerEvidence::model("Falcon")
        );
        assert_eq!(
            parse_model_banner(&[0x55, 0xaa, 0x20, 0x20]),
            IdentityBannerEvidence::Missing
        );
    }

    #[test]
    fn identity_parsers_tag_malformed_model_banner() {
        assert_eq!(
            parse_model_banner(b"NAME=Falcon\x00"),
            IdentityBannerEvidence::Malformed
        );
    }

    #[test]
    fn malformed_banner_evidence_reports_malformed_identity() {
        let resolution = identify_model(
            &StagedIdentityInput {
                advertised_name: Some("Begode_Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner_model: IdentityBannerEvidence::Malformed,
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::NoMatch);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::Malformed);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_passive_family_match());
        assert!(!resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn conflicting_stream_family_rejects_advertised_name_match() {
        let resolution = identify_model(
            &StagedIdentityInput {
                advertised_name: Some("Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero),
                banner_model: IdentityBannerEvidence::model("Falcon"),
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::NoMatch);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::NoMatch);
        assert_eq!(resolution.model, None);
        assert_eq!(resolution.evidence, IdentityEvidence::empty());
    }

    #[test]
    fn different_name_banner_reports_conflict() {
        let resolution = identify_model(
            &StagedIdentityInput {
                advertised_name: Some("Begode_Master"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner_model: IdentityBannerEvidence::model("Master"),
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::NoMatch);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::Conflict);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_passive_family_match());
        assert!(!resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn duplicate_matching_models_report_ambiguity() {
        static OTHER_FALCON: ModelRegistryEntry = ModelRegistryEntry {
            manufacturer: "Other",
            model: "Falcon",
            protocol_family: ProtocolFamily::BegodeGotway,
            advertised_name_hints: &["Falcon"],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &BEGODE_GATT,
            capabilities: Capabilities::from_supported_commands([]),
            verification: VerificationStatus::Inferred,
        };
        let resolution = identify_model(
            &StagedIdentityInput {
                advertised_name: Some("Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner_model: IdentityBannerEvidence::model("Falcon"),
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY, &OTHER_FALCON],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::Model);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::Ambiguous);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_banner_model_match());
    }
}
