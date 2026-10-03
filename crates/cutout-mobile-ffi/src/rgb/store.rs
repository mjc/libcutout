//! Rust-owned per-vehicle accessory selection and durable collection.

use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use super::{
    MobileRgbLightingAccessoryRecord, MobileRgbLightingProfileKindDto, MobileRgbLightingRecordError,
};

#[derive(Debug)]
struct Entry {
    record: Arc<MobileRgbLightingAccessoryRecord>,
    fingerprint: Option<String>,
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
}

#[derive(Debug, Default)]
struct Store {
    entries: Vec<Entry>,
    adopt_legacy: bool,
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
            .find(|entry| entry.record.vehicle_identifier() == vehicle_identifier)
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
        for entry in &store.entries {
            if entry.record.platform_identifier() != record.platform_identifier()
                && entry.record.vehicle_identifier() == vehicle_identifier
            {
                entry.record.set_vehicle_identifier(None)?;
            }
        }
        store
            .entries
            .retain(|entry| entry.record.platform_identifier() != record.platform_identifier());
        store.entries.push(Entry {
            record: Arc::clone(&record),
            fingerprint: Some(fingerprint),
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
}
