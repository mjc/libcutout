//! Phone input semantics. A pressed surface is never a motor-control grant.

/// Saved input layout, independent of controller qualification and activation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PhoneControlMode {
    /// No phone input surface.
    #[default]
    Disabled,
    /// Two bottom hold regions.
    Buttons,
    /// Left/right halves of Ride, excluding navigation.
    Area,
}

/// Semantic intent identified by an input surface, not a wire command or signed current.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveAction {
    /// Rider is holding the brake region.
    Brake,
    /// Rider is holding the throttle region.
    Throttle,
}

/// Native contact identity. Zero is reserved for an absent contact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneDriveContactId(core::num::NonZeroU64);

impl PhoneDriveContactId {
    /// Validates a native contact identity.
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        match core::num::NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Complete native contact observation; native owns framework touch objects only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveContacts {
    /// No contact remains on this surface.
    Released,
    /// One contact and its current region; `None` means outside both regions.
    Single {
        /// Stable identity for this contact's lifetime.
        contact: PhoneDriveContactId,
        /// Hit-tested region in the current native layout.
        region: Option<PhoneDriveAction>,
    },
    /// Two or more contacts are present; none may take ownership until all lift.
    Multiple,
}

/// Why an input generation or held gesture ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveCancelReason {
    /// Saved input mode changed.
    ModeChanged,
    /// Rotation or input-region bounds changed.
    LayoutChanged,
    /// Navigation, a sheet, or disappearance interrupted input.
    Navigation,
    /// The app lost active foreground focus.
    SceneInactive,
    /// The active connection or its identity changed.
    ConnectionChanged,
    /// The original contact left its original region.
    RegionLeft,
    /// More than one contact was observed.
    MultipleContacts,
    /// A different contact appeared without a complete release.
    ContactReplaced,
    /// Input generation identity cannot advance further.
    GenerationExhausted,
}

/// Gesture feedback only; none of these states implies controller activation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveInputPhase {
    /// No held input.
    Idle,
    /// A single contact remains in its original region.
    Pressed(PhoneDriveAction),
    /// All contacts must lift before another gesture may start.
    Cancelled(PhoneDriveCancelReason),
}

/// Immutable semantic input feedback, separate from drive eligibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneDriveInputSnapshot {
    /// Saved layout.
    pub mode: PhoneControlMode,
    /// Current input generation; absent after identity exhaustion.
    pub generation: Option<u64>,
    /// Gesture feedback, never an actuator receipt.
    pub phase: PhoneDriveInputPhase,
}

/// Portable input owner retained by the existing device session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhoneDriveInputState {
    mode: PhoneControlMode,
    epoch: InputEpoch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InputEpoch {
    Current {
        generation: core::num::NonZeroU64,
        gesture: Gesture,
    },
    Exhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Gesture {
    Idle,
    Pressed {
        contact: PhoneDriveContactId,
        action: PhoneDriveAction,
    },
    Suppressed(PhoneDriveCancelReason),
}

impl Gesture {
    const fn phase(self) -> PhoneDriveInputPhase {
        match self {
            Self::Idle => PhoneDriveInputPhase::Idle,
            Self::Pressed { action, .. } => PhoneDriveInputPhase::Pressed(action),
            Self::Suppressed(reason) => PhoneDriveInputPhase::Cancelled(reason),
        }
    }

    fn observe(self, contacts: PhoneDriveContacts) -> Self {
        match (self, contacts) {
            (_, PhoneDriveContacts::Released) => Self::Idle,
            (Self::Suppressed(reason), _) => Self::Suppressed(reason),
            (_, PhoneDriveContacts::Multiple) => {
                Self::Suppressed(PhoneDriveCancelReason::MultipleContacts)
            }
            (
                Self::Idle,
                PhoneDriveContacts::Single {
                    contact,
                    region: Some(action),
                },
            ) => Self::Pressed { contact, action },
            (Self::Idle, PhoneDriveContacts::Single { region: None, .. }) => {
                Self::Suppressed(PhoneDriveCancelReason::RegionLeft)
            }
            (
                Self::Pressed { contact, action },
                PhoneDriveContacts::Single {
                    contact: observed,
                    region,
                },
            ) => {
                if contact != observed {
                    Self::Suppressed(PhoneDriveCancelReason::ContactReplaced)
                } else if region != Some(action) {
                    Self::Suppressed(PhoneDriveCancelReason::RegionLeft)
                } else {
                    self
                }
            }
        }
    }
}

impl Default for PhoneDriveInputState {
    fn default() -> Self {
        Self {
            mode: PhoneControlMode::Disabled,
            epoch: InputEpoch::Current {
                generation: core::num::NonZeroU64::MIN,
                gesture: Gesture::Idle,
            },
        }
    }
}

impl PhoneDriveInputState {
    /// Returns feedback without granting control authority.
    #[must_use]
    pub const fn snapshot(&self) -> PhoneDriveInputSnapshot {
        match self.epoch {
            InputEpoch::Current {
                generation,
                gesture,
            } => PhoneDriveInputSnapshot {
                mode: self.mode,
                generation: Some(generation.get()),
                phase: gesture.phase(),
            },
            InputEpoch::Exhausted => PhoneDriveInputSnapshot {
                mode: self.mode,
                generation: None,
                phase: PhoneDriveInputPhase::Cancelled(PhoneDriveCancelReason::GenerationExhausted),
            },
        }
    }

    /// Changes layout without activating a controller.
    pub fn set_mode(&mut self, mode: PhoneControlMode) {
        if self.mode != mode {
            self.mode = mode;
            self.cancel(PhoneDriveCancelReason::ModeChanged);
        }
    }

    /// Observes contacts for the current generation only.
    pub fn observe(&mut self, observed_generation: u64, contacts: PhoneDriveContacts) {
        if self.mode == PhoneControlMode::Disabled {
            return;
        }
        if let InputEpoch::Current {
            generation,
            gesture,
        } = &mut self.epoch
            && generation.get() == observed_generation
        {
            *gesture = gesture.observe(contacts);
        }
    }

    /// Retires held input after a native or connection lifecycle change.
    pub fn cancel(&mut self, reason: PhoneDriveCancelReason) {
        self.epoch = match self.epoch {
            InputEpoch::Current { generation, .. } => match generation.checked_add(1) {
                Some(generation) => InputEpoch::Current {
                    generation,
                    gesture: Gesture::Suppressed(reason),
                },
                None => InputEpoch::Exhausted,
            },
            InputEpoch::Exhausted => InputEpoch::Exhausted,
        };
    }
}

/// A missing production control contract is independent of telemetry/settings support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveUnavailableReason {
    /// No verified current connection exists.
    Disconnected,
    /// The current route is not an e-skate phone-control candidate.
    VehicleUnsupported,
    /// No exact profile proves source-specific expiry, arbitration and physical behavior.
    ControllerUnqualified,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(id: u64, action: Option<PhoneDriveAction>) -> PhoneDriveContacts {
        PhoneDriveContacts::Single {
            contact: PhoneDriveContactId::new(id).unwrap(),
            region: action,
        }
    }

    fn configured(mode: PhoneControlMode) -> PhoneDriveInputState {
        let mut state = PhoneDriveInputState::default();
        state.set_mode(mode);
        state.observe(
            state.snapshot().generation.unwrap(),
            PhoneDriveContacts::Released,
        );
        state
    }

    #[test]
    fn phone_drive_region_exit_cancels_until_every_contact_lifts() {
        let mut state = configured(PhoneControlMode::Area);
        let generation = state.snapshot().generation.unwrap();
        state.observe(generation, contact(1, Some(PhoneDriveAction::Throttle)));
        assert_eq!(
            state.snapshot().phase,
            PhoneDriveInputPhase::Pressed(PhoneDriveAction::Throttle)
        );
        state.observe(generation, contact(1, Some(PhoneDriveAction::Brake)));
        assert_eq!(
            state.snapshot().phase,
            PhoneDriveInputPhase::Cancelled(PhoneDriveCancelReason::RegionLeft)
        );
        state.observe(generation, contact(1, Some(PhoneDriveAction::Throttle)));
        assert_eq!(
            state.snapshot().phase,
            PhoneDriveInputPhase::Cancelled(PhoneDriveCancelReason::RegionLeft)
        );
        state.observe(generation, PhoneDriveContacts::Released);
        state.observe(generation, contact(2, Some(PhoneDriveAction::Brake)));
        assert_eq!(
            state.snapshot().phase,
            PhoneDriveInputPhase::Pressed(PhoneDriveAction::Brake)
        );
    }

    #[test]
    fn phone_drive_disabled_and_zero_contact_do_not_create_intent() {
        let mut state = PhoneDriveInputState::default();
        assert!(PhoneDriveContactId::new(0).is_none());
        state.observe(1, contact(1, Some(PhoneDriveAction::Throttle)));
        assert_eq!(state.snapshot().phase, PhoneDriveInputPhase::Idle);
    }

    #[test]
    fn phone_drive_multitouch_and_replacement_require_complete_release() {
        for mode in [PhoneControlMode::Buttons, PhoneControlMode::Area] {
            for interruption in [
                PhoneDriveContacts::Multiple,
                contact(2, Some(PhoneDriveAction::Brake)),
            ] {
                let mut state = configured(mode);
                let generation = state.snapshot().generation.unwrap();
                state.observe(generation, contact(1, Some(PhoneDriveAction::Brake)));
                state.observe(generation, interruption);
                let cancelled = state.snapshot();
                assert!(matches!(
                    cancelled.phase,
                    PhoneDriveInputPhase::Cancelled(_)
                ));
                state.observe(generation, contact(1, Some(PhoneDriveAction::Brake)));
                assert_eq!(state.snapshot(), cancelled);
                state.observe(generation, PhoneDriveContacts::Released);
                state.observe(generation, contact(3, Some(PhoneDriveAction::Throttle)));
                assert_eq!(
                    state.snapshot().phase,
                    PhoneDriveInputPhase::Pressed(PhoneDriveAction::Throttle)
                );
            }
        }
    }

    #[test]
    fn phone_drive_mode_and_lifecycle_changes_retire_old_callbacks() {
        for reason in [
            PhoneDriveCancelReason::LayoutChanged,
            PhoneDriveCancelReason::Navigation,
            PhoneDriveCancelReason::SceneInactive,
            PhoneDriveCancelReason::ConnectionChanged,
        ] {
            let mut state = configured(PhoneControlMode::Area);
            let old = state.snapshot().generation.unwrap();
            state.observe(old, contact(1, Some(PhoneDriveAction::Throttle)));
            state.cancel(reason);
            let current = state.snapshot().generation.unwrap();
            assert_ne!(current, old);
            state.observe(old, PhoneDriveContacts::Released);
            state.observe(current, contact(1, Some(PhoneDriveAction::Throttle)));
            assert_eq!(
                state.snapshot().phase,
                PhoneDriveInputPhase::Cancelled(reason)
            );
            state.observe(current, PhoneDriveContacts::Released);
            state.observe(current, contact(2, Some(PhoneDriveAction::Brake)));
            let held = state.snapshot();
            state.observe(old, PhoneDriveContacts::Released);
            assert_eq!(state.snapshot(), held);
            state.set_mode(PhoneControlMode::Buttons);
            assert_eq!(
                state.snapshot().phase,
                PhoneDriveInputPhase::Cancelled(PhoneDriveCancelReason::ModeChanged)
            );
        }
    }

    #[test]
    fn phone_drive_identical_mode_keeps_input_and_outside_start_never_enters_on_drag() {
        let mut state = configured(PhoneControlMode::Buttons);
        let generation = state.snapshot().generation.unwrap();
        state.observe(generation, contact(1, None));
        state.observe(generation, contact(1, Some(PhoneDriveAction::Throttle)));
        assert_eq!(
            state.snapshot().phase,
            PhoneDriveInputPhase::Cancelled(PhoneDriveCancelReason::RegionLeft)
        );
        state.observe(generation, PhoneDriveContacts::Released);
        state.observe(generation, contact(2, Some(PhoneDriveAction::Brake)));
        let held = state.snapshot();
        state.set_mode(PhoneControlMode::Buttons);
        assert_eq!(state.snapshot(), held);
    }

    #[test]
    fn phone_drive_generation_exhaustion_cannot_reuse_an_old_input_identity() {
        let mut state = PhoneDriveInputState {
            mode: PhoneControlMode::Area,
            epoch: InputEpoch::Current {
                generation: core::num::NonZeroU64::new(u64::MAX).unwrap(),
                gesture: Gesture::Idle,
            },
        };
        state.cancel(PhoneDriveCancelReason::Navigation);
        let exhausted = state.snapshot();
        assert_eq!(exhausted.generation, None);
        state.observe(u64::MAX, contact(1, Some(PhoneDriveAction::Throttle)));
        state.set_mode(PhoneControlMode::Buttons);
        assert_eq!(state.snapshot().generation, None);
        assert_eq!(state.snapshot().phase, exhausted.phase);
    }
}
