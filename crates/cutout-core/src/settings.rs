//! Shared state for typed settings writes.

use crate::{ControlRefusalReason, Duration, MonotonicTimestamp};

/// Maximum time to await matching readback for a settings write.
pub const SETTING_WRITE_CONFIRMATION_TIMEOUT: Duration = Duration::from_seconds(2);

/// Provenance for a setting value held by the state reducer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingValueSource {
    /// Value was observed in live device telemetry or readback.
    LiveReadback,

    /// Value came from a capture or replay fixture.
    CaptureReplay,

    /// Value was supplied by the user request and is not device-confirmed.
    UserRequest,

    /// The source of the value is not known.
    Unknown,
}

/// A setting value paired with its provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingValue<Value> {
    /// Typed setting value.
    pub value: Value,

    /// Evidence source for the value.
    pub source: SettingValueSource,
}

/// Rust-owned lifecycle for one typed setting write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingState<Value> {
    /// No current value or write request is known.
    Unknown,

    /// A current value is available without a pending write.
    Current(SettingValue<Value>),

    /// A write was accepted for transport and awaits matching readback.
    Pending {
        /// Most recent current value, when one exists.
        current: Option<SettingValue<Value>>,
        /// Requested value awaiting confirmation.
        requested: Value,
        /// Host monotonic time at which the write was accepted.
        submitted_at: MonotonicTimestamp,
    },

    /// Readback confirmed the requested value.
    Confirmed {
        /// Confirmed current value.
        value: SettingValue<Value>,
        /// Host monotonic time at which matching readback arrived.
        confirmed_at: MonotonicTimestamp,
    },

    /// The write was refused before transport.
    Refused {
        /// Most recent current value, when one exists.
        current: Option<SettingValue<Value>>,
        /// Requested value, when a write had been submitted.
        requested: Option<Value>,
        /// Typed refusal reason.
        reason: ControlRefusalReason,
    },

    /// No matching readback arrived before the caller's timeout.
    TimedOut {
        /// Most recent current value, when one exists.
        current: Option<SettingValue<Value>>,
        /// Requested value that was not confirmed.
        requested: Value,
    },

    /// Transport or session failure prevented completion.
    Failed {
        /// Most recent current value, when one exists.
        current: Option<SettingValue<Value>>,
        /// Requested value, when a write had been submitted.
        requested: Option<Value>,
    },
}

impl<Value> SettingState<Value>
where
    Value: Copy + Eq,
{
    /// Creates an unknown setting state.
    #[must_use]
    pub const fn unknown() -> Self {
        Self::Unknown
    }

    /// Creates a current state from an observed value and source.
    #[must_use]
    pub const fn current(value: Value, source: SettingValueSource) -> Self {
        Self::Current(SettingValue { value, source })
    }

    /// Returns the current value held by any state, if available.
    #[must_use]
    pub fn current_value(self) -> Option<Value> {
        self.current_readback().map(|value| value.value)
    }

    /// Returns the requested value held by a pending or terminal write state.
    #[must_use]
    pub const fn requested_value(self) -> Option<Value> {
        match self {
            Self::Pending { requested, .. } | Self::TimedOut { requested, .. } => Some(requested),
            Self::Refused { requested, .. } | Self::Failed { requested, .. } => requested,
            Self::Unknown | Self::Current(_) | Self::Confirmed { .. } => None,
        }
    }

    /// Records an accepted write and waits for matching readback.
    pub fn submit(&mut self, requested: Value, submitted_at: MonotonicTimestamp) {
        let current = self.current_readback();
        *self = Self::Pending {
            current,
            requested,
            submitted_at,
        };
    }

    /// Confirms a matching live readback, returning whether it matched a pending request.
    pub fn confirm(&mut self, value: Value, confirmed_at: MonotonicTimestamp) -> bool {
        self.confirm_from(value, SettingValueSource::LiveReadback, confirmed_at)
    }

    /// Confirms a matching readback with explicit provenance.
    pub fn confirm_from(
        &mut self,
        value: Value,
        source: SettingValueSource,
        confirmed_at: MonotonicTimestamp,
    ) -> bool {
        let Self::Pending { requested, .. } = *self else {
            return false;
        };
        if requested != value {
            return false;
        }
        *self = Self::Confirmed {
            value: SettingValue { value, source },
            confirmed_at,
        };
        true
    }

    /// Applies a readback, confirming a matching pending request or becoming current.
    ///
    /// A mismatched readback never overwrites a pending request. The return value is true
    /// only when the readback confirmed that request.
    pub fn observe(
        &mut self,
        value: Value,
        source: SettingValueSource,
        observed_at: MonotonicTimestamp,
    ) -> bool {
        if self.confirm_from(value, source, observed_at) {
            return true;
        }
        if !matches!(self, Self::Pending { .. }) {
            *self = Self::Current(SettingValue { value, source });
        }
        false
    }

    /// Records a typed refusal without implying that a write reached transport.
    pub fn refuse(&mut self, reason: ControlRefusalReason) {
        let current = self.current_readback();
        let requested = self.requested_value();
        *self = Self::Refused {
            current,
            requested,
            reason,
        };
    }

    /// Records a timeout when the pending request has reached its deadline.
    ///
    /// Returns whether the state transitioned to timed out.
    pub fn timeout_if_elapsed(
        &mut self,
        now: MonotonicTimestamp,
        timeout: Duration,
    ) -> bool {
        let Self::Pending { submitted_at, .. } = *self else {
            return false;
        };
        if now.saturating_duration_since(submitted_at) < timeout {
            return false;
        }
        self.timeout();
        true
    }

    /// Records a timeout for the pending request.
    pub fn timeout(&mut self) {
        let Self::Pending {
            current, requested, ..
        } = *self
        else {
            return;
        };
        *self = Self::TimedOut { current, requested };
    }

    /// Records a transport or session failure for the pending request.
    pub fn fail(&mut self) {
        let current = self.current_readback();
        let requested = self.requested_value();
        *self = Self::Failed { current, requested };
    }

    fn current_readback(self) -> Option<SettingValue<Value>> {
        match self {
            Self::Unknown => None,
            Self::Current(value) | Self::Confirmed { value, .. } => Some(value),
            Self::Pending { current, .. }
            | Self::Refused { current, .. }
            | Self::TimedOut { current, .. }
            | Self::Failed { current, .. } => current,
        }
    }
}
