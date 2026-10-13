use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::NewChargingSpot;

/// A charging spot's UUID; storage preserves this value across edits and migration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ChargingSpotId(Uuid);
impl ChargingSpotId {
    /// Generates a new spot identity, independently of user-entered metadata.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    /// Reads an existing UUID without assigning a new identity.
    /// # Errors
    /// Returns the UUID parser's error for malformed input.
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(value).map(Self)
    }
    /// Returns the UUID for the existing map-point persistence boundary.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}
impl Default for ChargingSpotId {
    fn default() -> Self {
        Self::new()
    }
}

/// Origin retained by local storage; it does not certify power or access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargingProvenance {
    /// The installation explicitly marked this spot.
    ManualMark,
    /// The local record came from the community directory.
    Directory,
}

/// A locally created charging record with generated identity and validated public data.
/// This value does not claim server publication; outbox/persistence owns that state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingSpot {
    id: ChargingSpotId,
    input: NewChargingSpot,
    provenance: Option<ChargingProvenance>,
}
impl ChargingSpot {
    /// Assigns an identity to the three-field or optional-metadata input.
    #[must_use]
    pub fn new(input: NewChargingSpot) -> Self {
        Self {
            id: ChargingSpotId::new(),
            input,
            provenance: Some(ChargingProvenance::ManualMark),
        }
    }
    /// Reconstructs an existing record without regenerating its identity.
    #[must_use]
    pub const fn from_stored(
        id: ChargingSpotId,
        input: NewChargingSpot,
        provenance: Option<ChargingProvenance>,
    ) -> Self {
        Self {
            id,
            input,
            provenance,
        }
    }
    /// Returns retained origin, or Unknown for a record without provenance.
    #[must_use]
    pub const fn provenance(&self) -> Option<ChargingProvenance> {
        self.provenance
    }
    /// Returns the stable spot identity.
    #[must_use]
    pub const fn id(&self) -> ChargingSpotId {
        self.id
    }
    /// Returns validated charging data, without private ride or credential data.
    #[must_use]
    pub const fn input(&self) -> &NewChargingSpot {
        &self.input
    }
}
