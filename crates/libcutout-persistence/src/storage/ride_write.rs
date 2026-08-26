use std::time::{SystemTime, UNIX_EPOCH};

use cutout_ride_maps::{
    LocationAdmission, LocationSample, RideEvent, RideLifecycleState, RideMapSegmentId,
    TransitionError, distance_between,
};

use super::{RideSource, StorageError};

#[derive(Clone, Copy)]
pub(super) enum LocationWriteMode {
    Live,
    PevcapImport,
}

#[derive(Clone, Copy)]
pub(super) struct RideWriteState {
    source: RideSource,
    lifecycle: RideLifecycleState,
    monotonic_created_at_ms: Option<u64>,
    monotonic_last_event_ms: Option<u64>,
    paused_at_ms: Option<u64>,
    paused_duration_ms: u64,
    completed_duration_ms: u64,
    updated_at_ms: u64,
}

impl RideWriteState {
    #[cfg(test)]
    pub(super) const fn with_duration(
        source: RideSource,
        lifecycle: RideLifecycleState,
        updated_at_ms: u64,
        monotonic_created_at_ms: Option<u64>,
        duration_ms: u64,
        paused_at_ms: Option<u64>,
        paused_duration_ms: u64,
    ) -> Self {
        Self {
            source,
            lifecycle,
            monotonic_created_at_ms,
            monotonic_last_event_ms: None,
            paused_at_ms,
            paused_duration_ms,
            completed_duration_ms: duration_ms,
            updated_at_ms,
        }
    }

    #[cfg(test)]
    pub(super) const fn new(
        source: RideSource,
        lifecycle: RideLifecycleState,
        updated_at_ms: u64,
    ) -> Self {
        Self::new_with_timing(source, lifecycle, None, None, None, 0, 0, updated_at_ms)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "fields mirror the persisted lifecycle timing columns"
    )]
    pub(super) const fn new_with_timing(
        source: RideSource,
        lifecycle: RideLifecycleState,
        monotonic_created_at_ms: Option<u64>,
        monotonic_last_event_ms: Option<u64>,
        paused_at_ms: Option<u64>,
        paused_duration_ms: u64,
        completed_duration_ms: u64,
        updated_at_ms: u64,
    ) -> Self {
        Self {
            source,
            lifecycle,
            monotonic_created_at_ms,
            monotonic_last_event_ms,
            paused_at_ms,
            paused_duration_ms,
            completed_duration_ms,
            updated_at_ms,
        }
    }

    #[cfg(test)]
    pub(super) fn transition(
        self,
        event: RideEvent,
        occurred_at_ms: u64,
    ) -> Result<RideTransition, TransitionError> {
        self.transition_at(event, occurred_at_ms, None)
    }

    pub(super) fn transition_at(
        self,
        event: RideEvent,
        occurred_at_ms: u64,
        monotonic_at_ms: Option<u64>,
    ) -> Result<RideTransition, TransitionError> {
        let lifecycle = self.lifecycle.apply(event)?;
        let mut monotonic_created_at_ms = self.monotonic_created_at_ms;
        let mut monotonic_last_event_ms = self.monotonic_last_event_ms;
        let mut paused_at_ms = self.paused_at_ms;
        let mut paused_duration_ms = self.paused_duration_ms;
        let mut completed_duration_ms = self.completed_duration_ms;
        let at = monotonic_at_ms
            .or(self.monotonic_last_event_ms)
            .or(self.monotonic_created_at_ms);
        if self.lifecycle == RideLifecycleState::Draft
            && lifecycle == RideLifecycleState::Active
            && monotonic_created_at_ms.is_none()
        {
            monotonic_created_at_ms = at;
        }
        if let Some(at) = at {
            let at = monotonic_last_event_ms.map_or(at, |last| last.max(at));
            monotonic_last_event_ms = Some(at);
            match (self.lifecycle, lifecycle) {
                (RideLifecycleState::Active, RideLifecycleState::Paused) => {
                    paused_at_ms = Some(at);
                }
                (RideLifecycleState::Paused, RideLifecycleState::Active) => {
                    if let Some(paused_at) = paused_at_ms.take() {
                        paused_duration_ms =
                            paused_duration_ms.saturating_add(at.saturating_sub(paused_at));
                    }
                }
                (
                    RideLifecycleState::Active | RideLifecycleState::Paused,
                    RideLifecycleState::Stopped | RideLifecycleState::Interrupted,
                ) => {
                    if self.lifecycle == RideLifecycleState::Paused {
                        completed_duration_ms = active_duration_at(
                            monotonic_created_at_ms,
                            paused_at_ms.unwrap_or(at),
                            false,
                            None,
                            paused_duration_ms,
                        );
                    } else {
                        completed_duration_ms = active_duration_at(
                            monotonic_created_at_ms,
                            at,
                            false,
                            None,
                            paused_duration_ms,
                        );
                    }
                    paused_at_ms = None;
                }
                _ => {}
            }
        } else if matches!(
            (self.lifecycle, lifecycle),
            (
                RideLifecycleState::Paused,
                RideLifecycleState::Stopped | RideLifecycleState::Interrupted,
            )
        ) {
            // Without a monotonic timestamp, preserve the known timing and close the terminal
            // state rather than retaining a pause that can never be resumed.
            paused_at_ms = None;
        }
        Ok(RideTransition {
            lifecycle,
            monotonic_created_at_ms,
            monotonic_last_event_ms,
            paused_at_ms,
            paused_duration_ms,
            completed_duration_ms,
            updated_at_ms: self.updated_at_ms.max(occurred_at_ms),
        })
    }

    pub(super) fn decide_location(
        self,
        previous: Option<(u64, LocationSample)>,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        mode: LocationWriteMode,
    ) -> Result<LocationWriteDecision, RideLifecycleState> {
        if !self.accepts_location(mode) {
            return Err(self.lifecycle);
        }

        let admission = sample.admission(previous.as_ref().map(|(_, sample)| sample));
        if admission != LocationAdmission::Accepted {
            return Ok(LocationWriteDecision::Rejected(admission));
        }

        Ok(LocationWriteDecision::Accepted {
            distance_millimetres: previous
                .filter(|(previous_segment_id, _)| *previous_segment_id == segment_id.value())
                .map(|(_, previous)| distance_between(previous, sample).as_u64())
                .unwrap_or_default(),
            updated_at_ms: self
                .updated_at_ms
                .max(sample.wall_clock_unix_milliseconds().as_u64()),
        })
    }

    const fn accepts_location(self, mode: LocationWriteMode) -> bool {
        match mode {
            LocationWriteMode::Live => {
                matches!(self.source, RideSource::Live)
                    && matches!(
                        self.lifecycle,
                        RideLifecycleState::Active | RideLifecycleState::Paused
                    )
            }
            LocationWriteMode::PevcapImport => {
                matches!(self.source, RideSource::PevcapImport)
                    && matches!(self.lifecycle, RideLifecycleState::Draft)
            }
        }
    }
}

pub(super) struct RideTransition {
    lifecycle: RideLifecycleState,
    monotonic_created_at_ms: Option<u64>,
    monotonic_last_event_ms: Option<u64>,
    paused_at_ms: Option<u64>,
    paused_duration_ms: u64,
    completed_duration_ms: u64,
    updated_at_ms: u64,
}

impl RideTransition {
    #[cfg(test)]
    pub(super) const fn duration_milliseconds(&self) -> u64 {
        self.completed_duration_ms
    }

    pub(super) const fn lifecycle(&self) -> RideLifecycleState {
        self.lifecycle
    }

    pub(super) const fn updated_at_milliseconds(&self) -> u64 {
        self.updated_at_ms
    }

    pub(super) const fn monotonic_created_at_milliseconds(&self) -> Option<u64> {
        self.monotonic_created_at_ms
    }

    pub(super) const fn monotonic_last_event_milliseconds(&self) -> Option<u64> {
        self.monotonic_last_event_ms
    }

    pub(super) const fn paused_at_milliseconds(&self) -> Option<u64> {
        self.paused_at_ms
    }

    pub(super) const fn paused_duration_milliseconds(&self) -> u64 {
        self.paused_duration_ms
    }

    pub(super) const fn completed_duration_milliseconds(&self) -> u64 {
        self.completed_duration_ms
    }
}

const fn active_duration_at(
    created_at_ms: Option<u64>,
    at_ms: u64,
    paused: bool,
    paused_at_ms: Option<u64>,
    paused_duration_ms: u64,
) -> u64 {
    let Some(created_at_ms) = created_at_ms else {
        return 0;
    };
    let current_pause = if paused {
        match paused_at_ms {
            Some(paused_at) => at_ms.saturating_sub(paused_at),
            None => 0,
        }
    } else {
        0
    };
    at_ms
        .saturating_sub(created_at_ms)
        .saturating_sub(paused_duration_ms)
        .saturating_sub(current_pause)
}

pub(super) enum LocationWriteDecision {
    Accepted {
        distance_millimetres: u64,
        updated_at_ms: u64,
    },
    Rejected(LocationAdmission),
}

pub(super) fn wall_clock_now_milliseconds() -> Result<u64, StorageError> {
    let milliseconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    u64::try_from(milliseconds).map_err(|_| StorageError::InvalidStoredValue {
        field: "wall clock milliseconds",
        value: "out of range".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use cutout_ride_maps::{Coordinate, LocationSource, RideMapSegmentId};

    use super::*;

    #[test]
    fn ride_write_state_owns_monotonic_update_time() {
        let state = RideWriteState::new(RideSource::Live, RideLifecycleState::Draft, 20);
        let transition = state.transition(RideEvent::Start, 10).unwrap();
        assert_eq!(transition.updated_at_milliseconds(), 20);

        let state = RideWriteState::new(RideSource::Live, transition.lifecycle(), 20);
        let sample = LocationSample::new(
            Coordinate::from_degrees(40.0, -105.0).unwrap(),
            1,
            30,
            None,
            LocationSource::Live,
        );
        assert!(matches!(
            state
                .decide_location(
                    None,
                    sample,
                    RideMapSegmentId::new(0),
                    LocationWriteMode::Live,
                )
                .unwrap(),
            LocationWriteDecision::Accepted {
                updated_at_ms: 30,
                ..
            }
        ));
    }

    #[test]
    fn terminal_transition_from_pause_freezes_duration_and_closes_pause() {
        for event in [RideEvent::Stop, RideEvent::Interrupt] {
            let state = RideWriteState::with_duration(
                RideSource::Live,
                RideLifecycleState::Paused,
                20,
                Some(1_000),
                4_000,
                Some(5_000),
                0,
            );

            let transition = state.transition_at(event, 10_000, Some(10_000)).unwrap();

            assert_eq!(transition.duration_milliseconds(), 4_000);
            assert_eq!(transition.paused_at_milliseconds(), None);
            assert_eq!(transition.paused_duration_milliseconds(), 0);
        }
    }

    #[test]
    fn terminal_transition_without_monotonic_time_clears_pause_marker() {
        let state = RideWriteState::with_duration(
            RideSource::Live,
            RideLifecycleState::Paused,
            20,
            Some(1_000),
            4_000,
            Some(5_000),
            2_000,
        );

        let transition = state.transition_at(RideEvent::Stop, 10_000, None).unwrap();

        assert_eq!(transition.duration_milliseconds(), 2_000);
        assert_eq!(transition.paused_at_milliseconds(), None);
        assert_eq!(transition.paused_duration_milliseconds(), 2_000);
    }
}
