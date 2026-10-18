//! Physical vehicle classification, independent from protocol, model and capabilities.

/// Physical vehicle type; controller protocol and exact model remain separate facts.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum VehicleKind {
    /// No physical type has been established.
    #[default]
    Unknown,
    /// Electric unicycle.
    ElectricUnicycle,
    /// Single-wheel self-balancing board.
    Onewheel,
    /// Electric scooter.
    Scooter,
    /// Electric skateboard.
    Eskate,
    /// Electric bicycle.
    Ebike,
    /// Electric motorcycle.
    Emoto,
}

impl VehicleKind {
    /// Physical types a user may associate with a specific device.
    pub const USER_SELECTABLE: [Self; 6] = [
        Self::ElectricUnicycle,
        Self::Onewheel,
        Self::Scooter,
        Self::Eskate,
        Self::Ebike,
        Self::Emoto,
    ];
}

/// Work remaining before a physical vehicle presentation can be selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VehicleClassificationRequirement {
    /// Protocol-owned application discovery is still in progress.
    Pending,
    /// Application discovery finished without a classification; the device needs a user choice.
    UserSelectionRequired,
    /// Protocol evidence or the device's saved user choice establishes its type.
    Resolved,
}

/// Evidence behind a physical vehicle classification, independent from model identity.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum VehicleClassificationProvenance {
    /// The physical type is unknown.
    #[default]
    Unknown,
    /// Validated protocol semantics establish this physical type.
    Protocol,
    /// The user selected this type for this specific device.
    UserProvided,
}

/// A physical type and its provenance, constructed together to prevent contradictory states.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct VehicleClassification {
    kind: VehicleKind,
    provenance: VehicleClassificationProvenance,
}

impl VehicleClassification {
    /// No protocol or user evidence establishes a physical type.
    pub const UNKNOWN: Self = Self {
        kind: VehicleKind::Unknown,
        provenance: VehicleClassificationProvenance::Unknown,
    };

    /// Records a type established by validated protocol semantics.
    #[must_use]
    pub const fn from_protocol(kind: VehicleKind) -> Self {
        match kind {
            VehicleKind::Unknown => Self::UNKNOWN,
            VehicleKind::ElectricUnicycle
            | VehicleKind::Onewheel
            | VehicleKind::Scooter
            | VehicleKind::Eskate
            | VehicleKind::Ebike
            | VehicleKind::Emoto => Self {
                kind,
                provenance: VehicleClassificationProvenance::Protocol,
            },
        }
    }

    /// Records an explicit per-device choice; unknown is not a completed choice.
    #[must_use]
    pub const fn user_provided(kind: VehicleKind) -> Option<Self> {
        match kind {
            VehicleKind::Unknown => None,
            VehicleKind::ElectricUnicycle
            | VehicleKind::Onewheel
            | VehicleKind::Scooter
            | VehicleKind::Eskate
            | VehicleKind::Ebike
            | VehicleKind::Emoto => Some(Self {
                kind,
                provenance: VehicleClassificationProvenance::UserProvided,
            }),
        }
    }

    /// Returns the established physical type.
    #[must_use]
    pub const fn kind(self) -> VehicleKind {
        self.kind
    }

    /// Returns the evidence behind the classification.
    #[must_use]
    pub const fn provenance(self) -> VehicleClassificationProvenance {
        self.provenance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_vehicle_choice_requires_a_known_physical_kind() {
        assert!(VehicleClassification::user_provided(VehicleKind::Unknown).is_none());
        let choice = VehicleClassification::user_provided(VehicleKind::Eskate).unwrap();
        assert_eq!(choice.kind(), VehicleKind::Eskate);
        assert_eq!(
            choice.provenance(),
            VehicleClassificationProvenance::UserProvided
        );
        assert_eq!(
            VehicleClassification::default().kind(),
            VehicleKind::Unknown
        );
        assert_eq!(
            VehicleClassification::default().provenance(),
            VehicleClassificationProvenance::Unknown
        );
    }

    #[test]
    fn protocol_and_user_classification_retain_distinct_provenance() {
        let observed = VehicleClassification::from_protocol(VehicleKind::Onewheel);
        let selected = VehicleClassification::user_provided(VehicleKind::Onewheel).unwrap();
        assert_eq!(observed.kind(), selected.kind());
        assert_ne!(observed, selected);
        assert_eq!(
            observed.provenance(),
            VehicleClassificationProvenance::Protocol
        );
        assert_eq!(
            VehicleClassification::from_protocol(VehicleKind::Unknown),
            VehicleClassification::default()
        );
    }
}
