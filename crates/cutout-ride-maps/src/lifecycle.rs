use thiserror::Error;

/// Durable lifecycle state for one ride recording.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideLifecycleState {
    /// A recording has been created but not started.
    Draft,
    /// A recording is actively accepting samples.
    Active,
    /// A recording is temporarily not accepting samples.
    Paused,
    /// A recording has stopped and may be saved or discarded.
    Stopped,
    /// A recording was interrupted before completion.
    Interrupted,
    /// A recording was discarded.
    Discarded,
    /// A complete recording is durable.
    Saved,
    /// A recording was imported from an external artifact.
    Imported,
}

/// Domain event applied to a ride lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideEvent {
    /// Begin a live recording.
    Start,
    /// Pause an active recording.
    Pause,
    /// Resume a paused or interrupted recording.
    Resume,
    /// Stop a recording cleanly.
    Stop,
    /// Mark a recording as interrupted.
    Interrupt,
    /// Permanently discard a recording.
    Discard,
    /// Publish a stopped recording as durable history.
    Save,
    /// Publish a validated imported recording.
    Import,
}

impl RideLifecycleState {
    /// Whether this recording still owns a live sampling session, including a pause.
    #[must_use]
    pub const fn is_recording(self) -> bool {
        match self {
            Self::Active | Self::Paused => true,
            Self::Draft
            | Self::Stopped
            | Self::Interrupted
            | Self::Discarded
            | Self::Saved
            | Self::Imported => false,
        }
    }

    const fn is_discardable(self) -> bool {
        match self {
            Self::Stopped | Self::Interrupted => true,
            Self::Draft
            | Self::Active
            | Self::Paused
            | Self::Discarded
            | Self::Saved
            | Self::Imported => false,
        }
    }

    const fn only_allows_new_recording(self) -> bool {
        match self {
            Self::Saved | Self::Discarded => true,
            Self::Draft
            | Self::Active
            | Self::Paused
            | Self::Stopped
            | Self::Interrupted
            | Self::Imported => false,
        }
    }

    /// Whether a completed or intentionally ended recording has known route bounds.
    #[must_use]
    pub const fn has_recorded_bounds(self) -> bool {
        match self {
            Self::Stopped | Self::Interrupted | Self::Discarded | Self::Saved | Self::Imported => {
                true
            }
            Self::Draft | Self::Active | Self::Paused => false,
        }
    }

    /// Whether a connection may replace this ride with a new GPS-only recording.
    #[must_use]
    pub const fn allows_auto_recording_replacement(self) -> bool {
        match self {
            Self::Interrupted | Self::Saved | Self::Discarded => true,
            Self::Draft | Self::Active | Self::Paused | Self::Stopped | Self::Imported => false,
        }
    }

    /// Whether a live ride has reached a completed or interrupted state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        match self {
            Self::Stopped | Self::Interrupted | Self::Discarded | Self::Saved => true,
            Self::Draft | Self::Active | Self::Paused | Self::Imported => false,
        }
    }

    /// Actions offered for the current recording; Start creates a separate new ride.
    #[must_use]
    pub fn recording_actions(self) -> Vec<RideEvent> {
        if self.only_allows_new_recording() {
            return vec![RideEvent::Start];
        }
        [
            RideEvent::Start,
            RideEvent::Pause,
            RideEvent::Resume,
            RideEvent::Stop,
            RideEvent::Save,
            RideEvent::Discard,
        ]
        .into_iter()
        .filter(|event| self.apply(*event).is_ok())
        .filter(|event| *event != RideEvent::Discard || self.is_discardable())
        .collect()
    }

    /// Applies one event without performing I/O or mutating external state.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::Invalid`] when the event is not valid for the current state.
    pub fn apply(self, event: RideEvent) -> Result<Self, TransitionError> {
        self.transition(event).map(ValidatedRideTransition::next)
    }

    /// Validates an event and retains the state it was validated against.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::Invalid`] when the event is not valid for the current state.
    pub fn transition(self, event: RideEvent) -> Result<ValidatedRideTransition, TransitionError> {
        let next = match (self, event) {
            (Self::Draft, RideEvent::Start)
            | (Self::Paused | Self::Interrupted, RideEvent::Resume) => Self::Active,
            (Self::Draft, RideEvent::Import) => Self::Imported,
            (Self::Active, RideEvent::Pause) => Self::Paused,
            (Self::Active | Self::Paused, RideEvent::Stop) => Self::Stopped,
            (Self::Active | Self::Paused, RideEvent::Interrupt) => Self::Interrupted,
            (
                Self::Draft | Self::Active | Self::Paused | Self::Stopped | Self::Interrupted,
                RideEvent::Discard,
            ) => Self::Discarded,
            (Self::Stopped | Self::Interrupted, RideEvent::Save) => Self::Saved,
            _ => return Err(TransitionError::Invalid),
        };
        Ok(ValidatedRideTransition {
            previous: self,
            next,
        })
    }
}

/// A lifecycle transition validated against one specific source state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedRideTransition {
    previous: RideLifecycleState,
    next: RideLifecycleState,
}

impl ValidatedRideTransition {
    /// Returns the state used to validate this transition.
    #[must_use]
    pub const fn previous(self) -> RideLifecycleState {
        self.previous
    }

    /// Returns the validated destination state.
    #[must_use]
    pub const fn next(self) -> RideLifecycleState {
        self.next
    }
}

/// Failure to apply a ride lifecycle event.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransitionError {
    /// The event is not valid in the current state.
    #[error("ride lifecycle transition is invalid")]
    Invalid,
}
