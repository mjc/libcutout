//! Retained per-vehicle accessory collection and restore-compatibility policy.

use super::accessory::AccessoryRecord;
use cutout_core::{RgbLightingProfileKind, RgbLightingRecordError, RgbLightingRequestedState};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, PoisonError};
use uuid::Uuid;

fn same_identifier(left: &str, right: &str) -> bool {
    left == right
        || Uuid::parse_str(left)
            .ok()
            .zip(Uuid::parse_str(right).ok())
            .is_some_and(|(left, right)| left == right)
}

fn canonical_identifier(identifier: String) -> String {
    match Uuid::parse_str(&identifier) {
        Ok(uuid) => uuid.hyphenated().to_string().to_uppercase(),
        Err(_) => identifier,
    }
}

/// A compatible, explicitly opted-in restore request; it is not device evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreCandidate {
    /// Platform-scoped identity which must match the connected accessory.
    pub platform_identifier: String,
    /// Last saved user intent.
    pub requested_state: RgbLightingRequestedState,
}

#[derive(Debug)]
struct Entry {
    record: Arc<AccessoryRecord>,
    fingerprint: Option<String>,
    retired: bool,
}

impl Entry {
    fn compatible(&self, profile_version: u16, fingerprint: &str) -> bool {
        !self.retired
            && self.record.profile() == RgbLightingProfileKind::MelkOc21
            && self.record.profile_version() == profile_version
            && self.fingerprint.as_deref() == Some(fingerprint)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireStore {
    version: u8,
    entries: Vec<WireEntry>,
    adopt_legacy: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEntry {
    record: Vec<u8>,
    fingerprint: Option<String>,
    #[serde(default)]
    retired: bool,
}

#[derive(Debug, Default)]
struct State {
    entries: Vec<Entry>,
    adopt_legacy: bool,
}

/// One retained accessory collection with identity and profile compatibility policy.
#[derive(Debug)]
pub struct AccessoryStore {
    state: Mutex<State>,
    profile_version: u16,
    profile_fingerprint: String,
}

impl AccessoryStore {
    /// Creates an empty collection for the active profile contract.
    #[must_use]
    pub fn new(profile_version: u16, profile_fingerprint: String) -> Self {
        Self {
            state: Mutex::new(State::default()),
            profile_version,
            profile_fingerprint,
        }
    }

    /// Decodes the versioned collection, rejecting malformed or duplicate identities.
    ///
    /// # Errors
    /// Returns a record validation error for malformed storage.
    pub fn decode(
        bytes: &[u8],
        profile_version: u16,
        profile_fingerprint: String,
    ) -> Result<Self, RgbLightingRecordError> {
        let wire: WireStore =
            serde_json::from_slice(bytes).map_err(|_| RgbLightingRecordError::InvalidEncoding)?;
        if wire.version != 1 || wire.entries.len() > 32 {
            return Err(RgbLightingRecordError::InvalidEncoding);
        }
        let mut entries: Vec<Entry> = Vec::new();
        for wire_entry in wire.entries {
            let record = Arc::new(AccessoryRecord::decode(&wire_entry.record)?);
            let identifier = record.platform_identifier();
            if entries
                .iter()
                .any(|entry| same_identifier(&entry.record.platform_identifier(), &identifier))
            {
                return Err(RgbLightingRecordError::InvalidEncoding);
            }
            entries.push(Entry {
                record,
                fingerprint: wire_entry.fingerprint,
                retired: wire_entry.retired,
            });
        }
        Ok(Self {
            state: Mutex::new(State {
                entries,
                adopt_legacy: wire.adopt_legacy,
            }),
            profile_version,
            profile_fingerprint,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Encodes the collection atomically.
    ///
    /// # Errors
    /// Returns a record encoding error.
    pub fn encode(&self) -> Result<Vec<u8>, RgbLightingRecordError> {
        let state = self.lock();
        let entries = state
            .entries
            .iter()
            .map(|entry| {
                Ok(WireEntry {
                    record: entry.record.encode()?,
                    fingerprint: entry.fingerprint.clone(),
                    retired: entry.retired,
                })
            })
            .collect::<Result<Vec<_>, RgbLightingRecordError>>()?;
        serde_json::to_vec(&WireStore {
            version: 1,
            entries,
            adopt_legacy: state.adopt_legacy,
        })
        .map_err(|_| RgbLightingRecordError::InvalidEncoding)
    }

    /// Imports the previous single-accessory format once.
    pub fn import_legacy(&self, record: Arc<AccessoryRecord>, fingerprint: Option<String>) {
        let mut state = self.lock();
        if state.entries.is_empty() {
            state.adopt_legacy = record.vehicle_identifier().is_none();
            state.entries.push(Entry {
                record,
                fingerprint,
                retired: false,
            });
        }
    }

    /// Selects the record assigned to a vehicle, adopting one unassigned legacy record once.
    ///
    /// # Errors
    /// Returns a record validation error if legacy association fails.
    pub fn select_vehicle(
        &self,
        vehicle: Option<&str>,
    ) -> Result<Option<Arc<AccessoryRecord>>, RgbLightingRecordError> {
        let mut state = self.lock();
        if state.adopt_legacy && vehicle.is_some() && state.entries.len() == 1 {
            state.entries[0]
                .record
                .set_vehicle_identifier(vehicle.map(str::to_owned))?;
            state.adopt_legacy = false;
        }
        Ok(state
            .entries
            .iter()
            .rev()
            .find(|entry| !entry.retired && entry.record.vehicle_identifier().as_deref() == vehicle)
            .map(|entry| Arc::clone(&entry.record)))
    }

    /// Saves a verified accessory against the selected vehicle, retaining previous settings.
    ///
    /// # Errors
    /// Returns a record validation error for invalid identity, version, or collection bounds.
    pub fn pair(
        &self,
        identifier: String,
        vehicle: Option<&str>,
        fingerprint: String,
    ) -> Result<Arc<AccessoryRecord>, RgbLightingRecordError> {
        let mut state = self.lock();
        let record = if let Some(entry) = state
            .entries
            .iter()
            .find(|entry| same_identifier(&entry.record.platform_identifier(), &identifier))
        {
            Arc::clone(&entry.record)
        } else {
            if state.entries.len() >= 32 {
                return Err(RgbLightingRecordError::InvalidText);
            }
            Arc::new(AccessoryRecord::new(
                identifier,
                RgbLightingProfileKind::MelkOc21,
                self.profile_version,
            )?)
        };
        record.set_vehicle_identifier(vehicle.map(str::to_owned))?;
        for entry in &mut state.entries {
            if !same_identifier(
                &entry.record.platform_identifier(),
                &record.platform_identifier(),
            ) && entry.record.vehicle_identifier().as_deref() == vehicle
            {
                entry.retired = true;
            }
        }
        state.entries.retain(|entry| {
            !same_identifier(
                &entry.record.platform_identifier(),
                &record.platform_identifier(),
            )
        });
        state.entries.push(Entry {
            record: Arc::clone(&record),
            fingerprint: Some(fingerprint),
            retired: false,
        });
        state.adopt_legacy = false;
        Ok(record)
    }

    /// Returns the saved capability fingerprint.
    #[must_use]
    pub fn fingerprint(&self, identifier: &str) -> Option<String> {
        self.lock()
            .entries
            .iter()
            .find(|entry| same_identifier(&entry.record.platform_identifier(), identifier))
            .and_then(|entry| entry.fingerprint.clone())
    }

    /// Checks identity, persisted profile version, and capability fingerprint.
    #[must_use]
    pub fn is_compatible(&self, identifier: &str) -> bool {
        self.lock()
            .entries
            .iter()
            .find(|entry| same_identifier(&entry.record.platform_identifier(), identifier))
            .is_some_and(|entry| entry.compatible(self.profile_version, &self.profile_fingerprint))
    }

    /// Returns opted-in restore intent only for a compatible accessory.
    #[must_use]
    pub fn restore_candidate(&self, identifier: &str) -> Option<RestoreCandidate> {
        let state = self.lock();
        let entry = state
            .entries
            .iter()
            .find(|entry| same_identifier(&entry.record.platform_identifier(), identifier))?;
        if !entry.compatible(self.profile_version, &self.profile_fingerprint) {
            return None;
        }
        Some(RestoreCandidate {
            platform_identifier: canonical_identifier(entry.record.platform_identifier()),
            requested_state: entry.record.restore_request()?,
        })
    }

    /// Backfills absent fingerprint evidence without replacing a saved value.
    pub fn backfill_fingerprint(&self, identifier: &str, fingerprint: String) {
        if let Some(entry) = self
            .lock()
            .entries
            .iter_mut()
            .find(|entry| same_identifier(&entry.record.platform_identifier(), identifier))
        {
            entry.fingerprint.get_or_insert(fingerprint);
        }
    }

    /// Forgets only the selected accessory.
    pub fn forget(&self, identifier: &str) {
        self.lock()
            .entries
            .retain(|entry| !same_identifier(&entry.record.platform_identifier(), identifier));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FINGERPRINT: &str = "verified-profile";
    const ID: &str = "11111111-1111-1111-1111-111111111111";

    #[test]
    fn collection_roundtrip_preserves_vehicle_binding_and_compatibility() {
        let store = AccessoryStore::new(1, FINGERPRINT.to_owned());
        let record = store
            .pair(ID.to_owned(), Some("vehicle-a"), FINGERPRINT.to_owned())
            .expect("valid fixture");
        record
            .set_alias(Some("front lights".to_owned()))
            .expect("valid alias");
        assert!(store.is_compatible(ID));

        let reopened =
            AccessoryStore::decode(&store.encode().expect("encodes"), 1, FINGERPRINT.to_owned())
                .expect("decodes");
        let selected = reopened
            .select_vehicle(Some("vehicle-a"))
            .expect("valid vehicle")
            .expect("saved accessory");
        assert_eq!(selected.alias().as_deref(), Some("front lights"));
        assert!(
            reopened
                .select_vehicle(None)
                .expect("valid vehicle")
                .is_none()
        );
        assert!(reopened.restore_candidate(ID).is_none());
    }

    #[test]
    fn replacing_a_vehicle_binding_retires_the_previous_accessory() {
        let store = AccessoryStore::new(1, FINGERPRINT.to_owned());
        store
            .pair(ID.to_owned(), Some("vehicle-a"), FINGERPRINT.to_owned())
            .expect("first pairing");
        let replacement = store
            .pair(
                "AA:BB:CC:DD:EE:FF".to_owned(),
                Some("vehicle-a"),
                FINGERPRINT.to_owned(),
            )
            .expect("replacement pairing");
        assert_eq!(
            store
                .select_vehicle(Some("vehicle-a"))
                .expect("valid vehicle")
                .expect("replacement")
                .platform_identifier(),
            replacement.platform_identifier()
        );
        store.forget("AA:BB:CC:DD:EE:FF");
        assert!(
            store
                .select_vehicle(Some("vehicle-a"))
                .expect("valid vehicle")
                .is_none()
        );
    }
}
