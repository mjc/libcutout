use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CutoutSessionStateHandle, MobileMapCoordinateDto, MobilePhoneLocationSampleDto,
    MobileRideMapCore, MobileRideMapCoreInner, ride_maps,
};

/// One unsaved location suggested while the wheel or phone is charging during an open ride.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingStopPromptDto {
    /// Canonical recording identity.
    pub ride_id: String,
    /// GPS coordinate at charging detection.
    pub coordinate: MobileMapCoordinateDto,
}

/// Current native clock, location and presentation availability; device truth stays in Rust.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingStopContextDto {
    /// Latest phone location, with original timestamp and accuracy.
    pub location: Option<MobilePhoneLocationSampleDto>,
    /// Current session monotonic clock.
    pub now_ms: u64,
    /// Current Unix wall time for GPS freshness.
    pub now_unix_seconds: f64,
    /// Whether native UI can currently show a prompt without replacing another editor.
    pub can_present: bool,
    /// Current iOS battery state is charging or full on external power.
    pub phone_connected_to_power: bool,
}

/// A suggestion or the portable policy's reason for suppressing it.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum MobileChargingStopDecisionDto {
    /// Offer the normal spot editor; nothing has been saved.
    Offer { prompt: MobileChargingStopPromptDto },
    /// No open ride, or the wheel trigger has no verified associated connection.
    NoAssociatedRide,
    /// Recording has ended.
    RideEnded,
    /// Current resolved wheel state does not establish charging.
    NotCharging,
    /// Telemetry is missing, future-dated or expired.
    StaleTelemetry,
    /// GPS is absent, stale, invalid or too imprecise.
    LocationUnavailable,
    /// Already prompted within a mile during this ride.
    NearbyPrompt,
    /// Native presentation is currently unavailable.
    PresentationUnavailable,
}

impl From<ride_maps::ChargingStopDecision> for MobileChargingStopDecisionDto {
    fn from(decision: ride_maps::ChargingStopDecision) -> Self {
        use ride_maps::ChargingStopSuppression as Reason;
        match decision {
            ride_maps::ChargingStopDecision::Offer(prompt) => Self::Offer {
                prompt: prompt.into(),
            },
            ride_maps::ChargingStopDecision::Suppressed(reason) => match reason {
                Reason::NoAssociatedRide => Self::NoAssociatedRide,
                Reason::RideEnded => Self::RideEnded,
                Reason::NotCharging => Self::NotCharging,
                Reason::StaleTelemetry => Self::StaleTelemetry,
                Reason::LocationUnavailable => Self::LocationUnavailable,
                Reason::NearbyPrompt => Self::NearbyPrompt,
                Reason::PresentationUnavailable => Self::PresentationUnavailable,
            },
        }
    }
}

impl From<ride_maps::ChargingStopPrompt> for MobileChargingStopPromptDto {
    fn from(prompt: ride_maps::ChargingStopPrompt) -> Self {
        Self {
            ride_id: prompt.ride_id.to_string(),
            coordinate: MobileMapCoordinateDto {
                latitude_degrees: prompt.coordinate.latitude_degrees(),
                longitude_degrees: prompt.coordinate.longitude_degrees(),
            },
        }
    }
}

#[derive(Deserialize, Serialize)]
struct PromptHistory {
    ride_id: Uuid,
    locations: Vec<(i32, i32)>,
}

impl PromptHistory {
    fn restore(
        self,
    ) -> Option<cutout_application::charging::stop_prompt::ChargingStopPromptService> {
        let locations = self
            .locations
            .into_iter()
            .map(|(lat, lon)| ride_maps::Coordinate::from_fixed_parts(lat, lon).ok())
            .collect::<Option<Vec<_>>>()?;
        Some(
            cutout_application::charging::stop_prompt::ChargingStopPromptService::restored(
                self.ride_id,
                locations,
            ),
        )
    }
}

/// Rust-owned per-ride, per-location charging-stop admission and suppression history.
#[derive(Debug, uniffi::Object)]
pub struct MobileChargingStopPrompts {
    state: Mutex<cutout_application::charging::stop_prompt::ChargingStopPromptService>,
}

#[uniffi::export]
impl MobileChargingStopPrompts {
    /// Restores local suppression history. A different ride automatically starts fresh.
    #[uniffi::constructor]
    #[must_use]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns the optional native preference string"
    )]
    pub fn new(history_json: Option<String>) -> Self {
        let state = history_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<PromptHistory>(json).ok())
            .and_then(PromptHistory::restore)
            .unwrap_or_default();
        Self {
            state: Mutex::new(state),
        }
    }

    /// Acknowledges actual native presentation and returns local-only suppression history.
    /// Invalid or retired prompt identities cannot consume a location.
    #[must_use]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns prompt records at the presentation boundary"
    )]
    pub fn presented(&self, prompt: MobileChargingStopPromptDto) -> Option<String> {
        let prompt = ride_maps::ChargingStopPrompt {
            ride_id: Uuid::parse_str(&prompt.ride_id).ok()?,
            coordinate: ride_maps::Coordinate::from_degrees(
                prompt.coordinate.latitude_degrees,
                prompt.coordinate.longitude_degrees,
            )
            .ok()?,
        };
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !state.presented(prompt) {
            return None;
        }
        let (ride_id, locations) = state.history()?;
        serde_json::to_string(&PromptHistory {
            ride_id,
            locations: locations
                .iter()
                .map(|coordinate| {
                    (
                        coordinate.latitude().as_i32(),
                        coordinate.longitude().as_i32(),
                    )
                })
                .collect(),
        })
        .ok()
    }

    /// Accepting or dismissing never clears this ride's prompted areas.
    pub fn dismiss(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .dismiss();
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileChargingStopPrompts {
    /// Reads authoritative connection and recording facts away from the native UI thread.
    pub async fn observe(
        self: Arc<Self>,
        ride: Arc<MobileRideMapCore>,
        session: Arc<CutoutSessionStateHandle>,
        context: MobileChargingStopContextDto,
    ) -> MobileChargingStopDecisionDto {
        tokio::task::spawn_blocking(move || {
            let observation = charging_observation(&ride, &session, context);
            self.state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .observe(observation, context.can_present)
                .into()
        })
        .await
        .unwrap_or(MobileChargingStopDecisionDto::NoAssociatedRide)
    }
}

fn charging_observation(
    ride: &MobileRideMapCore,
    session: &CutoutSessionStateHandle,
    context: MobileChargingStopContextDto,
) -> Option<ride_maps::ChargingStopObservation> {
    if context.phone_connected_to_power {
        let state = ride.inner.lock().unwrap_or_else(PoisonError::into_inner);
        return observation_for_ride(
            &state,
            ride_maps::ChargingStopEvidence::PhoneExternalPower,
            context,
        );
    }
    // Match connection and telemetry under one owner; release it before acquiring the map owner.
    let (platform_identifier, generation, telemetry) = {
        let session = session.lock_inner();
        let connection = &session.session_state().connection;
        let token = connection.snapshot().token.as_ref()?;
        let verified = connection.verified_attempt(token)?;
        (
            verified.platform_identifier().to_owned(),
            verified.generation(),
            session.telemetry_snapshot()?,
        )
    };
    observation_for_connection(ride, &platform_identifier, generation, telemetry, context)
}

fn observation_for_connection(
    ride: &MobileRideMapCore,
    platform_identifier: &str,
    generation: u64,
    telemetry: cutout_core::TelemetrySnapshotDto,
    context: MobileChargingStopContextDto,
) -> Option<ride_maps::ChargingStopObservation> {
    let state = ride.inner.lock().unwrap_or_else(PoisonError::into_inner);
    state.require_ready().ok()?;
    if state.recorder.associated_vehicle() != Some(platform_identifier)
        || state.last_connection_transition_generation != Some(generation)
        || state.last_connection_transition_ride_id != state.ride_id
    {
        return None;
    }
    observation_for_ride(
        &state,
        ride_maps::ChargingStopEvidence::from_wheel(
            telemetry,
            ride_maps::MonotonicMilliseconds::new(context.now_ms),
        ),
        context,
    )
}

fn observation_for_ride(
    state: &MobileRideMapCoreInner,
    evidence: ride_maps::ChargingStopEvidence,
    context: MobileChargingStopContextDto,
) -> Option<ride_maps::ChargingStopObservation> {
    state.require_ready().ok()?;
    let location = context.location.and_then(|sample| {
        ride_maps::ChargingLocationFix::new(
            sample.latitude_degrees,
            sample.longitude_degrees,
            sample
                .source_timestamp_unix_seconds
                .unwrap_or(unix_seconds(sample.wall_clock_unix_ms)),
            sample.horizontal_accuracy_meters?,
        )
    });
    Some(ride_maps::ChargingStopObservation {
        ride_id: Uuid::from_slice(&state.ride_id.as_ref()?.bytes).ok()?,
        lifecycle: state.recorder.state()?,
        evidence,
        now: ride_maps::MonotonicMilliseconds::new(context.now_ms),
        location,
        now_unix_seconds: context.now_unix_seconds,
    })
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Current Unix milliseconds fit f64 integer precision; this is the native GPS clock's seconds representation"
)]
fn unix_seconds(milliseconds: u64) -> f64 {
    milliseconds as f64 / 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{
        ChargeMode, Measured, MonotonicTimestamp, RideOperatingState, Speed, TelemetrySnapshot,
    };

    fn location() -> MobilePhoneLocationSampleDto {
        MobilePhoneLocationSampleDto {
            wall_clock_unix_ms: 1_000_000,
            source_timestamp_unix_seconds: Some(1_000.0),
            latitude_degrees: 0.0,
            longitude_degrees: 0.0,
            altitude_meters: 0.0,
            horizontal_accuracy_meters: Some(5.0),
            vertical_accuracy_meters: None,
            speed_meters_per_second: None,
            speed_accuracy_meters_per_second: None,
            course_degrees: None,
            course_accuracy_degrees: None,
        }
    }

    #[test]
    fn charging_stop_requires_the_current_connection_to_be_associated_with_the_recording() {
        let telemetry = TelemetrySnapshot {
            at_ms: Some(MonotonicTimestamp::new(10_000)),
            speed_observed_at_ms: Some(MonotonicTimestamp::new(10_000)),
            speed: Some(Measured::reported(Speed::from_millimetres_per_second(0))),
            charge_mode: Some(Measured::reported(ChargeMode::Charging)),
            ..TelemetrySnapshot::default()
        }
        .into();
        let ride = MobileRideMapCore::new();
        ride.ensure_recording_for_vehicle_with_connection_generation(
            "wheel",
            1_000,
            Some(3),
            Some(crate::MobileRideMapAutomaticRecordingPolicyDto::StartAndResume),
        )
        .unwrap();
        let context = MobileChargingStopContextDto {
            location: Some(location()),
            now_ms: 10_000,
            now_unix_seconds: 1_000.0,
            can_present: true,
            phone_connected_to_power: false,
        };
        assert!(observation_for_connection(&ride, "other-wheel", 3, telemetry, context).is_none());
        assert!(observation_for_connection(&ride, "wheel", 2, telemetry, context).is_none());
        let observation =
            observation_for_connection(&ride, "wheel", 3, telemetry, context).unwrap();
        let mut state = ride_maps::ChargingStopPrompts::default();
        assert!(matches!(
            state.observe(Some(observation), true),
            ride_maps::ChargingStopDecision::Offer(_)
        ));
    }

    #[test]
    fn phone_power_reads_the_canonical_ride_without_requiring_wheel_connection() {
        let ride = MobileRideMapCore::new();
        let context = MobileChargingStopContextDto {
            location: Some(location()),
            now_ms: 10_000,
            now_unix_seconds: 1_000.0,
            can_present: true,
            phone_connected_to_power: true,
        };
        let session = CutoutSessionStateHandle::new();
        let observe = || charging_observation(&ride, &session, context);
        assert!(observe().is_none());
        ride.ensure_recording_for_vehicle_with_connection_generation(
            "wheel",
            1_000,
            Some(3),
            Some(crate::MobileRideMapAutomaticRecordingPolicyDto::StartAndResume),
        )
        .unwrap();
        assert!(
            charging_observation(
                &ride,
                &session,
                MobileChargingStopContextDto {
                    phone_connected_to_power: false,
                    ..context
                }
            )
            .is_none()
        );
        let mut policy = ride_maps::ChargingStopPrompts::default();
        assert!(matches!(
            policy.observe(observe(), true),
            ride_maps::ChargingStopDecision::Offer(_)
        ));
        let mut stale = observe().unwrap();
        stale.location = None;
        assert_eq!(
            policy.observe(Some(stale), true),
            ride_maps::ChargingStopDecision::Suppressed(
                ride_maps::ChargingStopSuppression::LocationUnavailable
            )
        );
    }

    #[test]
    fn charging_stop_history_round_trips_per_ride_and_rejects_invalid_coordinates() {
        let ride_id = Uuid::from_u128(1);
        let observation = ride_maps::ChargingStopObservation {
            ride_id,
            lifecycle: ride_maps::RideLifecycleState::Active,
            evidence: ride_maps::ChargingStopEvidence::Wheel {
                operating_state: RideOperatingState::Charging,
                telemetry_at: Some(ride_maps::MonotonicMilliseconds::new(10_000)),
            },
            now: ride_maps::MonotonicMilliseconds::new(10_000),
            location: ride_maps::ChargingLocationFix::new(0.0, 0.0, 1_000.0, 5.0),
            now_unix_seconds: 1_000.0,
        };
        let policy = MobileChargingStopPrompts::new(None);
        let decision = policy
            .state
            .lock()
            .unwrap()
            .observe(Some(observation), true);
        let ride_maps::ChargingStopDecision::Offer(prompt) = decision else {
            panic!("expected prompt")
        };
        let history = policy.presented(prompt.into()).unwrap();
        policy.dismiss();
        let restored = MobileChargingStopPrompts::new(Some(history));
        assert_eq!(
            restored
                .state
                .lock()
                .unwrap()
                .observe(Some(observation), true),
            ride_maps::ChargingStopDecision::Suppressed(
                ride_maps::ChargingStopSuppression::NearbyPrompt
            )
        );
        let corrupt = PromptHistory {
            ride_id,
            locations: vec![(900_000_001, 0)],
        };
        assert!(corrupt.restore().is_none());
        assert!(
            MobileChargingStopPrompts::new(Some("invalid".into()))
                .state
                .lock()
                .unwrap()
                .history()
                .is_none()
        );
    }
}
