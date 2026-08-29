//! Typed persistence for standalone RGB lighting accessories.

use crate::RgbLightingRequestedState;

/// Current version of the persisted standalone RGB accessory record.
pub const RGB_LIGHTING_RECORD_VERSION: u8 = 1;

/// Maximum UTF-8 bytes accepted for a persisted lighting label or identity.
pub const RGB_LIGHTING_MAX_TEXT_BYTES: usize = 128;

/// Maximum named presets retained for one accessory.
pub const RGB_LIGHTING_MAX_PRESETS: usize = 16;

/// A verified standalone RGB profile kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RgbLightingProfileKind {
    /// The verified ELK-BLEDOM/MELK profile for MELK-OC21.
    MelkOc21,
}

impl RgbLightingProfileKind {
    fn wire_name(self) -> &'static str {
        match self {
            Self::MelkOc21 => "melk_oc21",
        }
    }

    fn from_wire_name(name: &str) -> Option<Self> {
        match name {
            "melk_oc21" => Some(Self::MelkOc21),
            _ => None,
        }
    }
}

/// Evidence state for the most recently requested command.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RgbLightingConfirmationState {
    /// No command has an explicit confirmation result.
    #[default]
    Unknown,
    /// The user or protocol evidence confirmed the command.
    Confirmed,
    /// The command was explicitly observed as unconfirmed.
    Unconfirmed,
}

/// Persisted connection status, kept separate from requested and confirmed state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RgbLightingConnectionState {
    /// No current transport conclusion is available.
    #[default]
    Unknown,
    /// The accessory is known to be disconnected.
    Disconnected,
    /// The verified accessory is currently connected.
    Ready,
}

/// One named, solid-lighting preset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbLightingPreset {
    name: String,
    requested: RgbLightingRequestedState,
}

impl RgbLightingPreset {
    /// Creates a named preset after validating its bounded label.
    ///
    /// # Errors
    ///
    /// Returns [`RgbLightingRecordError::InvalidText`] for an empty or oversized name.
    pub fn new(
        name: String,
        requested: RgbLightingRequestedState,
    ) -> Result<Self, RgbLightingRecordError> {
        validate_text(&name)?;
        Ok(Self { name, requested })
    }

    /// Returns the user-visible preset name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the typed solid-lighting state stored by the preset.
    #[must_use]
    pub const fn requested(&self) -> RgbLightingRequestedState {
        self.requested
    }
}

/// Versioned, Rust-owned persisted record for one standalone RGB accessory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbLightingAccessoryRecord {
    platform_identifier: String,
    profile: RgbLightingProfileKind,
    profile_version: u16,
    alias: Option<String>,
    vehicle_identifier: Option<String>,
    requested_state: Option<RgbLightingRequestedState>,
    confirmed_state: Option<RgbLightingRequestedState>,
    confirmation: RgbLightingConfirmationState,
    connection: RgbLightingConnectionState,
    restore_enabled: bool,
    presets: Vec<RgbLightingPreset>,
}

impl RgbLightingAccessoryRecord {
    /// Creates a record for a verified profile and platform-scoped identity.
    ///
    /// # Errors
    ///
    /// Returns [`RgbLightingRecordError::InvalidText`] for an empty or oversized identity, or
    /// [`RgbLightingRecordError::InvalidProfileVersion`] for version zero.
    pub fn new(
        platform_identifier: String,
        profile: RgbLightingProfileKind,
        profile_version: u16,
    ) -> Result<Self, RgbLightingRecordError> {
        validate_text(&platform_identifier)?;
        if profile_version == 0 {
            return Err(RgbLightingRecordError::InvalidProfileVersion);
        }
        Ok(Self {
            platform_identifier,
            profile,
            profile_version,
            alias: None,
            vehicle_identifier: None,
            requested_state: None,
            confirmed_state: None,
            confirmation: RgbLightingConfirmationState::Unknown,
            connection: RgbLightingConnectionState::Unknown,
            restore_enabled: false,
            presets: Vec::new(),
        })
    }

    /// Returns the platform-scoped accessory identity.
    #[must_use]
    pub fn platform_identifier(&self) -> &str {
        &self.platform_identifier
    }

    /// Returns the verified profile kind.
    #[must_use]
    pub const fn profile(&self) -> RgbLightingProfileKind {
        self.profile
    }

    /// Returns the verified profile schema version.
    #[must_use]
    pub const fn profile_version(&self) -> u16 {
        self.profile_version
    }

    /// Returns the optional user alias.
    #[must_use]
    pub fn alias(&self) -> Option<&str> {
        self.alias.as_deref()
    }

    /// Sets or clears the user alias.
    ///
    /// # Errors
    ///
    /// Returns [`RgbLightingRecordError::InvalidText`] for an empty or oversized alias.
    pub fn set_alias(&mut self, alias: Option<String>) -> Result<(), RgbLightingRecordError> {
        if let Some(alias) = &alias {
            validate_text(alias)?;
        }
        self.alias = alias;
        Ok(())
    }

    /// Returns the optional installed-vehicle association.
    #[must_use]
    pub fn vehicle_identifier(&self) -> Option<&str> {
        self.vehicle_identifier.as_deref()
    }

    /// Sets or clears the installed-vehicle association.
    ///
    /// # Errors
    ///
    /// Returns [`RgbLightingRecordError::InvalidText`] for an empty or oversized identifier.
    pub fn set_vehicle_identifier(
        &mut self,
        identifier: Option<String>,
    ) -> Result<(), RgbLightingRecordError> {
        if let Some(identifier) = &identifier {
            validate_text(identifier)?;
        }
        self.vehicle_identifier = identifier;
        Ok(())
    }

    /// Returns the last requested state, if one was submitted.
    #[must_use]
    pub const fn requested_state(&self) -> Option<RgbLightingRequestedState> {
        self.requested_state
    }

    /// Records the last typed state requested by the user.
    pub const fn set_requested_state(&mut self, state: Option<RgbLightingRequestedState>) {
        self.requested_state = state;
    }

    /// Returns the last independently confirmed state, if any.
    #[must_use]
    pub const fn confirmed_state(&self) -> Option<RgbLightingRequestedState> {
        self.confirmed_state
    }

    /// Records the last independently confirmed state, if any.
    pub const fn set_confirmed_state(&mut self, state: Option<RgbLightingRequestedState>) {
        self.confirmed_state = state;
    }

    /// Returns the latest command confirmation evidence.
    #[must_use]
    pub const fn confirmation(&self) -> RgbLightingConfirmationState {
        self.confirmation
    }

    /// Records command confirmation evidence without changing requested state.
    pub const fn set_confirmation(&mut self, state: RgbLightingConfirmationState) {
        self.confirmation = state;
    }

    /// Returns the current transport state.
    #[must_use]
    pub const fn connection(&self) -> RgbLightingConnectionState {
        self.connection
    }

    /// Records current transport state without treating it as light output truth.
    pub const fn set_connection(&mut self, state: RgbLightingConnectionState) {
        self.connection = state;
    }

    /// Returns whether same-profile restore is explicitly enabled.
    #[must_use]
    pub const fn restore_enabled(&self) -> bool {
        self.restore_enabled
    }

    /// Sets the explicit restore preference.
    pub const fn set_restore_enabled(&mut self, enabled: bool) {
        self.restore_enabled = enabled;
    }

    /// Returns named presets in insertion order.
    #[must_use]
    pub fn presets(&self) -> &[RgbLightingPreset] {
        &self.presets
    }

    /// Adds one bounded, uniquely named preset.
    ///
    /// # Errors
    ///
    /// Returns [`RgbLightingRecordError::TooManyPresets`] at the bound or
    /// [`RgbLightingRecordError::DuplicatePreset`] for a repeated name.
    pub fn add_preset(&mut self, preset: RgbLightingPreset) -> Result<(), RgbLightingRecordError> {
        if self.presets.len() >= RGB_LIGHTING_MAX_PRESETS {
            return Err(RgbLightingRecordError::TooManyPresets);
        }
        if self
            .presets
            .iter()
            .any(|existing| existing.name == preset.name)
        {
            return Err(RgbLightingRecordError::DuplicatePreset);
        }
        self.presets.push(preset);
        Ok(())
    }

    /// Encodes the versioned record as bounded JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns [`RgbLightingRecordError::InvalidEncoding`] if serialization fails.
    #[cfg(feature = "serde")]
    pub fn encode(&self) -> Result<Vec<u8>, RgbLightingRecordError> {
        serde_json::to_vec(&WireRecord::from(self))
            .map_err(|_| RgbLightingRecordError::InvalidEncoding)
    }

    /// Decodes and validates a versioned record, rejecting unknown fields and values.
    ///
    /// # Errors
    ///
    /// Returns the matching [`RgbLightingRecordError`] for malformed bytes, unsupported versions,
    /// invalid profile/text/state fields, or preset bounds.
    #[cfg(feature = "serde")]
    pub fn decode(bytes: &[u8]) -> Result<Self, RgbLightingRecordError> {
        let wire: WireRecord =
            serde_json::from_slice(bytes).map_err(|_| RgbLightingRecordError::InvalidEncoding)?;
        Self::try_from(wire)
    }
}

/// Invalid or unsupported standalone RGB persistence data.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RgbLightingRecordError {
    /// Bytes do not match the record schema.
    #[error("invalid RGB lighting record encoding")]
    InvalidEncoding,
    /// The record schema version is not supported.
    #[error("unsupported RGB lighting record version")]
    UnsupportedVersion,
    /// An identity or user label is empty or too large.
    #[error("invalid RGB lighting record text")]
    InvalidText,
    /// The profile is not one this build verifies.
    #[error("invalid RGB lighting profile")]
    InvalidProfile,
    /// Profile version zero is not meaningful.
    #[error("invalid RGB lighting profile version")]
    InvalidProfileVersion,
    /// More presets than the bounded record allows were supplied.
    #[error("too many RGB lighting presets")]
    TooManyPresets,
    /// A preset name is already present in the record.
    #[error("duplicate RGB lighting preset")]
    DuplicatePreset,
    /// A persisted brightness value exceeded the typed 0..=100 domain.
    #[error("invalid RGB lighting state")]
    InvalidState,
}

fn validate_text(value: &str) -> Result<(), RgbLightingRecordError> {
    if value.trim().is_empty() || value.len() > RGB_LIGHTING_MAX_TEXT_BYTES {
        return Err(RgbLightingRecordError::InvalidText);
    }
    Ok(())
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct WireRecord {
    version: u8,
    platform_identifier: String,
    profile: String,
    profile_version: u16,
    alias: Option<String>,
    vehicle_identifier: Option<String>,
    requested_state: Option<WireState>,
    confirmed_state: Option<WireState>,
    confirmation: WireConfirmation,
    connection: WireConnection,
    restore_enabled: bool,
    presets: Vec<WirePreset>,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct WireState {
    power_on: bool,
    red: u8,
    green: u8,
    blue: u8,
    brightness: u8,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum WireConfirmation {
    Unknown,
    Confirmed,
    Unconfirmed,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum WireConnection {
    Unknown,
    Disconnected,
    Ready,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct WirePreset {
    name: String,
    requested_state: WireState,
}

#[cfg(feature = "serde")]
impl From<&RgbLightingAccessoryRecord> for WireRecord {
    fn from(record: &RgbLightingAccessoryRecord) -> Self {
        Self {
            version: RGB_LIGHTING_RECORD_VERSION,
            platform_identifier: record.platform_identifier.clone(),
            profile: record.profile.wire_name().to_owned(),
            profile_version: record.profile_version,
            alias: record.alias.clone(),
            vehicle_identifier: record.vehicle_identifier.clone(),
            requested_state: record.requested_state.map(WireState::from),
            confirmed_state: record.confirmed_state.map(WireState::from),
            confirmation: record.confirmation.into(),
            connection: record.connection.into(),
            restore_enabled: record.restore_enabled,
            presets: record
                .presets
                .iter()
                .map(|preset| WirePreset {
                    name: preset.name.clone(),
                    requested_state: WireState::from(preset.requested),
                })
                .collect(),
        }
    }
}

#[cfg(feature = "serde")]
impl TryFrom<WireRecord> for RgbLightingAccessoryRecord {
    type Error = RgbLightingRecordError;

    fn try_from(wire: WireRecord) -> Result<Self, Self::Error> {
        if wire.version != RGB_LIGHTING_RECORD_VERSION {
            return Err(RgbLightingRecordError::UnsupportedVersion);
        }
        let profile = RgbLightingProfileKind::from_wire_name(&wire.profile)
            .ok_or(RgbLightingRecordError::InvalidProfile)?;
        let mut record = Self::new(wire.platform_identifier, profile, wire.profile_version)?;
        record.set_alias(wire.alias)?;
        record.set_vehicle_identifier(wire.vehicle_identifier)?;
        record.requested_state = wire
            .requested_state
            .map(RgbLightingRequestedState::try_from)
            .transpose()?;
        record.confirmed_state = wire
            .confirmed_state
            .map(RgbLightingRequestedState::try_from)
            .transpose()?;
        record.confirmation = wire.confirmation.into();
        record.connection = wire.connection.into();
        record.restore_enabled = wire.restore_enabled;
        for preset in wire.presets {
            record.add_preset(RgbLightingPreset::new(
                preset.name,
                preset.requested_state.try_into()?,
            )?)?;
        }
        Ok(record)
    }
}

#[cfg(feature = "serde")]
impl From<RgbLightingRequestedState> for WireState {
    fn from(state: RgbLightingRequestedState) -> Self {
        Self {
            power_on: matches!(state.power(), crate::LightingPowerState::On),
            red: state.color().red(),
            green: state.color().green(),
            blue: state.color().blue(),
            brightness: state.brightness().as_percent(),
        }
    }
}

#[cfg(feature = "serde")]
impl TryFrom<WireState> for RgbLightingRequestedState {
    type Error = RgbLightingRecordError;

    fn try_from(state: WireState) -> Result<Self, Self::Error> {
        let brightness = crate::LightingBrightness::try_from_percent(state.brightness)
            .map_err(|_| RgbLightingRecordError::InvalidState)?;
        Ok(Self::new(
            if state.power_on {
                crate::LightingPowerState::On
            } else {
                crate::LightingPowerState::Off
            },
            crate::RgbColor::new(state.red, state.green, state.blue),
            brightness,
        ))
    }
}

#[cfg(feature = "serde")]
impl From<RgbLightingConfirmationState> for WireConfirmation {
    fn from(state: RgbLightingConfirmationState) -> Self {
        match state {
            RgbLightingConfirmationState::Unknown => Self::Unknown,
            RgbLightingConfirmationState::Confirmed => Self::Confirmed,
            RgbLightingConfirmationState::Unconfirmed => Self::Unconfirmed,
        }
    }
}

#[cfg(feature = "serde")]
impl From<WireConfirmation> for RgbLightingConfirmationState {
    fn from(state: WireConfirmation) -> Self {
        match state {
            WireConfirmation::Unknown => Self::Unknown,
            WireConfirmation::Confirmed => Self::Confirmed,
            WireConfirmation::Unconfirmed => Self::Unconfirmed,
        }
    }
}

#[cfg(feature = "serde")]
impl From<RgbLightingConnectionState> for WireConnection {
    fn from(state: RgbLightingConnectionState) -> Self {
        match state {
            RgbLightingConnectionState::Unknown => Self::Unknown,
            RgbLightingConnectionState::Disconnected => Self::Disconnected,
            RgbLightingConnectionState::Ready => Self::Ready,
        }
    }
}

#[cfg(feature = "serde")]
impl From<WireConnection> for RgbLightingConnectionState {
    fn from(state: WireConnection) -> Self {
        match state {
            WireConnection::Unknown => Self::Unknown,
            WireConnection::Disconnected => Self::Disconnected,
            WireConnection::Ready => Self::Ready,
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;

    fn state() -> RgbLightingRequestedState {
        RgbLightingRequestedState::new(
            crate::LightingPowerState::On,
            crate::RgbColor::new(1, 2, 3),
            crate::LightingBrightness::try_from_percent(42).expect("bounded brightness"),
        )
    }

    #[test]
    fn accessory_record_round_trips_typed_state_and_metadata() {
        let mut record = RgbLightingAccessoryRecord::new(
            "melk-1".to_owned(),
            RgbLightingProfileKind::MelkOc21,
            1,
        )
        .expect("valid record");
        record
            .set_alias(Some("Aero LEDs".to_owned()))
            .expect("alias");
        record
            .set_vehicle_identifier(Some("euc-1".to_owned()))
            .expect("association");
        record.set_requested_state(Some(state()));
        record.set_confirmed_state(Some(state()));
        record.set_confirmation(RgbLightingConfirmationState::Confirmed);
        record.set_connection(RgbLightingConnectionState::Ready);
        record.set_restore_enabled(true);
        record
            .add_preset(RgbLightingPreset::new("Cruise".to_owned(), state()).expect("preset"))
            .expect("unique preset");

        let encoded = record.encode().expect("record encodes");
        assert_eq!(RgbLightingAccessoryRecord::decode(&encoded), Ok(record));
    }

    #[test]
    fn accessory_record_rejects_unknown_version_and_invalid_brightness() {
        let unknown_version = br#"{"version":2,"platform_identifier":"melk-1","profile":"melk_oc21","profile_version":1,"alias":null,"vehicle_identifier":null,"requested_state":null,"confirmed_state":null,"confirmation":"unknown","connection":"unknown","restore_enabled":false,"presets":[]}"#;
        assert_eq!(
            RgbLightingAccessoryRecord::decode(unknown_version),
            Err(RgbLightingRecordError::UnsupportedVersion)
        );

        let invalid_brightness = br#"{"version":1,"platform_identifier":"melk-1","profile":"melk_oc21","profile_version":1,"alias":null,"vehicle_identifier":null,"requested_state":{"power_on":true,"red":1,"green":2,"blue":3,"brightness":101},"confirmed_state":null,"confirmation":"unknown","connection":"unknown","restore_enabled":false,"presets":[]}"#;
        assert_eq!(
            RgbLightingAccessoryRecord::decode(invalid_brightness),
            Err(RgbLightingRecordError::InvalidState)
        );
    }

    #[test]
    fn accessory_record_bounds_text_and_preset_count() {
        let oversized = "x".repeat(RGB_LIGHTING_MAX_TEXT_BYTES + 1);
        assert_eq!(
            RgbLightingAccessoryRecord::new(oversized, RgbLightingProfileKind::MelkOc21, 1),
            Err(RgbLightingRecordError::InvalidText)
        );

        let mut record = RgbLightingAccessoryRecord::new(
            "melk-1".to_owned(),
            RgbLightingProfileKind::MelkOc21,
            1,
        )
        .expect("valid record");
        for index in 0..RGB_LIGHTING_MAX_PRESETS {
            record
                .add_preset(
                    RgbLightingPreset::new(format!("preset-{index}"), state()).expect("preset"),
                )
                .expect("preset fits");
        }
        assert_eq!(
            record.add_preset(RgbLightingPreset::new("overflow".to_owned(), state()).unwrap()),
            Err(RgbLightingRecordError::TooManyPresets)
        );
    }
}
