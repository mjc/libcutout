use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

use cutout_ride_maps::{
    LocationAdmission, LocationSample, RideEvent, RideLifecycleState, TransitionError,
    distance_between_millimetres,
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
}

impl RideWriteState {
    pub(super) const fn new(
        source: RideSource,
        lifecycle: RideLifecycleState,
        updated_at_ms: u64,
    ) -> Self {
        Self {
            source,
            lifecycle,
            updated_at_ms,
        }
    }

    pub(super) fn transition(
        self,
        event: RideEvent,
        occurred_at_ms: u64,
    ) -> Result<RideTransition, TransitionError> {
        Ok(RideTransition {
            lifecycle: self.lifecycle.apply(event)?,
            updated_at_ms: self.updated_at_ms.max(occurred_at_ms),
        })
    }

    pub(super) fn decide_location(
        self,
        previous: Option<LocationSample>,
        sample: LocationSample,
        mode: LocationWriteMode,
    ) -> Result<LocationWriteDecision, RideLifecycleState> {
        if !self.accepts_location(mode) {
            return Err(self.lifecycle);
        }

        let admission = sample.admission(previous.as_ref());
        if admission != LocationAdmission::Accepted {
            return Ok(LocationWriteDecision::Rejected(admission));
        }

        Ok(LocationWriteDecision::Accepted {
            distance_millimetres: previous
                .map(|previous| distance_between_millimetres(previous, sample))
                .unwrap_or_default(),
            updated_at_ms: self
                .updated_at_ms
                .max(sample.wall_clock_unix_milliseconds()),
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
}

impl RideTransition {
    pub(super) const fn lifecycle(&self) -> RideLifecycleState {
        self.lifecycle
    }

    pub(super) const fn updated_at_milliseconds(&self) -> u64 {
        self.updated_at_ms
    }
}

pub(super) enum LocationWriteDecision {
    Accepted {
        distance_millimetres: u64,
        updated_at_ms: u64,
    },
    Rejected(LocationAdmission),
}

pub(super) fn wall_clock_now_milliseconds() -> Result<u64, SystemTimeError> {
    let milliseconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    Ok(u64::try_from(milliseconds).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use cutout_ride_maps::{Coordinate, LocationSource};

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
                .decide_location(None, sample, LocationWriteMode::Live)
                .unwrap(),
            LocationWriteDecision::Accepted {
                updated_at_ms: 30,
                ..
            }
        ));
    }
}
