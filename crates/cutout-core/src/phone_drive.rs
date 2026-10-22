//! Phone input semantics. A pressed surface is never a motor-control grant.

/// Explicit rider-selected current budgets, in milliamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneDriveLimits {
    throttle_milliamps: u32,
    brake_milliamps: u32,
}

/// A current budget is outside the supported UI range or is not a whole milliamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("enter a positive current of at most 500 A, in whole milliamps")]
pub struct PhoneDriveLimitsError;

impl Default for PhoneDriveLimits {
    fn default() -> Self {
        // VESC Tool's control UI uses 3 A. This is a product default, not a
        // board-specific safety limit; the rider can change it before activation.
        Self {
            throttle_milliamps: 3_000,
            brake_milliamps: 3_000,
        }
    }
}

impl PhoneDriveLimits {
    /// Checks independent positive budgets against the supported 500 A UI range.
    ///
    /// # Errors
    /// Returns an error for zero or an out-of-range budget.
    pub fn new(
        throttle_milliamps: u32,
        brake_milliamps: u32,
    ) -> Result<Self, PhoneDriveLimitsError> {
        if !(1..=500_000).contains(&throttle_milliamps) || !(1..=500_000).contains(&brake_milliamps)
        {
            return Err(PhoneDriveLimitsError);
        }
        Ok(Self {
            throttle_milliamps,
            brake_milliamps,
        })
    }

    /// Parses decimal amp fields without floating point or silent rounding.
    ///
    /// # Errors
    /// Returns an error for invalid text, fractional milliamps or unsupported budgets.
    pub fn from_amp_text(throttle: &str, brake: &str) -> Result<Self, PhoneDriveLimitsError> {
        Self::new(parse_amp_text(throttle)?, parse_amp_text(brake)?)
    }

    /// Requested throttle current budget.
    #[must_use]
    pub const fn throttle_milliamps(self) -> u32 {
        self.throttle_milliamps
    }

    /// Requested braking current magnitude.
    #[must_use]
    pub const fn brake_milliamps(self) -> u32 {
        self.brake_milliamps
    }
}

fn parse_amp_text(text: &str) -> Result<u32, PhoneDriveLimitsError> {
    let text = text.trim();
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(PhoneDriveLimitsError);
    }
    let fraction = match fraction {
        Some(value) => {
            let scale = match value.len() {
                1 => 100,
                2 => 10,
                3 => 1,
                _ => return Err(PhoneDriveLimitsError),
            };
            if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(PhoneDriveLimitsError);
            }
            value.parse::<u32>().map_err(|_| PhoneDriveLimitsError)? * scale
        }
        None => 0,
    };
    whole
        .parse::<u32>()
        .map_err(|_| PhoneDriveLimitsError)?
        .checked_mul(1_000)
        .and_then(|value| value.checked_add(fraction))
        .ok_or(PhoneDriveLimitsError)
}

/// Ephemeral activation identity; never persisted or restored after reconnect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneDriveActivationId(core::num::NonZeroU64);

impl PhoneDriveActivationId {
    /// Validates a nonzero activation identity.
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        match core::num::NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the activation identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Controller-path support and explicit per-attempt activation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveAvailability {
    /// Current connection or vehicle does not support this path.
    Unavailable(PhoneDriveUnavailableReason),
    /// Standard VESC path is available; deliberate activation is still required.
    Ready,
    /// Rider explicitly activated this connection attempt.
    Active(PhoneDriveActivationId),
}

/// Which bounded live-control intent a write carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveWriteKind {
    /// Current/brake-current request for a fresh held contact.
    Setpoint,
    /// Zero motor current; does not claim an on-controller stop.
    Release,
}

/// Session-owned admission checked again immediately before native submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneDriveWrite {
    /// Per-attempt activation that produced this intent.
    pub activation_id: PhoneDriveActivationId,
    /// Latest intent sequence; superseded writes must be rejected.
    pub sequence: u64,
    /// Original semantic input generation.
    pub generation: u64,
    /// Exclusive monotonic submission deadline in milliseconds.
    pub submit_before_ms: u64,
    /// Setpoint or separately retained release intent.
    pub kind: PhoneDriveWriteKind,
}

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
    /// A native contact identity was malformed; no gesture may start before release.
    Invalid,
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
    /// Same-connection vehicle or drivetrain configuration changed.
    ConfigurationChanged,
    /// Explicit activation or deactivation retired the previous input.
    ActivationChanged,
    /// The native held-contact heartbeat expired.
    InputExpired,
    /// Submission failed or its outcome is unknown.
    TransportUnavailable,
    /// The controller reported a fault.
    ControllerFault,
    /// Native touch delivery was cancelled by the platform.
    TouchCancelled,
    /// A native contact identity was malformed.
    InvalidContact,
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
            (_, PhoneDriveContacts::Invalid) => {
                Self::Suppressed(PhoneDriveCancelReason::InvalidContact)
            }
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

/// Unsupported connection/vehicle path, independent of saved settings and activation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveUnavailableReason {
    /// No verified current connection exists.
    Disconnected,
    /// The current route is not an e-skate phone-control candidate.
    VehicleUnsupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_drive_budgets_are_checked_independently_and_parse_exact_milliamps() {
        let limits = PhoneDriveLimits::new(3_000, 4_125).unwrap();
        assert_eq!(limits.throttle_milliamps(), 3_000);
        assert_eq!(limits.brake_milliamps(), 4_125);
        assert_eq!(
            PhoneDriveLimits::from_amp_text(" 3.000 ", "4.125").unwrap(),
            limits
        );
        assert!(PhoneDriveLimits::new(0, 3_000).is_err());
        assert!(PhoneDriveLimits::new(3_000, 500_001).is_err());
        assert!(PhoneDriveLimits::new(500_000, 1).is_ok());
        for invalid in [
            "", "-3", "+3", "NaN", "inf", "1e3", "0", "500.001", "3.0001", ".5", "3.", "0.0009",
        ] {
            assert!(
                PhoneDriveLimits::from_amp_text(invalid, "3").is_err(),
                "{invalid}"
            );
            assert!(
                PhoneDriveLimits::from_amp_text("3", invalid).is_err(),
                "{invalid}"
            );
        }
        assert_eq!(PhoneDriveActivationId::new(1).unwrap().get(), 1);
        assert_eq!(PhoneDriveActivationId::new(0), None);
    }

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
