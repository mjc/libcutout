//! Shared device-action descriptors, invocation, and progress normalization.

use cutout_core::{
    CommandKind, DeviceActionId, DeviceActionProgress, DeviceCommand, Measured, SettingsEntry,
    SettingsReadback,
};

use crate::{AERO_FIELD_GYRO_CALIBRATION_STATE, DeviceControlProfile};

/// How a native client should present an action trigger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionRole {
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
    /// Static write evidence.
    pub access: ActionAccess,
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
                        access: if validation_mode || self.verified.supports_command_kind(kind) {
                            ActionAccess::Available
                        } else {
                            ActionAccess::Unverified
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
        id: DeviceActionId,
        validation_mode: bool,
    ) -> Result<DeviceCommand, ActionRequestError> {
        let descriptor = self
            .action_descriptors(validation_mode)
            .into_iter()
            .find(|descriptor| descriptor.id == id)
            .ok_or(ActionRequestError::Unavailable)?;
        if descriptor.access == ActionAccess::Unverified {
            return Err(ActionRequestError::Unverified);
        }
        Ok(match id {
            DeviceActionId::ResetTripMeter => DeviceCommand::ResetTripMeter,
            DeviceActionId::GyroCalibration => DeviceCommand::SetAeroGyroCalibration,
        })
    }

    /// Converts action progress without interpreting it as a numeric setting.
    #[must_use]
    pub fn normalize_action_readback(self, readback: SettingsReadback) -> Vec<ActionObservation> {
        if !self
            .available
            .supports_command_kind(CommandKind::SetAeroGyroCalibration)
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
        DeviceActionId::ResetTripMeter,
        "actions.trip_meter_reset.label",
        "actions.trip_meter_reset.help",
        0,
        ActionRole::Destructive,
        ActionConfirmation::None,
    ),
    (
        DeviceActionId::GyroCalibration,
        "actions.gyro_calibration.label",
        "actions.gyro_calibration.help",
        1,
        ActionRole::Procedure,
        ActionConfirmation::ProgressReadback,
    ),
];

const fn action_kind(id: DeviceActionId) -> CommandKind {
    match id {
        DeviceActionId::ResetTripMeter => CommandKind::ResetTripMeter,
        DeviceActionId::GyroCalibration => CommandKind::SetAeroGyroCalibration,
    }
}
