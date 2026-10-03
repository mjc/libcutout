//! Rust-owned per-vehicle accessory selection and durable collection.

use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    MobileMelkLightingRestoreStateDto, MobileRgbLightingAccessoryRecord,
    MobileRgbLightingProfileKindDto, MobileRgbLightingRecordError,
};

/// An opted-in request for one compatible accessory; it is not device-state evidence.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRgbLightingRestoreCandidate {
    /// Canonical platform identity that must match the connected accessory.
    pub platform_identifier: String,
    /// Last saved user intent to replay.
    pub requested_state: MobileMelkLightingRestoreStateDto,
}

#[derive(Debug)]
struct Entry {
    record: Arc<MobileRgbLightingAccessoryRecord>,
    fingerprint: Option<String>,
    retired: bool,
}

impl Entry {
    fn is_compatible(&self) -> bool {
        !self.retired
            && self.record.profile() == MobileRgbLightingProfileKindDto::MelkOc21
            && self.record.profile_version() == super::mobile_melk_lighting_profile_version()
            && self.fingerprint.as_deref()
                == Some(super::mobile_melk_lighting_capabilities_fingerprint().as_str())
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
struct Store {
    entries: Vec<Entry>,
    adopt_legacy: bool,
}

impl Store {
    fn entry(&self, identifier: &str) -> Option<&Entry> {
        let identifier = Uuid::parse_str(identifier).ok()?;
        self.entries.iter().find(|entry| {
            Uuid::parse_str(&entry.record.platform_identifier()).ok() == Some(identifier)
        })
    }
}

/// Accessories retained across vehicle changes and app restarts.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileRgbLightingAccessoryStore {
    inner: Mutex<Store>,
}

#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI methods require owned boundary inputs"
)]
impl MobileRgbLightingAccessoryStore {
    /// Creates an empty collection.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Loads a versioned collection, rejecting invalid or duplicate identities.
    ///
    /// # Errors
    /// Returns a record error for malformed or unsupported persisted data.
    #[uniffi::constructor]
    pub fn decode(bytes: Vec<u8>) -> Result<Arc<Self>, MobileRgbLightingRecordError> {
        let wire: WireStore = serde_json::from_slice(&bytes)
            .map_err(|_| MobileRgbLightingRecordError::InvalidEncoding)?;
        if wire.version != 1 || wire.entries.len() > 32 {
            return Err(MobileRgbLightingRecordError::InvalidEncoding);
        }
        let mut entries: Vec<Entry> = Vec::new();
        for entry in wire.entries {
            let record = MobileRgbLightingAccessoryRecord::decode(entry.record)?;
            if entries.iter().any(|existing| {
                existing.record.platform_identifier() == record.platform_identifier()
            }) {
                return Err(MobileRgbLightingRecordError::InvalidEncoding);
            }
            entries.push(Entry {
                record,
                fingerprint: entry.fingerprint,
                retired: entry.retired,
            });
        }
        Ok(Arc::new(Self {
            inner: Mutex::new(Store {
                entries,
                adopt_legacy: wire.adopt_legacy,
            }),
        }))
    }

    /// Encodes every accessory and its profile fingerprint atomically.
    ///
    /// # Errors
    /// Returns a record error when serialization fails.
    pub fn encode(&self) -> Result<Vec<u8>, MobileRgbLightingRecordError> {
        let store = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let entries = store
            .entries
            .iter()
            .map(|entry| {
                Ok(WireEntry {
                    record: entry.record.encode()?,
                    fingerprint: entry.fingerprint.clone(),
                    retired: entry.retired,
                })
            })
            .collect::<Result<Vec<_>, MobileRgbLightingRecordError>>()?;
        serde_json::to_vec(&WireStore {
            version: 1,
            entries,
            adopt_legacy: store.adopt_legacy,
        })
        .map_err(|_| MobileRgbLightingRecordError::InvalidEncoding)
    }

    /// Imports the previous single-accessory format once.
    pub fn import_legacy(
        &self,
        record: Arc<MobileRgbLightingAccessoryRecord>,
        fingerprint: Option<String>,
    ) {
        let mut store = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if store.entries.is_empty() {
            store.adopt_legacy = record.vehicle_identifier().is_none();
            store.entries.push(Entry {
                record,
                fingerprint,
                retired: false,
            });
        }
    }

    /// Chooses the accessory saved for this vehicle. A legacy unassigned accessory is adopted once.
    ///
    /// # Errors
    /// Returns a record error for an invalid vehicle identifier.
    pub fn select_vehicle(
        &self,
        vehicle_identifier: Option<String>,
    ) -> Result<Option<Arc<MobileRgbLightingAccessoryRecord>>, MobileRgbLightingRecordError> {
        let mut store = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if store.adopt_legacy && vehicle_identifier.is_some() && store.entries.len() == 1 {
            store.entries[0]
                .record
                .set_vehicle_identifier(vehicle_identifier.clone())?;
            store.adopt_legacy = false;
        }
        Ok(store
            .entries
            .iter()
            .rev()
            .find(|entry| !entry.retired && entry.record.vehicle_identifier() == vehicle_identifier)
            .map(|entry| Arc::clone(&entry.record)))
    }

    /// Saves a verified accessory against the current vehicle, retaining its settings.
    ///
    /// # Errors
    /// Returns a record error for invalid identifiers or a full collection.
    pub fn pair(
        &self,
        platform_identifier: String,
        vehicle_identifier: Option<String>,
        fingerprint: String,
    ) -> Result<Arc<MobileRgbLightingAccessoryRecord>, MobileRgbLightingRecordError> {
        let mut store = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let record = if let Some(entry) = store
            .entries
            .iter()
            .find(|entry| entry.record.platform_identifier() == platform_identifier)
        {
            Arc::clone(&entry.record)
        } else {
            if store.entries.len() >= 32 {
                return Err(MobileRgbLightingRecordError::InvalidText);
            }
            MobileRgbLightingAccessoryRecord::new(
                platform_identifier,
                MobileRgbLightingProfileKindDto::MelkOc21,
                super::mobile_melk_lighting_profile_version(),
            )?
        };
        record.set_vehicle_identifier(vehicle_identifier.clone())?;
        for entry in &mut store.entries {
            if entry.record.platform_identifier() != record.platform_identifier()
                && entry.record.vehicle_identifier() == vehicle_identifier
            {
                entry.retired = true;
            }
        }
        store
            .entries
            .retain(|entry| entry.record.platform_identifier() != record.platform_identifier());
        store.entries.push(Entry {
            record: Arc::clone(&record),
            fingerprint: Some(fingerprint),
            retired: false,
        });
        store.adopt_legacy = false;
        Ok(record)
    }

    /// Returns the saved capability fingerprint for one accessory.
    #[must_use]
    pub fn fingerprint(&self, platform_identifier: String) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entries
            .iter()
            .find(|entry| entry.record.platform_identifier() == platform_identifier)
            .and_then(|entry| entry.fingerprint.clone())
    }

    /// Checks persisted profile and capability evidence for this accessory identity.
    #[must_use]
    pub fn is_compatible_with_current_profile(&self, platform_identifier: String) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(&platform_identifier)
            .is_some_and(Entry::is_compatible)
    }

    /// Returns opted-in saved intent only for a valid identity and compatible profile.
    ///
    /// Confirmation is independent: replay never upgrades a request to a device acknowledgement.
    #[must_use]
    pub fn restore_candidate(
        &self,
        platform_identifier: String,
    ) -> Option<MobileRgbLightingRestoreCandidate> {
        let store = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = store.entry(&platform_identifier)?;
        if !entry.is_compatible() {
            return None;
        }
        Some(MobileRgbLightingRestoreCandidate {
            platform_identifier: Uuid::parse_str(&entry.record.platform_identifier())
                .ok()?
                .hyphenated()
                .to_string()
                .to_uppercase(),
            requested_state: entry.record.restore_request()?,
        })
    }

    /// Backfills the fingerprint for an old record without replacing present evidence.
    pub fn backfill_fingerprint(&self, platform_identifier: String, fingerprint: String) {
        if let Some(entry) = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entries
            .iter_mut()
            .find(|entry| entry.record.platform_identifier() == platform_identifier)
        {
            entry.fingerprint.get_or_insert(fingerprint);
        }
    }

    /// Removes only the selected accessory; other vehicles keep their settings.
    pub fn forget(&self, platform_identifier: String) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entries
            .retain(|entry| entry.record.platform_identifier() != platform_identifier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: &str = "11111111-1111-1111-1111-111111111111";
    const B: &str = "22222222-2222-2222-2222-222222222222";

    #[test]
    fn restore_requires_valid_same_profile_identity_and_preserves_evidence() {
        use super::super::{
            MobileMelkLightingRestoreStateDto, MobileRgbLightingConfirmationStateDto,
        };
        let store = MobileRgbLightingAccessoryStore::new();
        let record = store
            .pair(
                A.into(),
                Some("aero".into()),
                super::super::mobile_melk_lighting_capabilities_fingerprint(),
            )
            .unwrap();
        let requested = MobileMelkLightingRestoreStateDto {
            power_on: true,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 42,
            playback: None,
        };
        assert!(store.restore_candidate(A.into()).is_none());
        record.set_requested_state(Some(requested)).unwrap();
        assert!(store.restore_candidate(A.into()).is_none());
        record.set_restore_enabled(true);
        let restored = MobileRgbLightingAccessoryStore::decode(store.encode().unwrap()).unwrap();
        let candidate = restored.restore_candidate(A.into()).unwrap();
        assert_eq!(candidate.platform_identifier, A);
        assert_eq!(candidate.requested_state, requested);
        assert!(restored.restore_candidate(B.into()).is_none());
        assert!(!restored.is_compatible_with_current_profile(B.into()));
        assert_eq!(
            record.confirmation(),
            MobileRgbLightingConfirmationStateDto::Unknown
        );
        assert!(record.confirmed_state().is_none());

        let stale = MobileRgbLightingAccessoryStore::new();
        stale.import_legacy(Arc::clone(&record), Some("old-fingerprint".into()));
        assert!(stale.restore_candidate(A.into()).is_none());
        assert!(!stale.is_compatible_with_current_profile(A.into()));
        let missing = MobileRgbLightingAccessoryStore::new();
        missing.import_legacy(record, None);
        assert!(missing.restore_candidate(A.into()).is_none());

        let invalid = MobileRgbLightingAccessoryStore::new();
        let invalid_record = MobileRgbLightingAccessoryRecord::new(
            "not-a-bluetooth-id".into(),
            MobileRgbLightingProfileKindDto::MelkOc21,
            1,
        )
        .unwrap();
        invalid_record.set_restore_enabled(true);
        invalid_record.set_requested_state(Some(requested)).unwrap();
        invalid.import_legacy(
            invalid_record,
            Some(super::super::mobile_melk_lighting_capabilities_fingerprint()),
        );
        assert!(
            invalid
                .restore_candidate("not-a-bluetooth-id".into())
                .is_none()
        );
        assert!(!invalid.is_compatible_with_current_profile("not-a-bluetooth-id".into()));
    }

    #[test]
    fn vehicle_switch_and_restart_recall_each_accessory_and_alias() {
        let store = MobileRgbLightingAccessoryStore::new();
        let aero = store
            .pair(A.into(), Some("aero".into()), "fingerprint".into())
            .unwrap();
        aero.set_alias(Some("aero lights".into())).unwrap();
        let pev = store
            .pair(B.into(), Some("pev".into()), "fingerprint".into())
            .unwrap();
        pev.set_restore_enabled(true);
        let restored = MobileRgbLightingAccessoryStore::decode(store.encode().unwrap()).unwrap();
        assert_eq!(
            restored
                .select_vehicle(Some("aero".into()))
                .unwrap()
                .unwrap()
                .alias()
                .as_deref(),
            Some("aero lights")
        );
        assert!(
            restored
                .select_vehicle(Some("pev".into()))
                .unwrap()
                .unwrap()
                .restore_enabled()
        );
        assert!(
            restored
                .select_vehicle(Some("other".into()))
                .unwrap()
                .is_none()
        );
        assert!(restored.select_vehicle(None).unwrap().is_none());
        restored.forget(A.into());
        assert!(
            restored
                .select_vehicle(Some("aero".into()))
                .unwrap()
                .is_none()
        );
        assert!(
            restored
                .select_vehicle(Some("pev".into()))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn legacy_accessory_is_associated_once_and_preserves_its_alias() {
        let store = MobileRgbLightingAccessoryStore::new();
        let record = MobileRgbLightingAccessoryRecord::new(
            A.into(),
            MobileRgbLightingProfileKindDto::MelkOc21,
            1,
        )
        .unwrap();
        record.set_alias(Some("aero lights".into())).unwrap();
        store.import_legacy(record, None);
        let selected = store.select_vehicle(Some("aero".into())).unwrap().unwrap();
        assert_eq!(selected.vehicle_identifier().as_deref(), Some("aero"));
        assert!(
            store
                .select_vehicle(Some("other".into()))
                .unwrap()
                .is_none()
        );
        assert_eq!(selected.alias().as_deref(), Some("aero lights"));
    }
    #[test]
    fn replacing_and_forgetting_lights_does_not_restore_an_old_vehicle_binding() {
        let store = MobileRgbLightingAccessoryStore::new();
        store
            .pair(A.into(), Some("aero".into()), "fingerprint".into())
            .unwrap();
        store
            .pair(B.into(), Some("aero".into()), "fingerprint".into())
            .unwrap();
        store.forget(B.into());
        assert!(store.select_vehicle(Some("aero".into())).unwrap().is_none());
    }

    #[test]
    fn replaced_vehicle_lights_do_not_become_standalone_after_restart() {
        let store = MobileRgbLightingAccessoryStore::new();
        let old = store
            .pair(A.into(), Some("aero".into()), "fingerprint".into())
            .unwrap();
        old.set_alias(Some("old aero lights".into())).unwrap();
        store
            .pair(B.into(), Some("aero".into()), "fingerprint".into())
            .unwrap();
        let reopened = MobileRgbLightingAccessoryStore::decode(store.encode().unwrap()).unwrap();
        assert!(reopened.select_vehicle(None).unwrap().is_none());
        reopened.forget(B.into());
        assert!(
            reopened
                .select_vehicle(Some("aero".into()))
                .unwrap()
                .is_none()
        );
        assert!(reopened.select_vehicle(None).unwrap().is_none());
        let paired_again = reopened
            .pair(A.into(), Some("aero".into()), "fingerprint".into())
            .unwrap();
        assert_eq!(paired_again.alias().as_deref(), Some("old aero lights"));
        assert!(reopened.select_vehicle(None).unwrap().is_none());
        reopened.pair(B.into(), None, "fingerprint".into()).unwrap();
        assert_eq!(
            reopened
                .select_vehicle(None)
                .unwrap()
                .unwrap()
                .platform_identifier(),
            B
        );
        assert_eq!(
            reopened
                .select_vehicle(Some("aero".into()))
                .unwrap()
                .unwrap()
                .platform_identifier(),
            A
        );
    }
}
