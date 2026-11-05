//! Persisted RGB-accessory state and conversion boundary.

use cutout_application::lighting::accessory::AccessoryRecord;
use std::sync::Arc;

use cutout_core::{
    LightingBrightness, LightingPowerState, RgbColor,
    RgbLightingConfirmationState as CoreRgbLightingConfirmationState,
    RgbLightingConnectionState as CoreRgbLightingConnectionState,
    RgbLightingPartialState as CoreRgbLightingPartialState, RgbLightingPreset,
    RgbLightingProfileKind, RgbLightingRecordError as CoreRgbLightingRecordError,
    RgbLightingRequestedState,
};
use uuid::Uuid;

use super::melk::{MobileLightingPlaybackDto, MobileMelkLightingRestoreStateDto};

/// A lossless Bluetooth UUID crossing the `UniFFI` boundary.
///
/// `UniFFI` does not directly lower `uuid::Uuid`. Keeping the two network-order
/// 64-bit words avoids an untyped byte vector or textual UUID at the mobile
/// boundary while Rust continues to use `Uuid` internally.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBluetoothUuid {
    /// The most-significant 64 bits in network order.
    pub most_significant_bits: u64,
    /// The least-significant 64 bits in network order.
    pub least_significant_bits: u64,
}

impl From<Uuid> for MobileBluetoothUuid {
    fn from(uuid: Uuid) -> Self {
        let value = uuid.as_u128();
        Self {
            most_significant_bits: (value >> 64) as u64,
            least_significant_bits: u64::try_from(value & u128::from(u64::MAX))
                .expect("UUID low word is masked to u64"),
        }
    }
}

impl From<MobileBluetoothUuid> for Uuid {
    fn from(uuid: MobileBluetoothUuid) -> Self {
        Self::from_u128(
            (u128::from(uuid.most_significant_bits) << 64)
                | u128::from(uuid.least_significant_bits),
        )
    }
}

/// Supported persisted RGB-controller profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRgbLightingProfileKindDto {
    /// ELK-BLEDOM/MELK profile verified for MELK-OC21.
    MelkOc21,
}

/// Persisted command-confirmation evidence for a standalone RGB accessory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRgbLightingConfirmationStateDto {
    /// No explicit confirmation exists.
    Unknown,
    /// The command was independently confirmed.
    Confirmed,
    /// The command was explicitly left unconfirmed.
    Unconfirmed,
}

/// One field proven by a partial lighting command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRgbLightingPartialStateDto {
    /// Accessory power.
    Power,
    /// Solid RGB color.
    Color,
    /// Brightness percentage.
    Brightness,
    /// Speed of the selected controller effect.
    EffectSpeed,
}

/// Persisted transport status for a standalone RGB accessory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRgbLightingConnectionStateDto {
    /// No current transport conclusion is available.
    Unknown,
    /// The accessory is known to be disconnected.
    Disconnected,
    /// The verified accessory is currently connected.
    Ready,
}

/// One named solid-lighting preset crossing the mobile boundary.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRgbLightingPresetDto {
    /// User-visible preset name.
    pub name: String,

    /// Typed solid-lighting state stored by the preset.
    pub requested: MobileMelkLightingRestoreStateDto,
}

/// Invalid persisted RGB accessory data presented by the mobile platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileRgbLightingRecordError {
    /// Bytes do not match the record schema.
    #[error("invalid RGB lighting record encoding")]
    InvalidEncoding,
    /// The record schema version is not supported.
    #[error("unsupported RGB lighting record version")]
    UnsupportedVersion,
    /// An identity or user label is empty or too large.
    #[error("invalid RGB lighting record text")]
    InvalidText,
    /// The profile is not verified by this build.
    #[error("invalid RGB lighting profile")]
    InvalidProfile,
    /// Profile version zero is not meaningful.
    #[error("invalid RGB lighting profile version")]
    InvalidProfileVersion,
    /// The bounded preset count was exceeded.
    #[error("too many RGB lighting presets")]
    TooManyPresets,
    /// A preset name is already present.
    #[error("duplicate RGB lighting preset")]
    DuplicatePreset,
    /// A persisted state was outside the typed domain.
    #[error("invalid RGB lighting state")]
    InvalidState,
}

impl From<CoreRgbLightingRecordError> for MobileRgbLightingRecordError {
    fn from(error: CoreRgbLightingRecordError) -> Self {
        match error {
            CoreRgbLightingRecordError::InvalidEncoding => Self::InvalidEncoding,
            CoreRgbLightingRecordError::UnsupportedVersion => Self::UnsupportedVersion,
            CoreRgbLightingRecordError::InvalidText => Self::InvalidText,
            CoreRgbLightingRecordError::InvalidProfile => Self::InvalidProfile,
            CoreRgbLightingRecordError::InvalidProfileVersion => Self::InvalidProfileVersion,
            CoreRgbLightingRecordError::TooManyPresets => Self::TooManyPresets,
            CoreRgbLightingRecordError::DuplicatePreset => Self::DuplicatePreset,
            CoreRgbLightingRecordError::InvalidState => Self::InvalidState,
        }
    }
}

impl From<MobileRgbLightingProfileKindDto> for RgbLightingProfileKind {
    fn from(profile: MobileRgbLightingProfileKindDto) -> Self {
        match profile {
            MobileRgbLightingProfileKindDto::MelkOc21 => Self::MelkOc21,
        }
    }
}

impl From<RgbLightingProfileKind> for MobileRgbLightingProfileKindDto {
    fn from(profile: RgbLightingProfileKind) -> Self {
        match profile {
            RgbLightingProfileKind::MelkOc21 => Self::MelkOc21,
        }
    }
}

impl From<MobileRgbLightingConfirmationStateDto> for CoreRgbLightingConfirmationState {
    fn from(state: MobileRgbLightingConfirmationStateDto) -> Self {
        match state {
            MobileRgbLightingConfirmationStateDto::Unknown => Self::Unknown,
            MobileRgbLightingConfirmationStateDto::Confirmed => Self::Confirmed,
            MobileRgbLightingConfirmationStateDto::Unconfirmed => Self::Unconfirmed,
        }
    }
}

impl From<CoreRgbLightingConfirmationState> for MobileRgbLightingConfirmationStateDto {
    fn from(state: CoreRgbLightingConfirmationState) -> Self {
        match state {
            CoreRgbLightingConfirmationState::Unknown => Self::Unknown,
            CoreRgbLightingConfirmationState::Confirmed => Self::Confirmed,
            CoreRgbLightingConfirmationState::Unconfirmed => Self::Unconfirmed,
        }
    }
}

impl From<MobileRgbLightingPartialStateDto> for CoreRgbLightingPartialState {
    fn from(state: MobileRgbLightingPartialStateDto) -> Self {
        match state {
            MobileRgbLightingPartialStateDto::Power => Self::Power,
            MobileRgbLightingPartialStateDto::Color => Self::Color,
            MobileRgbLightingPartialStateDto::Brightness => Self::Brightness,
            MobileRgbLightingPartialStateDto::EffectSpeed => Self::EffectSpeed,
        }
    }
}

impl From<MobileRgbLightingConnectionStateDto> for CoreRgbLightingConnectionState {
    fn from(state: MobileRgbLightingConnectionStateDto) -> Self {
        match state {
            MobileRgbLightingConnectionStateDto::Unknown => Self::Unknown,
            MobileRgbLightingConnectionStateDto::Disconnected => Self::Disconnected,
            MobileRgbLightingConnectionStateDto::Ready => Self::Ready,
        }
    }
}

impl From<CoreRgbLightingConnectionState> for MobileRgbLightingConnectionStateDto {
    fn from(state: CoreRgbLightingConnectionState) -> Self {
        match state {
            CoreRgbLightingConnectionState::Unknown => Self::Unknown,
            CoreRgbLightingConnectionState::Disconnected => Self::Disconnected,
            CoreRgbLightingConnectionState::Ready => Self::Ready,
        }
    }
}

impl TryFrom<MobileMelkLightingRestoreStateDto> for RgbLightingRequestedState {
    type Error = MobileRgbLightingRecordError;

    fn try_from(state: MobileMelkLightingRestoreStateDto) -> Result<Self, Self::Error> {
        let brightness = LightingBrightness::try_from_percent(state.brightness)
            .map_err(|_| MobileRgbLightingRecordError::InvalidState)?;
        Ok(Self::new(
            if state.power_on {
                LightingPowerState::On
            } else {
                LightingPowerState::Off
            },
            RgbColor::new(state.red, state.green, state.blue),
            brightness,
        )
        .with_playback(
            state
                .playback
                .unwrap_or(MobileLightingPlaybackDto::Solid)
                .try_into()?,
        ))
    }
}

impl From<RgbLightingRequestedState> for MobileMelkLightingRestoreStateDto {
    fn from(state: RgbLightingRequestedState) -> Self {
        Self {
            power_on: state.power() == LightingPowerState::On,
            red: state.color().red(),
            green: state.color().green(),
            blue: state.color().blue(),
            brightness: state.brightness().as_percent(),
            playback: (state.playback() != cutout_core::LightingPlayback::Solid)
                .then(|| state.playback().into()),
        }
    }
}

impl From<RgbLightingPreset> for MobileRgbLightingPresetDto {
    fn from(preset: RgbLightingPreset) -> Self {
        Self {
            name: preset.name().to_owned(),
            requested: preset.requested().into(),
        }
    }
}

/// Rust-owned versioned persistence record for one standalone RGB accessory.
#[derive(Debug, uniffi::Object)]
pub struct MobileRgbLightingAccessoryRecord {
    pub(super) inner: Arc<AccessoryRecord>,
}

impl MobileRgbLightingAccessoryRecord {
    pub(super) fn from_application(inner: Arc<AccessoryRecord>) -> Self {
        Self { inner }
    }
}

#[uniffi::export]
impl MobileRgbLightingAccessoryRecord {
    /// Creates an empty record for one verified profile.
    ///
    /// # Errors
    ///
    /// Returns a typed record error when the identity or profile version is invalid.
    #[uniffi::constructor]
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports owned inputs")]
    pub fn new(
        platform_identifier: String,
        profile: MobileRgbLightingProfileKindDto,
        profile_version: u16,
    ) -> Result<Arc<Self>, MobileRgbLightingRecordError> {
        Ok(Arc::new(Self {
            inner: Arc::new(
                AccessoryRecord::new(platform_identifier, profile.into(), profile_version)
                    .map_err(MobileRgbLightingRecordError::from)?,
            ),
        }))
    }

    /// Decodes a persisted record without exposing JSON to Swift.
    ///
    /// # Errors
    ///
    /// Returns a typed record error when bytes are malformed, unsupported, or out of bounds.
    #[uniffi::constructor]
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports owned inputs")]
    pub fn decode(bytes: Vec<u8>) -> Result<Arc<Self>, MobileRgbLightingRecordError> {
        Ok(Arc::new(Self {
            inner: Arc::new(
                AccessoryRecord::decode(&bytes).map_err(MobileRgbLightingRecordError::from)?,
            ),
        }))
    }

    /// Encodes this record as versioned bytes.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRgbLightingRecordError::InvalidEncoding`] when Rust cannot serialize the
    /// record.
    pub fn encode(&self) -> Result<Vec<u8>, MobileRgbLightingRecordError> {
        self.inner.encode().map_err(Into::into)
    }

    /// Returns the platform-scoped identity, canonicalizing Bluetooth UUIDs and preserving other
    /// opaque platform identifiers exactly.
    #[must_use]
    pub fn platform_identifier(&self) -> String {
        let identifier = self.inner.platform_identifier();
        match Uuid::parse_str(&identifier) {
            Ok(uuid) => uuid.hyphenated().to_string().to_uppercase(),
            Err(_) => identifier,
        }
    }

    /// Returns the verified profile kind.
    #[must_use]
    pub fn profile(&self) -> MobileRgbLightingProfileKindDto {
        self.inner.profile().into()
    }

    /// Returns the verified profile schema version.
    #[must_use]
    pub fn profile_version(&self) -> u16 {
        self.inner.profile_version()
    }

    /// Returns the optional user alias.
    #[must_use]
    pub fn alias(&self) -> Option<String> {
        self.inner.alias()
    }

    /// Sets or clears the user alias.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRgbLightingRecordError::InvalidText`] for an empty or oversized alias.
    pub fn set_alias(&self, alias: Option<String>) -> Result<(), MobileRgbLightingRecordError> {
        self.inner.set_alias(alias).map_err(Into::into)
    }

    /// Returns the optional installed-vehicle association.
    #[must_use]
    pub fn vehicle_identifier(&self) -> Option<String> {
        self.inner.vehicle_identifier()
    }

    /// Sets or clears the installed-vehicle association.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRgbLightingRecordError::InvalidText`] for an empty or oversized identifier.
    pub fn set_vehicle_identifier(
        &self,
        identifier: Option<String>,
    ) -> Result<(), MobileRgbLightingRecordError> {
        self.inner
            .set_vehicle_identifier(identifier)
            .map_err(Into::into)
    }

    /// Returns the last requested solid-lighting state.
    pub fn requested_state(&self) -> Option<MobileMelkLightingRestoreStateDto> {
        self.inner.requested_state().map(Into::into)
    }

    /// Records the last typed state requested by the user.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRgbLightingRecordError::InvalidState`] for brightness above 100 percent.
    pub fn set_requested_state(
        &self,
        state: Option<MobileMelkLightingRestoreStateDto>,
    ) -> Result<(), MobileRgbLightingRecordError> {
        let state = state.map(TryInto::try_into).transpose()?;
        self.inner.set_requested_state(state);
        Ok(())
    }

    /// Returns the last independently confirmed state.
    pub fn confirmed_state(&self) -> Option<MobileMelkLightingRestoreStateDto> {
        self.inner.confirmed_state().map(Into::into)
    }

    /// Records the last independently confirmed state.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRgbLightingRecordError::InvalidState`] for brightness above 100 percent.
    pub fn set_confirmed_state(
        &self,
        state: Option<MobileMelkLightingRestoreStateDto>,
    ) -> Result<(), MobileRgbLightingRecordError> {
        let state = state.map(TryInto::try_into).transpose()?;
        self.inner.set_confirmed_state(state);
        Ok(())
    }

    /// Merges one confirmed partial field into the existing complete baseline.
    ///
    /// Returns `false` when no complete baseline exists or the field cannot be merged with the
    /// stored playback mode.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRgbLightingRecordError::InvalidState`] when the supplied state is invalid.
    pub fn confirm_partial_state(
        &self,
        state: MobileMelkLightingRestoreStateDto,
        field: MobileRgbLightingPartialStateDto,
    ) -> Result<bool, MobileRgbLightingRecordError> {
        let state = state.try_into()?;
        Ok(self.inner.confirm_partial_state(state, field.into()))
    }

    /// Returns the latest command-confirmation evidence.
    #[must_use]
    pub fn confirmation(&self) -> MobileRgbLightingConfirmationStateDto {
        self.inner.confirmation().into()
    }

    /// Records command-confirmation evidence.
    pub fn set_confirmation(&self, state: MobileRgbLightingConfirmationStateDto) {
        self.inner.set_confirmation(state.into());
    }

    /// Returns the latest persisted schedule-command confirmation.
    pub fn last_schedule_confirmation(&self) -> Option<MobileRgbLightingConfirmationStateDto> {
        self.inner.last_schedule_confirmation().map(Into::into)
    }

    /// Records schedule-command confirmation without affecting lighting restore eligibility.
    pub fn set_last_schedule_confirmation(&self, state: MobileRgbLightingConfirmationStateDto) {
        self.inner.set_last_schedule_confirmation(state.into());
    }

    /// Returns the persisted transport status.
    #[must_use]
    pub fn connection(&self) -> MobileRgbLightingConnectionStateDto {
        self.inner.connection().into()
    }

    /// Records transport status without treating it as output truth.
    pub fn set_connection(&self, state: MobileRgbLightingConnectionStateDto) {
        self.inner.set_connection(state.into());
    }

    /// Returns whether explicit same-profile restore is enabled.
    #[must_use]
    pub fn restore_enabled(&self) -> bool {
        self.inner.restore_enabled()
    }

    /// Sets the explicit restore preference.
    pub fn set_restore_enabled(&self, enabled: bool) {
        self.inner.set_restore_enabled(enabled);
    }

    /// Returns all named presets in insertion order.
    pub fn presets(&self) -> Vec<MobileRgbLightingPresetDto> {
        self.inner.presets().into_iter().map(Into::into).collect()
    }

    /// Adds one named solid-lighting preset.
    ///
    /// # Errors
    ///
    /// Returns a typed record error for invalid state/name, duplicate names, or the preset bound.
    pub fn add_preset(
        &self,
        name: String,
        requested: MobileMelkLightingRestoreStateDto,
    ) -> Result<(), MobileRgbLightingRecordError> {
        let preset = RgbLightingPreset::new(name, requested.try_into()?)
            .map_err(MobileRgbLightingRecordError::from)?;
        self.inner.add_preset(preset).map_err(Into::into)
    }

    /// Removes a named app scene and reports whether it existed.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "String is the stable UniFFI ABI for this exported method"
    )]
    #[must_use]
    pub fn remove_preset(&self, name: String) -> bool {
        self.inner.remove_preset(&name)
    }

    /// Replaces a named app scene and reports whether it existed.
    ///
    /// # Errors
    ///
    /// Returns a typed record error when the replacement state or name is invalid.
    pub fn replace_preset(
        &self,
        name: String,
        requested: MobileMelkLightingRestoreStateDto,
    ) -> Result<bool, MobileRgbLightingRecordError> {
        let preset = RgbLightingPreset::new(name, requested.try_into()?)
            .map_err(MobileRgbLightingRecordError::from)?;
        Ok(self.inner.replace_preset(preset))
    }
}
