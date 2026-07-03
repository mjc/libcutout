use crate::{BtleNotification, ConnectionSummary};

/// Confidence reported by a host-supplied bridge identity observer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeIdentityConfidence {
    /// Non-actuating hints matched, but no model was resolved.
    HintsOnly,

    /// A concrete model was resolved.
    Model,
}

/// Transport-neutral identity evidence bits reported by the host/model layer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BridgeIdentityEvidence(u8);

/// Typed evidence bit recorded by a host/model identity observer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeIdentityEvidenceKind {
    /// Advertised-name evidence contributed to the decision.
    AdvertisedNameHint,

    /// GATT fingerprint evidence contributed to the decision.
    GattHint,

    /// Passive notification family evidence contributed to the decision.
    PassiveFamilyMatch,

    /// Model banner evidence contributed to the decision.
    BannerModelMatch,

    /// Protocol-owned model id evidence contributed to the decision.
    ProtocolModelId,
}

impl BridgeIdentityEvidence {
    const ADVERTISED_NAME_HINT: u8 = 1 << 0;
    const GATT_HINT: u8 = 1 << 1;
    const PASSIVE_FAMILY_MATCH: u8 = 1 << 2;
    const BANNER_MODEL_MATCH: u8 = 1 << 3;
    const PROTOCOL_MODEL_ID: u8 = 1 << 4;

    /// Empty identity evidence.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns evidence with an additional typed evidence bit.
    #[must_use]
    pub const fn with(self, kind: BridgeIdentityEvidenceKind) -> Self {
        Self(self.0 | kind.mask())
    }

    /// Returns whether advertised-name evidence contributed to the decision.
    #[must_use]
    pub const fn has_advertised_name_hint(self) -> bool {
        self.has(BridgeIdentityEvidenceKind::AdvertisedNameHint)
    }

    /// Returns whether GATT fingerprint evidence contributed to the decision.
    #[must_use]
    pub const fn has_gatt_hint(self) -> bool {
        self.has(BridgeIdentityEvidenceKind::GattHint)
    }

    /// Returns whether passive notification family evidence contributed to the decision.
    #[must_use]
    pub const fn has_passive_family_match(self) -> bool {
        self.has(BridgeIdentityEvidenceKind::PassiveFamilyMatch)
    }

    /// Returns whether model banner evidence contributed to the decision.
    #[must_use]
    pub const fn has_banner_model_match(self) -> bool {
        self.has(BridgeIdentityEvidenceKind::BannerModelMatch)
    }

    /// Returns whether protocol-owned model-id evidence contributed to the decision.
    #[must_use]
    pub const fn has_protocol_model_id(self) -> bool {
        self.has(BridgeIdentityEvidenceKind::ProtocolModelId)
    }

    const fn has(self, kind: BridgeIdentityEvidenceKind) -> bool {
        self.0 & kind.mask() != 0
    }
}

impl BridgeIdentityEvidenceKind {
    const fn mask(self) -> u8 {
        match self {
            Self::AdvertisedNameHint => BridgeIdentityEvidence::ADVERTISED_NAME_HINT,
            Self::GattHint => BridgeIdentityEvidence::GATT_HINT,
            Self::PassiveFamilyMatch => BridgeIdentityEvidence::PASSIVE_FAMILY_MATCH,
            Self::BannerModelMatch => BridgeIdentityEvidence::BANNER_MODEL_MATCH,
            Self::ProtocolModelId => BridgeIdentityEvidence::PROTOCOL_MODEL_ID,
        }
    }
}

/// Staged identity resolution surfaced by a BTLE bridge run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeIdentityResolution {
    /// Resolved manufacturer, when model confidence was reached.
    pub manufacturer: Option<&'static str>,

    /// Resolved model, when model confidence was reached.
    pub model: Option<&'static str>,

    /// Confidence reported by the host/model layer.
    pub confidence: BridgeIdentityConfidence,

    /// Evidence that contributed to the decision.
    pub evidence: BridgeIdentityEvidence,
}

/// Host-supplied identity observer for typed BTLE evidence.
pub trait BridgeIdentityObserver: Send {
    /// Observes connection metadata before notifications are processed.
    fn observe_connection(&mut self, summary: &ConnectionSummary);

    /// Observes a typed BTLE notification.
    fn observe_notification(&mut self, notification: &BtleNotification);

    /// Returns the current identity resolution, if any.
    fn resolution(&self) -> Option<BridgeIdentityResolution>;
}
