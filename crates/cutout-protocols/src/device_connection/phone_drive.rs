//! Explicit local VESC control through the existing connection and transport owners.

use cutout_core::{
    ApplicationQueryTransportStatus, ConnectionAttemptToken, MonotonicTimestamp, PhoneControlMode,
    PhoneDriveAction, PhoneDriveActivationId, PhoneDriveAvailability, PhoneDriveCancelReason,
    PhoneDriveContacts, PhoneDriveInputPhase, PhoneDriveInputSnapshot, PhoneDriveLimits,
    PhoneDriveUnavailableReason, PhoneDriveWrite, PhoneDriveWriteKind, ProtocolFamily,
    SessionOutput, TransportAction, VehicleKind, WriteMode,
};

use super::{DeviceConnectionSession, DeviceConnectionStep};

const CADENCE_MS: u64 = 50;
const INPUT_MAX_AGE_MS: u64 = 150;
const SUBMISSION_WINDOW_MS: u64 = 50;

#[cfg(test)]
#[path = "phone_drive_tests.rs"]
mod tests;

/// An explicit phone-control activation refused before motor commands are admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PhoneDriveActivationError {
    /// The connection attempt is no longer current and verified.
    #[error("connection unavailable")]
    ConnectionUnavailable,
    /// The route does not support standard local VESC phone commands.
    #[error("vehicle unsupported")]
    VehicleUnsupported,
    /// No saved settings have been loaded for this exact connection attempt.
    #[error("phone settings are not loaded for this connection")]
    SettingsNotLoaded,
    /// Phone control is disabled in the saved layout.
    #[error("phone control mode disabled")]
    ModeDisabled,
    /// Activation requires an observed complete release after any cancellation.
    #[error("lift all contacts before activating")]
    InputHeld,
    /// A settings operation already owns the transport.
    #[error("another device operation is active")]
    Busy,
    /// The caller's host clock moved backwards.
    #[error("phone control clock moved backwards")]
    ClockRegression,
    /// No unused activation identifier remains.
    #[error("phone control activation identifier exhausted")]
    IdentifierExhausted,
    /// The controller currently reports a fault.
    #[error("controller reports a fault")]
    ControllerFault,
}

#[derive(Clone, Copy, Debug)]
struct Activation {
    id: PhoneDriveActivationId,
    limits: PhoneDriveLimits,
    observed_at: Option<u64>,
    next_setpoint_at: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct PendingWrite {
    admission: PhoneDriveWrite,
    action: Option<PhoneDriveAction>,
    milliamps: u32,
    issued_at: u64,
    emitted: bool,
}

#[derive(Debug, Default)]
pub(super) struct PhoneDriveControl {
    activation: Option<Activation>,
    pending: Option<PendingWrite>,
    next_activation: u64,
    next_sequence: u64,
    controller_fault: bool,
}

impl DeviceConnectionSession {
    /// Reports standard VESC route support separately from explicit activation.
    #[must_use]
    pub fn phone_drive_availability(&self) -> PhoneDriveAvailability {
        let connection = self.state.connection.snapshot();
        let Some(token) = &connection.token else {
            return PhoneDriveAvailability::Unavailable(PhoneDriveUnavailableReason::Disconnected);
        };
        if !self.state.connection.is_verified(token) {
            return PhoneDriveAvailability::Unavailable(PhoneDriveUnavailableReason::Disconnected);
        }
        match self.device.as_ref().map(crate::DeviceSession::identity) {
            Some(identity)
                if identity.protocol == ProtocolFamily::Vesc
                    && identity.vehicle_kind() == VehicleKind::Eskate
                    && identity.vesc_application
                        == Some(crate::VescApplicationDiscovery::Absent) =>
            {
                self.phone_drive_activation().map_or(
                    PhoneDriveAvailability::Ready,
                    PhoneDriveAvailability::Active,
                )
            }
            Some(_) => {
                PhoneDriveAvailability::Unavailable(PhoneDriveUnavailableReason::VehicleUnsupported)
            }
            None => PhoneDriveAvailability::Unavailable(PhoneDriveUnavailableReason::Disconnected),
        }
    }

    /// Returns the current explicit activation, independent from saved layout.
    #[must_use]
    pub fn phone_drive_activation(&self) -> Option<PhoneDriveActivationId> {
        self.phone_drive_control
            .activation
            .map(|activation| activation.id)
    }

    /// Activates local standard VESC phone commands for one current attempt.
    ///
    /// # Errors
    /// Returns a typed refusal without activating unavailable routes or held input.
    pub fn activate_phone_drive(
        &mut self,
        token: &ConnectionAttemptToken,
        limits: PhoneDriveLimits,
        at: MonotonicTimestamp,
    ) -> Result<PhoneDriveActivationId, PhoneDriveActivationError> {
        if !self.state.connection.is_verified(token) {
            return Err(PhoneDriveActivationError::ConnectionUnavailable);
        }
        if self.phone_drive_preferences_token.as_ref() != Some(token) {
            return Err(PhoneDriveActivationError::SettingsNotLoaded);
        }
        if self.phone_drive_settings_presented_for.as_ref() == Some(token) {
            return Err(PhoneDriveActivationError::Busy);
        }
        match self.phone_drive_availability() {
            PhoneDriveAvailability::Unavailable(_) => {
                return Err(PhoneDriveActivationError::VehicleUnsupported);
            }
            PhoneDriveAvailability::Ready | PhoneDriveAvailability::Active(_) => {}
        }
        let input = self.phone_drive_input_snapshot();
        if input.mode == PhoneControlMode::Disabled {
            return Err(PhoneDriveActivationError::ModeDisabled);
        }
        if at < self.last_input_at {
            return Err(PhoneDriveActivationError::ClockRegression);
        }
        if self.phone_drive_control.controller_fault {
            return Err(PhoneDriveActivationError::ControllerFault);
        }
        if let Some(pending) = self.phone_drive_control.pending
            && pending.admission.kind == PhoneDriveWriteKind::Release
        {
            if at.get() < pending.admission.submit_before_ms {
                return Err(PhoneDriveActivationError::Busy);
            }
            self.phone_drive_control.pending = None;
            self.phone_drive_control.activation = None;
            self.state
                .phone_drive_input
                .cancel(PhoneDriveCancelReason::TransportUnavailable);
        }
        if self.phone_drive_input_snapshot().phase != PhoneDriveInputPhase::Idle {
            return Err(PhoneDriveActivationError::InputHeld);
        }
        if self
            .device
            .as_ref()
            .is_some_and(crate::DeviceSession::settings_busy)
            || !self.setting_operations.is_empty()
        {
            return Err(PhoneDriveActivationError::Busy);
        }
        let next = self
            .phone_drive_control
            .next_activation
            .checked_add(1)
            .and_then(PhoneDriveActivationId::new)
            .ok_or(PhoneDriveActivationError::IdentifierExhausted)?;
        self.state
            .phone_drive_input
            .cancel(PhoneDriveCancelReason::ActivationChanged);
        if self.phone_drive_input_snapshot().generation.is_none() {
            return Err(PhoneDriveActivationError::IdentifierExhausted);
        }
        self.phone_drive_control.next_activation = next.get();
        self.phone_drive_control.pending = None;
        self.phone_drive_control.activation = Some(Activation {
            id: next,
            limits,
            observed_at: None,
            next_setpoint_at: None,
        });
        self.last_input_at = at;
        Ok(next)
    }

    /// Observes native contact time separately from host receipt time.
    #[must_use]
    pub fn observe_phone_drive_contacts_at(
        &mut self,
        token: &ConnectionAttemptToken,
        generation: u64,
        contacts: PhoneDriveContacts,
        observed_at: MonotonicTimestamp,
        at: MonotonicTimestamp,
    ) -> Option<DeviceConnectionStep> {
        if !self.state.connection.is_verified(token) {
            return None;
        }
        if self.phone_drive_preferences_token.as_ref() != Some(token)
            || self.phone_drive_settings_presented_for.as_ref() == Some(token)
        {
            return None;
        }
        let before = self.phone_drive_input_snapshot();
        if before.generation != Some(generation) {
            return self.phone_drive_step(token, at.max(self.last_input_at));
        }
        if at < self.last_input_at
            || observed_at > at
            || at.get().saturating_sub(observed_at.get()) >= INPUT_MAX_AGE_MS
            || self
                .phone_drive_control
                .activation
                .and_then(|activation| activation.observed_at)
                .is_some_and(|previous| observed_at.get() < previous)
        {
            self.cancel_phone_drive(PhoneDriveCancelReason::InputExpired);
            return self.tick_phone_drive(token, self.last_input_at.max(at), true);
        }
        self.last_input_at = at;
        match self.phone_drive_availability() {
            PhoneDriveAvailability::Ready | PhoneDriveAvailability::Active(_) => {
                self.state.phone_drive_input.observe(generation, contacts);
            }
            PhoneDriveAvailability::Unavailable(_) => {}
        }
        let mut after = self.phone_drive_input_snapshot();
        if let PhoneDriveInputPhase::Pressed(_) = after.phase
            && self
                .phone_drive_control
                .pending
                .is_some_and(|pending| pending.admission.kind == PhoneDriveWriteKind::Release)
        {
            // A fresh contact cannot bypass an unsent neutral or replay once it submits.
            self.state
                .phone_drive_input
                .observe(generation, PhoneDriveContacts::Invalid);
            after = self.phone_drive_input_snapshot();
        }
        if let Some(activation) = self.phone_drive_control.activation.as_mut() {
            activation.observed_at = Some(observed_at.get());
            match after.phase {
                PhoneDriveInputPhase::Pressed(action) => {
                    if before.phase != after.phase {
                        let limits = activation.limits;
                        let milliamps = match action {
                            PhoneDriveAction::Throttle => limits.throttle_milliamps(),
                            PhoneDriveAction::Brake => limits.brake_milliamps(),
                        };
                        self.prepare_phone_drive_write(Some(action), milliamps, at);
                    }
                }
                PhoneDriveInputPhase::Idle => {
                    activation.next_setpoint_at = None;
                    if let PhoneDriveInputPhase::Pressed(_) = before.phase {
                        self.prepare_phone_drive_write(None, 0, at);
                    }
                }
                PhoneDriveInputPhase::Cancelled(reason) => {
                    activation.next_setpoint_at = None;
                    if before.phase != after.phase {
                        if gesture_local_cancel(reason) {
                            if !self.phone_drive_control.pending.is_some_and(|pending| {
                                pending.admission.kind == PhoneDriveWriteKind::Release
                            }) {
                                self.prepare_phone_drive_write(None, 0, at);
                            }
                        } else {
                            self.cancel_phone_drive(reason);
                        }
                    }
                }
            }
        }
        // Heartbeats refresh input age but cadence emission remains owned by the shared timer.
        let mut step = self.phone_drive_step(token, at)?;
        self.append_phone_drive_write(&mut step, at, true);
        Some(step)
    }

    /// Advances drive deadlines, even when ordinary protocol polling cannot run.
    #[must_use]
    pub fn tick_phone_drive(
        &mut self,
        token: &ConnectionAttemptToken,
        at: MonotonicTimestamp,
        can_submit: bool,
    ) -> Option<DeviceConnectionStep> {
        if !self.state.connection.is_verified(token) {
            return None;
        }
        if at < self.last_input_at {
            self.cancel_phone_drive(PhoneDriveCancelReason::InputExpired);
        }
        let at = at.max(self.last_input_at);
        self.advance_phone_drive_deadlines(at);
        if can_submit
            && self.phone_drive_control.pending.is_none()
            && let Some(activation) = self.phone_drive_control.activation
            && activation
                .next_setpoint_at
                .is_some_and(|due| at.get() >= due)
            && let PhoneDriveInputPhase::Pressed(action) = self.phone_drive_input_snapshot().phase
        {
            let milliamps = match action {
                PhoneDriveAction::Throttle => activation.limits.throttle_milliamps(),
                PhoneDriveAction::Brake => activation.limits.brake_milliamps(),
            };
            self.prepare_phone_drive_write(Some(action), milliamps, at);
        }
        let mut step = self.phone_drive_step(token, at)?;
        self.append_phone_drive_write(&mut step, at, can_submit);
        Some(step)
    }

    /// Revalidates a drive write immediately before its native submission.
    #[must_use]
    pub fn phone_drive_transport_is_current(
        &self,
        token: &ConnectionAttemptToken,
        write: PhoneDriveWrite,
        at: MonotonicTimestamp,
    ) -> bool {
        if !self.state.connection.is_verified(token) {
            return false;
        }
        if self.phone_drive_settings_presented_for.as_ref() == Some(token)
            && write.kind == PhoneDriveWriteKind::Setpoint
        {
            return false;
        }
        let Some(pending) = self.phone_drive_control.pending else {
            return false;
        };
        if pending.admission != write
            || at.get() < pending.issued_at
            || at.get() >= write.submit_before_ms
        {
            return false;
        }
        match write.kind {
            PhoneDriveWriteKind::Release => true,
            PhoneDriveWriteKind::Setpoint => {
                let Some(activation) = self.phone_drive_control.activation else {
                    return false;
                };
                activation.id == write.activation_id
                    && self.phone_drive_input_snapshot().generation == Some(write.generation)
                    && self.phone_drive_input_snapshot().phase
                        == pending
                            .action
                            .map_or(PhoneDriveInputPhase::Idle, PhoneDriveInputPhase::Pressed)
                    && activation.observed_at.is_some_and(|observed| {
                        observed <= at.get() && at.get() - observed < INPUT_MAX_AGE_MS
                    })
            }
        }
    }

    /// Records host submission evidence without claiming controller application.
    pub fn mark_phone_drive_transport(
        &mut self,
        token: &ConnectionAttemptToken,
        write: PhoneDriveWrite,
        status: ApplicationQueryTransportStatus,
        at: MonotonicTimestamp,
    ) -> bool {
        if !self.state.connection.is_verified(token)
            || (self.phone_drive_settings_presented_for.as_ref() == Some(token)
                && write.kind == PhoneDriveWriteKind::Setpoint)
            || !self
                .phone_drive_control
                .pending
                .is_some_and(|pending| pending.admission == write && at.get() >= pending.issued_at)
        {
            return false;
        }
        match status {
            ApplicationQueryTransportStatus::Queued => {}
            ApplicationQueryTransportStatus::Submitted => self.phone_drive_control.pending = None,
            ApplicationQueryTransportStatus::Rejected
            | ApplicationQueryTransportStatus::Cancelled
            | ApplicationQueryTransportStatus::SubmissionUnknown => {
                self.phone_drive_control.pending = None;
                match write.kind {
                    PhoneDriveWriteKind::Setpoint => {
                        self.last_input_at = self.last_input_at.max(at);
                        self.cancel_phone_drive(PhoneDriveCancelReason::TransportUnavailable);
                    }
                    PhoneDriveWriteKind::Release => {
                        self.phone_drive_control.activation = None;
                        self.state
                            .phone_drive_input
                            .cancel(PhoneDriveCancelReason::TransportUnavailable);
                    }
                }
            }
        }
        true
    }

    /// Returns input feedback independently from controller application.
    #[must_use]
    pub fn phone_drive_input_snapshot(&self) -> PhoneDriveInputSnapshot {
        self.state.phone_drive_input.snapshot()
    }

    /// Updates contact feedback without renewing live-control input age.
    #[cfg(test)]
    pub fn observe_phone_drive_contacts(
        &mut self,
        token: &ConnectionAttemptToken,
        generation: u64,
        contacts: PhoneDriveContacts,
    ) {
        if self.phone_drive_preferences_are_current(token)
            && self.phone_drive_settings_presented_for.as_ref() != Some(token)
            && self.phone_drive_activation().is_none()
        {
            match self.phone_drive_availability() {
                PhoneDriveAvailability::Ready => {
                    self.state.phone_drive_input.observe(generation, contacts);
                }
                PhoneDriveAvailability::Unavailable(_) | PhoneDriveAvailability::Active(_) => {}
            }
        }
    }

    /// Retires held input and retains bounded neutral intent for the same live attempt.
    pub fn cancel_phone_drive(&mut self, reason: PhoneDriveCancelReason) {
        if reason == PhoneDriveCancelReason::ConnectionChanged {
            self.phone_drive_control.activation = None;
            self.phone_drive_control.pending = None;
            self.phone_drive_control.controller_fault = false;
            self.phone_drive_preferences_token = None;
            self.phone_drive_settings_presented_for = None;
            self.state
                .phone_drive_input
                .set_mode(PhoneControlMode::Disabled);
            self.state.phone_drive_input.cancel(reason);
            self.state.phone_drive_limits = PhoneDriveLimits::default();
            return;
        }
        self.state.phone_drive_input.cancel(reason);
        if self.phone_drive_control.activation.is_some() {
            if !self
                .phone_drive_control
                .pending
                .is_some_and(|pending| pending.admission.kind == PhoneDriveWriteKind::Release)
            {
                self.prepare_phone_drive_write(None, 0, self.last_input_at);
            }
            if gesture_local_cancel(reason) {
                if let Some(activation) = self.phone_drive_control.activation.as_mut() {
                    activation.next_setpoint_at = None;
                }
            } else {
                self.phone_drive_control.activation = None;
            }
        }
    }

    /// Applies a complete persisted configuration to the exact currently eligible attempt.
    ///
    /// The generation is ephemeral and is never used as a database key.
    pub fn apply_phone_drive_preferences(
        &mut self,
        token: &ConnectionAttemptToken,
        mode: PhoneControlMode,
        limits: PhoneDriveLimits,
    ) -> bool {
        if !self.state.connection.is_verified(token)
            || matches!(
                self.phone_drive_availability(),
                PhoneDriveAvailability::Unavailable(_)
            )
        {
            return false;
        }
        if self.phone_drive_input_snapshot().mode != mode {
            self.cancel_phone_drive(PhoneDriveCancelReason::ModeChanged);
        } else if self.state.phone_drive_limits != limits {
            self.cancel_phone_drive(PhoneDriveCancelReason::ActivationChanged);
        }
        self.state.phone_drive_input.set_mode(mode);
        self.state.phone_drive_limits = limits;
        self.phone_drive_preferences_token = Some(token.clone());
        true
    }

    /// Whether this attempt is currently eligible to show device-scoped phone settings.
    #[must_use]
    pub fn phone_drive_target_is_current(&self, token: &ConnectionAttemptToken) -> bool {
        self.state.connection.is_verified(token)
            && !matches!(
                self.phone_drive_availability(),
                PhoneDriveAvailability::Unavailable(_)
            )
    }

    /// Whether the full saved configuration belongs to this verified attempt.
    #[must_use]
    pub fn phone_drive_preferences_are_current(&self, token: &ConnectionAttemptToken) -> bool {
        self.phone_drive_preferences_token.as_ref() == Some(token)
            && self.phone_drive_target_is_current(token)
    }

    /// Reports that the existing Ride input surface is covered by its settings sheet.
    /// Opening retires held input and activation; dismissing never restores either.
    pub fn set_phone_drive_settings_presented(
        &mut self,
        token: &ConnectionAttemptToken,
        presented: bool,
    ) -> bool {
        if !self.phone_drive_target_is_current(token) {
            return false;
        }
        if presented {
            if self.phone_drive_settings_presented_for.as_ref() != Some(token) {
                self.cancel_phone_drive(PhoneDriveCancelReason::Navigation);
                self.phone_drive_settings_presented_for = Some(token.clone());
            }
            true
        } else if self.phone_drive_settings_presented_for.as_ref() == Some(token) {
            self.phone_drive_settings_presented_for = None;
            true
        } else {
            false
        }
    }

    /// Whether this exact connection owns the currently presented settings sheet.
    #[must_use]
    pub fn phone_drive_settings_are_presented(&self, token: &ConnectionAttemptToken) -> bool {
        self.phone_drive_settings_presented_for.as_ref() == Some(token)
            && self.phone_drive_target_is_current(token)
    }

    /// Cancels from a native lifecycle event using its current host receipt time.
    pub fn cancel_phone_drive_at(
        &mut self,
        reason: PhoneDriveCancelReason,
        at: MonotonicTimestamp,
    ) {
        self.last_input_at = self.last_input_at.max(at);
        self.cancel_phone_drive(reason);
    }

    pub(super) fn phone_drive_next_wakeup_at(
        &self,
        can_submit: bool,
    ) -> Option<MonotonicTimestamp> {
        let pending = self.phone_drive_control.pending;
        let activation = self.phone_drive_control.activation;
        let held = if let PhoneDriveInputPhase::Pressed(_) = self.phone_drive_input_snapshot().phase
        {
            true
        } else {
            false
        };
        [
            pending.map(|pending| pending.admission.submit_before_ms),
            pending
                .filter(|pending| !pending.emitted && can_submit)
                .map(|_| self.last_input_at.get()),
            activation
                .filter(|_| held)
                .and_then(|activation| activation.observed_at)
                .and_then(|at| at.checked_add(INPUT_MAX_AGE_MS)),
            activation
                .filter(|_| can_submit && pending.is_none())
                .and_then(|activation| activation.next_setpoint_at),
        ]
        .into_iter()
        .flatten()
        .min()
        .map(MonotonicTimestamp::new)
    }

    pub(super) fn observe_phone_drive_fault(&mut self, outputs: &[SessionOutput]) {
        for output in outputs {
            if let SessionOutput::Event(cutout_core::DeviceEvent::ReadOnlyResponse(
                cutout_core::ReadOnlyResponse::RawTelemetry(readback),
            )) = output
                && let Some(field) = readback
                    .fields
                    .iter()
                    .find(|field| field.id == crate::VESC_RAW_CURRENT_FAULT_CODE_FIELD_ID)
            {
                self.phone_drive_control.controller_fault = field.value != 0;
                if field.value != 0 {
                    self.cancel_phone_drive(PhoneDriveCancelReason::ControllerFault);
                }
            }
        }
    }

    fn advance_phone_drive_deadlines(&mut self, at: MonotonicTimestamp) {
        if let PhoneDriveInputPhase::Pressed(_) = self.phone_drive_input_snapshot().phase
            && self
                .phone_drive_control
                .activation
                .and_then(|activation| activation.observed_at)
                .is_none_or(|observed| {
                    observed > at.get() || at.get() - observed >= INPUT_MAX_AGE_MS
                })
            && self.phone_drive_control.activation.is_some()
        {
            self.last_input_at = at;
            self.cancel_phone_drive(PhoneDriveCancelReason::InputExpired);
        }
        if let Some(pending) = self.phone_drive_control.pending
            && at.get() >= pending.admission.submit_before_ms
        {
            self.phone_drive_control.pending = None;
            self.last_input_at = at;
            match pending.admission.kind {
                PhoneDriveWriteKind::Setpoint => {
                    self.cancel_phone_drive(PhoneDriveCancelReason::TransportUnavailable);
                }
                PhoneDriveWriteKind::Release => {
                    self.phone_drive_control.activation = None;
                    self.state
                        .phone_drive_input
                        .cancel(PhoneDriveCancelReason::TransportUnavailable);
                }
            }
        }
    }

    fn prepare_phone_drive_write(
        &mut self,
        action: Option<PhoneDriveAction>,
        milliamps: u32,
        at: MonotonicTimestamp,
    ) {
        let Some(activation) = self.phone_drive_control.activation else {
            return;
        };
        let Some(sequence) = self.phone_drive_control.next_sequence.checked_add(1) else {
            self.phone_drive_control.activation = None;
            self.phone_drive_control.pending = None;
            self.state
                .phone_drive_input
                .cancel(PhoneDriveCancelReason::GenerationExhausted);
            return;
        };
        let Some(deadline) = at.get().checked_add(SUBMISSION_WINDOW_MS) else {
            self.phone_drive_control.activation = None;
            self.phone_drive_control.pending = None;
            self.state
                .phone_drive_input
                .cancel(PhoneDriveCancelReason::GenerationExhausted);
            return;
        };
        let Some(generation) = self.phone_drive_input_snapshot().generation else {
            return;
        };
        self.phone_drive_control.next_sequence = sequence;
        self.phone_drive_control.pending = Some(PendingWrite {
            admission: PhoneDriveWrite {
                activation_id: activation.id,
                sequence,
                generation,
                submit_before_ms: deadline,
                kind: action.map_or(PhoneDriveWriteKind::Release, |_| {
                    PhoneDriveWriteKind::Setpoint
                }),
            },
            action,
            milliamps,
            issued_at: at.get(),
            emitted: false,
        });
        if let Some(activation) = self.phone_drive_control.activation.as_mut() {
            activation.next_setpoint_at = action.and_then(|_| at.get().checked_add(CADENCE_MS));
        }
    }

    fn phone_drive_step(
        &mut self,
        token: &ConnectionAttemptToken,
        at: MonotonicTimestamp,
    ) -> Option<DeviceConnectionStep> {
        if !self.state.connection.is_verified(token) {
            return None;
        }
        self.last_input_at = self.last_input_at.max(at);
        let device = self.device.as_ref()?;
        Some(DeviceConnectionStep {
            notification_framing: super::NotificationFramingEvidence::Unclassified,
            session: self.snapshot(),
            result: device.phone_drive_step()?,
            telemetry: device.current_snapshot(),
            vesc_drivetrain_context: self.effective_vesc_drivetrain_context(),
            configured_vesc_drivetrains: self.configured_vesc_drivetrains(token),
            diagnostics: device.diagnostics(),
        })
    }

    fn append_phone_drive_write(
        &mut self,
        step: &mut DeviceConnectionStep,
        at: MonotonicTimestamp,
        can_submit: bool,
    ) {
        let Some(pending) = self.phone_drive_control.pending else {
            return;
        };
        if pending.emitted || !can_submit || at.get() >= pending.admission.submit_before_ms {
            return;
        }
        let Some(mut bytes) =
            crate::vesc_codec::encode_phone_drive_current(pending.action, pending.milliamps)
        else {
            self.phone_drive_control.activation = None;
            self.phone_drive_control.pending = None;
            self.state
                .phone_drive_input
                .cancel(PhoneDriveCancelReason::TransportUnavailable);
            return;
        };
        bytes.set_phone_drive(pending.admission);
        step.result.outputs.insert(
            0,
            SessionOutput::Transport(TransportAction::Write {
                channel: crate::VESC_WRITE_CHANNEL,
                bytes,
                mode: WriteMode::WithoutResponse,
            }),
        );
        if let Some(pending) = self.phone_drive_control.pending.as_mut() {
            pending.emitted = true;
        }
    }
}

const fn gesture_local_cancel(reason: PhoneDriveCancelReason) -> bool {
    match reason {
        PhoneDriveCancelReason::RegionLeft
        | PhoneDriveCancelReason::MultipleContacts
        | PhoneDriveCancelReason::ContactReplaced
        | PhoneDriveCancelReason::InvalidContact => true,
        _ => false,
    }
}
