use crate::{Duration, MonotonicTimestamp};
use uuid::Uuid;

/// Maximum telemetry age before a ride and its `ActivityKit` projection become stale.
pub const RIDE_SESSION_STALE_AFTER: Duration = Duration::from_milliseconds(2_000);

/// Stable identity for one logical ride, including reconnects of the same transport.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RideSessionIdentity {
    platform_identifier: String,
    session_id: Uuid,
}

impl RideSessionIdentity {
    /// Creates a new Rust-owned logical ride identity.
    #[must_use]
    pub fn new_session(platform_identifier: String) -> Self {
        Self::new(platform_identifier, Uuid::new_v4())
    }

    /// Creates a logical ride identity from a platform device identifier and durable session ID.
    #[must_use]
    pub const fn new(platform_identifier: String, session_id: Uuid) -> Self {
        Self {
            platform_identifier,
            session_id,
        }
    }

    /// Returns the stable platform device identifier.
    #[must_use]
    pub fn platform_identifier(&self) -> &str {
        &self.platform_identifier
    }

    /// Returns the durable identifier for this logical ride.
    #[must_use]
    pub const fn session_id(&self) -> Uuid {
        self.session_id
    }
}

/// Whether the app process is currently presenting foreground UI.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RideSessionAppPresence {
    /// The app is foregrounded.
    #[default]
    Foreground,
    /// The app is backgrounded or suspended.
    Background,
}

/// Terminal reason for a logical ride session.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RideSessionEndReason {
    /// The rider explicitly disconnected the device.
    UserDisconnect,
    /// The rider explicitly stopped the ride.
    UserStop,
    /// A different logical ride replaced this one.
    ReplacedByNewSession,
    /// Reconnection attempts were exhausted.
    ReconnectExhausted,
    /// The app explicitly reset its session.
    AppReset,
    /// The session cannot recover from a failure.
    UnrecoverableSessionFailure,
}

/// Current logical phase of one ride session.
#[non_exhaustive]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum RideSessionPhase {
    /// No logical ride exists.
    #[default]
    Idle,
    /// The ride exists and its `ActivityKit` projection is starting.
    Starting,
    /// The ride is receiving current transport data.
    Active,
    /// The same ride is waiting for transport reconnection.
    Reconnecting,
    /// Telemetry exceeded its freshness deadline.
    Stale,
    /// The logical ride is executing its terminal effects.
    Ending(RideSessionEndReason),
    /// The logical ride has completed its terminal effects.
    Ended(RideSessionEndReason),
}

/// Current state of the `ActivityKit` projection for the logical ride.
#[non_exhaustive]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ActivityProjectionState {
    /// No `ActivityKit` activity exists.
    #[default]
    Absent,
    /// `ActivityKit` has been asked to start an activity.
    Starting,
    /// `ActivityKit` confirmed an active activity.
    Active {
        /// Platform-owned `ActivityKit` identifier.
        activity_id: String,
    },
    /// The activity remains visible but its content is stale.
    Stale {
        /// Platform-owned `ActivityKit` identifier.
        activity_id: String,
    },
    /// `ActivityKit` has been asked to end the activity.
    Ending,
    /// `ActivityKit` confirmed that the activity ended.
    Ended,
    /// `ActivityKit` cannot currently project the ride.
    Unavailable,
}

/// Typed event applied to the logical ride reducer.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RideSessionInput {
    /// Starts a new logical ride.
    Start {
        /// Identity that must remain stable through reconnect and backgrounding.
        identity: RideSessionIdentity,
    },
    /// `ActivityKit` confirmed a successful start or adoption.
    ActivityStarted {
        /// Logical ride whose platform request completed.
        identity: RideSessionIdentity,
        /// Platform-owned `ActivityKit` identifier.
        activity_id: String,
    },
    /// `ActivityKit` confirmed the terminal end operation.
    ActivityEnded {
        /// Logical ride whose platform request completed.
        identity: RideSessionIdentity,
    },
    /// `ActivityKit` could not execute the requested projection.
    ActivityUnavailable {
        /// Logical ride whose platform request failed.
        identity: RideSessionIdentity,
    },
    /// The app entered the background.
    AppBackgrounded,
    /// The app returned to the foreground.
    AppForegrounded,
    /// Bluetooth transport disconnected without ending the logical ride.
    BluetoothDisconnected {
        /// Time at which the disconnect was observed.
        at: MonotonicTimestamp,
    },
    /// Bluetooth transport reconnected to the same logical ride.
    BluetoothConnected,
    /// Fresh telemetry was observed for the logical ride.
    TelemetryObserved {
        /// Time at which telemetry was observed.
        at: MonotonicTimestamp,
    },
    /// Evaluates telemetry freshness against the Rust-owned deadline.
    FreshnessChecked {
        /// Current monotonic time.
        now: MonotonicTimestamp,
    },
    /// The rider explicitly disconnected the device.
    UserDisconnected,
    /// The rider explicitly stopped the ride.
    UserStopped,
    /// The transport retry policy can no longer continue this logical ride.
    ReconnectExhausted,
    /// The app explicitly reset its logical ride session.
    AppReset,
    /// The logical ride cannot recover from a session failure.
    UnrecoverableSessionFailure,
}

/// Single platform effect requested by one reducer transition.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RideSessionEffect {
    /// No platform work is required.
    None,
    /// Start or adopt one `ActivityKit` activity.
    StartActivity {
        /// Logical ride to project.
        identity: RideSessionIdentity,
    },
    /// Update the existing `ActivityKit` activity.
    UpdateActivity {
        /// Logical ride whose activity should update.
        identity: RideSessionIdentity,
    },
    /// Mark the existing `ActivityKit` activity stale.
    MarkActivityStale {
        /// Logical ride whose activity should become stale.
        identity: RideSessionIdentity,
    },
    /// End the existing `ActivityKit` activity.
    EndActivity {
        /// Logical ride whose activity should end.
        identity: RideSessionIdentity,
        /// Terminal reason to publish.
        reason: RideSessionEndReason,
    },
    /// Flush capture data without ending the logical ride.
    RequestCaptureFlush {
        /// Logical ride whose associated capture should flush.
        identity: RideSessionIdentity,
    },
}

/// Rust-owned logical ride state and desired `ActivityKit` projection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RideSessionLifecycle {
    phase: RideSessionPhase,
    identity: Option<RideSessionIdentity>,
    pending_identity: Option<RideSessionIdentity>,
    activity: ActivityProjectionState,
    last_telemetry_at: Option<MonotonicTimestamp>,
    app_presence: RideSessionAppPresence,
}

impl RideSessionLifecycle {
    /// Returns the logical ride phase.
    #[must_use]
    pub const fn phase(&self) -> &RideSessionPhase {
        &self.phase
    }

    /// Returns the logical ride identity, when a ride exists.
    #[must_use]
    pub const fn identity(&self) -> Option<&RideSessionIdentity> {
        self.identity.as_ref()
    }

    /// Returns the desired `ActivityKit` projection state.
    #[must_use]
    pub const fn activity(&self) -> &ActivityProjectionState {
        &self.activity
    }

    /// Returns the most recent telemetry timestamp.
    #[must_use]
    pub const fn last_telemetry_at(&self) -> Option<MonotonicTimestamp> {
        self.last_telemetry_at
    }

    /// Returns whether app UI is foregrounded or backgrounded.
    #[must_use]
    pub const fn app_presence(&self) -> RideSessionAppPresence {
        self.app_presence
    }

    /// Applies one typed event and returns the next immutable state plus at most one effect.
    #[must_use]
    pub fn transition(&self, input: RideSessionInput) -> RideSessionDecision {
        match input {
            RideSessionInput::Start { identity } => self.start(identity),
            RideSessionInput::ActivityStarted {
                identity,
                activity_id,
            } => self.activity_started(&identity, activity_id),
            RideSessionInput::ActivityEnded { identity } => self.activity_ended(&identity),
            RideSessionInput::ActivityUnavailable { identity } => {
                self.activity_unavailable(&identity)
            }
            RideSessionInput::AppBackgrounded => self.app_backgrounded(),
            RideSessionInput::AppForegrounded => self.app_foregrounded(),
            RideSessionInput::BluetoothDisconnected { at } => self.bluetooth_disconnected(at),
            RideSessionInput::BluetoothConnected => self.bluetooth_connected(),
            RideSessionInput::TelemetryObserved { at } => self.telemetry_observed(at),
            RideSessionInput::FreshnessChecked { now } => {
                self.freshness_checked(now, RIDE_SESSION_STALE_AFTER)
            }
            RideSessionInput::UserDisconnected => self.end(RideSessionEndReason::UserDisconnect),
            RideSessionInput::UserStopped => self.end(RideSessionEndReason::UserStop),
            RideSessionInput::ReconnectExhausted => {
                self.end(RideSessionEndReason::ReconnectExhausted)
            }
            RideSessionInput::AppReset => self.end(RideSessionEndReason::AppReset),
            RideSessionInput::UnrecoverableSessionFailure => {
                self.end(RideSessionEndReason::UnrecoverableSessionFailure)
            }
        }
    }

    fn start(&self, identity: RideSessionIdentity) -> RideSessionDecision {
        if self.identity.as_ref() == Some(&identity)
            && !matches!(
                self.phase,
                RideSessionPhase::Idle | RideSessionPhase::Ended(_)
            )
        {
            return self.decision(RideSessionEffect::None);
        }

        if let Some(current_identity) = self.identity.clone().filter(|_| {
            !matches!(
                self.phase,
                RideSessionPhase::Idle | RideSessionPhase::Ended(_)
            )
        }) {
            let mut state = self.clone();
            state.phase = RideSessionPhase::Ending(RideSessionEndReason::ReplacedByNewSession);
            state.pending_identity = Some(identity);
            state.activity = ActivityProjectionState::Ending;
            return RideSessionDecision::new(
                state,
                RideSessionEffect::EndActivity {
                    identity: current_identity,
                    reason: RideSessionEndReason::ReplacedByNewSession,
                },
            );
        }

        RideSessionDecision::new(
            Self {
                phase: RideSessionPhase::Starting,
                identity: Some(identity.clone()),
                pending_identity: None,
                activity: ActivityProjectionState::Starting,
                last_telemetry_at: None,
                app_presence: RideSessionAppPresence::Foreground,
            },
            RideSessionEffect::StartActivity { identity },
        )
    }

    fn activity_started(
        &self,
        identity: &RideSessionIdentity,
        activity_id: String,
    ) -> RideSessionDecision {
        if self.identity.as_ref() != Some(identity)
            || !matches!(self.phase, RideSessionPhase::Starting)
        {
            return self.decision(RideSessionEffect::None);
        }
        let mut state = self.clone();
        state.phase = RideSessionPhase::Active;
        state.activity = ActivityProjectionState::Active { activity_id };
        RideSessionDecision::new(state, RideSessionEffect::None)
    }

    fn activity_ended(&self, identity: &RideSessionIdentity) -> RideSessionDecision {
        if self.identity.as_ref() != Some(identity) {
            return self.decision(RideSessionEffect::None);
        }
        let RideSessionPhase::Ending(reason) = self.phase else {
            return self.decision(RideSessionEffect::None);
        };
        if let Some(identity) = self.pending_identity.clone() {
            return RideSessionDecision::new(
                Self {
                    phase: RideSessionPhase::Starting,
                    identity: Some(identity.clone()),
                    pending_identity: None,
                    activity: ActivityProjectionState::Starting,
                    last_telemetry_at: None,
                    app_presence: self.app_presence,
                },
                RideSessionEffect::StartActivity {
                    identity: identity.clone(),
                },
            );
        }

        let mut state = self.clone();
        state.phase = RideSessionPhase::Ended(reason);
        state.activity = ActivityProjectionState::Ended;
        RideSessionDecision::new(state, RideSessionEffect::None)
    }

    fn activity_unavailable(&self, identity: &RideSessionIdentity) -> RideSessionDecision {
        if self.identity.as_ref() != Some(identity)
            || matches!(
                self.phase,
                RideSessionPhase::Idle | RideSessionPhase::Ended(_)
            )
        {
            return self.decision(RideSessionEffect::None);
        }
        let mut state = self.clone();
        state.activity = ActivityProjectionState::Unavailable;
        RideSessionDecision::new(state, RideSessionEffect::None)
    }

    fn app_backgrounded(&self) -> RideSessionDecision {
        let Some(identity) = self.identity.clone() else {
            return self.decision(RideSessionEffect::None);
        };
        if matches!(
            self.phase,
            RideSessionPhase::Idle | RideSessionPhase::Ending(_) | RideSessionPhase::Ended(_)
        ) {
            return self.decision(RideSessionEffect::None);
        }
        if self.app_presence == RideSessionAppPresence::Background {
            return self.decision(RideSessionEffect::None);
        }
        let mut state = self.clone();
        state.app_presence = RideSessionAppPresence::Background;
        RideSessionDecision::new(state, RideSessionEffect::RequestCaptureFlush { identity })
    }

    fn app_foregrounded(&self) -> RideSessionDecision {
        if self.app_presence == RideSessionAppPresence::Foreground {
            return self.decision(RideSessionEffect::None);
        }
        let mut state = self.clone();
        state.app_presence = RideSessionAppPresence::Foreground;
        RideSessionDecision::new(state, RideSessionEffect::None)
    }

    fn bluetooth_disconnected(&self, at: MonotonicTimestamp) -> RideSessionDecision {
        let Some(identity) = self.identity.clone() else {
            return self.decision(RideSessionEffect::None);
        };
        if matches!(
            self.phase,
            RideSessionPhase::Idle | RideSessionPhase::Ending(_) | RideSessionPhase::Ended(_)
        ) {
            return self.decision(RideSessionEffect::None);
        }
        let mut state = self.clone();
        state.phase = RideSessionPhase::Reconnecting;
        state.last_telemetry_at.get_or_insert(at);
        let effect = match &state.activity {
            ActivityProjectionState::Active { activity_id }
            | ActivityProjectionState::Stale { activity_id } => {
                state.activity = ActivityProjectionState::Stale {
                    activity_id: activity_id.clone(),
                };
                RideSessionEffect::MarkActivityStale { identity }
            }
            _ => RideSessionEffect::None,
        };
        RideSessionDecision::new(state, effect)
    }

    fn bluetooth_connected(&self) -> RideSessionDecision {
        if !matches!(
            self.phase,
            RideSessionPhase::Reconnecting | RideSessionPhase::Stale
        ) {
            return self.decision(RideSessionEffect::None);
        }
        let mut state = self.clone();
        state.phase = RideSessionPhase::Active;
        RideSessionDecision::new(state, RideSessionEffect::None)
    }

    fn telemetry_observed(&self, at: MonotonicTimestamp) -> RideSessionDecision {
        let Some(identity) = self.identity.clone() else {
            return self.decision(RideSessionEffect::None);
        };
        if matches!(
            self.phase,
            RideSessionPhase::Idle | RideSessionPhase::Ending(_) | RideSessionPhase::Ended(_)
        ) {
            return self.decision(RideSessionEffect::None);
        }
        let mut state = self.clone();
        state.last_telemetry_at = Some(at);
        state.phase = RideSessionPhase::Active;
        let effect = match &state.activity {
            ActivityProjectionState::Active { .. } => RideSessionEffect::UpdateActivity {
                identity: identity.clone(),
            },
            ActivityProjectionState::Stale { activity_id } => {
                state.activity = ActivityProjectionState::Active {
                    activity_id: activity_id.clone(),
                };
                RideSessionEffect::UpdateActivity { identity }
            }
            _ => RideSessionEffect::None,
        };
        RideSessionDecision::new(state, effect)
    }

    fn freshness_checked(
        &self,
        now: MonotonicTimestamp,
        stale_after: Duration,
    ) -> RideSessionDecision {
        let (Some(identity), Some(last_telemetry_at)) =
            (self.identity.clone(), self.last_telemetry_at)
        else {
            return self.decision(RideSessionEffect::None);
        };
        if matches!(
            self.phase,
            RideSessionPhase::Idle
                | RideSessionPhase::Stale
                | RideSessionPhase::Ending(_)
                | RideSessionPhase::Ended(_)
        ) || now.saturating_duration_since(last_telemetry_at) < stale_after
        {
            return self.decision(RideSessionEffect::None);
        }

        let mut state = self.clone();
        state.phase = RideSessionPhase::Stale;
        let effect = match &state.activity {
            ActivityProjectionState::Active { activity_id } => {
                state.activity = ActivityProjectionState::Stale {
                    activity_id: activity_id.clone(),
                };
                RideSessionEffect::MarkActivityStale { identity }
            }
            _ => RideSessionEffect::None,
        };
        RideSessionDecision::new(state, effect)
    }

    fn end(&self, reason: RideSessionEndReason) -> RideSessionDecision {
        let Some(identity) = self.identity.clone() else {
            return self.decision(RideSessionEffect::None);
        };
        if matches!(
            self.phase,
            RideSessionPhase::Idle | RideSessionPhase::Ended(_)
        ) {
            return self.decision(RideSessionEffect::None);
        }
        if let RideSessionPhase::Ending(existing_reason) = self.phase {
            if existing_reason == reason && self.activity == ActivityProjectionState::Unavailable {
                let mut state = self.clone();
                state.activity = ActivityProjectionState::Ending;
                return RideSessionDecision::new(
                    state,
                    RideSessionEffect::EndActivity { identity, reason },
                );
            }
            return self.decision(RideSessionEffect::None);
        }
        let mut state = self.clone();
        state.phase = RideSessionPhase::Ending(reason);
        state.pending_identity = None;
        state.activity = ActivityProjectionState::Ending;
        RideSessionDecision::new(state, RideSessionEffect::EndActivity { identity, reason })
    }

    fn decision(&self, effect: RideSessionEffect) -> RideSessionDecision {
        RideSessionDecision::new(self.clone(), effect)
    }
}

/// Immutable result of applying one lifecycle input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideSessionDecision {
    state: RideSessionLifecycle,
    effect: RideSessionEffect,
}

impl RideSessionDecision {
    const fn new(state: RideSessionLifecycle, effect: RideSessionEffect) -> Self {
        Self { state, effect }
    }

    /// Returns the next logical ride state.
    #[must_use]
    pub const fn state(&self) -> &RideSessionLifecycle {
        &self.state
    }

    /// Returns the one requested platform effect, if any.
    #[must_use]
    pub const fn effect(&self) -> &RideSessionEffect {
        &self.effect
    }

    /// Splits the decision into owned state and effect values.
    #[must_use]
    pub fn into_parts(self) -> (RideSessionLifecycle, RideSessionEffect) {
        (self.state, self.effect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MonotonicTimestamp;
    use uuid::Uuid;

    #[test]
    fn ride_session_survives_background_and_reconnect_then_ends_once() {
        let identity = RideSessionIdentity::new("vesc-1".to_owned(), Uuid::from_u128(1));
        let initial = RideSessionLifecycle::default();

        let started = initial.transition(RideSessionInput::Start {
            identity: identity.clone(),
        });
        assert_eq!(
            started.effect(),
            &RideSessionEffect::StartActivity {
                identity: identity.clone()
            }
        );
        assert_eq!(started.state().phase(), &RideSessionPhase::Starting);

        let acknowledged = started
            .state()
            .transition(RideSessionInput::ActivityStarted {
                identity: identity.clone(),
                activity_id: "activity-1".to_owned(),
            });
        assert_eq!(acknowledged.effect(), &RideSessionEffect::None);
        assert_eq!(acknowledged.state().phase(), &RideSessionPhase::Active);

        let backgrounded = acknowledged
            .state()
            .transition(RideSessionInput::AppBackgrounded);
        assert_eq!(
            backgrounded.effect(),
            &RideSessionEffect::RequestCaptureFlush {
                identity: identity.clone()
            }
        );
        assert_eq!(backgrounded.state().phase(), &RideSessionPhase::Active);
        assert_eq!(
            backgrounded.state().app_presence(),
            RideSessionAppPresence::Background
        );
        assert_eq!(backgrounded.state().identity(), Some(&identity));

        let reconnecting =
            backgrounded
                .state()
                .transition(RideSessionInput::BluetoothDisconnected {
                    at: MonotonicTimestamp::from_milliseconds(2_000),
                });
        assert_eq!(
            reconnecting.effect(),
            &RideSessionEffect::MarkActivityStale {
                identity: identity.clone()
            }
        );
        assert_eq!(
            reconnecting.state().phase(),
            &RideSessionPhase::Reconnecting
        );
        assert_eq!(reconnecting.state().identity(), Some(&identity));

        let resumed = reconnecting
            .state()
            .transition(RideSessionInput::TelemetryObserved {
                at: MonotonicTimestamp::from_milliseconds(2_500),
            });
        assert_eq!(
            resumed.effect(),
            &RideSessionEffect::UpdateActivity {
                identity: identity.clone()
            }
        );
        assert_eq!(resumed.state().phase(), &RideSessionPhase::Active);
        assert_eq!(
            resumed.state().app_presence(),
            RideSessionAppPresence::Background
        );
        assert_eq!(resumed.state().identity(), Some(&identity));

        let ending = resumed
            .state()
            .transition(RideSessionInput::UserDisconnected);
        assert_eq!(
            ending.effect(),
            &RideSessionEffect::EndActivity {
                identity: identity.clone(),
                reason: RideSessionEndReason::UserDisconnect
            }
        );
        assert_eq!(
            ending.state().phase(),
            &RideSessionPhase::Ending(RideSessionEndReason::UserDisconnect)
        );

        let ended = ending.state().transition(RideSessionInput::ActivityEnded {
            identity: identity.clone(),
        });
        assert_eq!(ended.effect(), &RideSessionEffect::None);
        assert_eq!(
            ended.state().phase(),
            &RideSessionPhase::Ended(RideSessionEndReason::UserDisconnect)
        );

        let late = ended
            .state()
            .transition(RideSessionInput::TelemetryObserved {
                at: MonotonicTimestamp::from_milliseconds(3_000),
            });
        assert_eq!(late.effect(), &RideSessionEffect::None);
        assert_eq!(late.state(), ended.state());
    }

    #[test]
    fn replacement_ends_old_identity_before_starting_new_and_rejects_late_callbacks() {
        let first = RideSessionIdentity::new("vesc-1".to_owned(), Uuid::from_u128(1));
        let second = RideSessionIdentity::new("vesc-1".to_owned(), Uuid::from_u128(2));
        let started = RideSessionLifecycle::default().transition(RideSessionInput::Start {
            identity: first.clone(),
        });
        let active = started
            .state()
            .transition(RideSessionInput::ActivityStarted {
                identity: first.clone(),
                activity_id: "activity-1".to_owned(),
            });

        let replacing = active.state().transition(RideSessionInput::Start {
            identity: second.clone(),
        });
        assert_eq!(
            replacing.effect(),
            &RideSessionEffect::EndActivity {
                identity: first.clone(),
                reason: RideSessionEndReason::ReplacedByNewSession,
            }
        );
        assert_eq!(replacing.state().identity(), Some(&first));

        let starting_second = replacing
            .state()
            .transition(RideSessionInput::ActivityEnded {
                identity: first.clone(),
            });
        assert_eq!(
            starting_second.effect(),
            &RideSessionEffect::StartActivity {
                identity: second.clone(),
            }
        );
        assert_eq!(starting_second.state().identity(), Some(&second));

        let late_first = starting_second
            .state()
            .transition(RideSessionInput::ActivityStarted {
                identity: first,
                activity_id: "late-activity".to_owned(),
            });
        assert_eq!(late_first.effect(), &RideSessionEffect::None);
        assert_eq!(late_first.state(), starting_second.state());

        let active_second = late_first
            .state()
            .transition(RideSessionInput::ActivityStarted {
                identity: second,
                activity_id: "activity-2".to_owned(),
            });
        assert_eq!(active_second.state().phase(), &RideSessionPhase::Active);
    }

    #[test]
    fn freshness_deadline_marks_activity_stale_once() {
        let identity = RideSessionIdentity::new("vesc-1".to_owned(), Uuid::from_u128(1));
        let started = RideSessionLifecycle::default().transition(RideSessionInput::Start {
            identity: identity.clone(),
        });
        let active = started
            .state()
            .transition(RideSessionInput::ActivityStarted {
                identity: identity.clone(),
                activity_id: "activity-1".to_owned(),
            });
        let fresh = active
            .state()
            .transition(RideSessionInput::TelemetryObserved {
                at: MonotonicTimestamp::from_milliseconds(1_000),
            });

        let before_deadline = fresh
            .state()
            .transition(RideSessionInput::FreshnessChecked {
                now: MonotonicTimestamp::from_milliseconds(2_999),
            });
        assert_eq!(before_deadline.effect(), &RideSessionEffect::None);
        assert_eq!(before_deadline.state().phase(), &RideSessionPhase::Active);

        let stale = before_deadline
            .state()
            .transition(RideSessionInput::FreshnessChecked {
                now: MonotonicTimestamp::from_milliseconds(3_000),
            });
        assert_eq!(
            stale.effect(),
            &RideSessionEffect::MarkActivityStale {
                identity: identity.clone(),
            }
        );
        assert_eq!(stale.state().phase(), &RideSessionPhase::Stale);

        let repeated = stale
            .state()
            .transition(RideSessionInput::FreshnessChecked {
                now: MonotonicTimestamp::from_milliseconds(4_000),
            });
        assert_eq!(repeated.effect(), &RideSessionEffect::None);
        assert_eq!(repeated.state(), stale.state());
    }

    #[test]
    fn failed_activity_end_can_be_retried_without_ending_the_ride_twice() {
        let identity = RideSessionIdentity::new("vesc-1".to_owned(), Uuid::from_u128(7));
        let started = RideSessionLifecycle::default().transition(RideSessionInput::Start {
            identity: identity.clone(),
        });
        let active = started
            .state()
            .transition(RideSessionInput::ActivityStarted {
                identity: identity.clone(),
                activity_id: "activity-1".to_owned(),
            });
        let ending = active
            .state()
            .transition(RideSessionInput::UserDisconnected);
        let unavailable = ending
            .state()
            .transition(RideSessionInput::ActivityUnavailable {
                identity: identity.clone(),
            });
        let retry = unavailable
            .state()
            .transition(RideSessionInput::UserDisconnected);

        assert_eq!(
            unavailable.state().phase(),
            &RideSessionPhase::Ending(RideSessionEndReason::UserDisconnect)
        );
        assert_eq!(
            unavailable.state().activity(),
            &ActivityProjectionState::Unavailable
        );
        assert_eq!(
            retry.effect(),
            &RideSessionEffect::EndActivity {
                identity,
                reason: RideSessionEndReason::UserDisconnect,
            }
        );
    }

    #[test]
    fn reconnect_exhaustion_ends_the_same_ride_with_the_typed_reason() {
        let identity = RideSessionIdentity::new("vesc-1".to_owned(), Uuid::from_u128(8));
        let started = RideSessionLifecycle::default().transition(RideSessionInput::Start {
            identity: identity.clone(),
        });
        let active = started
            .state()
            .transition(RideSessionInput::ActivityStarted {
                identity: identity.clone(),
                activity_id: "activity-1".to_owned(),
            });
        let reconnecting = active
            .state()
            .transition(RideSessionInput::BluetoothDisconnected {
                at: MonotonicTimestamp::from_milliseconds(2_000),
            });

        let ending = reconnecting
            .state()
            .transition(RideSessionInput::ReconnectExhausted);

        assert_eq!(
            ending.effect(),
            &RideSessionEffect::EndActivity {
                identity,
                reason: RideSessionEndReason::ReconnectExhausted,
            }
        );
        assert_eq!(
            ending.state().phase(),
            &RideSessionPhase::Ending(RideSessionEndReason::ReconnectExhausted)
        );
    }

    #[test]
    fn explicit_session_failures_preserve_their_typed_terminal_reasons() {
        for (input, reason) in [
            (RideSessionInput::AppReset, RideSessionEndReason::AppReset),
            (
                RideSessionInput::UnrecoverableSessionFailure,
                RideSessionEndReason::UnrecoverableSessionFailure,
            ),
        ] {
            let identity = RideSessionIdentity::new("vesc-1".to_owned(), Uuid::from_u128(9));
            let started = RideSessionLifecycle::default().transition(RideSessionInput::Start {
                identity: identity.clone(),
            });
            let active = started
                .state()
                .transition(RideSessionInput::ActivityStarted {
                    identity: identity.clone(),
                    activity_id: "activity-1".to_owned(),
                });

            let ending = active.state().transition(input);

            assert_eq!(
                ending.effect(),
                &RideSessionEffect::EndActivity { identity, reason }
            );
            assert_eq!(ending.state().phase(), &RideSessionPhase::Ending(reason));
        }
    }
}
