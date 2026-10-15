use cutout_core::{
    CONNECTED_WHEEL_MOTION_FRESHNESS_MS, ChargeMode, MonotonicTimestamp, RideOperatingState, Speed,
    ValueQualityDto, VerificationStatus, VerificationStatusDto,
};
use uuid::Uuid;

use crate::{ChargingLocationFix, Coordinate, MonotonicMilliseconds, RideLifecycleState};
use crate::{charging_location::coordinate_sample, distance_between, select_charging_location_fix};

/// One international mile, expressed in the map domain's millimetres.
pub const CHARGING_STOP_PROMPT_RADIUS_MILLIMETRES: u64 = 1_609_344;

/// Independent evidence sources share one ride's geographic prompt history.
#[derive(Clone, Copy, Debug)]
pub enum ChargingStopEvidence {
    /// Wheel state resolved with fresh motion evidence and its original telemetry time.
    Wheel {
        /// Resolved operating state.
        operating_state: RideOperatingState,
        /// Original telemetry update time.
        telemetry_at: Option<MonotonicMilliseconds>,
    },
    /// The operating system currently reports the phone connected to external power.
    PhoneExternalPower,
}

impl ChargingStopEvidence {
    /// Resolves wheel charging evidence using the core telemetry trust and freshness rules.
    #[must_use]
    pub fn from_wheel(
        telemetry: cutout_core::TelemetrySnapshotDto,
        now: MonotonicMilliseconds,
    ) -> Self {
        let operating_state = RideOperatingState::resolve_fresh(
            telemetry.operating_state.map(Into::into),
            telemetry.charge_mode.and_then(trusted_charge_mode),
            telemetry
                .speed
                .map(|reading| Speed::from_millimetres_per_second(reading.value)),
            telemetry
                .speed_observed_at_ms
                .map(|at| MonotonicTimestamp::new(at.milliseconds)),
            Some(MonotonicTimestamp::new(now.as_u64())),
        )
        .0;

        Self::Wheel {
            operating_state,
            telemetry_at: telemetry
                .at_ms
                .map(|at| MonotonicMilliseconds::new(at.milliseconds)),
        }
    }

    fn suppression(self, now: MonotonicMilliseconds) -> Option<ChargingStopSuppression> {
        let Self::Wheel {
            operating_state,
            telemetry_at,
        } = self
        else {
            return None;
        };
        if operating_state != RideOperatingState::Charging {
            return Some(ChargingStopSuppression::NotCharging);
        }
        if !telemetry_at.is_some_and(|at| {
            now.as_u64()
                .checked_sub(at.as_u64())
                .is_some_and(|age| age <= CONNECTED_WHEEL_MOTION_FRESHNESS_MS)
        }) {
            return Some(ChargingStopSuppression::StaleTelemetry);
        }
        None
    }
}

fn trusted_charge_mode(reading: cutout_core::ChargeModeReadingDto) -> Option<ChargeMode> {
    if reading.quality != ValueQualityDto::Known {
        return None;
    }
    let verification = match reading.verification {
        VerificationStatusDto::Unverified => VerificationStatus::Unverified,
        VerificationStatusDto::Inferred => VerificationStatus::Inferred,
        VerificationStatusDto::SourceVerified => VerificationStatus::SourceVerified,
        VerificationStatusDto::HardwareVerified => VerificationStatus::HardwareVerified,
        VerificationStatusDto::SourceAndHardwareVerified => {
            VerificationStatus::SourceAndHardwareVerified
        }
    };
    verification
        .is_trusted()
        .then(|| ChargeMode::from(reading.value))
}

/// Current ride and independently resolved charging evidence.
#[derive(Clone, Copy, Debug)]
pub struct ChargingStopObservation {
    /// Canonical recording identity, unchanged across a pause or application restart.
    pub ride_id: Uuid,
    /// Current recording lifecycle.
    pub lifecycle: RideLifecycleState,
    /// A resolved wheel observation or current native phone power state.
    pub evidence: ChargingStopEvidence,
    /// Current host monotonic time.
    pub now: MonotonicMilliseconds,
    /// Latest validated phone location.
    pub location: Option<ChargingLocationFix>,
    /// Current wall time for source-location freshness.
    pub now_unix_seconds: f64,
}

/// An unsaved suggestion at a location observed during a specific ride.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargingStopPrompt {
    /// Canonical ride owning this suggestion.
    pub ride_id: Uuid,
    /// The charging location when the prompt was first offered.
    pub coordinate: Coordinate,
}

/// Why an automatic charging suggestion is currently unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargingStopSuppression {
    /// No open ride is available, or wheel evidence lacks a verified association.
    NoAssociatedRide,
    /// The recording has ended.
    RideEnded,
    /// Wheel evidence does not establish charging.
    NotCharging,
    /// Telemetry is missing, from the future or expired.
    StaleTelemetry,
    /// No fresh accurate location is available.
    LocationUnavailable,
    /// This ride has already prompted within one mile.
    NearbyPrompt,
    /// The app cannot currently show a prompt.
    PresentationUnavailable,
}

/// A prompt or an explainable suppression; neither writes a charging spot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargingStopDecision {
    /// Offer the existing charging editor at this location.
    Offer(ChargingStopPrompt),
    /// Do not interrupt the rider.
    Suppressed(ChargingStopSuppression),
}

/// Per-recording geographic prompt history and the current dismissible suggestion.
#[derive(Debug, Default)]
pub struct ChargingStopPrompts {
    ride_id: Option<Uuid>,
    locations: Vec<Coordinate>,
    pending: Option<ChargingStopPrompt>,
}

impl ChargingStopPrompts {
    /// Restores only one ride's locally retained prompt locations.
    #[must_use]
    pub fn restored(ride_id: Uuid, locations: Vec<Coordinate>) -> Self {
        Self {
            ride_id: Some(ride_id),
            locations,
            pending: None,
        }
    }

    /// Returns per-ride suppression history for local persistence.
    #[must_use]
    pub fn history(&self) -> Option<(Uuid, &[Coordinate])> {
        self.ride_id.map(|id| (id, self.locations.as_slice()))
    }

    /// Evaluates one current snapshot without inferring charging from current sign.
    pub fn observe(
        &mut self,
        observation: Option<ChargingStopObservation>,
        can_present: bool,
    ) -> ChargingStopDecision {
        let Some(observation) = observation else {
            return self.suppress(ChargingStopSuppression::NoAssociatedRide);
        };
        if self.ride_id != Some(observation.ride_id) {
            *self = Self::restored(observation.ride_id, Vec::new());
        }
        if !observation.lifecycle.is_recording() {
            return self.suppress(ChargingStopSuppression::RideEnded);
        }
        if let Some(reason) = observation.evidence.suppression(observation.now) {
            return self.suppress(reason);
        }
        let location = observation
            .location
            .and_then(|fix| select_charging_location_fix(None, fix, observation.now_unix_seconds))
            .filter(|fix| fix.horizontal_accuracy_meters() <= 100.0);
        let Some(location) = location else {
            return self.suppress(ChargingStopSuppression::LocationUnavailable);
        };
        if !can_present {
            return self.suppress(ChargingStopSuppression::PresentationUnavailable);
        }
        if let Some(pending) = self.pending {
            return ChargingStopDecision::Offer(pending);
        }
        let coordinate = location.coordinate();
        if self.locations.iter().any(|previous| {
            distance_between(coordinate_sample(*previous), coordinate_sample(coordinate)).as_u64()
                <= CHARGING_STOP_PROMPT_RADIUS_MILLIMETRES
        }) {
            return ChargingStopDecision::Suppressed(ChargingStopSuppression::NearbyPrompt);
        }
        let prompt = ChargingStopPrompt {
            ride_id: observation.ride_id,
            coordinate,
        };
        self.pending = Some(prompt);
        ChargingStopDecision::Offer(prompt)
    }

    /// Records a suggestion only after the native UI admits it for presentation.
    /// A cancelled asynchronous read cannot consume an unseen prompt location.
    pub fn presented(&mut self, prompt: ChargingStopPrompt) -> bool {
        if self.pending != Some(prompt) {
            return false;
        }
        if !self.locations.contains(&prompt.coordinate) {
            self.locations.push(prompt.coordinate);
        }
        true
    }

    /// Dismisses or accepts the current suggestion without clearing geographic history.
    pub fn dismiss(&mut self) {
        self.pending = None;
    }

    fn suppress(&mut self, reason: ChargingStopSuppression) -> ChargingStopDecision {
        self.pending = None;
        ChargingStopDecision::Suppressed(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{Measured, TelemetrySnapshot, ValueQuality, ValueSource};

    fn telemetry() -> TelemetrySnapshot {
        TelemetrySnapshot {
            at_ms: Some(MonotonicTimestamp::new(10_000)),
            speed_observed_at_ms: Some(MonotonicTimestamp::new(10_000)),
            speed: Some(Measured::reported(Speed::from_millimetres_per_second(0))),
            charge_mode: Some(Measured::reported(ChargeMode::Charging)),
            ..TelemetrySnapshot::default()
        }
    }

    fn wheel_state(snapshot: TelemetrySnapshot) -> ChargingStopEvidence {
        ChargingStopEvidence::from_wheel(snapshot.into(), MonotonicMilliseconds::new(10_000))
    }

    fn observation(ride_id: Uuid, latitude: f64) -> ChargingStopObservation {
        ChargingStopObservation {
            ride_id,
            lifecycle: RideLifecycleState::Active,
            evidence: ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                telemetry_at: Some(MonotonicMilliseconds::new(10_000)),
            },
            now: MonotonicMilliseconds::new(10_000),
            location: ChargingLocationFix::new(latitude, 0.0, 1_000.0, 5.0),
            now_unix_seconds: 1_000.0,
        }
    }

    #[test]
    fn wheel_evidence_uses_motion_guard_and_never_current_in_alone() {
        let mut snapshot = telemetry();
        assert!(matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                ..
            }
        ));
        snapshot.speed = Some(Measured::reported(Speed::from_millimetres_per_second(
            1_000,
        )));
        assert!(matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Riding,
                ..
            }
        ));
        snapshot = telemetry();
        snapshot.operating_state = Some(RideOperatingState::Riding);
        assert!(matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Riding,
                ..
            }
        ));
        snapshot = telemetry();
        snapshot.speed_observed_at_ms = Some(MonotonicTimestamp::new(4_999));
        assert!(!matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                ..
            }
        ));
        snapshot = telemetry();
        snapshot.charge_mode = None;
        snapshot.battery_current = Some(Measured::reported(
            cutout_core::BatteryCurrent::current_in(2_000),
        ));
        assert!(!matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                ..
            }
        ));
    }

    #[test]
    fn wheel_evidence_requires_trusted_known_charge_mode_unless_state_is_explicit() {
        let mut snapshot = telemetry();
        snapshot.charge_mode = Some(Measured::estimated(ChargeMode::Charging));
        assert!(!matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                ..
            }
        ));

        snapshot = telemetry();
        snapshot.charge_mode = Some(Measured {
            value: ChargeMode::Charging,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::Unverified,
        });
        assert!(!matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                ..
            }
        ));

        snapshot = telemetry();
        assert!(matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                ..
            }
        ));

        snapshot.charge_mode = None;
        snapshot.operating_state = Some(RideOperatingState::Charging);
        assert!(matches!(
            wheel_state(snapshot),
            ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                ..
            }
        ));
    }

    #[test]
    fn phone_power_can_prompt_and_shares_suppression_with_the_wheel() {
        let ride = Uuid::from_u128(1);
        let phone = ChargingStopObservation {
            evidence: ChargingStopEvidence::PhoneExternalPower,
            ..observation(ride, 0.0)
        };
        let mut prompts = ChargingStopPrompts::default();
        present_and_dismiss(&mut prompts, phone);
        assert_eq!(
            prompts.observe(Some(observation(ride, 0.0)), true),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::NearbyPrompt)
        );
        assert!(matches!(
            prompts.observe(
                Some(ChargingStopObservation {
                    ride_id: Uuid::from_u128(2),
                    ..phone
                }),
                true
            ),
            ChargingStopDecision::Offer(_)
        ));
        assert_eq!(
            prompts.observe(
                Some(ChargingStopObservation {
                    lifecycle: RideLifecycleState::Stopped,
                    ..phone
                }),
                true
            ),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::RideEnded)
        );
    }

    #[test]
    fn charging_stop_offers_a_location_without_saving_a_spot() {
        let ride_id = Uuid::from_u128(1);
        let mut prompts = ChargingStopPrompts::default();
        assert_eq!(
            prompts.observe(Some(observation(ride_id, 0.0)), true),
            ChargingStopDecision::Offer(ChargingStopPrompt {
                ride_id,
                coordinate: Coordinate::from_degrees(0.0, 0.0).unwrap(),
            })
        );
    }

    fn present_and_dismiss(
        prompts: &mut ChargingStopPrompts,
        observation: ChargingStopObservation,
    ) {
        let ChargingStopDecision::Offer(prompt) = prompts.observe(Some(observation), true) else {
            panic!("expected a charging prompt");
        };
        assert!(prompts.presented(prompt));
        prompts.dismiss();
    }

    #[test]
    fn one_mile_suppression_is_per_ride_and_remembers_each_prompted_area() {
        let ride = Uuid::from_u128(1);
        let mut prompts = ChargingStopPrompts::default();
        present_and_dismiss(&mut prompts, observation(ride, 0.0));
        // 0.014 latitude degrees is about 1,557 m; 0.015 is about 1,668 m.
        assert_eq!(
            prompts.observe(Some(observation(ride, 0.014)), true),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::NearbyPrompt)
        );
        present_and_dismiss(&mut prompts, observation(ride, 0.015));
        present_and_dismiss(&mut prompts, observation(ride, 0.03));
        assert_eq!(
            prompts.observe(Some(observation(ride, 0.0)), true),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::NearbyPrompt)
        );
        assert!(matches!(
            prompts.observe(Some(observation(Uuid::from_u128(2), 0.0)), true),
            ChargingStopDecision::Offer(_)
        ));
    }

    #[test]
    fn restored_ride_keeps_suppression_but_a_new_ride_is_eligible_at_the_same_location() {
        let ride = Uuid::from_u128(1);
        let coordinate = Coordinate::from_degrees(0.0, 0.0).unwrap();
        let mut prompts = ChargingStopPrompts::restored(ride, vec![coordinate]);
        assert_eq!(
            prompts.observe(Some(observation(ride, 0.0)), true),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::NearbyPrompt)
        );
        assert!(matches!(
            prompts.observe(Some(observation(Uuid::from_u128(2), 0.0)), true),
            ChargingStopDecision::Offer(_)
        ));
    }

    #[test]
    fn unavailable_presentation_and_abandoned_reads_do_not_suppress_unseen_locations() {
        let ride = Uuid::from_u128(1);
        let mut prompts = ChargingStopPrompts::default();
        assert_eq!(
            prompts.observe(Some(observation(ride, 0.0)), false),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::PresentationUnavailable)
        );
        assert!(matches!(
            prompts.observe(Some(observation(ride, 0.0)), true),
            ChargingStopDecision::Offer(_)
        ));
        prompts.dismiss();
        assert!(matches!(
            prompts.observe(Some(observation(ride, 0.0)), true),
            ChargingStopDecision::Offer(_)
        ));
    }

    #[test]
    fn repeated_callbacks_retain_one_prompt_and_dismissal_suppresses_it() {
        let ride = Uuid::from_u128(1);
        let mut prompts = ChargingStopPrompts::default();
        let first = prompts.observe(Some(observation(ride, 0.0)), true);
        assert_eq!(
            first,
            prompts.observe(Some(observation(ride, 0.0001)), true)
        );
        let ChargingStopDecision::Offer(prompt) = first else {
            panic!("expected prompt")
        };
        assert!(prompts.presented(prompt));
        assert!(prompts.presented(prompt));
        assert_eq!(prompts.history().unwrap().1.len(), 1);
        prompts.dismiss();
        assert_eq!(
            prompts.observe(Some(observation(ride, 0.0)), true),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::NearbyPrompt)
        );
    }

    #[test]
    fn stale_future_missing_or_inaccurate_evidence_never_prompts() {
        let mut prompts = ChargingStopPrompts::default();
        let eligible = observation(Uuid::from_u128(1), 0.0);
        for telemetry_at in [
            None,
            Some(MonotonicMilliseconds::new(4_999)),
            Some(MonotonicMilliseconds::new(10_001)),
        ] {
            assert_eq!(
                prompts.observe(
                    Some(ChargingStopObservation {
                        evidence: ChargingStopEvidence::Wheel {
                            operating_state: RideOperatingState::Charging,
                            telemetry_at,
                        },
                        ..eligible
                    }),
                    true
                ),
                ChargingStopDecision::Suppressed(ChargingStopSuppression::StaleTelemetry)
            );
        }
        for location in [
            None,
            ChargingLocationFix::new(0.0, 0.0, 879.0, 5.0),
            ChargingLocationFix::new(0.0, 0.0, 1_001.0, 5.0),
            ChargingLocationFix::new(0.0, 0.0, 1_000.0, 101.0),
        ] {
            assert_eq!(
                prompts.observe(
                    Some(ChargingStopObservation {
                        location,
                        ..eligible
                    }),
                    true
                ),
                ChargingStopDecision::Suppressed(ChargingStopSuppression::LocationUnavailable)
            );
        }
        for operating_state in [
            RideOperatingState::Unknown,
            RideOperatingState::Parked,
            RideOperatingState::Standing,
            RideOperatingState::Riding,
        ] {
            assert_eq!(
                prompts.observe(
                    Some(ChargingStopObservation {
                        evidence: ChargingStopEvidence::Wheel {
                            operating_state,
                            telemetry_at: Some(eligible.now),
                        },
                        ..eligible
                    }),
                    true
                ),
                ChargingStopDecision::Suppressed(ChargingStopSuppression::NotCharging)
            );
        }
        assert_eq!(
            prompts.observe(None, true),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::NoAssociatedRide)
        );
    }

    #[test]
    fn paused_ride_can_prompt_but_ended_ride_cannot_and_invalidates_pending_offer() {
        let mut prompts = ChargingStopPrompts::default();
        let eligible = observation(Uuid::from_u128(1), 0.0);
        let decision = prompts.observe(
            Some(ChargingStopObservation {
                lifecycle: RideLifecycleState::Paused,
                ..eligible
            }),
            true,
        );
        let ChargingStopDecision::Offer(prompt) = decision else {
            panic!("expected prompt")
        };
        assert_eq!(
            prompts.observe(
                Some(ChargingStopObservation {
                    lifecycle: RideLifecycleState::Stopped,
                    ..eligible
                }),
                true
            ),
            ChargingStopDecision::Suppressed(ChargingStopSuppression::RideEnded)
        );
        assert!(!prompts.presented(prompt));
    }
}
