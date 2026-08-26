use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

use cutout_ride_maps::{
    LocationAdmission, LocationSample, RideEvent, RideLifecycleState, RideMapSegmentId,
    TransitionError, distance_between,
};

use super::RideSource;

#[derive(Clone, Copy)]
pub(super) enum LocationWriteMode {
    Live,
    PevcapImport,
}

#[derive(Clone, Copy)]
pub(super) struct RideWriteState {
    source: RideSource,
    lifecycle: RideLifecycleState,
    updated_at_ms: u64,
    monotonic_created_at_ms: Option<u64>,
    duration_ms: u64,
    paused_at_ms: Option<u64>,
    paused_duration_ms: u64,
}

impl RideWriteState {
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
            updated_at_ms,
            monotonic_created_at_ms,
            duration_ms,
            paused_at_ms,
            paused_duration_ms,
        }
    }

    pub(super) fn transition_at(
        self,
        event: RideEvent,
        occurred_at_ms: u64,
        monotonic_at_ms: Option<u64>,
    ) -> Result<RideTransition, TransitionError> {
        let lifecycle = self.lifecycle.apply(event)?;
        let mut duration_ms = self.duration_ms;
        let mut paused_at_ms = self.paused_at_ms;
        let mut paused_duration_ms = self.paused_duration_ms;

        if let Some(at_ms) = monotonic_at_ms {
            match (self.lifecycle, lifecycle) {
                (RideLifecycleState::Active, RideLifecycleState::Paused) => {
                    duration_ms = duration_ms.max(self.active_duration_at(at_ms));
                    paused_at_ms = Some(at_ms.max(self.monotonic_created_at_ms.unwrap_or(at_ms)));
                }
                (RideLifecycleState::Paused, RideLifecycleState::Active) => {
                    if let Some(paused_at_ms) = paused_at_ms {
                        paused_duration_ms =
                            paused_duration_ms.saturating_add(at_ms.saturating_sub(paused_at_ms));
                    }
                    paused_at_ms = None;
                }
                (
                    RideLifecycleState::Active | RideLifecycleState::Paused,
                    RideLifecycleState::Stopped | RideLifecycleState::Interrupted,
                ) => {
                    duration_ms = duration_ms.max(self.active_duration_at(at_ms));
                    if let Some(paused_at_ms) = paused_at_ms.take() {
                        paused_duration_ms =
                            paused_duration_ms.saturating_add(at_ms.saturating_sub(paused_at_ms));
                    }
                }
                _ => {}
            }
        }
        Ok(RideTransition {
            lifecycle,
            updated_at_ms: self.updated_at_ms.max(occurred_at_ms),
            duration_ms,
            paused_at_ms,
            paused_duration_ms,
        })
    }

    pub(super) const fn duration_at(&self, monotonic_at_ms: u64) -> u64 {
        let active_duration = self.active_duration_at(monotonic_at_ms);
        if active_duration > self.duration_ms {
            active_duration
        } else {
            self.duration_ms
        }
    }

    const fn active_duration_at(&self, monotonic_at_ms: u64) -> u64 {
        match self.monotonic_created_at_ms {
            Some(start_ms) => {
                let current_pause_ms = match self.paused_at_ms {
                    Some(paused_at_ms) => monotonic_at_ms.saturating_sub(paused_at_ms),
                    None => 0,
                };
                monotonic_at_ms
                    .saturating_sub(start_ms)
                    .saturating_sub(current_pause_ms)
                    .saturating_sub(self.paused_duration_ms)
            }
            None => self.duration_ms,
        }
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
            duration_milliseconds: self.duration_at(sample.monotonic_milliseconds().as_u64()),
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
    updated_at_ms: u64,
    duration_ms: u64,
    paused_at_ms: Option<u64>,
    paused_duration_ms: u64,
}

impl RideTransition {
    pub(super) const fn lifecycle(&self) -> RideLifecycleState {
        self.lifecycle
    }

    pub(super) const fn updated_at_milliseconds(&self) -> u64 {
        self.updated_at_ms
    }

    pub(super) const fn duration_milliseconds(&self) -> u64 {
        self.duration_ms
    }

    pub(super) const fn paused_at_milliseconds(&self) -> Option<u64> {
        self.paused_at_ms
    }

    pub(super) const fn paused_duration_milliseconds(&self) -> u64 {
        self.paused_duration_ms
    }
}

pub(super) enum LocationWriteDecision {
    Accepted {
        distance_millimetres: u64,
        updated_at_ms: u64,
        duration_milliseconds: u64,
    },
    Rejected(LocationAdmission),
}

pub(super) fn wall_clock_now_milliseconds() -> Result<u64, SystemTimeError> {
    let milliseconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    Ok(u64::try_from(milliseconds).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use cutout_ride_maps::{Coordinate, LocationSource, RideMapSegmentId};

    use super::*;

    #[test]
    fn ride_write_state_owns_monotonic_update_time() {
        let state = RideWriteState::with_duration(
            RideSource::Live,
            RideLifecycleState::Draft,
            20,
            None,
            0,
            None,
            0,
        );
        let transition = state.transition_at(RideEvent::Start, 10, None).unwrap();
        assert_eq!(transition.updated_at_milliseconds(), 20);

        let state = RideWriteState::with_duration(
            RideSource::Live,
            transition.lifecycle(),
            20,
            None,
            0,
            None,
            0,
        );
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
            assert_eq!(transition.paused_duration_milliseconds(), 5_000);
        }
    }
}
