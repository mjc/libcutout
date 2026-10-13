use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Coordinate;

/// Maximum UTF-8 byte length of a normalized public access note.
pub const MAX_ACCESS_NOTE_BYTES: usize = 2_000;

/// Explicit access cost required for a new charging spot.
///
/// Paid spots need no price or purchase condition to be valid.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingCost {
    /// Charging is available without payment or a required purchase.
    Free,
    /// Access involves payment or a required purchase; terms may be unknown.
    Paid,
}

/// A contributor's requested vote on a charging pin, independent of counts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingVote {
    /// The contributor reports that the spot is useful.
    Up,
    /// The contributor reports that the spot is not useful.
    Down,
}

impl ChargingVote {
    /// Applies a pin action to the contributor's current requested state.
    ///
    /// Selecting the same choice clears it; the other choice replaces it.
    /// Sync sends the resulting state, never an increment to aggregate counts.
    #[must_use]
    pub fn select(self, current: Option<Self>) -> Option<Self> {
        if current == Some(self) {
            None
        } else {
            Some(self)
        }
    }
}

/// Validated public instructions for accessing a charging spot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessNote(String);

impl AccessNote {
    /// Creates an access note, trimming surrounding whitespace.
    ///
    /// # Errors
    ///
    /// Rejects blank notes, notes over [`MAX_ACCESS_NOTE_BYTES`] UTF-8 bytes and
    /// embedded control characters other than line breaks and tabs.
    pub fn new(value: &str) -> Result<Self, AccessNoteError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(AccessNoteError::Blank);
        }
        if value.len() > MAX_ACCESS_NOTE_BYTES {
            return Err(AccessNoteError::TooLong);
        }
        if value
            .chars()
            .any(|character| character.is_control() && !['\n', '\r', '\t'].contains(&character))
        {
            return Err(AccessNoteError::ControlCharacter);
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the public access instructions.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Failure while validating public access instructions.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AccessNoteError {
    /// No instructions remain after trimming whitespace.
    #[error("access note is blank")]
    Blank,
    /// Normalized instructions exceed the UTF-8 byte limit.
    #[error("access note exceeds {MAX_ACCESS_NOTE_BYTES} UTF-8 bytes")]
    TooLong,
    /// Instructions contain a control character other than line breaks or tabs.
    #[error("access note contains an unsupported control character")]
    ControlCharacter,
}

/// The minimum validated user input for a new community charging spot.
///
/// Identity is assigned by persistence. No name, venue, price or supply facts
/// are required here, and no power suggestions are inserted by construction.
///
/// ```
/// use cutout_ride_maps::{ChargingCost, Coordinate, NewChargingSpot};
///
/// let pin = Coordinate::from_degrees(40.0, -105.0)?;
/// let spot = NewChargingSpot::new(pin, "Ask staff for the patio outlet.", ChargingCost::Paid)?;
/// assert_eq!(spot.cost(), ChargingCost::Paid);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewChargingSpot {
    coordinate: Coordinate,
    access_note: AccessNote,
    cost: ChargingCost,
}

impl NewChargingSpot {
    /// Validates the three required user fields.
    ///
    /// # Errors
    ///
    /// Returns a typed error for invalid access instructions. Coordinates are
    /// already checked by [`Coordinate`], and cost must be explicitly supplied.
    pub fn new(
        coordinate: Coordinate,
        access_note: &str,
        cost: ChargingCost,
    ) -> Result<Self, AccessNoteError> {
        Ok(Self {
            coordinate,
            access_note: AccessNote::new(access_note)?,
            cost,
        })
    }

    /// Returns the validated map pin.
    #[must_use]
    pub const fn coordinate(&self) -> Coordinate {
        self.coordinate
    }

    /// Returns the public access instructions.
    #[must_use]
    pub const fn access_note(&self) -> &AccessNote {
        &self.access_note
    }

    /// Returns the explicit Free/Paid choice.
    #[must_use]
    pub const fn cost(&self) -> ChargingCost {
        self.cost
    }
}
