//! Typed phone contact feedback; this boundary cannot grant motor authority.

use crate::{CutoutSessionStateHandle, MobileConnectionAttemptTokenDto, MobileRideDatabaseError};
use cutout_core as core;

#[cfg(test)]
mod tests;

/// Persisted input layout, independent of controller qualification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneControlModeDto {
    /// No phone control surfaces.
    Disabled,
    /// Bottom Brake and Throttle regions.
    Buttons,
    /// Left/right Ride halves above navigation.
    Area,
}

impl From<MobilePhoneControlModeDto> for core::PhoneControlMode {
    fn from(value: MobilePhoneControlModeDto) -> Self {
        match value {
            MobilePhoneControlModeDto::Disabled => Self::Disabled,
            MobilePhoneControlModeDto::Buttons => Self::Buttons,
            MobilePhoneControlModeDto::Area => Self::Area,
        }
    }
}
impl From<core::PhoneControlMode> for MobilePhoneControlModeDto {
    fn from(value: core::PhoneControlMode) -> Self {
        match value {
            core::PhoneControlMode::Disabled => Self::Disabled,
            core::PhoneControlMode::Buttons => Self::Buttons,
            core::PhoneControlMode::Area => Self::Area,
        }
    }
}

/// Semantic region intent, never a motor command or a current sign.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveActionDto {
    /// Brake region.
    Brake,
    /// Throttle region.
    Throttle,
}
impl From<MobilePhoneDriveActionDto> for core::PhoneDriveAction {
    fn from(value: MobilePhoneDriveActionDto) -> Self {
        match value {
            MobilePhoneDriveActionDto::Brake => Self::Brake,
            MobilePhoneDriveActionDto::Throttle => Self::Throttle,
        }
    }
}
impl From<core::PhoneDriveAction> for MobilePhoneDriveActionDto {
    fn from(value: core::PhoneDriveAction) -> Self {
        match value {
            core::PhoneDriveAction::Brake => Self::Brake,
            core::PhoneDriveAction::Throttle => Self::Throttle,
        }
    }
}

/// Input cancellation reason, independent of any controller response.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveCancelReasonDto {
    /// Native touch delivery was cancelled by the platform.
    TouchCancelled,
    /// Native contact identity was malformed.
    InvalidContact,
    /// Layout preference changed.
    ModeChanged,
    /// Input-region geometry changed.
    LayoutChanged,
    /// Navigation, sheet presentation or disappearance.
    Navigation,
    /// Foreground focus was lost.
    SceneInactive,
    /// Connection or identity changed.
    ConnectionChanged,
    /// Contact left its starting region.
    RegionLeft,
    /// Multiple contacts were observed.
    MultipleContacts,
    /// Contact changed without complete release.
    ContactReplaced,
    /// No further input generation can be allocated.
    GenerationExhausted,
}
impl From<MobilePhoneDriveCancelReasonDto> for core::PhoneDriveCancelReason {
    fn from(value: MobilePhoneDriveCancelReasonDto) -> Self {
        match value {
            MobilePhoneDriveCancelReasonDto::TouchCancelled => Self::TouchCancelled,
            MobilePhoneDriveCancelReasonDto::InvalidContact => Self::InvalidContact,
            MobilePhoneDriveCancelReasonDto::ModeChanged => Self::ModeChanged,
            MobilePhoneDriveCancelReasonDto::LayoutChanged => Self::LayoutChanged,
            MobilePhoneDriveCancelReasonDto::Navigation => Self::Navigation,
            MobilePhoneDriveCancelReasonDto::SceneInactive => Self::SceneInactive,
            MobilePhoneDriveCancelReasonDto::ConnectionChanged => Self::ConnectionChanged,
            MobilePhoneDriveCancelReasonDto::RegionLeft => Self::RegionLeft,
            MobilePhoneDriveCancelReasonDto::MultipleContacts => Self::MultipleContacts,
            MobilePhoneDriveCancelReasonDto::ContactReplaced => Self::ContactReplaced,
            MobilePhoneDriveCancelReasonDto::GenerationExhausted => Self::GenerationExhausted,
        }
    }
}
impl From<core::PhoneDriveCancelReason> for MobilePhoneDriveCancelReasonDto {
    fn from(value: core::PhoneDriveCancelReason) -> Self {
        match value {
            core::PhoneDriveCancelReason::TouchCancelled => Self::TouchCancelled,
            core::PhoneDriveCancelReason::InvalidContact => Self::InvalidContact,
            core::PhoneDriveCancelReason::ModeChanged => Self::ModeChanged,
            core::PhoneDriveCancelReason::LayoutChanged => Self::LayoutChanged,
            core::PhoneDriveCancelReason::Navigation => Self::Navigation,
            core::PhoneDriveCancelReason::SceneInactive => Self::SceneInactive,
            core::PhoneDriveCancelReason::ConnectionChanged => Self::ConnectionChanged,
            core::PhoneDriveCancelReason::RegionLeft => Self::RegionLeft,
            core::PhoneDriveCancelReason::MultipleContacts => Self::MultipleContacts,
            core::PhoneDriveCancelReason::ContactReplaced => Self::ContactReplaced,
            core::PhoneDriveCancelReason::GenerationExhausted => Self::GenerationExhausted,
        }
    }
}

/// Complete native contact observation in the caller's retained input generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveContactsDto {
    /// No contacts remain.
    Released,
    /// One native contact and its hit-tested region.
    Single {
        /// Nonzero native contact identity.
        contact_id: u64,
        /// Hit-tested region; absent outside both regions.
        region: Option<MobilePhoneDriveActionDto>,
    },
    /// At least two contacts remain.
    Multiple,
}
impl From<MobilePhoneDriveContactsDto> for core::PhoneDriveContacts {
    fn from(value: MobilePhoneDriveContactsDto) -> Self {
        match value {
            MobilePhoneDriveContactsDto::Released => Self::Released,
            MobilePhoneDriveContactsDto::Multiple => Self::Multiple,
            MobilePhoneDriveContactsDto::Single { contact_id, region } => {
                match core::PhoneDriveContactId::new(contact_id) {
                    Some(contact) => Self::Single {
                        contact,
                        region: region.map(Into::into),
                    },
                    None => Self::Invalid,
                }
            }
        }
    }
}

/// Semantic feedback, never an armed state, write receipt, or motor outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveInputPhaseDto {
    /// No held contact.
    Idle,
    /// Contact remains inside its original region.
    Pressed {
        /// Region originally held by this contact.
        action: MobilePhoneDriveActionDto,
    },
    /// All contacts must lift before another gesture.
    Cancelled {
        /// Why this contact generation cannot continue.
        reason: MobilePhoneDriveCancelReasonDto,
    },
}

/// Missing prerequisites for live phone control.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveUnavailableReasonDto {
    /// No verified live connection.
    Disconnected,
    /// Current physical vehicle/route is not an e-skate candidate.
    VehicleUnsupported,
    /// No exact controller contract is qualified.
    ControllerUnqualified,
}

/// One canonical input and eligibility projection from the existing Rust owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneDriveSnapshotDto {
    /// Saved layout only.
    pub mode: MobilePhoneControlModeDto,
    /// Ephemeral input identity; never persisted.
    pub generation: Option<u64>,
    /// Contact feedback only.
    pub phase: MobilePhoneDriveInputPhaseDto,
    /// Production drive remains unavailable until an exact profile is qualified.
    pub unavailability: MobilePhoneDriveUnavailableReasonDto,
}

fn snapshot(owner: &cutout_protocols::DeviceConnectionSession) -> MobilePhoneDriveSnapshotDto {
    let input = owner.phone_drive_input_snapshot();
    MobilePhoneDriveSnapshotDto {
        mode: input.mode.into(),
        generation: input.generation,
        phase: match input.phase {
            core::PhoneDriveInputPhase::Idle => MobilePhoneDriveInputPhaseDto::Idle,
            core::PhoneDriveInputPhase::Pressed(action) => MobilePhoneDriveInputPhaseDto::Pressed {
                action: action.into(),
            },
            core::PhoneDriveInputPhase::Cancelled(reason) => {
                MobilePhoneDriveInputPhaseDto::Cancelled {
                    reason: reason.into(),
                }
            }
        },
        unavailability: match owner.phone_drive_unavailability() {
            core::PhoneDriveUnavailableReason::Disconnected => {
                MobilePhoneDriveUnavailableReasonDto::Disconnected
            }
            core::PhoneDriveUnavailableReason::VehicleUnsupported => {
                MobilePhoneDriveUnavailableReasonDto::VehicleUnsupported
            }
            core::PhoneDriveUnavailableReason::ControllerUnqualified => {
                MobilePhoneDriveUnavailableReasonDto::ControllerUnqualified
            }
        },
    }
}

/// Durable preference failure; no native fallback value is substituted.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobilePhoneControlError {
    /// Session was created without the existing database service.
    #[error("phone input preferences require the ride database")]
    PersistenceUnavailable,
    /// Existing database service rejected the operation.
    #[error("phone input preference storage failed: {error}")]
    Persistence { error: MobileRideDatabaseError },
}

#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns boundary records."
)]
impl CutoutSessionStateHandle {
    /// Reads canonical semantic feedback without emitting transport work.
    #[must_use]
    pub fn phone_drive_snapshot(&self) -> MobilePhoneDriveSnapshotDto {
        snapshot(&self.lock_inner())
    }

    /// Loads only saved layout; an old contact or activation can never be restored.
    ///
    /// # Errors
    /// Returns unavailable database or the actual storage failure.
    pub fn load_phone_control_mode(
        &self,
    ) -> Result<MobilePhoneDriveSnapshotDto, MobilePhoneControlError> {
        let _ordering = self
            .configuration_updates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mode = self
            .diagnostic_database
            .as_ref()
            .ok_or(MobilePhoneControlError::PersistenceUnavailable)?
            .phone_control_mode()
            .map_err(|error| MobilePhoneControlError::Persistence {
                error: crate::map_ride_database_error(error),
            })?;
        let mut owner = self.lock_inner();
        owner.set_phone_control_mode(mode);
        Ok(snapshot(&owner))
    }

    /// Saves layout through the existing database worker before publishing it.
    /// Cancellation is retained even when persistence fails.
    ///
    /// # Errors
    /// Returns unavailable database or the actual storage failure.
    pub fn save_phone_control_mode(
        &self,
        mode: MobilePhoneControlModeDto,
    ) -> Result<MobilePhoneDriveSnapshotDto, MobilePhoneControlError> {
        let _ordering = self
            .configuration_updates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mode = mode.into();
        {
            let mut owner = self.lock_inner();
            if owner.phone_drive_input_snapshot().mode != mode {
                owner.cancel_phone_drive(core::PhoneDriveCancelReason::ModeChanged);
            }
        }
        self.diagnostic_database
            .as_ref()
            .ok_or(MobilePhoneControlError::PersistenceUnavailable)?
            .save_phone_control_mode(mode)
            .map_err(|error| MobilePhoneControlError::Persistence {
                error: crate::map_ride_database_error(error),
            })?;
        let mut owner = self.lock_inner();
        owner.set_phone_control_mode(mode);
        Ok(snapshot(&owner))
    }

    /// Observes complete contact state for its original connection and input generation.
    #[must_use]
    pub fn observe_phone_drive_contacts(
        &self,
        token: MobileConnectionAttemptTokenDto,
        generation: u64,
        contacts: MobilePhoneDriveContactsDto,
    ) -> MobilePhoneDriveSnapshotDto {
        let mut owner = self.lock_inner();
        owner.observe_phone_drive_contacts(&token.into(), generation, contacts.into());
        snapshot(&owner)
    }

    /// Retires semantic input without claiming a controller stopped or applied braking.
    #[must_use]
    pub fn cancel_phone_drive(
        &self,
        reason: MobilePhoneDriveCancelReasonDto,
    ) -> MobilePhoneDriveSnapshotDto {
        let mut owner = self.lock_inner();
        owner.cancel_phone_drive(reason.into());
        snapshot(&owner)
    }
}
