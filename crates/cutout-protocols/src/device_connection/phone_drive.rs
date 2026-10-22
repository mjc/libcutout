//! Phone gesture state is not an actuator or a settings transport operation.

use cutout_core::{
    ConnectionAttemptToken, PhoneControlMode, PhoneDriveCancelReason, PhoneDriveContacts,
    PhoneDriveInputSnapshot, PhoneDriveUnavailableReason,
};

use super::DeviceConnectionSession;

impl DeviceConnectionSession {
    /// Returns input feedback without interpreting it as an actuation grant.
    #[must_use]
    pub fn phone_drive_input_snapshot(&self) -> PhoneDriveInputSnapshot {
        self.state.phone_drive_input.snapshot()
    }

    /// Reports why this connection cannot activate phone drive.
    ///
    /// No production profile currently proves independent controller expiry and remote
    /// arbitration. Readable VESC telemetry and settings grants never supply that proof.
    #[must_use]
    pub fn phone_drive_unavailability(&self) -> PhoneDriveUnavailableReason {
        let connection = self.state.connection.snapshot();
        let Some(token) = &connection.token else {
            return PhoneDriveUnavailableReason::Disconnected;
        };
        if !self.state.connection.is_verified(token) {
            return PhoneDriveUnavailableReason::Disconnected;
        }
        match self.device.as_ref().map(crate::DeviceSession::identity) {
            Some(identity)
                if identity.protocol == cutout_core::ProtocolFamily::Vesc
                    && identity.vehicle_kind() == cutout_core::VehicleKind::Eskate =>
            {
                PhoneDriveUnavailableReason::ControllerUnqualified
            }
            Some(_) => PhoneDriveUnavailableReason::VehicleUnsupported,
            None => PhoneDriveUnavailableReason::Disconnected,
        }
    }

    /// Applies an already durably saved layout; does not activate phone drive.
    pub fn set_phone_control_mode(&mut self, mode: PhoneControlMode) {
        self.state.phone_drive_input.set_mode(mode);
    }

    /// Updates semantic contact feedback for the owning verified connection only.
    /// This operation cannot produce transport effects or grant motor authority.
    pub fn observe_phone_drive_contacts(
        &mut self,
        token: &ConnectionAttemptToken,
        generation: u64,
        contacts: PhoneDriveContacts,
    ) {
        if self.state.connection.is_verified(token)
            && self.phone_drive_unavailability()
                == PhoneDriveUnavailableReason::ControllerUnqualified
        {
            self.state.phone_drive_input.observe(generation, contacts);
        }
    }

    /// Retires held input without claiming any on-controller stop or brake outcome.
    pub fn cancel_phone_drive(&mut self, reason: PhoneDriveCancelReason) {
        self.state.phone_drive_input.cancel(reason);
    }
}
