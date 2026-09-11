//! MELK-OC21 protocol-specific RGB FFI surface.

use std::sync::Arc;

use crate::MobileBluetoothUuid;
use cutout_core::{
    LightingBrightness, LightingPowerState, RgbColor, RgbLightingCommand,
    RgbLightingRequestedState, RgbLightingRestoreDecision,
    RgbLightingRestoreMarker as CoreRgbLightingRestoreMarker, TransportAction, WriteMode,
};
use cutout_protocols::{MelkGattEvidence, MelkLightingProfile};

mod commands;
mod session;

pub use commands::*;
pub use session::*;

/// CoreBluetooth write mode required by the MELK protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingWriteModeDto {
    /// Write without waiting for a transport acknowledgement.
    WithoutResponse,
}

/// Typed GATT-role evidence observed for a standalone MELK controller.
///
/// Swift/CoreBluetooth derives these flags from discovered UUIDs and
/// characteristic properties; it never supplies UUIDs or protocol bytes to
/// the Rust profile boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingGattEvidence {
    /// The verified `FFF0` primary service was observed.
    pub service_present: bool,

    /// The verified `FFF3` characteristic supports write-without-response.
    pub write_without_response: bool,

    /// The verified `FFF4` characteristic supports notifications or indicates.
    pub notify_or_indicate: bool,
}

/// One bounded MELK lighting write plus its independent confirmation policy.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingWriteDto {
    /// Characteristic receiving the command frame.
    pub characteristic: MobileBluetoothUuid,

    /// Candidate protocol frame emitted by Rust.
    pub payload: Vec<u8>,

    /// Required transport write mode.
    pub mode: MobileMelkLightingWriteModeDto,

    /// Characteristic whose notifications may confirm the command.
    pub confirmation_characteristic: MobileBluetoothUuid,

    /// Capture-backed minimum command interval, when known.
    pub minimum_interval_ms: Option<u16>,
}

/// Evidence-record capabilities exposed to the mobile lighting UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent capability flags mirror the typed protocol evidence"
)]
pub struct MobileMelkLightingCapabilitiesDto {
    /// Effect IDs with physical evidence on the MELK-OC21 controller.
    pub verified_effect_ids: Vec<u8>,
    /// Whether controller-local microphone modes are verified.
    pub controller_microphone: bool,
    /// Whether controller-local schedules are verified.
    pub schedules: bool,
    /// Whether independently addressable zones are verified.
    pub addressable_zones: bool,
    /// Whether controller-native named scenes are verified.
    pub scenes: bool,
}

/// Returns the conservative, capture-backed MELK-OC21 capability set.
#[uniffi::export]
#[must_use]
pub fn mobile_melk_lighting_capabilities() -> MobileMelkLightingCapabilitiesDto {
    let capabilities = MelkLightingProfile::capabilities();
    MobileMelkLightingCapabilitiesDto {
        verified_effect_ids: capabilities.verified_effect_ids.to_vec(),
        controller_microphone: capabilities.controller_microphone,
        schedules: capabilities.schedules,
        addressable_zones: capabilities.addressable_zones,
        scenes: capabilities.scenes,
    }
}

/// Returns the Rust-owned persisted MELK lighting profile version.
#[uniffi::export]
#[must_use]
pub fn mobile_melk_lighting_profile_version() -> u16 {
    cutout_core::rgb_lighting_profile_version()
}

/// Invalid input presented to the MELK lighting boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileMelkLightingError {
    /// Name and observed GATT evidence did not identify the candidate profile.
    #[error("invalid MELK GATT evidence")]
    InvalidGattEvidence,

    /// Brightness was outside the protocol's 0..=100 range.
    #[error("invalid MELK brightness percentage")]
    InvalidBrightness,

    /// Playback parameters are outside the supported controller ranges.
    #[error("invalid MELK playback parameters")]
    InvalidPlayback,

    /// The scheduler slot is outside the controller's supported ranges.
    #[error("invalid MELK schedule")]
    InvalidSchedule,

    /// The capability is known by protocol shape but lacks capture-backed physical evidence.
    #[error("unsupported MELK lighting capability")]
    UnsupportedCapability,

    /// The controller clock is outside the supported ranges.
    #[error("invalid MELK clock")]
    InvalidClock,
}

/// Verified standalone RGB profile persisted by the mobile boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingRestoreStateDto {
    /// Whether the controller was requested on.
    pub power_on: bool,

    /// Requested red channel.
    pub red: u8,

    /// Requested green channel.
    pub green: u8,

    /// Requested blue channel.
    pub blue: u8,

    /// Requested brightness percentage.
    pub brightness: u8,

    /// Controller playback; omitted values preserve legacy solid-color presets.
    #[uniffi(default = None)]
    pub playback: Option<MobileLightingPlaybackDto>,
}

/// Typed result kind for an attempted lighting restore.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingRestoreDecisionKindDto {
    /// Restore was disabled by the user.
    Disabled,

    /// `CoreBluetooth` restored a different platform identity.
    DifferentAccessory,

    /// The remembered state may be restored.
    Restore,
}

/// Result of reconciling a persisted lighting marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingRestoreDecisionDto {
    /// Typed decision kind.
    pub kind: MobileMelkLightingRestoreDecisionKindDto,

    /// Requested state when `kind` is `Restore`.
    pub requested: Option<MobileMelkLightingRestoreStateDto>,
}

/// Rust-owned marker for one selected MELK accessory and its confirmed state.
#[derive(Debug, uniffi::Object)]
pub struct MobileMelkLightingRestoreMarker {
    marker: CoreRgbLightingRestoreMarker,
}

#[uniffi::export]
impl MobileMelkLightingRestoreMarker {
    /// Creates a marker from a confirmed, bounded requested state.
    ///
    /// # Errors
    ///
    /// Returns `InvalidBrightness` when the requested brightness is outside the
    /// protocol's percentage range.
    #[uniffi::constructor]
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports owned inputs")]
    pub fn new(
        platform_identifier: String,
        requested: MobileMelkLightingRestoreStateDto,
    ) -> Result<Arc<Self>, MobileMelkLightingError> {
        let brightness = LightingBrightness::try_from_percent(requested.brightness)
            .map_err(|_| MobileMelkLightingError::InvalidBrightness)?;
        let playback = requested
            .playback
            .unwrap_or(MobileLightingPlaybackDto::Solid)
            .try_into()
            .map_err(|_| MobileMelkLightingError::InvalidPlayback)?;
        let requested = RgbLightingRequestedState::new(
            if requested.power_on {
                LightingPowerState::On
            } else {
                LightingPowerState::Off
            },
            RgbColor::new(requested.red, requested.green, requested.blue),
            brightness,
        )
        .with_playback(playback);
        Ok(Arc::new(Self {
            marker: CoreRgbLightingRestoreMarker::new(platform_identifier, requested),
        }))
    }

    /// Reconciles a marker with a restored platform identity and opt-in flag.
    #[must_use]
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports owned inputs")]
    pub fn recover(
        &self,
        restored_platform_identifier: String,
        restore_enabled: bool,
    ) -> MobileMelkLightingRestoreDecisionDto {
        match self
            .marker
            .recover(&restored_platform_identifier, restore_enabled)
        {
            RgbLightingRestoreDecision::Disabled => MobileMelkLightingRestoreDecisionDto {
                kind: MobileMelkLightingRestoreDecisionKindDto::Disabled,
                requested: None,
            },
            RgbLightingRestoreDecision::DifferentAccessory => {
                MobileMelkLightingRestoreDecisionDto {
                    kind: MobileMelkLightingRestoreDecisionKindDto::DifferentAccessory,
                    requested: None,
                }
            }
            RgbLightingRestoreDecision::Restore(requested) => {
                MobileMelkLightingRestoreDecisionDto {
                    kind: MobileMelkLightingRestoreDecisionKindDto::Restore,
                    requested: Some(requested.into()),
                }
            }
        }
    }
}

/// Rust-owned candidate profile for an observed `MELK-OC21` controller.
#[derive(Debug, uniffi::Object)]
pub struct MobileMelkLightingProfile;

#[uniffi::export]
impl MobileMelkLightingProfile {
    /// Selects the profile only when the family name and all typed GATT roles agree.
    ///
    /// # Errors
    ///
    /// Returns [`MobileMelkLightingError::InvalidGattEvidence`] when the identity evidence does
    /// not match the candidate profile.
    #[uniffi::constructor]
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports owned inputs")]
    pub fn new(
        name: String,
        evidence: MobileMelkLightingGattEvidence,
    ) -> Result<Arc<Self>, MobileMelkLightingError> {
        let evidence = MelkGattEvidence {
            service: evidence
                .service_present
                .then_some(cutout_protocols::MELK_SERVICE_CHANNEL),
            write: evidence
                .write_without_response
                .then_some(cutout_protocols::MELK_WRITE_CHANNEL),
            notify: evidence
                .notify_or_indicate
                .then_some(cutout_protocols::MELK_NOTIFY_CHANNEL),
        };
        MelkLightingProfile::identify(&name, evidence)
            .map(|_| Arc::new(Self))
            .ok_or(MobileMelkLightingError::InvalidGattEvidence)
    }
    /// Returns the MELK login sequence required by some firmware revisions.
    #[must_use]
    pub fn initialization(&self) -> Vec<MobileMelkLightingWriteDto> {
        MelkLightingProfile::initialization_actions()
            .into_iter()
            .map(mobile_melk_transport_action)
            .collect()
    }

    /// Creates an on/off command write.
    #[must_use]
    pub fn set_power(&self, on: bool) -> MobileMelkLightingWriteDto {
        mobile_melk_write(RgbLightingCommand::SetPower(if on {
            LightingPowerState::On
        } else {
            LightingPowerState::Off
        }))
    }

    /// Creates a solid RGB command write.
    #[must_use]
    pub fn set_solid_color(&self, red: u8, green: u8, blue: u8) -> MobileMelkLightingWriteDto {
        mobile_melk_write(RgbLightingCommand::SetSolidColor(RgbColor::new(
            red, green, blue,
        )))
    }

    /// Creates a brightness command write.
    ///
    /// # Errors
    ///
    /// Returns [`MobileMelkLightingError::InvalidBrightness`] when `percentage` is greater than
    /// 100.
    pub fn set_brightness(
        &self,
        percentage: u8,
    ) -> Result<MobileMelkLightingWriteDto, MobileMelkLightingError> {
        let brightness = LightingBrightness::try_from_percent(percentage)
            .map_err(|_| MobileMelkLightingError::InvalidBrightness)?;
        Ok(mobile_melk_write(RgbLightingCommand::SetBrightness(
            brightness,
        )))
    }
}

pub(crate) fn mobile_melk_transport_action(action: TransportAction) -> MobileMelkLightingWriteDto {
    let TransportAction::Write {
        channel,
        bytes,
        mode,
    } = action
    else {
        unreachable!("MELK lighting commands always produce writes");
    };
    let policy = MelkLightingProfile::write_policy();
    MobileMelkLightingWriteDto {
        characteristic: channel.as_uuid().into(),
        payload: bytes.as_slice().to_vec(),
        mode: mobile_melk_write_mode(mode),
        confirmation_characteristic: policy.confirmation_channel.as_uuid().into(),
        minimum_interval_ms: policy.minimum_interval_ms,
    }
}

pub(crate) fn mobile_melk_write(command: RgbLightingCommand) -> MobileMelkLightingWriteDto {
    mobile_melk_transport_action(MelkLightingProfile::write_action(command))
}

pub(crate) fn mobile_melk_control(command: cutout_core::MelkControl) -> MobileMelkLightingWriteDto {
    mobile_melk_transport_action(MelkLightingProfile::control_action(command))
}

fn mobile_melk_write_mode(mode: WriteMode) -> MobileMelkLightingWriteModeDto {
    match mode {
        WriteMode::WithoutResponse => MobileMelkLightingWriteModeDto::WithoutResponse,
        WriteMode::WithResponse => unreachable!("MELK policy is write without response"),
    }
}
