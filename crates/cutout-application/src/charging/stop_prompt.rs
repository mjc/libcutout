use cutout_ride_maps::{ChargingStopDecision, ChargingStopObservation, ChargingStopPrompts};

/// Portable per-ride admission and suppression policy for charging-stop suggestions.
#[derive(Debug, Default)]
pub struct ChargingStopPromptService {
    prompts: ChargingStopPrompts,
}

impl ChargingStopPromptService {
    /// Restores local history for one ride.
    #[must_use]
    pub fn restored(ride_id: uuid::Uuid, locations: Vec<cutout_ride_maps::Coordinate>) -> Self {
        Self {
            prompts: ChargingStopPrompts::restored(ride_id, locations),
        }
    }

    /// Returns current local suppression history.
    #[must_use]
    pub fn history(&self) -> Option<(uuid::Uuid, &[cutout_ride_maps::Coordinate])> {
        self.prompts.history()
    }

    /// Evaluates evidence and presentation availability without saving a spot.
    pub fn observe(
        &mut self,
        observation: Option<ChargingStopObservation>,
        can_present: bool,
    ) -> ChargingStopDecision {
        self.prompts.observe(observation, can_present)
    }

    /// Records a prompt only after native UI admits it for presentation.
    pub fn presented(&mut self, prompt: cutout_ride_maps::ChargingStopPrompt) -> bool {
        self.prompts.presented(prompt)
    }

    /// Dismisses the pending prompt while retaining per-ride suppression history.
    pub fn dismiss(&mut self) {
        self.prompts.dismiss();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_ride_maps::{
        ChargingLocationFix, ChargingStopEvidence, MonotonicMilliseconds, RideLifecycleState,
    };

    #[test]
    fn only_an_admitted_prompt_consumes_a_location_and_dismissal_keeps_history() {
        let ride_id = uuid::Uuid::from_u128(1);
        let observation = ChargingStopObservation {
            ride_id,
            lifecycle: RideLifecycleState::Active,
            evidence: ChargingStopEvidence::Wheel {
                operating_state: cutout_core::RideOperatingState::Charging,
                telemetry_at: Some(MonotonicMilliseconds::new(10_000)),
            },
            now: MonotonicMilliseconds::new(10_000),
            location: ChargingLocationFix::new(0.0, 0.0, 1_000.0, 5.0),
            now_unix_seconds: 1_000.0,
        };
        let mut service = ChargingStopPromptService::default();
        let ChargingStopDecision::Offer(prompt) = service.observe(Some(observation), true) else {
            panic!("expected prompt");
        };
        assert_eq!(service.history(), Some((ride_id, &[][..])));
        assert!(service.presented(prompt));
        service.dismiss();
        assert_eq!(service.history().map(|(_, points)| points.len()), Some(1));
    }
}
