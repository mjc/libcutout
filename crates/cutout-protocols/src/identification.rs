use std::{borrow::Borrow, str};

use arrayvec::ArrayVec;
use bytes::Bytes;
use cutout_core::{GattFingerprint, ModelRegistryEntry, ProtocolFamily};

use crate::{
    BegodeBanner, BegodeBannerParse, BegodeFrame, BegodeFrameParseResult, BegodeFrameReassembler,
    DeviceFamily, ProtocolFamilyClassification, VeteranFrame, VeteranTelemetry,
    classify_begode_ascii_banner,
};

const DETECTION_MAX_GATT_FINGERPRINTS: usize = 16;

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
    const PROTOCOL_MODEL_ID: u8 = 1 << 4;

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

    /// Returns whether protocol-owned model-id evidence was present.
    #[must_use]
    pub const fn has_protocol_model_id(self) -> bool {
        self.0 & Self::PROTOCOL_MODEL_ID != 0
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

    const fn with_protocol_model_id(self) -> Self {
        Self(self.0 | Self::PROTOCOL_MODEL_ID)
    }
}

/// Model identity decoded from protocol-owned bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolModelIdentity {
    /// Protocol family that owned and decoded the model id.
    pub family: ProtocolFamily,

    /// Protocol-native model id.
    pub model_id: u16,
}

/// Protocol-owned model identity evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolModelIdentityEvidence {
    /// No protocol model id was present.
    Missing,

    /// A protocol-owned decoder produced a model id.
    ModelId(ProtocolModelIdentity),

    /// The bytes looked like protocol identity but were malformed.
    Malformed,
}

impl ProtocolModelIdentityEvidence {
    /// Creates protocol-owned model-id evidence.
    #[must_use]
    pub const fn model_id(family: ProtocolFamily, model_id: u16) -> Self {
        Self::ModelId(ProtocolModelIdentity { family, model_id })
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

    /// Most recent protocol-owned model-id evidence.
    pub protocol_model: ProtocolModelIdentityEvidence,
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

    /// Protocol-owned model-id evidence preserved with the decision.
    pub protocol_model: ProtocolModelIdentityEvidence,
}

/// Caller-owned state machine for incremental device detection.
#[derive(Clone, Debug)]
pub struct DeviceDetectionSession {
    pending_probe: Option<PendingProbe>,
    resolution: DeviceDetectionResolution,
    gatt: ArrayVec<GattFingerprint, DETECTION_MAX_GATT_FINGERPRINTS>,
    begode_reassembler: BegodeFrameReassembler,
}

/// Raw advertised-name bytes retained as device identity provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvertisedName(Bytes);

impl AdvertisedName {
    /// Copies borrowed advertised-name bytes into owned provenance.
    #[must_use]
    pub fn copy_from_slice(bytes: &[u8]) -> Self {
        Self(Bytes::copy_from_slice(bytes))
    }

    /// Returns the original advertised-name bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Returns the advertised name only when the bytes are valid UTF-8.
    #[must_use]
    pub fn get(&self) -> Option<&str> {
        str::from_utf8(self.as_bytes()).ok()
    }
}

/// Raw model-banner bytes retained as probe-response provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelBanner(Bytes);

impl ModelBanner {
    /// Copies borrowed model-banner bytes into owned provenance.
    #[must_use]
    pub fn copy_from_slice(bytes: &[u8]) -> Self {
        Self(Bytes::copy_from_slice(bytes))
    }

    /// Returns the original model-banner bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Returns the model banner only when the bytes are valid banner text.
    #[must_use]
    pub fn get(&self) -> Option<&str> {
        str::from_utf8(self.as_bytes())
            .ok()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .filter(|model| {
                model
                    .bytes()
                    .all(|byte| matches!(byte, b'\n' | b'\r' | b'\t' | 0x20..=0x7e))
            })
    }
}

/// Raw firmware-banner bytes retained as probe-response provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareBanner(Bytes);

impl FirmwareBanner {
    /// Copies borrowed firmware-banner bytes into owned provenance.
    #[must_use]
    pub fn copy_from_slice(bytes: &[u8]) -> Self {
        Self(Bytes::copy_from_slice(bytes))
    }

    /// Returns the original firmware-banner bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Returns the firmware banner only when the bytes are valid banner text.
    #[must_use]
    pub fn get(&self) -> Option<&str> {
        match classify_begode_ascii_banner(self.as_bytes()) {
            BegodeBannerParse::Banner(BegodeBanner::Firmware { banner, .. }) => Some(banner),
            BegodeBannerParse::Banner(BegodeBanner::ModelName(_) | BegodeBanner::Imu(_))
            | BegodeBannerParse::Empty
            | BegodeBannerParse::BinaryFrame
            | BegodeBannerParse::NonAscii
            | BegodeBannerParse::UnknownText => None,
        }
    }
}

/// Raw IMU-banner bytes retained as probe-response provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImuBanner(Bytes);

impl ImuBanner {
    /// Copies borrowed IMU-banner bytes into owned provenance.
    #[must_use]
    pub fn copy_from_slice(bytes: &[u8]) -> Self {
        Self(Bytes::copy_from_slice(bytes))
    }

    /// Returns the original IMU-banner bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Returns the IMU banner only when the bytes are valid banner text.
    #[must_use]
    pub fn get(&self) -> Option<&str> {
        match classify_begode_ascii_banner(self.as_bytes()) {
            BegodeBannerParse::Banner(BegodeBanner::Imu(_)) => {
                str::from_utf8(self.as_bytes()).ok().map(str::trim)
            }
            BegodeBannerParse::Banner(
                BegodeBanner::Firmware { .. } | BegodeBanner::ModelName(_),
            )
            | BegodeBannerParse::Empty
            | BegodeBannerParse::BinaryFrame
            | BegodeBannerParse::NonAscii
            | BegodeBannerParse::UnknownText => None,
        }
    }
}

/// Pending probe state remembered by the detection session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingProbe {
    /// Begode `N` probe awaiting a `NAME...` response.
    BegodeName,

    /// Begode `V` probe awaiting a firmware/code banner response.
    BegodeFirmware,

    /// Begode `M` probe awaiting an IMU banner response.
    BegodeImu,
}

/// Current protocol-family state for the detection session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProtocolFamilyState {
    /// No protocol family has been confirmed.
    #[default]
    Unknown,

    /// Veteran / `LeaperKim` / NOSFET has been confirmed.
    VeteranLeaperkimNosfet,

    /// Begode / `GotWay` has been confirmed.
    BegodeGotway,

    /// Strong wire evidence reported incompatible protocol families.
    Conflict,
}

/// Owned resolution produced by the caller-owned detection session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceDetectionResolution {
    /// Latest staged identity resolution.
    pub staged: StagedIdentityResolution,

    /// Current protocol-family state.
    pub protocol: ProtocolFamilyState,

    /// Latest raw advertisement name retained as provenance.
    pub advertised_name: Option<AdvertisedName>,

    /// Latest raw model banner retained as probe provenance.
    pub model_banner: Option<ModelBanner>,

    /// Latest raw firmware banner retained as probe provenance.
    pub firmware_banner: Option<FirmwareBanner>,

    /// Latest raw IMU banner retained as probe provenance.
    pub imu_banner: Option<ImuBanner>,

    /// Latest probe that was issued but did not produce a matching response.
    pub missing_probe_response: Option<PendingProbe>,

    /// Latest probe that produced malformed identity evidence.
    pub malformed_probe_response: Option<PendingProbe>,
}

/// Incremental device-detection event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceDetectionEvent<'a> {
    /// Advertisement hint reported by the caller.
    Advertisement {
        /// Raw advertised-name bytes, when present.
        name: Option<&'a [u8]>,
    },

    /// GATT fingerprint snapshot reported by the caller.
    Gatt {
        /// Current GATT fingerprints.
        gatt: &'a [GattFingerprint],
    },

    /// Notification evidence reported by the caller.
    Notification {
        /// Raw notification bytes.
        bytes: &'a [u8],
    },

    /// Probe write issued by the caller.
    ProbeWrite {
        /// Probe kind.
        probe: PendingProbe,
    },

    /// Probe timeout reported by the caller.
    ProbeTimeout {
        /// Probe kind.
        probe: PendingProbe,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NotificationDecision<'a> {
    protocol: ProtocolFamilyState,
    banner_model: IdentityBannerEvidence<'a>,
    protocol_model: ProtocolModelIdentityEvidence,
    firmware_banner: bool,
    imu_banner: bool,
}

impl DeviceDetectionSession {
    /// Creates an empty caller-owned detection session.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending_probe: None,
            resolution: DeviceDetectionResolution {
                staged: StagedIdentityResolution::NO_MATCH,
                protocol: ProtocolFamilyState::Unknown,
                advertised_name: None,
                model_banner: None,
                firmware_banner: None,
                imu_banner: None,
                missing_probe_response: None,
                malformed_probe_response: None,
            },
            gatt: ArrayVec::new(),
            begode_reassembler: BegodeFrameReassembler::default(),
        }
    }

    /// Observes one ordered detection event and updates the session state.
    #[must_use]
    pub fn observe(&mut self, event: DeviceDetectionEvent<'_>) -> DeviceDetectionResolution {
        match event {
            DeviceDetectionEvent::Advertisement { name } => {
                self.resolution.advertised_name = name.map(AdvertisedName::copy_from_slice);
                self.refresh_resolution(None, None, self.resolution.protocol);
            }
            DeviceDetectionEvent::Gatt { gatt } => {
                self.gatt.clear();
                self.gatt
                    .extend(gatt.iter().copied().take(DETECTION_MAX_GATT_FINGERPRINTS));
                self.refresh_resolution(None, None, self.resolution.protocol);
            }
            DeviceDetectionEvent::Notification { bytes } => {
                let pending_probe = self.pending_probe;
                let begode_frame = bytes
                    .iter()
                    .filter_map(|byte| self.begode_reassembler.feed_byte_result(*byte).ok())
                    .find_map(|result| match result {
                        BegodeFrameParseResult::Complete(frame) => Some(*frame.as_slice()),
                        BegodeFrameParseResult::Seeking | BegodeFrameParseResult::Buffered => None,
                    });
                let bytes = begode_frame
                    .as_ref()
                    .map_or(bytes, |frame| frame.as_slice());
                let decision = NotificationDecision::from_bytes(
                    self.resolution.protocol,
                    pending_probe,
                    bytes,
                );
                let malformed_model_banner =
                    match (decision.banner_model, self.resolution.staged.model) {
                        (IdentityBannerEvidence::Malformed, None) => {
                            Some(ModelBanner::copy_from_slice(model_banner_bytes(bytes)))
                        }
                        _ => None,
                    };
                if matches!(
                    decision.banner_model,
                    IdentityBannerEvidence::Model(_) | IdentityBannerEvidence::Malformed
                ) || decision.firmware_banner
                    || decision.imu_banner
                {
                    self.pending_probe = None;
                    self.resolution.missing_probe_response = None;
                }
                self.resolution.malformed_probe_response =
                    match (decision.banner_model, pending_probe) {
                        (IdentityBannerEvidence::Malformed, probe) => probe,
                        (IdentityBannerEvidence::Model(_), _) => None,
                        (IdentityBannerEvidence::Missing, _) => {
                            self.resolution.malformed_probe_response
                        }
                    };
                let firmware_banner = decision
                    .firmware_banner
                    .then(|| FirmwareBanner::copy_from_slice(bytes));
                let imu_banner = decision
                    .imu_banner
                    .then(|| ImuBanner::copy_from_slice(bytes));
                let banner_model = match (decision.banner_model, self.resolution.staged.model) {
                    (IdentityBannerEvidence::Malformed, Some(_)) => None,
                    _ => decision.banner_model_update(),
                };
                let protocol_model = match (
                    decision.protocol == self.resolution.protocol,
                    decision.protocol_model,
                ) {
                    (false, ProtocolModelIdentityEvidence::Missing) => {
                        Some(ProtocolModelIdentityEvidence::Missing)
                    }
                    (_, ProtocolModelIdentityEvidence::Missing) => None,
                    (_, protocol_model) => Some(protocol_model),
                };
                self.refresh_resolution(banner_model, protocol_model, decision.protocol);
                if let Some(model_banner) = malformed_model_banner {
                    self.resolution.model_banner = Some(model_banner);
                }
                if let Some(firmware_banner) = firmware_banner {
                    self.resolution.firmware_banner = Some(firmware_banner);
                }
                if let Some(imu_banner) = imu_banner {
                    self.resolution.imu_banner = Some(imu_banner);
                }
            }
            DeviceDetectionEvent::ProbeWrite { probe } => self.pending_probe = Some(probe),
            DeviceDetectionEvent::ProbeTimeout { probe } => {
                if self.pending_probe == Some(probe) {
                    self.pending_probe = None;
                }
                self.resolution.missing_probe_response = Some(probe);
            }
        }

        self.resolution.clone()
    }

    /// Returns the current detection resolution.
    #[must_use]
    pub fn resolution(&self) -> &DeviceDetectionResolution {
        &self.resolution
    }

    fn refresh_resolution(
        &mut self,
        banner_model: Option<IdentityBannerEvidence<'_>>,
        protocol_model: Option<ProtocolModelIdentityEvidence>,
        protocol: ProtocolFamilyState,
    ) {
        let advertised_name = self
            .resolution
            .advertised_name
            .as_ref()
            .and_then(AdvertisedName::get);
        let banner_model = banner_model.unwrap_or_else(|| {
            self.resolution
                .model_banner
                .as_ref()
                .and_then(ModelBanner::get)
                .map_or(
                    IdentityBannerEvidence::Missing,
                    IdentityBannerEvidence::model,
                )
        });
        let protocol_model = protocol_model.unwrap_or(self.resolution.staged.protocol_model);
        let input = StagedIdentityInput {
            advertised_name,
            gatt: self.gatt.as_slice(),
            stream_family: protocol.into_classification(),
            banner_model,
            protocol_model,
        };
        let staged = match protocol {
            ProtocolFamilyState::Conflict => StagedIdentityResolution {
                model: None,
                outcome: StagedIdentityOutcome::Conflict,
                confidence: IdentityConfidence::NoMatch,
                evidence: IdentityEvidence::empty().with_passive_family_match(),
                protocol_model: ProtocolModelIdentityEvidence::Missing,
            },
            ProtocolFamilyState::Unknown
            | ProtocolFamilyState::VeteranLeaperkimNosfet
            | ProtocolFamilyState::BegodeGotway => identify_known_model(&input),
        };
        self.resolution = DeviceDetectionResolution {
            staged,
            protocol,
            advertised_name: self.resolution.advertised_name.clone(),
            model_banner: match banner_model {
                IdentityBannerEvidence::Model(model) => {
                    Some(ModelBanner::copy_from_slice(model.model.as_bytes()))
                }
                IdentityBannerEvidence::Missing => self.resolution.model_banner.clone(),
                IdentityBannerEvidence::Malformed => None,
            },
            firmware_banner: self.resolution.firmware_banner.clone(),
            imu_banner: self.resolution.imu_banner.clone(),
            missing_probe_response: self.resolution.missing_probe_response,
            malformed_probe_response: self.resolution.malformed_probe_response,
        };
    }
}

fn model_banner_bytes(bytes: &[u8]) -> &[u8] {
    bytes
        .strip_prefix(b"NAME=")
        .or_else(|| bytes.strip_prefix(b"NAME:"))
        .unwrap_or(bytes)
}

impl<'a> NotificationDecision<'a> {
    fn from_bytes(
        current_protocol: ProtocolFamilyState,
        pending_probe: Option<PendingProbe>,
        bytes: &[u8],
    ) -> NotificationDecision<'_> {
        let banner_model = match pending_probe {
            Some(PendingProbe::BegodeName) => parse_model_banner(bytes),
            Some(PendingProbe::BegodeFirmware | PendingProbe::BegodeImu) | None => {
                IdentityBannerEvidence::Missing
            }
        };
        let firmware_banner = match pending_probe {
            Some(PendingProbe::BegodeFirmware) => {
                matches!(
                    classify_begode_ascii_banner(bytes),
                    BegodeBannerParse::Banner(BegodeBanner::Firmware { .. })
                )
            }
            Some(PendingProbe::BegodeName | PendingProbe::BegodeImu) | None => false,
        };
        let imu_banner = match pending_probe {
            Some(PendingProbe::BegodeImu) => matches!(
                classify_begode_ascii_banner(bytes),
                BegodeBannerParse::Banner(BegodeBanner::Imu(_))
            ),
            Some(PendingProbe::BegodeName | PendingProbe::BegodeFirmware) | None => false,
        };
        let (observed_protocol, protocol_model) = match VeteranFrame::try_from_slice(bytes) {
            Ok(frame) => (
                ProtocolFamilyState::VeteranLeaperkimNosfet,
                VeteranTelemetry::decode(&frame)
                    .map_or(ProtocolModelIdentityEvidence::Missing, |telemetry| {
                        telemetry.firmware.protocol_model_identity()
                    }),
            ),
            Err(_) if BegodeFrame::try_from_slice(bytes).is_ok() => (
                ProtocolFamilyState::BegodeGotway,
                ProtocolModelIdentityEvidence::Missing,
            ),
            Err(_) => (
                ProtocolFamilyState::Unknown,
                ProtocolModelIdentityEvidence::Missing,
            ),
        };
        let protocol = current_protocol.merge_observed(observed_protocol);
        let protocol_model = match protocol {
            ProtocolFamilyState::Conflict => ProtocolModelIdentityEvidence::Missing,
            ProtocolFamilyState::Unknown
            | ProtocolFamilyState::VeteranLeaperkimNosfet
            | ProtocolFamilyState::BegodeGotway => protocol_model,
        };

        NotificationDecision {
            protocol,
            banner_model,
            protocol_model,
            firmware_banner,
            imu_banner,
        }
    }

    fn banner_model_update(self) -> Option<IdentityBannerEvidence<'a>> {
        match self.banner_model {
            IdentityBannerEvidence::Missing => None,
            IdentityBannerEvidence::Model(_) | IdentityBannerEvidence::Malformed => {
                Some(self.banner_model)
            }
        }
    }
}

impl Default for DeviceDetectionSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolFamilyState {
    fn merge_observed(self, observed: Self) -> Self {
        match (self, observed) {
            (Self::Conflict, _) | (_, Self::Conflict) => Self::Conflict,
            (current, Self::Unknown) => current,
            (Self::Unknown, observed) => observed,
            (current, observed) if current == observed => current,
            (
                Self::VeteranLeaperkimNosfet | Self::BegodeGotway,
                Self::VeteranLeaperkimNosfet | Self::BegodeGotway,
            ) => Self::Conflict,
        }
    }

    fn into_classification(self) -> ProtocolFamilyClassification {
        match self {
            Self::Unknown | Self::Conflict => ProtocolFamilyClassification::Pending,
            Self::VeteranLeaperkimNosfet => {
                ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero)
            }
            Self::BegodeGotway => ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
        }
    }
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

/// Parses untrusted identity bytes as model banner evidence.
#[must_use]
pub fn parse_model_banner(bytes: &[u8]) -> IdentityBannerEvidence<'_> {
    parse_begode_model_banner(bytes)
}

impl StagedIdentityResolution {
    const NO_MATCH: Self = Self {
        model: None,
        outcome: StagedIdentityOutcome::NoMatch,
        confidence: IdentityConfidence::NoMatch,
        evidence: IdentityEvidence::empty(),
        protocol_model: ProtocolModelIdentityEvidence::Missing,
    };
}

/// Identifies the best registry model candidate from staged, non-actuating evidence.
#[must_use]
pub fn identify_model(
    input: &StagedIdentityInput<'_, impl Clone + IntoIterator<Item: Borrow<GattFingerprint>>>,
    registry: &[&'static ModelRegistryEntry],
) -> StagedIdentityResolution {
    protocol_model_resolution(input, registry).unwrap_or_else(|| {
        protocol_family_from_classification(input.stream_family).map_or_else(
            || hints_only_resolution(input, registry),
            |expected_family| {
                best_resolution(
                    registry
                        .iter()
                        .copied()
                        .filter(|entry| entry.protocol_family == expected_family)
                        .map(|entry| family_resolution(input, entry)),
                )
            },
        )
    })
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
            protocol_model: input.protocol_model,
        })
        .filter(|resolution| resolution.evidence != IdentityEvidence::empty())
        .max_by_key(|resolution| resolution.confidence)
        .unwrap_or(StagedIdentityResolution::NO_MATCH)
}

fn protocol_model_resolution(
    input: &StagedIdentityInput<'_, impl Clone + IntoIterator<Item: Borrow<GattFingerprint>>>,
    registry: &[&'static ModelRegistryEntry],
) -> Option<StagedIdentityResolution> {
    match input.protocol_model {
        ProtocolModelIdentityEvidence::Missing => None,
        ProtocolModelIdentityEvidence::Malformed => Some(StagedIdentityResolution {
            model: None,
            outcome: StagedIdentityOutcome::Malformed,
            confidence: IdentityConfidence::NoMatch,
            evidence: IdentityEvidence::empty().with_protocol_model_id(),
            protocol_model: input.protocol_model,
        }),
        ProtocolModelIdentityEvidence::ModelId(model_id) => {
            let matched = registry
                .iter()
                .copied()
                .filter(|entry| {
                    entry.protocol_family == model_id.family
                        && entry
                            .wire_model_id
                            .is_some_and(|wire_model_id| wire_model_id.value == model_id.model_id)
                })
                .map(|entry| StagedIdentityResolution {
                    model: Some(entry),
                    outcome: StagedIdentityOutcome::Matched,
                    confidence: IdentityConfidence::Model,
                    evidence: candidate_hints(input, entry)
                        .with_passive_family_match()
                        .with_protocol_model_id(),
                    protocol_model: input.protocol_model,
                });
            let resolution = best_resolution(matched);
            Some(if resolution.outcome == StagedIdentityOutcome::NoMatch {
                StagedIdentityResolution {
                    model: None,
                    outcome: StagedIdentityOutcome::FamilyOnly,
                    confidence: IdentityConfidence::FamilyOnly,
                    evidence: IdentityEvidence::empty()
                        .with_passive_family_match()
                        .with_protocol_model_id(),
                    protocol_model: input.protocol_model,
                }
            } else {
                resolution
            })
        }
    }
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
                protocol_model: input.protocol_model,
            }
        }
        IdentityBannerEvidence::Model(_) => StagedIdentityResolution {
            model: None,
            outcome: StagedIdentityOutcome::Conflict,
            confidence: IdentityConfidence::NoMatch,
            evidence,
            protocol_model: input.protocol_model,
        },
        IdentityBannerEvidence::Malformed => StagedIdentityResolution {
            model: None,
            outcome: StagedIdentityOutcome::Malformed,
            confidence: IdentityConfidence::NoMatch,
            evidence,
            protocol_model: input.protocol_model,
        },
        IdentityBannerEvidence::Missing => StagedIdentityResolution {
            model: None,
            outcome: StagedIdentityOutcome::FamilyOnly,
            confidence: IdentityConfidence::FamilyOnly,
            evidence,
            protocol_model: input.protocol_model,
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
            protocol_model: best.protocol_model,
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
    contains_ascii_ignore_case(name, entry.model.as_str())
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
        Capabilities, GattChannel, GattFingerprint, GattRoles, ModelRegistryEntry, ProtocolFamily,
        VerificationStatus,
    };

    use crate::{
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_SERVICE_CHANNEL,
        DeviceDetectionEvent, DeviceDetectionSession, DeviceFamily, IdentityBannerEvidence,
        IdentityConfidence, IdentityEvidence, NOSFET_AERO_REGISTRY_ENTRY, PendingProbe,
        ProtocolFamilyClassification, ProtocolModelIdentityEvidence, StagedIdentityInput,
        StagedIdentityOutcome, identify_known_model, identify_model, parse_model_banner,
    };

    const BEGODE_GATT: [GattFingerprint; 1] = [GattFingerprint {
        service: BEGODE_SERVICE_CHANNEL,
        characteristic: BEGODE_DATA_CHANNEL,
        roles: GattRoles::empty()
            .with_write_without_response()
            .with_notify(),
        verification: VerificationStatus::HardwareVerified,
    }];
    const UNKNOWN_GATT: GattFingerprint = GattFingerprint {
        service: GattChannel::from_bytes([0x11; 16]),
        characteristic: GattChannel::from_bytes([0x22; 16]),
        roles: GattRoles::empty(),
        verification: VerificationStatus::Inferred,
    };
    const NO_GATT: [GattFingerprint; 0] = [];
    const BEGODE_LIVE_A_FRAME: [u8; 24] =
        hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");

    fn synthetic_veteran_frame_with_model_id(model_id: u16) -> [u8; 42] {
        let mut bytes = [0_u8; 42];
        bytes[0..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        bytes[28..30].copy_from_slice(&(model_id * 1_000).to_be_bytes());
        bytes
    }

    #[test]
    fn advertised_name_and_shared_gatt_are_hints_only() {
        let resolution = identify_model(
            &StagedIdentityInput {
                advertised_name: Some("Falcon"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Pending,
                banner_model: IdentityBannerEvidence::Missing,
                protocol_model: ProtocolModelIdentityEvidence::Missing,
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
                protocol_model: ProtocolModelIdentityEvidence::Missing,
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
            protocol_model: ProtocolModelIdentityEvidence::Missing,
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
            protocol_model: ProtocolModelIdentityEvidence::Missing,
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
            protocol_model: ProtocolModelIdentityEvidence::Missing,
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
                protocol_model: ProtocolModelIdentityEvidence::Missing,
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
                protocol_model: ProtocolModelIdentityEvidence::Missing,
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
                protocol_model: ProtocolModelIdentityEvidence::Missing,
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
            manufacturer: cutout_core::ManufacturerKey::new("Other"),
            model: cutout_core::ModelKey::new("Falcon"),
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
                protocol_model: ProtocolModelIdentityEvidence::Missing,
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY, &OTHER_FALCON],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::Model);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::Ambiguous);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn overlapping_gatt_and_family_without_model_evidence_is_ambiguous() {
        static OTHER_BEGODE: ModelRegistryEntry = ModelRegistryEntry {
            manufacturer: cutout_core::ManufacturerKey::new("Other"),
            model: cutout_core::ModelKey::new("Shared Pipe"),
            protocol_family: ProtocolFamily::BegodeGotway,
            advertised_name_hints: &["Shared"],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &BEGODE_GATT,
            capabilities: Capabilities::from_supported_commands([]),
            verification: VerificationStatus::Inferred,
        };

        let resolution = identify_model(
            &StagedIdentityInput {
                advertised_name: Some("Shared"),
                gatt: &BEGODE_GATT,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner_model: IdentityBannerEvidence::Missing,
                protocol_model: ProtocolModelIdentityEvidence::Missing,
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY, &OTHER_BEGODE],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::Ambiguous);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_gatt_hint());
        assert!(resolution.evidence.has_passive_family_match());
        assert!(!resolution.evidence.has_banner_model_match());
    }

    #[test]
    fn veteran_protocol_model_id_resolves_registered_aero() {
        let resolution = identify_known_model(&StagedIdentityInput {
            advertised_name: Some("NF2557"),
            gatt: NO_GATT,
            stream_family: ProtocolFamilyClassification::Pending,
            banner_model: IdentityBannerEvidence::Missing,
            protocol_model: ProtocolModelIdentityEvidence::model_id(
                ProtocolFamily::VeteranLeaperkimNosfet,
                43,
            ),
        });

        assert_eq!(resolution.confidence, IdentityConfidence::Model);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::Matched);
        assert_eq!(resolution.model, Some(&NOSFET_AERO_REGISTRY_ENTRY));
        assert_eq!(
            resolution.protocol_model,
            ProtocolModelIdentityEvidence::model_id(ProtocolFamily::VeteranLeaperkimNosfet, 43)
        );
        assert!(resolution.evidence.has_protocol_model_id());
    }

    #[test]
    fn unknown_veteran_protocol_model_id_preserves_family_without_inventing_aero() {
        let resolution = identify_known_model(&StagedIdentityInput {
            advertised_name: Some("Aero"),
            gatt: NO_GATT,
            stream_family: ProtocolFamilyClassification::Pending,
            banner_model: IdentityBannerEvidence::Missing,
            protocol_model: ProtocolModelIdentityEvidence::model_id(
                ProtocolFamily::VeteranLeaperkimNosfet,
                99,
            ),
        });

        assert_eq!(resolution.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::FamilyOnly);
        assert_eq!(resolution.model, None);
        assert_eq!(
            resolution.protocol_model,
            ProtocolModelIdentityEvidence::model_id(ProtocolFamily::VeteranLeaperkimNosfet, 99)
        );
        assert!(resolution.evidence.has_protocol_model_id());
    }

    #[test]
    fn caller_owned_detection_session_keeps_name_hint_unconfirmed() {
        let mut session = DeviceDetectionSession::new();

        let _ = session.observe(DeviceDetectionEvent::Advertisement {
            name: Some(b"NF2557"),
        });
        let update = session.observe(DeviceDetectionEvent::Gatt { gatt: &BEGODE_GATT });

        assert_ne!(update.staged.confidence, IdentityConfidence::Model);
        assert_eq!(update.staged.model, None);
        assert_eq!(update.protocol, crate::ProtocolFamilyState::Unknown);
        assert_eq!(
            update.advertised_name.as_ref().and_then(|name| name.get()),
            Some("NF2557")
        );
        assert_eq!(session.resolution(), &update);
    }

    #[test]
    fn caller_owned_detection_session_keeps_gotway_name_hint_unconfirmed() {
        let mut session = DeviceDetectionSession::new();

        let _ = session.observe(DeviceDetectionEvent::Advertisement {
            name: Some(b"GotWay_002441"),
        });
        let update = session.observe(DeviceDetectionEvent::Gatt { gatt: &BEGODE_GATT });

        assert_ne!(update.staged.confidence, IdentityConfidence::Model);
        assert_eq!(update.staged.model, None);
        assert_eq!(update.protocol, crate::ProtocolFamilyState::Unknown);
        assert_eq!(
            update.advertised_name.as_ref().and_then(|name| name.get()),
            Some("GotWay_002441")
        );
    }

    #[test]
    fn caller_owned_detection_session_caps_retained_gatt_fingerprints() {
        let mut session = DeviceDetectionSession::new();
        let mut gatt = [UNKNOWN_GATT; super::DETECTION_MAX_GATT_FINGERPRINTS + 1];
        gatt[super::DETECTION_MAX_GATT_FINGERPRINTS] = BEGODE_GATT[0];

        let update = session.observe(DeviceDetectionEvent::Gatt { gatt: &gatt });

        assert_eq!(update.staged.confidence, IdentityConfidence::NoMatch);
        assert_eq!(update.staged.model, None);
    }

    #[test]
    fn caller_owned_detection_session_retains_non_utf8_advertisement_bytes() {
        let mut session = DeviceDetectionSession::new();

        let update = session.observe(DeviceDetectionEvent::Advertisement {
            name: Some(&[b'N', b'F', 0xff]),
        });

        let advertised_name = update.advertised_name.as_ref().unwrap();
        assert_eq!(advertised_name.as_bytes(), &[b'N', b'F', 0xff]);
        assert_eq!(advertised_name.get(), None);
        assert_eq!(update.staged.outcome, StagedIdentityOutcome::NoMatch);
    }

    #[test]
    fn caller_owned_detection_session_associates_probe_followed_by_banner() {
        let mut session = DeviceDetectionSession::new();

        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });
        let update = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME:Falcon",
        });

        assert_eq!(update.staged.outcome, StagedIdentityOutcome::NoMatch);
        assert_eq!(
            update.model_banner.as_ref().and_then(|banner| banner.get()),
            Some("Falcon")
        );
    }

    #[test]
    fn caller_owned_detection_session_records_missing_begode_model_probe_response() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });

        let resolution = session.observe(DeviceDetectionEvent::ProbeTimeout {
            probe: PendingProbe::BegodeName,
        });

        assert_eq!(
            resolution.missing_probe_response,
            Some(PendingProbe::BegodeName)
        );
        assert_eq!(resolution.model_banner, None);
        assert_eq!(resolution.staged.model, None);
    }

    #[test]
    fn caller_owned_detection_session_records_malformed_begode_model_probe_response() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });

        let resolution = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon\0",
        });

        assert_eq!(
            resolution.malformed_probe_response,
            Some(PendingProbe::BegodeName)
        );
        assert_eq!(resolution.missing_probe_response, None);
        assert_eq!(
            resolution
                .model_banner
                .as_ref()
                .map(|banner| banner.as_bytes()),
            Some(b"Falcon\0".as_slice())
        );
        assert_eq!(resolution.staged.model, None);
    }

    #[test]
    fn caller_owned_detection_session_confirms_aero_from_veteran_model_id_without_name() {
        let mut session = DeviceDetectionSession::new();
        let frame = synthetic_veteran_frame_with_model_id(43);

        let update = session.observe(DeviceDetectionEvent::Notification { bytes: &frame });

        assert_eq!(
            update.protocol,
            crate::ProtocolFamilyState::VeteranLeaperkimNosfet
        );
        assert_eq!(update.staged.confidence, IdentityConfidence::Model);
        assert_eq!(update.staged.outcome, StagedIdentityOutcome::Matched);
        assert_eq!(update.staged.model, Some(&NOSFET_AERO_REGISTRY_ENTRY));
        assert!(update.staged.evidence.has_protocol_model_id());
    }

    #[test]
    fn caller_owned_detection_session_reports_conflict_when_protocol_family_changes() {
        let mut session = DeviceDetectionSession::new();
        let frame = synthetic_veteran_frame_with_model_id(43);
        let _ = session.observe(DeviceDetectionEvent::Notification { bytes: &frame });

        let update = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });

        assert_eq!(update.protocol, crate::ProtocolFamilyState::Conflict);
        assert_eq!(update.staged.confidence, IdentityConfidence::NoMatch);
        assert_eq!(update.staged.outcome, StagedIdentityOutcome::Conflict);
        assert_eq!(update.staged.model, None);
        assert_eq!(
            update.staged.protocol_model,
            ProtocolModelIdentityEvidence::Missing
        );
    }

    #[test]
    fn caller_owned_detection_session_keeps_unknown_veteran_model_at_family_only() {
        let mut session = DeviceDetectionSession::new();
        let frame = synthetic_veteran_frame_with_model_id(60);

        let update = session.observe(DeviceDetectionEvent::Notification { bytes: &frame });

        assert_eq!(
            update.protocol,
            crate::ProtocolFamilyState::VeteranLeaperkimNosfet
        );
        assert_eq!(update.staged.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(update.staged.model, None);
        assert_eq!(
            update.staged.protocol_model,
            ProtocolModelIdentityEvidence::model_id(ProtocolFamily::VeteranLeaperkimNosfet, 60)
        );
    }

    #[test]
    fn caller_owned_detection_session_confirms_begode_family_without_falcon_model() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Advertisement {
            name: Some(b"GotWay_002441"),
        });

        let update = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });

        assert_eq!(update.protocol, crate::ProtocolFamilyState::BegodeGotway);
        assert_eq!(update.staged.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(update.staged.model, None);
        assert_eq!(
            update.advertised_name.as_ref().and_then(|name| name.get()),
            Some("GotWay_002441")
        );
    }

    #[test]
    fn caller_owned_detection_session_reassembles_fragmented_begode_family_frame() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Advertisement {
            name: Some(b"GotWay_002441"),
        });
        let partial = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME[..20],
        });

        let update = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME[20..],
        });

        assert_eq!(partial.protocol, crate::ProtocolFamilyState::Unknown);
        assert_eq!(update.protocol, crate::ProtocolFamilyState::BegodeGotway);
        assert_eq!(update.staged.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(update.staged.model, None);
    }

    #[test]
    fn caller_owned_detection_session_uses_begode_name_response_after_probe() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });

        let update = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon",
        });

        assert_eq!(update.protocol, crate::ProtocolFamilyState::BegodeGotway);
        assert_eq!(update.staged.confidence, IdentityConfidence::Model);
        assert_eq!(update.staged.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert!(update.staged.evidence.has_banner_model_match());
        assert_eq!(
            update.model_banner.as_ref().and_then(|banner| banner.get()),
            Some("Falcon")
        );
    }

    #[test]
    fn caller_owned_detection_session_preserves_begode_firmware_banner_after_probe() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeFirmware,
        });

        let update = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"GW FALCON 1.0",
        });

        assert_eq!(update.protocol, crate::ProtocolFamilyState::BegodeGotway);
        assert_eq!(update.staged.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(update.staged.model, None);
        assert_eq!(
            update
                .firmware_banner
                .as_ref()
                .and_then(|banner| banner.get()),
            Some("GW FALCON 1.0")
        );
        assert_eq!(
            update
                .firmware_banner
                .as_ref()
                .map(super::FirmwareBanner::as_bytes),
            Some(&b"GW FALCON 1.0"[..])
        );
    }

    #[test]
    fn caller_owned_detection_session_preserves_begode_imu_banner_after_probe() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeImu,
        });

        let update = session.observe(DeviceDetectionEvent::Notification { bytes: b"MPU6500" });

        assert_eq!(update.protocol, crate::ProtocolFamilyState::BegodeGotway);
        assert_eq!(update.staged.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(update.staged.model, None);
        assert_eq!(
            update.imu_banner.as_ref().and_then(|banner| banner.get()),
            Some("MPU6500")
        );
        assert_eq!(
            update.imu_banner.as_ref().map(super::ImuBanner::as_bytes),
            Some(&b"MPU6500"[..])
        );
    }

    #[test]
    fn caller_owned_detection_session_keeps_begode_probe_across_unrelated_notification() {
        let mut session = DeviceDetectionSession::new();

        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });
        let unrelated = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });

        assert_eq!(unrelated.model_banner, None);

        let update = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon",
        });

        assert_eq!(update.staged.confidence, IdentityConfidence::Model);
        assert_eq!(update.staged.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert_eq!(
            update.model_banner.as_ref().and_then(|banner| banner.get()),
            Some("Falcon")
        );
    }

    #[test]
    fn caller_owned_detection_session_keeps_model_after_advertisement_refresh() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon",
        });

        let update = session.observe(DeviceDetectionEvent::Advertisement {
            name: Some(b"GotWay_002441"),
        });

        assert_eq!(update.staged.confidence, IdentityConfidence::Model);
        assert_eq!(update.staged.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert_eq!(
            update.model_banner.as_ref().and_then(|banner| banner.get()),
            Some("Falcon")
        );
    }

    #[test]
    fn caller_owned_detection_session_keeps_model_after_gatt_refresh() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon",
        });

        let update = session.observe(DeviceDetectionEvent::Gatt {
            gatt: BEGODE_FALCON_REGISTRY_ENTRY.gatt,
        });

        assert_eq!(update.staged.confidence, IdentityConfidence::Model);
        assert_eq!(update.staged.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert_eq!(
            update.model_banner.as_ref().and_then(|banner| banner.get()),
            Some("Falcon")
        );
    }

    #[test]
    fn caller_owned_detection_session_keeps_model_after_unrelated_notification() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon",
        });

        let update = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });

        assert_eq!(update.staged.confidence, IdentityConfidence::Model);
        assert_eq!(update.staged.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert_eq!(
            update.model_banner.as_ref().and_then(|banner| banner.get()),
            Some("Falcon")
        );
    }

    #[test]
    fn caller_owned_detection_session_ignores_malformed_probe_after_model_resolution() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });
        let _ = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon",
        });
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });

        let malformed = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon\x00",
        });
        let unrelated = session.observe(DeviceDetectionEvent::Notification {
            bytes: &BEGODE_LIVE_A_FRAME,
        });

        assert_eq!(
            malformed
                .model_banner
                .as_ref()
                .and_then(|banner| banner.get()),
            Some("Falcon")
        );
        assert_eq!(malformed.staged.confidence, IdentityConfidence::Model);
        assert_eq!(malformed.staged.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
        assert_eq!(
            unrelated
                .model_banner
                .as_ref()
                .and_then(|banner| banner.get()),
            Some("Falcon")
        );
        assert_eq!(unrelated.staged.confidence, IdentityConfidence::Model);
        assert_eq!(unrelated.staged.model, Some(&BEGODE_FALCON_REGISTRY_ENTRY));
    }

    #[test]
    fn caller_owned_detection_session_preserves_raw_malformed_probe_response() {
        let mut session = DeviceDetectionSession::new();
        let _ = session.observe(DeviceDetectionEvent::ProbeWrite {
            probe: PendingProbe::BegodeName,
        });

        let malformed = session.observe(DeviceDetectionEvent::Notification {
            bytes: b"NAME=Falcon\x00",
        });

        assert_eq!(
            malformed
                .model_banner
                .as_ref()
                .map(super::ModelBanner::as_bytes),
            Some(&b"Falcon\x00"[..])
        );

        let refreshed = session.observe(DeviceDetectionEvent::Gatt {
            gatt: BEGODE_FALCON_REGISTRY_ENTRY.gatt,
        });

        assert_ne!(refreshed.staged.confidence, IdentityConfidence::Model);
        assert_eq!(refreshed.staged.model, None);
    }

    #[test]
    fn unknown_veteran_protocol_model_id_is_not_ambiguous_across_registered_models() {
        static OTHER_VETERAN: ModelRegistryEntry = ModelRegistryEntry {
            manufacturer: cutout_core::ManufacturerKey::new("Other"),
            model: cutout_core::ModelKey::new("Veteran Other"),
            protocol_family: ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &["Other"],
            wire_model_id: Some(cutout_core::VerifiedValue {
                value: 44,
                verification: VerificationStatus::HardwareVerified,
            }),
            battery: None,
            bms: None,
            gatt: &NO_GATT,
            capabilities: Capabilities::from_supported_commands([]),
            verification: VerificationStatus::Inferred,
        };

        let resolution = identify_model(
            &StagedIdentityInput {
                advertised_name: Some("Aero"),
                gatt: NO_GATT,
                stream_family: ProtocolFamilyClassification::Pending,
                banner_model: IdentityBannerEvidence::Missing,
                protocol_model: ProtocolModelIdentityEvidence::model_id(
                    ProtocolFamily::VeteranLeaperkimNosfet,
                    99,
                ),
            },
            &[&NOSFET_AERO_REGISTRY_ENTRY, &OTHER_VETERAN],
        );

        assert_eq!(resolution.confidence, IdentityConfidence::FamilyOnly);
        assert_eq!(resolution.outcome, StagedIdentityOutcome::FamilyOnly);
        assert_eq!(resolution.model, None);
        assert!(resolution.evidence.has_protocol_model_id());
    }
}
