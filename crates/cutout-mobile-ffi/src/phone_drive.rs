//! Typed phone control facade; the shared Rust session owns admission and effects.

use crate::{CutoutSessionStateHandle, MobileConnectionAttemptTokenDto, MobileRideDatabaseError};
use cutout_core as core;
use libcutout_persistence as persistence;

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
    /// Same-connection vehicle or drivetrain configuration changed.
    ConfigurationChanged,
    /// Explicit activation/deactivation changed the input identity.
    ActivationChanged,
    /// Held-contact heartbeat expired.
    InputExpired,
    /// Native submission failed or its outcome is unknown.
    TransportUnavailable,
    /// The controller reported a fault.
    ControllerFault,
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
            MobilePhoneDriveCancelReasonDto::ConfigurationChanged => Self::ConfigurationChanged,
            MobilePhoneDriveCancelReasonDto::ActivationChanged => Self::ActivationChanged,
            MobilePhoneDriveCancelReasonDto::InputExpired => Self::InputExpired,
            MobilePhoneDriveCancelReasonDto::TransportUnavailable => Self::TransportUnavailable,
            MobilePhoneDriveCancelReasonDto::ControllerFault => Self::ControllerFault,
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
            core::PhoneDriveCancelReason::ConfigurationChanged => Self::ConfigurationChanged,
            core::PhoneDriveCancelReason::ActivationChanged => Self::ActivationChanged,
            core::PhoneDriveCancelReason::InputExpired => Self::InputExpired,
            core::PhoneDriveCancelReason::TransportUnavailable => Self::TransportUnavailable,
            core::PhoneDriveCancelReason::ControllerFault => Self::ControllerFault,
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
}

/// Availability never implies activation from a saved preference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveAvailabilityDto {
    /// Connection or vehicle does not support this control path.
    Unavailable {
        /// Why this path is unavailable.
        reason: MobilePhoneDriveUnavailableReasonDto,
    },
    /// Rider may explicitly activate the standard VESC path.
    Ready,
    /// Current connection was explicitly activated.
    Active {
        /// Ephemeral activation identity.
        activation_id: u64,
    },
}

/// Saved rider-selected current requests, independent of activation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneDriveLimitsDto {
    /// Positive throttle-current budget.
    pub throttle_milliamps: u32,
    /// Positive braking-current magnitude.
    pub brake_milliamps: u32,
}
impl From<core::PhoneDriveLimits> for MobilePhoneDriveLimitsDto {
    fn from(value: core::PhoneDriveLimits) -> Self {
        Self {
            throttle_milliamps: value.throttle_milliamps(),
            brake_milliamps: value.brake_milliamps(),
        }
    }
}

/// Admission kind for the existing Bluetooth writer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveWriteKindDto {
    /// Fresh held-current request.
    Setpoint,
    /// Bounded zero motor-current request.
    Release,
}

/// Exact live-control intent revalidated immediately before native submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneDriveWriteDto {
    /// Ephemeral activation identity.
    pub activation_id: u64,
    /// Supersession sequence.
    pub sequence: u64,
    /// Original contact generation.
    pub generation: u64,
    /// Exclusive monotonic submission deadline.
    pub submit_before_ms: u64,
    /// Setpoint or release intent.
    pub kind: MobilePhoneDriveWriteKindDto,
}
impl From<core::PhoneDriveWrite> for MobilePhoneDriveWriteDto {
    fn from(value: core::PhoneDriveWrite) -> Self {
        Self {
            activation_id: value.activation_id.get(),
            sequence: value.sequence,
            generation: value.generation,
            submit_before_ms: value.submit_before_ms,
            kind: match value.kind {
                core::PhoneDriveWriteKind::Setpoint => MobilePhoneDriveWriteKindDto::Setpoint,
                core::PhoneDriveWriteKind::Release => MobilePhoneDriveWriteKindDto::Release,
            },
        }
    }
}
impl MobilePhoneDriveWriteDto {
    fn checked(self) -> Option<core::PhoneDriveWrite> {
        Some(core::PhoneDriveWrite {
            activation_id: core::PhoneDriveActivationId::new(self.activation_id)?,
            sequence: self.sequence,
            generation: self.generation,
            submit_before_ms: self.submit_before_ms,
            kind: match self.kind {
                MobilePhoneDriveWriteKindDto::Setpoint => core::PhoneDriveWriteKind::Setpoint,
                MobilePhoneDriveWriteKindDto::Release => core::PhoneDriveWriteKind::Release,
            },
        })
    }
}

/// Native evidence for a drive write; never proof of motor application.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveTransportStatusDto {
    /// Native queue retained the write without submitting it.
    Queued,
    /// Submitted to the host peripheral API.
    Submitted,
    /// Rejected before submission.
    Rejected,
    /// Cancelled before submission.
    Cancelled,
    /// Submission outcome cannot be established.
    SubmissionUnknown,
}
impl From<MobilePhoneDriveTransportStatusDto> for core::ApplicationQueryTransportStatus {
    fn from(value: MobilePhoneDriveTransportStatusDto) -> Self {
        match value {
            MobilePhoneDriveTransportStatusDto::Queued => Self::Queued,
            MobilePhoneDriveTransportStatusDto::Submitted => Self::Submitted,
            MobilePhoneDriveTransportStatusDto::Rejected => Self::Rejected,
            MobilePhoneDriveTransportStatusDto::Cancelled => Self::Cancelled,
            MobilePhoneDriveTransportStatusDto::SubmissionUnknown => Self::SubmissionUnknown,
        }
    }
}

/// One canonical input and eligibility projection from the existing Rust owner.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePhoneDriveSnapshotDto {
    /// Stable platform identifier only when settings were loaded for the current attempt.
    pub device_identity: Option<String>,
    /// Saved layout only.
    pub mode: MobilePhoneControlModeDto,
    /// Ephemeral input identity; never persisted.
    pub generation: Option<u64>,
    /// Contact feedback only.
    pub phase: MobilePhoneDriveInputPhaseDto,
    /// Supported path and explicit per-attempt activation.
    pub availability: MobilePhoneDriveAvailabilityDto,
    /// Saved current budgets; never an arming state.
    pub limits: MobilePhoneDriveLimitsDto,
}

fn snapshot(owner: &cutout_protocols::DeviceConnectionSession) -> MobilePhoneDriveSnapshotDto {
    let input = owner.phone_drive_input_snapshot();
    let current_token = owner.snapshot().connection.token;
    MobilePhoneDriveSnapshotDto {
        device_identity: current_token
            .as_ref()
            .filter(|token| owner.phone_drive_preferences_are_current(token))
            .map(|token| token.platform_identifier().to_owned()),
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
        availability: match owner.phone_drive_availability() {
            core::PhoneDriveAvailability::Unavailable(reason) => {
                MobilePhoneDriveAvailabilityDto::Unavailable {
                    reason: match reason {
                        core::PhoneDriveUnavailableReason::Disconnected => {
                            MobilePhoneDriveUnavailableReasonDto::Disconnected
                        }
                        core::PhoneDriveUnavailableReason::VehicleUnsupported => {
                            MobilePhoneDriveUnavailableReasonDto::VehicleUnsupported
                        }
                    },
                }
            }
            core::PhoneDriveAvailability::Ready => MobilePhoneDriveAvailabilityDto::Ready,
            core::PhoneDriveAvailability::Active(id) => MobilePhoneDriveAvailabilityDto::Active {
                activation_id: id.get(),
            },
        },
        limits: owner.session_state().phone_drive_limits.into(),
    }
}

/// Durable preference failure; no native fallback value is substituted.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobilePhoneControlError {
    /// Current fields failed checked Rust parsing or range validation.
    #[error("enter a current from 0.001 to 500 A for each control")]
    InvalidLimits,
    /// The caller's exact connection attempt is no longer current.
    #[error("phone settings belong to a replaced device connection")]
    DeviceChanged,
    /// Input was cancelled while a preference write was in progress.
    #[error("phone input changed while saving settings")]
    InputGenerationChanged,
    /// The active device is not an eligible VESC e-skate ride.
    #[error("phone settings require an eligible e-skate ride")]
    VehicleUnsupported,
    /// Saving is permitted only from this attempt's Ride settings sheet.
    #[error("phone settings sheet is not open for this device")]
    SettingsNotPresented,
    /// Activation refused on the current connection.
    #[error("phone activation refused: {reason:?}")]
    Activation {
        /// Typed refusal from the existing session owner.
        reason: MobilePhoneDriveActivationRefusalDto,
    },
    /// Session was created without the existing database service.
    #[error("phone input preferences require the ride database")]
    PersistenceUnavailable,
    /// Existing database service rejected the operation.
    #[error("phone input preference storage failed: {error}")]
    Persistence { error: MobileRideDatabaseError },
}

/// Explicit activation refusal, independent of storage errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePhoneDriveActivationRefusalDto {
    /// The current VESC telemetry reports a controller fault.
    ControllerFault,
    /// No current verified connection.
    ConnectionUnavailable,
    /// Wrong vehicle/protocol/application route.
    VehicleUnsupported,
    /// Saved mode is disabled.
    ModeDisabled,
    /// A contact is already held.
    InputHeld,
    /// A conflicting operation is active.
    Busy,
    /// The saved device preferences were not loaded for this exact attempt.
    SettingsNotLoaded,
    /// Clock moved backwards.
    ClockRegression,
    /// The requested activation belongs to a retired input generation.
    InputGenerationChanged,
    /// Activation identity exhausted.
    IdentifierExhausted,
}
impl From<cutout_protocols::PhoneDriveActivationError> for MobilePhoneDriveActivationRefusalDto {
    fn from(value: cutout_protocols::PhoneDriveActivationError) -> Self {
        use cutout_protocols::PhoneDriveActivationError as E;
        match value {
            E::ControllerFault => Self::ControllerFault,
            E::ConnectionUnavailable => Self::ConnectionUnavailable,
            E::VehicleUnsupported => Self::VehicleUnsupported,
            E::ModeDisabled => Self::ModeDisabled,
            E::InputHeld => Self::InputHeld,
            E::Busy => Self::Busy,
            E::SettingsNotLoaded => Self::SettingsNotLoaded,
            E::ClockRegression => Self::ClockRegression,
            E::InputGenerationChanged => Self::InputGenerationChanged,
            E::IdentifierExhausted => Self::IdentifierExhausted,
        }
    }
}

fn validate_phone_drive_target(
    owner: &cutout_protocols::DeviceConnectionSession,
    token: &MobileConnectionAttemptTokenDto,
    attempt: &core::ConnectionAttemptToken,
) -> Result<(), MobilePhoneControlError> {
    if owner.snapshot().connection.token.as_ref() != Some(attempt) {
        return Err(MobilePhoneControlError::DeviceChanged);
    }
    if !owner.phone_drive_target_is_current(attempt) {
        return Err(MobilePhoneControlError::VehicleUnsupported);
    }
    if attempt.platform_identifier() != token.platform_identifier {
        return Err(MobilePhoneControlError::DeviceChanged);
    }
    Ok(())
}

fn load_phone_drive_preferences_with(
    handle: &CutoutSessionStateHandle,
    token: MobileConnectionAttemptTokenDto,
    load: impl FnOnce(&str) -> Result<persistence::PhoneDriveSettingsRecord, MobilePhoneControlError>,
) -> Result<MobilePhoneDriveSnapshotDto, MobilePhoneControlError> {
    let _ordering = handle
        .configuration_updates
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let attempt: core::ConnectionAttemptToken = token.clone().into();
    {
        let owner = handle.lock_inner();
        validate_phone_drive_target(&owner, &token, &attempt)?;
    }
    let preferences = load(&token.platform_identifier)?;
    let mut owner = handle.lock_inner();
    validate_phone_drive_target(&owner, &token, &attempt)?;
    if !owner.apply_phone_drive_preferences(&attempt, preferences.mode(), preferences.limits()) {
        return Err(MobilePhoneControlError::DeviceChanged);
    }
    Ok(snapshot(&owner))
}

fn save_phone_drive_preferences_with(
    handle: &CutoutSessionStateHandle,
    token: MobileConnectionAttemptTokenDto,
    mode: MobilePhoneControlModeDto,
    throttle_amps_text: String,
    brake_amps_text: String,
    save: impl FnOnce(
        &str,
        persistence::PhoneDriveSettingsRecord,
    ) -> Result<(), MobilePhoneControlError>,
) -> Result<MobilePhoneDriveSnapshotDto, MobilePhoneControlError> {
    let _ordering = handle
        .configuration_updates
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let attempt: core::ConnectionAttemptToken = token.clone().into();
    let expected_input_generation = {
        let mut owner = handle.lock_inner();
        validate_phone_drive_target(&owner, &token, &attempt)?;
        if !owner.phone_drive_settings_are_presented(&attempt) {
            return Err(MobilePhoneControlError::SettingsNotPresented);
        }
        if !owner.phone_drive_preferences_are_current(&attempt) {
            return Err(MobilePhoneControlError::DeviceChanged);
        }
        owner.cancel_phone_drive(core::PhoneDriveCancelReason::ActivationChanged);
        owner
            .phone_drive_input_snapshot()
            .generation
            .ok_or(MobilePhoneControlError::DeviceChanged)?
    };
    let limits = core::PhoneDriveLimits::from_amp_text(&throttle_amps_text, &brake_amps_text)
        .map_err(|_| MobilePhoneControlError::InvalidLimits)?;
    let mode = mode.into();
    save(
        &token.platform_identifier,
        persistence::PhoneDriveSettingsRecord::new(mode, limits),
    )?;
    let mut owner = handle.lock_inner();
    validate_phone_drive_target(&owner, &token, &attempt)?;
    if !owner.phone_drive_settings_are_presented(&attempt) {
        return Err(MobilePhoneControlError::SettingsNotPresented);
    }
    if owner.phone_drive_input_snapshot().generation != Some(expected_input_generation) {
        return Err(MobilePhoneControlError::InputGenerationChanged);
    }
    if !owner.apply_phone_drive_preferences(&attempt, mode, limits) {
        return Err(MobilePhoneControlError::DeviceChanged);
    }
    Ok(snapshot(&owner))
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

    /// Loads mode and current limits for the exact eligible connection attempt.
    ///
    /// # Errors
    /// Returns target, availability, database, or stored-value errors.
    pub fn load_phone_drive_preferences(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> Result<MobilePhoneDriveSnapshotDto, MobilePhoneControlError> {
        let database = self
            .diagnostic_database
            .as_ref()
            .ok_or(MobilePhoneControlError::PersistenceUnavailable)?;
        load_phone_drive_preferences_with(self, token, |identity| {
            database.phone_drive_preferences(identity).map_err(|error| {
                MobilePhoneControlError::Persistence {
                    error: crate::map_ride_database_error(error),
                }
            })
        })
    }

    /// Saves mode and both limits atomically for this open Ride settings sheet.
    ///
    /// # Errors
    /// Returns target, validation, database, or sheet-ownership errors.
    pub fn save_phone_drive_preferences(
        &self,
        token: MobileConnectionAttemptTokenDto,
        mode: MobilePhoneControlModeDto,
        throttle_amps_text: String,
        brake_amps_text: String,
    ) -> Result<MobilePhoneDriveSnapshotDto, MobilePhoneControlError> {
        let database = self
            .diagnostic_database
            .as_ref()
            .ok_or(MobilePhoneControlError::PersistenceUnavailable)?;
        save_phone_drive_preferences_with(
            self,
            token,
            mode,
            throttle_amps_text,
            brake_amps_text,
            |identity, record| {
                database
                    .save_phone_drive_preferences(identity, record)
                    .map_err(|error| MobilePhoneControlError::Persistence {
                        error: crate::map_ride_database_error(error),
                    })
            },
        )
    }

    /// Reports presentation of the existing Ride settings sheet to the Rust input owner.
    ///
    /// # Errors
    /// Returns when the token no longer identifies the eligible Ride.
    pub fn set_phone_drive_settings_presented(
        &self,
        token: MobileConnectionAttemptTokenDto,
        presented: bool,
    ) -> Result<MobilePhoneDriveSnapshotDto, MobilePhoneControlError> {
        let mut owner = self.lock_inner();
        let attempt: core::ConnectionAttemptToken = token.clone().into();
        validate_phone_drive_target(&owner, &token, &attempt)?;
        if !owner.set_phone_drive_settings_presented(&attempt, presented) {
            return Err(MobilePhoneControlError::DeviceChanged);
        }
        Ok(snapshot(&owner))
    }

    /// Explicitly activates the current supported connection with saved current budgets.
    ///
    /// # Errors
    /// Returns the session owner's typed refusal.
    pub fn activate_phone_drive(
        &self,
        token: MobileConnectionAttemptTokenDto,
        expected_input_generation: u64,
        monotonic_ms: u64,
    ) -> Result<MobilePhoneDriveSnapshotDto, MobilePhoneControlError> {
        let _ordering = self
            .configuration_updates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut owner = self.lock_inner();
        let limits = owner.session_state().phone_drive_limits;
        owner
            .activate_phone_drive(
                &token.into(),
                expected_input_generation,
                limits,
                core::MonotonicTimestamp::new(monotonic_ms),
            )
            .map_err(|reason| MobilePhoneControlError::Activation {
                reason: reason.into(),
            })?;
        Ok(snapshot(&owner))
    }

    /// Delivers actual contact-observation time separately from queue receipt time.
    #[must_use]
    pub fn observe_phone_drive_contacts_at(
        &self,
        token: MobileConnectionAttemptTokenDto,
        generation: u64,
        contacts: MobilePhoneDriveContactsDto,
        observed_at_ms: u64,
        monotonic_ms: u64,
    ) -> Option<crate::MobileDeviceSessionStepDto> {
        self.lock_inner()
            .observe_phone_drive_contacts_at(
                &token.into(),
                generation,
                contacts.into(),
                core::MonotonicTimestamp::new(observed_at_ms),
                core::MonotonicTimestamp::new(monotonic_ms),
            )
            .map(Into::into)
    }

    /// Advances the shared drive deadline/cadence without renewing a contact heartbeat.
    #[must_use]
    pub fn tick_phone_drive(
        &self,
        token: MobileConnectionAttemptTokenDto,
        monotonic_ms: u64,
        can_submit: bool,
    ) -> Option<crate::MobileDeviceSessionStepDto> {
        self.lock_inner()
            .tick_phone_drive(
                &token.into(),
                core::MonotonicTimestamp::new(monotonic_ms),
                can_submit,
            )
            .map(Into::into)
    }

    /// Rechecks exact intent identity immediately before the sole native writer submits.
    #[must_use]
    pub fn phone_drive_transport_is_current(
        &self,
        token: MobileConnectionAttemptTokenDto,
        write: MobilePhoneDriveWriteDto,
        monotonic_ms: u64,
    ) -> bool {
        let Some(write) = write.checked() else {
            return false;
        };
        self.lock_inner().phone_drive_transport_is_current(
            &token.into(),
            write,
            core::MonotonicTimestamp::new(monotonic_ms),
        )
    }

    /// Records native submission evidence without claiming motor application.
    #[must_use]
    pub fn mark_phone_drive_transport(
        &self,
        token: MobileConnectionAttemptTokenDto,
        write: MobilePhoneDriveWriteDto,
        status: MobilePhoneDriveTransportStatusDto,
        monotonic_ms: u64,
    ) -> bool {
        let Some(write) = write.checked() else {
            return false;
        };
        self.lock_inner().mark_phone_drive_transport(
            &token.into(),
            write,
            status.into(),
            core::MonotonicTimestamp::new(monotonic_ms),
        )
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

    /// Cancels at the current host time so the pending neutral has a fresh bounded deadline.
    #[must_use]
    pub fn cancel_phone_drive_at(
        &self,
        reason: MobilePhoneDriveCancelReasonDto,
        monotonic_ms: u64,
    ) -> MobilePhoneDriveSnapshotDto {
        let mut owner = self.lock_inner();
        owner.cancel_phone_drive_at(reason.into(), core::MonotonicTimestamp::new(monotonic_ms));
        snapshot(&owner)
    }
}
