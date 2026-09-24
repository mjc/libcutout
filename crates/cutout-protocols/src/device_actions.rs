//! Shared device-action descriptors, invocation, and progress normalization.

use cutout_core::{
    CommandKind, DeviceActionId, DeviceActionProgress, DeviceActionRequest, DeviceActionStep,
    DeviceActionsState, DeviceCommand, Measured, MonotonicTimestamp, SettingsEntry,
    SettingsReadback, VerificationStatus,
};

use crate::{AERO_FIELD_GYRO_CALIBRATION_STATE, DeviceControlProfile};

/// How a native client should present an action trigger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionRole {
    /// The action is an immediate momentary control.
    Momentary,
    /// The action clears device data and requires confirmation.
    Destructive,
    /// The action starts or advances a physical procedure.
    Procedure,
}

/// Evidence available after invoking an action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionConfirmation {
    /// No device state is currently available to confirm completion.
    None,
    /// The device reports explicit progress phases.
    ProgressReadback,
}

/// Static action availability before live session guards run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionAccess {
    /// The current mode permits invocation; live session guards still apply.
    Available,
    /// The encoder exists but use requires explicit validation mode.
    Unverified,
}

/// Native-neutral action definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionDescriptor {
    /// Stable action identity.
    pub id: DeviceActionId,
    /// Shared localization key for the action label.
    pub label_key: &'static str,
    /// Shared localization key for procedure or consequence text.
    pub help_key: &'static str,
    /// Stable display order among actions.
    pub order: u16,
    /// Presentation and confirmation behavior.
    pub role: ActionRole,
    /// Static invocation availability.
    pub access: ActionAccess,
    /// Write evidence, independent of production availability and progress evidence.
    pub write_verification: VerificationStatus,
    /// Device evidence available after invocation.
    pub confirmation: ActionConfirmation,
}

/// A semantic action request rejected before live session checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ActionRequestError {
    /// The selected protocol does not expose this action.
    #[error("action unavailable")]
    Unavailable,
    /// The action requires explicit validation mode.
    #[error("action is unverified")]
    Unverified,
    /// The procedure step does not belong to the requested action.
    #[error("action procedure step is invalid")]
    InvalidStep,
}

/// A present action-progress field with its original evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionObservation {
    /// Stable action identity.
    pub id: DeviceActionId,
    /// Reported progress, or explicit unknown for an unrecognized phase.
    pub progress: Option<Measured<DeviceActionProgress>>,
}

impl DeviceControlProfile {
    /// Returns actions supported by the selected protocol profile.
    #[must_use]
    pub fn action_descriptors(self, validation_mode: bool) -> Vec<ActionDescriptor> {
        ACTIONS
            .iter()
            .filter_map(|&(id, label_key, help_key, order, role, confirmation)| {
                let kind = action_kind(id);
                self.available
                    .supports_command_kind(kind)
                    .then_some(ActionDescriptor {
                        id,
                        label_key,
                        help_key,
                        order,
                        role,
                        access: if validation_mode || self.production.supports_command_kind(kind) {
                            ActionAccess::Available
                        } else {
                            ActionAccess::Unverified
                        },
                        write_verification: if self.verified.supports_command_kind(kind) {
                            VerificationStatus::HardwareVerified
                        } else {
                            VerificationStatus::Unverified
                        },
                        confirmation,
                    })
            })
            .collect()
    }

    /// Resolves a semantic action through the selected protocol profile.
    ///
    /// # Errors
    /// Rejects actions unavailable to the profile or disallowed outside validation mode.
    pub fn action_command(
        self,
        request: DeviceActionRequest,
        validation_mode: bool,
    ) -> Result<DeviceCommand, ActionRequestError> {
        if !valid_action_step(request) {
            return Err(ActionRequestError::InvalidStep);
        }
        let descriptor = self
            .action_descriptors(validation_mode)
            .into_iter()
            .find(|descriptor| descriptor.id == request.id)
            .ok_or(ActionRequestError::Unavailable)?;
        if descriptor.access == ActionAccess::Unverified {
            return Err(ActionRequestError::Unverified);
        }
        Ok(match request.id {
            DeviceActionId::Horn => DeviceCommand::SoundHorn,
            DeviceActionId::ResetTripMeter | DeviceActionId::GyroCalibration => {
                DeviceCommand::InvokeAction(request)
            }
        })
    }

    /// Converts action progress without interpreting it as a numeric setting.
    #[must_use]
    pub fn normalize_action_readback(self, readback: SettingsReadback) -> Vec<ActionObservation> {
        if !self
            .available
            .supports_command_kind(CommandKind::GyroCalibration)
        {
            return Vec::new();
        }
        readback
            .entries()
            .into_iter()
            .flatten()
            .filter(|entry| entry.field.id == AERO_FIELD_GYRO_CALIBRATION_STATE)
            .map(normalize_gyro_progress)
            .collect()
    }

    /// Applies normalized action progress to the shared lifecycle owner.
    pub fn apply_action_readback(
        self,
        state: &mut DeviceActionsState,
        readback: SettingsReadback,
        observed_at: MonotonicTimestamp,
    ) {
        for observation in self.normalize_action_readback(readback) {
            if let Some(progress) = observation.progress {
                state.observe_measured(observation.id, progress, observed_at);
            } else {
                state.invalidate_progress(observation.id, observed_at);
            }
        }
    }
}

const fn valid_action_step(request: DeviceActionRequest) -> bool {
    match (request.id, request.step) {
        (DeviceActionId::Horn | DeviceActionId::ResetTripMeter, DeviceActionStep::Invoke)
        | (
            DeviceActionId::GyroCalibration,
            DeviceActionStep::PrepareGyroCalibration | DeviceActionStep::StartGyroCalibration,
        ) => true,
        _ => false,
    }
}

fn normalize_gyro_progress(entry: SettingsEntry) -> ActionObservation {
    let progress = match entry.field.value {
        1 => Some(DeviceActionProgress::AdjustingAttitude),
        2 => Some(DeviceActionProgress::ReadyToCalibrate),
        _ => None,
    };
    ActionObservation {
        id: DeviceActionId::GyroCalibration,
        progress: progress.map(|value| Measured {
            value,
            source: entry.source,
            quality: entry.quality,
            verification: entry.verification,
        }),
    }
}

const ACTIONS: &[(
    DeviceActionId,
    &str,
    &str,
    u16,
    ActionRole,
    ActionConfirmation,
)] = &[
    (
        DeviceActionId::Horn,
        "actions.horn.label",
        "actions.horn.help",
        0,
        ActionRole::Momentary,
        ActionConfirmation::None,
    ),
    (
        DeviceActionId::ResetTripMeter,
        "actions.trip_meter_reset.label",
        "actions.trip_meter_reset.help",
        1,
        ActionRole::Destructive,
        ActionConfirmation::None,
    ),
    (
        DeviceActionId::GyroCalibration,
        "actions.gyro_calibration.label",
        "actions.gyro_calibration.help",
        2,
        ActionRole::Procedure,
        ActionConfirmation::ProgressReadback,
    ),
];

const fn action_kind(id: DeviceActionId) -> CommandKind {
    match id {
        DeviceActionId::Horn => CommandKind::SoundHorn,
        DeviceActionId::ResetTripMeter => CommandKind::ResetTripMeter,
        DeviceActionId::GyroCalibration => CommandKind::GyroCalibration,
    }
}
