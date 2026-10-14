use thiserror::Error;

use crate::MonotonicMilliseconds;

/// Accumulated elapsed ride time in milliseconds.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ElapsedTimeMilliseconds(u64);

impl ElapsedTimeMilliseconds {
    /// Creates an elapsed duration from its persisted representation.
    #[must_use]
    pub const fn new(milliseconds: u64) -> Self {
        Self(milliseconds)
    }

    /// Returns the elapsed duration as milliseconds.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Availability and coverage of an elapsed-time value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElapsedTimeSummary {
    /// No elapsed-time evidence is available.
    Unavailable,
    /// The elapsed-time value covers the known ride interval.
    Complete {
        /// Known elapsed duration.
        duration: ElapsedTimeMilliseconds,
    },
    /// The duration is a known lower bound because a restart or missing interval lost coverage.
    Partial {
        /// Known elapsed duration.
        duration: ElapsedTimeMilliseconds,
    },
}

impl ElapsedTimeSummary {
    /// Returns the known duration, or `None` when elapsed time is unavailable.
    #[must_use]
    pub const fn duration(self) -> Option<ElapsedTimeMilliseconds> {
        match self {
            Self::Unavailable => None,
            Self::Complete { duration } | Self::Partial { duration } => Some(duration),
        }
    }
}

/// Integer fields used to store an elapsed-time continuation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElapsedTimeStateParts {
    /// Accumulated duration, absent when elapsed time is unavailable.
    pub duration_milliseconds: Option<u64>,
    /// Monotonic cursor for the current or interrupted interval.
    pub anchor_milliseconds: Option<u64>,
    /// Whether elapsed time currently advances with the monotonic clock.
    pub running: bool,
    /// Whether the duration is only a lower bound.
    pub partial: bool,
}

/// Invalid persisted elapsed-time state.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ElapsedTimeError {
    /// The persisted duration, anchor, running, and availability fields disagree.
    #[error("elapsed-time continuation state is inconsistent")]
    InvalidState,
}

/// Pure elapsed-time continuation for a ride lifecycle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElapsedTimeContinuation {
    parts: ElapsedTimeStateParts,
}

impl ElapsedTimeContinuation {
    /// Rehydrates a continuation from persisted scalar fields.
    ///
    /// # Errors
    ///
    /// Returns [`ElapsedTimeError::InvalidState`] when an unavailable duration has live state or
    /// a running continuation has no monotonic anchor.
    pub fn from_state_parts(parts: ElapsedTimeStateParts) -> Result<Self, ElapsedTimeError> {
        if (parts.duration_milliseconds.is_none()
            && (parts.anchor_milliseconds.is_some() || parts.running || parts.partial))
            || (parts.running && parts.anchor_milliseconds.is_none())
        {
            return Err(ElapsedTimeError::InvalidState);
        }
        Ok(Self { parts })
    }

    /// Starts a ride with a complete zero-duration elapsed-time value.
    #[must_use]
    pub const fn started(at: MonotonicMilliseconds) -> Self {
        Self {
            parts: ElapsedTimeStateParts {
                duration_milliseconds: Some(0),
                anchor_milliseconds: Some(at.as_u64()),
                running: true,
                partial: false,
            },
        }
    }

    /// Creates a frozen continuation from a previously published summary.
    #[must_use]
    pub const fn from_summary(summary: ElapsedTimeSummary) -> Self {
        match summary {
            ElapsedTimeSummary::Unavailable => Self {
                parts: ElapsedTimeStateParts {
                    duration_milliseconds: None,
                    anchor_milliseconds: None,
                    running: false,
                    partial: false,
                },
            },
            ElapsedTimeSummary::Complete { duration } => Self {
                parts: ElapsedTimeStateParts {
                    duration_milliseconds: Some(duration.as_u64()),
                    anchor_milliseconds: None,
                    running: false,
                    partial: false,
                },
            },
            ElapsedTimeSummary::Partial { duration } => Self {
                parts: ElapsedTimeStateParts {
                    duration_milliseconds: Some(duration.as_u64()),
                    anchor_milliseconds: None,
                    running: false,
                    partial: true,
                },
            },
        }
    }

    /// Advances a live interval to `at`, ignoring timestamps earlier than its anchor.
    pub fn observe(&mut self, at: MonotonicMilliseconds) {
        if !self.parts.running {
            return;
        }
        let Some(anchor) = self.parts.anchor_milliseconds else {
            return;
        };
        let at = at.as_u64();
        if at <= anchor {
            return;
        }
        self.add_duration(at - anchor);
        self.parts.anchor_milliseconds = Some(at);
    }

    /// Projects a summary to `at` without mutating the continuation.
    #[must_use]
    pub fn projected_at(self, at: MonotonicMilliseconds) -> ElapsedTimeSummary {
        let Some(duration) = self.parts.duration_milliseconds else {
            return ElapsedTimeSummary::Unavailable;
        };
        let projected = if self.parts.running {
            let elapsed = self
                .parts
                .anchor_milliseconds
                .map_or(0, |anchor| at.as_u64().saturating_sub(anchor));
            ElapsedTimeMilliseconds::new(duration.saturating_add(elapsed))
        } else {
            ElapsedTimeMilliseconds::new(duration)
        };
        if self.parts.partial {
            ElapsedTimeSummary::Partial {
                duration: projected,
            }
        } else {
            ElapsedTimeSummary::Complete {
                duration: projected,
            }
        }
    }

    /// Observes `at` while the ride is paused.
    ///
    /// Elapsed time includes pauses, so this leaves the continuation running.
    pub fn paused(&mut self, at: MonotonicMilliseconds) {
        self.observe(at);
    }

    /// Observes `at` and records an interruption while retaining the anchor for same-process
    /// recovery, so the interruption gap remains part of elapsed time.
    pub fn interrupted(&mut self, at: MonotonicMilliseconds) {
        self.observe(at);
        self.parts.running = false;
    }

    /// Resumes elapsed-time accounting at `at`.
    ///
    /// A retained interruption anchor includes the same-process gap. Without an anchor, this
    /// starts a new interval; an unavailable prior duration becomes a partial zero lower bound.
    pub fn resumed(&mut self, at: MonotonicMilliseconds) {
        if self.parts.running {
            self.observe(at);
            return;
        }
        if self.parts.duration_milliseconds.is_none() {
            self.parts.duration_milliseconds = Some(0);
            self.parts.partial = true;
        }
        if let Some(anchor) = self.parts.anchor_milliseconds {
            let at = at.as_u64();
            if at > anchor {
                self.add_duration(at - anchor);
            }
        }
        self.parts.anchor_milliseconds = Some(
            self.parts
                .anchor_milliseconds
                .map_or(at.as_u64(), |anchor| anchor.max(at.as_u64())),
        );
        self.parts.running = true;
    }

    /// Observes `at` and stops, freezing the elapsed duration.
    pub fn stopped(&mut self, at: MonotonicMilliseconds) {
        self.observe(at);
        self.parts.running = false;
        self.parts.anchor_milliseconds = None;
    }

    /// Converts a restored continuation into a new process generation.
    ///
    /// Any known duration is retained as a partial lower bound because the monotonic gap across
    /// the restart cannot be measured.
    #[must_use]
    pub const fn restored_after_restart(mut self) -> Self {
        self.parts.anchor_milliseconds = None;
        self.parts.running = false;
        if self.parts.duration_milliseconds.is_some() {
            self.parts.partial = true;
        }
        self
    }

    /// Returns the current elapsed-time summary.
    #[must_use]
    pub const fn summary(self) -> ElapsedTimeSummary {
        match self.parts.duration_milliseconds {
            None => ElapsedTimeSummary::Unavailable,
            Some(milliseconds) if self.parts.partial => ElapsedTimeSummary::Partial {
                duration: ElapsedTimeMilliseconds::new(milliseconds),
            },
            Some(milliseconds) => ElapsedTimeSummary::Complete {
                duration: ElapsedTimeMilliseconds::new(milliseconds),
            },
        }
    }

    /// Returns the explicit integer fields for transactional persistence.
    #[must_use]
    pub const fn state_parts(self) -> ElapsedTimeStateParts {
        self.parts
    }

    fn add_duration(&mut self, milliseconds: u64) {
        if let Some(duration) = self.parts.duration_milliseconds {
            self.parts.duration_milliseconds = Some(duration.saturating_add(milliseconds));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ElapsedTimeContinuation, ElapsedTimeMilliseconds, ElapsedTimeStateParts,
        ElapsedTimeSummary, MonotonicMilliseconds,
    };

    fn at(milliseconds: u64) -> MonotonicMilliseconds {
        MonotonicMilliseconds::new(milliseconds)
    }

    #[test]
    fn active_pause_active_includes_the_pause_interval() {
        let mut elapsed = ElapsedTimeContinuation::started(at(0));
        elapsed.observe(at(60));
        elapsed.paused(at(60));
        elapsed.resumed(at(180));
        elapsed.observe(at(240));
        assert_eq!(
            elapsed.summary(),
            ElapsedTimeSummary::Complete {
                duration: ElapsedTimeMilliseconds::new(240),
            }
        );
    }

    #[test]
    fn projections_respect_active_and_paused_state() {
        let mut elapsed = ElapsedTimeContinuation::started(at(10));
        assert_eq!(
            elapsed.projected_at(at(40)).duration(),
            Some(ElapsedTimeMilliseconds::new(30))
        );
        elapsed.paused(at(40));
        assert_eq!(
            elapsed.projected_at(at(90)).duration(),
            Some(ElapsedTimeMilliseconds::new(80))
        );
    }

    #[test]
    fn stopped_time_stays_frozen_across_repeated_saves() {
        let mut elapsed = ElapsedTimeContinuation::started(at(0));
        elapsed.stopped(at(60));
        elapsed.stopped(at(500));
        assert_eq!(
            elapsed.summary().duration(),
            Some(ElapsedTimeMilliseconds::new(60))
        );
    }

    #[test]
    fn same_process_interrupt_resume_counts_the_gap() {
        let mut elapsed = ElapsedTimeContinuation::started(at(0));
        elapsed.interrupted(at(60));
        elapsed.resumed(at(180));
        elapsed.observe(at(240));
        assert_eq!(
            elapsed.summary().duration(),
            Some(ElapsedTimeMilliseconds::new(240))
        );
    }

    #[test]
    fn restart_discards_unknown_gap_and_marks_retained_prefix_partial() {
        let mut elapsed = ElapsedTimeContinuation::started(at(0));
        elapsed.observe(at(60));
        let persisted = elapsed.state_parts();
        let restored = ElapsedTimeContinuation::from_state_parts(persisted).unwrap();
        let mut restarted = restored.restored_after_restart();
        restarted.resumed(at(1000));
        restarted.observe(at(1060));
        assert_eq!(
            restarted.summary(),
            ElapsedTimeSummary::Partial {
                duration: ElapsedTimeMilliseconds::new(120),
            }
        );
    }

    #[test]
    fn rolled_back_observation_and_projection_never_rewind_elapsed_time() {
        let mut elapsed = ElapsedTimeContinuation::started(at(10));
        elapsed.observe(at(80));
        elapsed.observe(at(30));
        assert_eq!(elapsed.projected_at(at(20)), elapsed.projected_at(at(80)));
        assert_eq!(
            elapsed.summary().duration(),
            Some(ElapsedTimeMilliseconds::new(70))
        );
    }

    #[test]
    fn rolled_back_resume_cannot_recount_the_prior_interval() {
        let mut elapsed = ElapsedTimeContinuation::started(at(10));
        elapsed.interrupted(at(80));
        elapsed.resumed(at(30));
        elapsed.observe(at(80));
        assert_eq!(
            elapsed.summary().duration(),
            Some(ElapsedTimeMilliseconds::new(70))
        );
    }

    #[test]
    fn resuming_unavailable_time_starts_a_partial_lower_bound() {
        let mut elapsed = ElapsedTimeContinuation::default();
        elapsed.resumed(at(10));
        elapsed.observe(at(30));
        assert_eq!(
            elapsed.summary(),
            ElapsedTimeSummary::Partial {
                duration: ElapsedTimeMilliseconds::new(20),
            }
        );
    }

    #[test]
    fn impossible_hydrated_state_is_rejected() {
        let parts = ElapsedTimeStateParts {
            duration_milliseconds: None,
            anchor_milliseconds: Some(10),
            running: true,
            partial: false,
        };
        assert!(ElapsedTimeContinuation::from_state_parts(parts).is_err());
    }

    #[test]
    fn saved_summary_has_no_live_projection() {
        let saved = ElapsedTimeContinuation::from_summary(ElapsedTimeSummary::Complete {
            duration: ElapsedTimeMilliseconds::new(60),
        });
        assert_eq!(
            saved.projected_at(at(1000)).duration(),
            Some(ElapsedTimeMilliseconds::new(60))
        );
    }
}
