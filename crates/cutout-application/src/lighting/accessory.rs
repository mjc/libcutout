//! Shared persistent accessory record API used by native boundary adapters.

use cutout_core::{
    RgbLightingAccessoryRecord as CoreRecord, RgbLightingConfirmationState,
    RgbLightingConnectionState, RgbLightingPartialState, RgbLightingPreset, RgbLightingProfileKind,
    RgbLightingRecordError, RgbLightingRequestedState,
};
use std::sync::{Mutex, PoisonError};

/// Opaque, synchronized persistent state for one verified lighting accessory.
#[derive(Debug)]
pub struct AccessoryRecord {
    inner: Mutex<CoreRecord>,
}

impl AccessoryRecord {
    /// Creates an accessory record after validating its identity and profile version.
    ///
    /// # Errors
    /// Returns the core record validation error.
    pub fn new(
        identifier: String,
        profile: RgbLightingProfileKind,
        version: u16,
    ) -> Result<Self, RgbLightingRecordError> {
        Ok(Self {
            inner: Mutex::new(CoreRecord::new(identifier, profile, version)?),
        })
    }

    /// Decodes a validated persisted accessory record.
    ///
    /// # Errors
    /// Returns the core record decoding error.
    pub fn decode(bytes: &[u8]) -> Result<Self, RgbLightingRecordError> {
        Ok(Self {
            inner: Mutex::new(CoreRecord::decode(bytes)?),
        })
    }

    /// Encodes this accessory record.
    ///
    /// # Errors
    /// Returns the core record encoding error.
    pub fn encode(&self) -> Result<Vec<u8>, RgbLightingRecordError> {
        self.lock().encode()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CoreRecord> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns the platform-scoped identity.
    #[must_use]
    pub fn platform_identifier(&self) -> String {
        self.lock().platform_identifier().to_owned()
    }
    /// Returns the verified profile.
    #[must_use]
    pub fn profile(&self) -> RgbLightingProfileKind {
        self.lock().profile()
    }
    /// Returns the profile schema version.
    #[must_use]
    pub fn profile_version(&self) -> u16 {
        self.lock().profile_version()
    }
    /// Returns the user alias.
    #[must_use]
    pub fn alias(&self) -> Option<String> {
        self.lock().alias().map(str::to_owned)
    }
    /// Updates the user alias.
    ///
    /// # Errors
    /// Returns an error for an empty or oversized alias.
    pub fn set_alias(&self, alias: Option<String>) -> Result<(), RgbLightingRecordError> {
        self.lock().set_alias(alias)
    }
    /// Returns the installed-vehicle association.
    #[must_use]
    pub fn vehicle_identifier(&self) -> Option<String> {
        self.lock().vehicle_identifier().map(str::to_owned)
    }
    /// Updates the installed-vehicle association.
    ///
    /// # Errors
    /// Returns an error for an empty or oversized vehicle identifier.
    pub fn set_vehicle_identifier(
        &self,
        identifier: Option<String>,
    ) -> Result<(), RgbLightingRecordError> {
        self.lock().set_vehicle_identifier(identifier)
    }
    /// Returns the requested lighting state.
    #[must_use]
    pub fn requested_state(&self) -> Option<RgbLightingRequestedState> {
        self.lock().requested_state()
    }
    /// Updates the requested lighting state.
    pub fn set_requested_state(&self, state: Option<RgbLightingRequestedState>) {
        self.lock().set_requested_state(state);
    }
    /// Returns the independently confirmed state.
    #[must_use]
    pub fn confirmed_state(&self) -> Option<RgbLightingRequestedState> {
        self.lock().confirmed_state()
    }
    /// Updates the independently confirmed state.
    pub fn set_confirmed_state(&self, state: Option<RgbLightingRequestedState>) {
        self.lock().set_confirmed_state(state);
    }
    /// Merges a confirmed partial field into the saved baseline.
    pub fn confirm_partial_state(
        &self,
        state: RgbLightingRequestedState,
        field: RgbLightingPartialState,
    ) -> bool {
        self.lock().confirm_partial_state(state, field)
    }
    /// Returns command-confirmation evidence.
    #[must_use]
    pub fn confirmation(&self) -> RgbLightingConfirmationState {
        self.lock().confirmation()
    }
    /// Updates command-confirmation evidence.
    pub fn set_confirmation(&self, state: RgbLightingConfirmationState) {
        self.lock().set_confirmation(state);
    }
    /// Returns persisted schedule-command confirmation.
    #[must_use]
    pub fn last_schedule_confirmation(&self) -> Option<RgbLightingConfirmationState> {
        self.lock().last_schedule_confirmation()
    }
    /// Updates persisted schedule-command confirmation.
    pub fn set_last_schedule_confirmation(&self, state: RgbLightingConfirmationState) {
        self.lock().set_last_schedule_confirmation(state);
    }
    /// Returns transport status.
    #[must_use]
    pub fn connection(&self) -> RgbLightingConnectionState {
        self.lock().connection()
    }
    /// Updates transport status.
    pub fn set_connection(&self, state: RgbLightingConnectionState) {
        self.lock().set_connection(state);
    }
    /// Returns whether restore was explicitly enabled.
    #[must_use]
    pub fn restore_enabled(&self) -> bool {
        self.lock().restore_enabled()
    }
    /// Updates the explicit restore preference.
    pub fn set_restore_enabled(&self, enabled: bool) {
        self.lock().set_restore_enabled(enabled);
    }
    /// Returns opted-in restore intent.
    #[must_use]
    pub fn restore_request(&self) -> Option<RgbLightingRequestedState> {
        self.lock().restore_request()
    }
    /// Returns saved presets.
    #[must_use]
    pub fn presets(&self) -> Vec<RgbLightingPreset> {
        self.lock().presets().to_vec()
    }
    /// Adds one bounded preset.
    ///
    /// # Errors
    /// Returns an error for a duplicate preset or the collection bound.
    pub fn add_preset(&self, preset: RgbLightingPreset) -> Result<(), RgbLightingRecordError> {
        self.lock().add_preset(preset)
    }
    /// Removes a named preset.
    pub fn remove_preset(&self, name: &str) -> bool {
        self.lock().remove_preset(name)
    }
    /// Replaces a named preset.
    pub fn replace_preset(&self, preset: RgbLightingPreset) -> bool {
        self.lock().replace_preset(preset)
    }
}
