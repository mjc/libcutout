//! Retained battery estimation, current freshness and voltage-sag learning.

use cutout_core::{
    BatteryCurrent, BatteryLevel, BatteryLevelBasis, Capacity, CapacitySource, ChargeEstimateInput,
    ChargeEstimateResetReason, ChargeEstimateState, ChargeEstimateUnavailableReason, ChargeFlow,
    ChargeMode, ChargeProfileIdentity, ChargeSessionIdentity, Duration as CoreDuration,
    EstimateConfidence, Measured, MonotonicTimestamp, PowerFlowDirection,
    RideOperatingState as CoreRideOperatingState, Speed, TelemetryFreshness, Temperature,
    UsablePackCapacity, ValueQuality, ValueQuality as CoreValueQuality, ValueSource,
    VerificationStatus, Voltage, VoltageSagEstimate, VoltageSagEstimator, VoltageSagInput,
    VoltageSagModel,
};
use std::sync::{Mutex, PoisonError};

/// Battery profile and its independent capacity and flow verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargeProfile {
    pub session_id: u64,
    pub profile_id: u32,
    pub capacity_milliamp_hours: u32,
    pub capacity_source: CapacitySource,
    pub verification: VerificationStatus,
    pub charge_flow_verification: VerificationStatus,
}

/// Original observed values and field clocks used by the estimate operation.
#[derive(Clone, Copy, Debug)]
pub struct EstimateTelemetry {
    pub at_ms: Option<MonotonicTimestamp>,
    pub battery_current_observed_at_ms: Option<MonotonicTimestamp>,
    pub voltage_observed_at_ms: Option<MonotonicTimestamp>,
    pub speed_observed_at_ms: Option<MonotonicTimestamp>,
    pub battery_current: Option<Measured<BatteryCurrent>>,
    pub voltage: Option<Measured<Voltage>>,
    pub speed: Option<Speed>,
    pub charge_mode: Option<Measured<ChargeMode>>,
    pub operating_state: CoreRideOperatingState,
    pub power_flow: Option<PowerFlowDirection>,
    pub battery_level_reported: Option<Measured<BatteryLevel>>,
    pub battery_level_estimated: Option<Measured<BatteryLevel>>,
    pub battery_temperature: Option<Measured<Temperature>>,
}

/// One evaluation clock and its independently observed telemetry.
#[derive(Clone, Copy, Debug)]
pub struct EstimateInput {
    pub at: MonotonicTimestamp,
    pub snapshot: EstimateTelemetry,
    pub freshness: CoreDuration,
}

/// Estimate state with the reset cause and current sag evidence.
#[derive(Clone, Copy, Debug)]
pub struct EstimateResult {
    pub state: ChargeEstimateState,
    pub reset_reason: Option<ChargeEstimateResetReason>,
    pub voltage_sag: Option<VoltageSagEstimate>,
}

#[derive(Debug)]
struct EstimatorState {
    estimator: cutout_core::ChargeEstimator,
    voltage_sag: VoltageSagEstimator,
    last_sag_pair: Option<(MonotonicTimestamp, MonotonicTimestamp)>,
    last_sag_estimate: Option<VoltageSagEstimate>,
    profile: Option<ChargeProfile>,
}

/// Retained battery estimate and sag-learning operation shared by application clients.
#[derive(Debug)]
pub struct ChargeEstimatorApplication {
    state: Mutex<EstimatorState>,
}

impl ChargeEstimatorApplication {
    /// Creates an empty estimator. A verified profile must be configured before use.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the confirmed NOSFET Aero pack basis: 30s2p Samsung 50S,
    /// with a 10 Ah profile capacity. Charge-flow polarity remains unverified
    /// until the device hardware matrix is complete.
    pub fn configure_nosfet_aero_30s2p_samsung_50s_profile(&self) {
        self.configure_profile(ChargeProfile {
            session_id: 43,
            profile_id: 43,
            capacity_milliamp_hours: 10_000,
            capacity_source: CapacitySource::ProtocolProfile,
            verification: VerificationStatus::SourceVerified,
            charge_flow_verification: VerificationStatus::Unverified,
        });
    }

    /// Configures the current Falcon battery basis: 24s2p Samsung 50S,
    /// 100.8 V full-charge class and approximately 900 Wh nominal energy.
    /// Charge-flow polarity remains unverified until the device hardware
    /// matrix is complete.
    pub fn configure_begode_falcon_24s2p_samsung_50s_profile(&self) {
        self.configure_profile(ChargeProfile {
            session_id: 44,
            profile_id: 44,
            capacity_milliamp_hours: 10_000,
            capacity_source: CapacitySource::ProtocolProfile,
            verification: VerificationStatus::SourceVerified,
            charge_flow_verification: VerificationStatus::Unverified,
        });
    }

    /// Replaces the usable pack profile and resets bounded history.
    pub fn configure_profile(&self, profile: ChargeProfile) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let incompatible_profile = state.profile.is_some_and(|current| {
            current.profile_id != profile.profile_id
                || current.capacity_milliamp_hours != profile.capacity_milliamp_hours
        });
        state.estimator.reset();
        if incompatible_profile {
            state.voltage_sag.reset();
        } else {
            state.voltage_sag.reset_observations();
        }
        state.last_sag_pair = None;
        state.last_sag_estimate = None;
        state.profile = Some(profile);
    }

    /// Removes the usable pack profile and resets bounded history.
    pub fn clear_profile(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.estimator.reset();
        state.voltage_sag.reset_observations();
        state.last_sag_pair = None;
        state.last_sag_estimate = None;
        state.profile = None;
    }

    /// Resets the bounded current and sag windows.
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.estimator.reset();
        state.voltage_sag.reset_observations();
        state.last_sag_pair = None;
        state.last_sag_estimate = None;
    }

    /// Returns the durable learned resistance for persistence by the platform layer.
    #[must_use]
    pub fn voltage_sag_model(&self) -> Option<VoltageSagModel> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .voltage_sag
            .model()
    }

    /// Restores a validated resistance model scoped by the caller to the current vehicle.
    #[must_use]
    pub fn restore_voltage_sag_model(&self, model: VoltageSagModel) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.last_sag_pair = None;
        state.last_sag_estimate = None;
        state.voltage_sag.restore_model(model)
    }

    /// Explicitly clears the durable learned resistance for the active EUC.
    pub fn clear_voltage_sag_model(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.voltage_sag.reset();
        state.last_sag_pair = None;
        state.last_sag_estimate = None;
    }

    /// Admits one typed telemetry sample and returns its presentation state.
    pub fn update(&self, input: EstimateInput) -> EstimateResult {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let snapshot = input.snapshot;
        let now = input.at;
        let charge_mode = resolve_charge_mode(&snapshot, now);
        let charge_flow_verification = state
            .profile
            .map_or(VerificationStatus::Unverified, |profile| {
                profile.charge_flow_verification
            });
        let current_observed_at = snapshot.battery_current_observed_at_ms;
        let current_is_fresh = current_observed_at.is_some_and(|observed_at| {
            now >= observed_at
                && now.saturating_duration_since(observed_at).as_milliseconds()
                    <= cutout_core::CONNECTED_WHEEL_MOTION_FRESHNESS_MS
        });
        let mut flow = charge_flow(&snapshot, charge_mode, charge_flow_verification);
        if !current_is_fresh {
            flow.value = ChargeFlow::Unknown;
        }
        let voltage_sag = update_voltage_sag(
            &mut state,
            &snapshot,
            now,
            input.freshness.as_milliseconds(),
            flow.value,
        );
        let Some(profile) = state.profile else {
            return EstimateResult {
                state: ChargeEstimateState::Unavailable {
                    reason: ChargeEstimateUnavailableReason::CapacityMissing,
                },
                reset_reason: None,
                voltage_sag,
            };
        };
        let battery_level = snapshot
            .battery_level_reported
            .map(BatteryLevelBasis::reported)
            .or_else(|| {
                snapshot.battery_level_estimated.map(|level| {
                    BatteryLevelBasis::profile_estimated(
                        level,
                        ChargeProfileIdentity::new(profile.profile_id),
                        EstimateConfidence::Medium,
                    )
                })
            })
            .unwrap_or(BatteryLevelBasis::Unavailable);
        let at = MonotonicTimestamp::new(input.at.as_milliseconds());
        let battery_current = current_is_fresh
            .then_some(snapshot.battery_current)
            .flatten();
        let snapshot_observed_at = snapshot.at_ms.unwrap_or(at);
        // Other telemetry fields can advance the snapshot without a new current sample.
        let observed_at = current_observed_at
            .filter(|_| {
                battery_current.is_some_and(|current| current.value.magnitude_milliamps() != 0)
            })
            .unwrap_or(snapshot_observed_at);
        let result = state.estimator.update(ChargeEstimateInput {
            session: ChargeSessionIdentity::new(profile.session_id),
            profile: ChargeProfileIdentity::new(profile.profile_id),
            at,
            observed_at,
            battery_current,
            charge_mode,
            flow,
            battery_level,
            usable_capacity: UsablePackCapacity::new(
                Capacity::from_milliamp_hours(profile.capacity_milliamp_hours),
                profile.capacity_source,
                profile.verification,
            ),
            battery_temperature: snapshot.battery_temperature,
            voltage_sag,
            freshness: TelemetryFreshness::new(CoreDuration::from_milliseconds(
                input.freshness.as_milliseconds(),
            )),
        });
        EstimateResult {
            state: result,
            reset_reason: state.estimator.last_reset_reason(),
            voltage_sag,
        }
    }
}

fn update_voltage_sag(
    state: &mut EstimatorState,
    snapshot: &EstimateTelemetry,
    now: MonotonicTimestamp,
    freshness_ms: u64,
    flow: ChargeFlow,
) -> Option<VoltageSagEstimate> {
    let current_at = snapshot.battery_current_observed_at_ms;
    let voltage_at = snapshot.voltage_observed_at_ms;
    let is_fresh = |observed_at: Option<MonotonicTimestamp>| {
        observed_at.is_some_and(|observed_at| {
            now >= observed_at
                && now.saturating_duration_since(observed_at).as_milliseconds()
                    <= cutout_core::CONNECTED_WHEEL_MOTION_FRESHNESS_MS
        })
    };
    if !is_fresh(current_at) || !is_fresh(voltage_at) {
        state.voltage_sag.reset_observations();
        state.last_sag_pair = None;
        state.last_sag_estimate = None;
        return None;
    }

    let (Some(voltage), Some(battery_current), Some(voltage_at), Some(current_at)) = (
        snapshot.voltage,
        snapshot.battery_current,
        voltage_at,
        current_at,
    ) else {
        state.voltage_sag.reset_observations();
        state.last_sag_pair = None;
        state.last_sag_estimate = None;
        return None;
    };

    let pair_regressed = state
        .last_sag_pair
        .is_some_and(|(last_voltage, last_current)| {
            voltage_at < last_voltage || current_at < last_current
        });
    if pair_regressed {
        state.voltage_sag.reset_observations();
        state.last_sag_pair = None;
        state.last_sag_estimate = None;
    }

    let measured_voltage = voltage;
    let measured_current = battery_current;
    let inputs_are_trusted = measured_voltage.verification.is_trusted()
        && measured_voltage.quality != CoreValueQuality::Inferred
        && measured_current.verification.is_trusted()
        && measured_current.quality != CoreValueQuality::Inferred;
    let current_is_out =
        measured_current.value.direction() == cutout_core::BatteryCurrentDirection::Out;
    let flow_is_valid = inputs_are_trusted && current_is_out && flow == ChargeFlow::Discharging;
    let pair_is_new = state
        .last_sag_pair
        .is_none_or(|(last_voltage, last_current)| {
            voltage_at > last_voltage && current_at > last_current
        });

    if pair_is_new {
        let result = state.voltage_sag.update(VoltageSagInput {
            at: voltage_at.max(current_at),
            flow,
            voltage: measured_voltage,
            battery_current: measured_current,
            freshness: TelemetryFreshness::new(CoreDuration::from_milliseconds(freshness_ms)),
        });
        state.last_sag_pair = Some((voltage_at, current_at));
        state.last_sag_estimate = result;
        return result;
    }

    if flow_is_valid {
        return state
            .last_sag_estimate
            .filter(|estimate| estimate.calculated_at <= now && estimate.valid_until >= now);
    }

    // An unloaded zero is a valid baseline while the other field catches up.
    if !inputs_are_trusted
        || flow != ChargeFlow::Discharging
        || measured_current.value.direction() != cutout_core::BatteryCurrentDirection::Zero
    {
        state.voltage_sag.reset_observations();
        state.last_sag_pair = None;
    }
    state.last_sag_estimate = None;
    None
}

fn resolve_charge_mode(
    snapshot: &EstimateTelemetry,
    now: MonotonicTimestamp,
) -> Measured<ChargeMode> {
    let reported = snapshot.operating_state;
    let speed = snapshot.speed;
    let speed_at = snapshot.speed_observed_at_ms;
    let raw_charge_mode = snapshot.charge_mode.map(|mode| mode.value);
    let (_, guarded_charge_mode) = CoreRideOperatingState::resolve_fresh(
        Some(reported),
        raw_charge_mode,
        speed,
        speed_at,
        Some(now),
    );
    snapshot.charge_mode.map_or_else(
        || Measured::estimated(ChargeMode::NotCharging),
        |reading| {
            let mut reading = reading;
            reading.value = guarded_charge_mode.unwrap_or(ChargeMode::Unknown);
            if reading.value != raw_charge_mode.unwrap_or(ChargeMode::NotCharging) {
                reading.source = ValueSource::Calculated;
                reading.quality = ValueQuality::Inferred;
                reading.verification = VerificationStatus::Inferred;
            }
            reading
        },
    )
}

impl Default for ChargeEstimatorApplication {
    fn default() -> Self {
        Self {
            state: Mutex::new(EstimatorState {
                estimator: cutout_core::ChargeEstimator::new(),
                voltage_sag: VoltageSagEstimator::new(),
                last_sag_pair: None,
                last_sag_estimate: None,
                profile: None,
            }),
        }
    }
}

fn charge_flow(
    snapshot: &EstimateTelemetry,
    charge_mode: Measured<ChargeMode>,
    charge_flow_verification: VerificationStatus,
) -> Measured<ChargeFlow> {
    let flow = if charge_mode.value.is_active() {
        match snapshot.power_flow {
            Some(PowerFlowDirection::Discharge) if charge_flow_verification.is_trusted() => {
                ChargeFlow::Discharging
            }
            Some(PowerFlowDirection::Regeneration) if charge_flow_verification.is_trusted() => {
                ChargeFlow::Regeneration
            }
            Some(
                PowerFlowDirection::Unknown
                | PowerFlowDirection::Discharge
                | PowerFlowDirection::Regeneration,
            ) => ChargeFlow::Unknown,
            Some(PowerFlowDirection::Charging | PowerFlowDirection::Zero) | None => {
                ChargeFlow::Charging
            }
        }
    } else {
        match snapshot.power_flow {
            Some(PowerFlowDirection::Discharge) => ChargeFlow::Discharging,
            Some(PowerFlowDirection::Regeneration) => ChargeFlow::Regeneration,
            Some(PowerFlowDirection::Zero) => ChargeFlow::Zero,
            Some(PowerFlowDirection::Charging) => ChargeFlow::Charging,
            Some(PowerFlowDirection::Unknown) | None => ChargeFlow::Unknown,
        }
    };
    Measured {
        value: flow,
        source: charge_mode.source,
        quality: charge_mode.quality,
        verification: charge_mode
            .verification
            .intersection(charge_flow_verification),
    }
}
