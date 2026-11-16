//! Recorded phone-drive contacts. This phase has no motor-write admission.

use cutout_core::{
    PhoneControlMode, PhoneDriveContacts, PhoneDriveInputSnapshot, PhoneDriveInputState,
};

/// Replay phase. Its only result is input feedback.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PhoneDriveReplay;

impl PhoneDriveReplay {
    /// Classifies one recorded contact with the existing input owner.
    ///
    /// The owner is discarded with the call. It has no write admission.
    #[must_use]
    pub(super) fn replay(
        mode: PhoneControlMode,
        contacts: PhoneDriveContacts,
    ) -> PhoneDriveInputSnapshot {
        let mut input = PhoneDriveInputState::default();
        input.set_mode(mode);
        let Some(generation) = input.snapshot().generation else {
            return input.snapshot();
        };
        // Enabling a layout retires that generation. This sample has no
        // earlier contact, so clear the retirement before reading it.
        input.observe(generation, PhoneDriveContacts::Released);
        input.observe(generation, contacts);
        input.snapshot()
    }
}
