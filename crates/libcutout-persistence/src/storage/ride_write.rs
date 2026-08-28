use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

use cutout_ride_maps::{
    LocationAdmission, LocationSample, MonotonicMilliseconds, RideDurationMilliseconds, RideEvent,
    RideLifecycleState, RideLifecycleTiming, RideMapSegmentId, TransitionError,
    clamped_transition_timestamp, distance_between,
};

use super::{RideSource, RoutePoint};

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
    latest_monotonic_ms: Option<u64>,
}

impl RideWriteState {
    #[cfg(test)]
    const fn with_duration(
        source: RideSource,
        lifecycle: RideLifecycleState,
        updated_at_ms: u64,
        monotonic_created_at_ms: Option<u64>,
        duration_ms: u64,
        paused_at_ms: Option<u64>,
        paused_duration_ms: u64,
    ) -> Self {
        Self::with_duration_and_latest(
            source,
            lifecycle,
            updated_at_ms,
            monotonic_created_at_ms,
            duration_ms,
            paused_at_ms,
            paused_duration_ms,
            None,
        )
    }

    pub(super) const fn with_duration_and_latest(
        source: RideSource,
        lifecycle: RideLifecycleState,
        updated_at_ms: u64,
        monotonic_created_at_ms: Option<u64>,
        duration_ms: u64,
        paused_at_ms: Option<u64>,
        paused_duration_ms: u64,
        latest_monotonic_ms: Option<u64>,
    ) -> Self {
        Self {
            source,
            lifecycle,
            updated_at_ms,
            monotonic_created_at_ms,
            duration_ms,
            paused_at_ms,
            paused_duration_ms,
            latest_monotonic_ms,
        }
    }

    pub(super) fn transition_at(
        self,
        event: RideEvent,
        occurred_at_ms: u64,
        monotonic_at_ms: Option<u64>,
    ) -> Result<RideTransition, TransitionError> {
        let lifecycle = self.lifecycle.apply(event)?;
        let monotonic_at_ms = monotonic_at_ms.map(|value| {
            clamped_transition_timestamp(
                self.monotonic_created_at_ms
                    .map(MonotonicMilliseconds::new)
                    .unwrap_or_default(),
                self.latest_monotonic_ms.map(MonotonicMilliseconds::new),
                MonotonicMilliseconds::new(value),
            )
            .as_u64()
        });
        let timing = self.timing().transition(
            self.lifecycle,
            lifecycle,
            self.monotonic_created_at_ms.map(MonotonicMilliseconds::new),
            monotonic_at_ms.map(MonotonicMilliseconds::new),
        );
        Ok(RideTransition {
            lifecycle,
            updated_at_ms: self.updated_at_ms.max(occurred_at_ms),
            duration_ms: timing.duration_milliseconds.as_u64(),
            paused_at_ms: timing
                .paused_at_milliseconds
                .map(MonotonicMilliseconds::as_u64),
            paused_duration_ms: timing.paused_duration_milliseconds.as_u64(),
        })
    }

    pub(super) fn duration_at(&self, monotonic_at_ms: u64) -> u64 {
        self.timing()
            .duration_at(
                Some(self.lifecycle),
                self.monotonic_created_at_ms.map(MonotonicMilliseconds::new),
                MonotonicMilliseconds::new(monotonic_at_ms),
            )
            .as_u64()
    }

    fn timing(self) -> RideLifecycleTiming {
        RideLifecycleTiming {
            duration_milliseconds: RideDurationMilliseconds::new(self.duration_ms),
            paused_at_milliseconds: self.paused_at_ms.map(MonotonicMilliseconds::new),
            paused_duration_milliseconds: RideDurationMilliseconds::new(self.paused_duration_ms),
        }
    }

    pub(super) fn decide_location(
        self,
        previous: Option<RoutePoint>,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        mode: LocationWriteMode,
    ) -> Result<LocationWriteDecision, RideLifecycleState> {
        if !self.accepts_location(mode) {
            return Err(self.lifecycle);
        }

        let previous_sample = previous.as_ref().map(|point| point.sample());
        let admission = sample.admission(previous_sample.as_ref());
        if admission != LocationAdmission::Accepted {
            return Ok(LocationWriteDecision::Rejected(admission));
        }

        Ok(LocationWriteDecision::Accepted {
            distance_millimetres: previous
                .filter(|previous| previous.segment_id() == segment_id)
                .map(|previous| distance_between(previous.sample(), sample).as_u64())
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

    #[test]
    fn terminal_transition_without_monotonic_time_clears_pause_marker() {
        for event in [RideEvent::Stop, RideEvent::Interrupt] {
            let state = RideWriteState::with_duration(
                RideSource::Live,
                RideLifecycleState::Paused,
                20,
                Some(1_000),
                4_000,
                Some(5_000),
                2_000,
            );

            let transition = state.transition_at(event, 10_000, None).unwrap();

            assert_eq!(transition.duration_milliseconds(), 4_000);
            assert_eq!(transition.paused_at_milliseconds(), None);
            assert_eq!(transition.paused_duration_milliseconds(), 2_000);
        }
    }

    #[test]
    fn transition_time_is_clamped_to_the_latest_durable_sample() {
        let state = RideWriteState::with_duration_and_latest(
            RideSource::Live,
            RideLifecycleState::Active,
            20,
            Some(1_000),
            0,
            None,
            0,
            Some(5_000),
        );

        let transition = state
            .transition_at(RideEvent::Pause, 30, Some(3_000))
            .unwrap();

        assert_eq!(transition.paused_at_milliseconds(), Some(5_000));
        assert_eq!(transition.duration_milliseconds(), 4_000);
    }

    #[test]
    fn paused_location_duration_excludes_the_in_flight_pause() {
        let state = RideWriteState::with_duration(
            RideSource::Live,
            RideLifecycleState::Paused,
            20,
            Some(1_000),
            4_000,
            Some(5_000),
            0,
        );
        let sample = LocationSample::new(
            Coordinate::from_degrees(40.0, -105.0).unwrap(),
            10_000,
            1_700_000_010_000,
            None,
            LocationSource::Live,
        );

        let LocationWriteDecision::Accepted {
            duration_milliseconds,
            ..
        } = state
            .decide_location(
                None,
                sample,
                RideMapSegmentId::new(0),
                LocationWriteMode::Live,
            )
            .unwrap()
        else {
            panic!("paused location should be admitted");
        };

        assert_eq!(duration_milliseconds, 4_000);
    }
}
