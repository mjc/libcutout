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
    /// Resume a paused recording.
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
    /// Applies one event without performing I/O or mutating external state.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::Invalid`] when the event is not valid for the current state.
    pub fn apply(self, event: RideEvent) -> Result<Self, TransitionError> {
        let next = match (self, event) {
            (Self::Draft, RideEvent::Start) | (Self::Paused, RideEvent::Resume) => Self::Active,
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
        Ok(next)
    }
}

/// Failure to apply a ride lifecycle event.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransitionError {
    /// The event is not valid in the current state.
    #[error("ride lifecycle transition is invalid")]
    Invalid,
}
