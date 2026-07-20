//! Conservative, allocation-free charging time estimates.

use crate::{
    BatteryCurrent, BatteryLevel, Capacity, ChargeMode, Duration, Measured, MonotonicTimestamp,
    Temperature, ValueQuality, ValueSource, VerificationStatus, Voltage,
};
use thiserror::Error;

const MIN_SAMPLES: u16 = 3;
const MIN_OBSERVATION_MILLISECONDS: u64 = 30_000;
const MIN_CHARGE_CURRENT_MILLIAMPS: i64 = 100;
const MIN_SAG_CURRENT_STEP_MILLIAMPS: u64 = 500;
const MIN_SAG_VOLTAGE_STEP_MILLIVOLTS: u64 = 50;
const MAX_SAG_LOAD_STEP_MILLISECONDS: u64 = 5_000;
const MAX_EFFECTIVE_RESISTANCE_MILLIOHMS: u64 = 2_000;
const STABLE_VARIABILITY_PERMILLE: u64 = 100;
const EWMA_SHIFT: i64 = 2;
const MILLISECONDS_PER_MILLIAMP_HOUR: u128 = 3_600_000;

/// Stable identity for the active device/profile session.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChargeSessionIdentity(u64);

impl ChargeSessionIdentity {
    /// Creates an identity from a host-owned stable value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the host-owned stable value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identity for a verified battery or charger charge profile.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChargeProfileIdentity(u32);

impl ChargeProfileIdentity {
    /// Creates a profile identity.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the profile identity value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Canonical direction of pack current after protocol interpretation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChargeFlow {
    /// Current is flowing into the battery pack.
    Charging,

    /// Current is flowing out of the battery pack.
    Discharging,

    /// Current is being returned during vehicle regeneration.
    Regeneration,

    /// Pack current is effectively zero.
    Zero,

    /// The protocol has not established a safe direction.
    Unknown,
}

impl ChargeFlow {
    /// Returns whether this is a canonical charging direction.
    #[must_use]
    pub const fn is_charging(self) -> bool {
        matches!(self, Self::Charging)
    }
}

/// Source of a usable charge capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CapacitySource {
    /// Capacity comes from a protocol-confirmed battery profile.
    ProtocolProfile,

    /// Capacity comes from a measured pack characterization.
    HardwareMeasured,

    /// Capacity is an estimate rather than a verified pack value.
    Estimated,
}

/// A charge capacity explicitly approved for time-to-full arithmetic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsablePackCapacity {
    /// Capacity in milliamp-hours.
    pub value: Capacity,

    /// Evidence source for this capacity.
    pub source: CapacitySource,

    /// Verification state for this capacity.
    pub verification: VerificationStatus,
}

impl UsablePackCapacity {
    /// Creates a usable pack capacity with explicit provenance.
    #[must_use]
    pub const fn new(
        value: Capacity,
        source: CapacitySource,
        verification: VerificationStatus,
    ) -> Self {
        Self {
            value,
            source,
            verification,
        }
    }

    /// Returns the stored capacity in milliamp-hours.
    #[must_use]
    pub const fn as_milliamp_hours(self) -> u32 {
        self.value.as_milliamp_hours()
    }
}

/// The SOC evidence used by the estimator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BatteryLevelBasis {
    /// Device-reported SOC with its protocol provenance preserved.
    Reported(Measured<BatteryLevel>),

    /// Profile-derived SOC with the profile and confidence preserved.
    ProfileEstimated {
        /// Estimated SOC value.
        level: Measured<BatteryLevel>,
        /// Profile that produced the estimate.
        profile: ChargeProfileIdentity,
        /// Confidence in the profile-derived SOC.
        confidence: EstimateConfidence,
    },

    /// No usable SOC evidence is available.
    Unavailable,
}

impl BatteryLevelBasis {
    /// Creates a reported-SOC basis.
    #[must_use]
    pub const fn reported(level: Measured<BatteryLevel>) -> Self {
        Self::Reported(level)
    }

    /// Creates a profile-estimated SOC basis.
    #[must_use]
    pub const fn profile_estimated(
        level: Measured<BatteryLevel>,
        profile: ChargeProfileIdentity,
        confidence: EstimateConfidence,
    ) -> Self {
        Self::ProfileEstimated {
            level,
            profile,
            confidence,
        }
    }

    /// Returns the SOC value when the basis contains one.
    #[must_use]
    pub const fn level(self) -> Option<BatteryLevel> {
        match self {
            Self::Reported(level) | Self::ProfileEstimated { level, .. } => Some(level.value),
            Self::Unavailable => None,
        }
    }
}

/// Confidence assigned to an estimate or its evidence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum EstimateConfidence {
    /// Evidence is weak or substantially inferred.
    Low,

    /// Evidence is useful but has material uncertainty.
    Medium,

    /// Evidence is verified and stable enough for a narrow estimate.
    High,
}

/// Freshness policy applied to each telemetry sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryFreshness {
    /// Maximum allowed age of the sample.
    pub max_age: Duration,
}

impl TelemetryFreshness {
    /// Creates a freshness policy.
    #[must_use]
    pub const fn new(max_age: Duration) -> Self {
        Self { max_age }
    }
}

/// Signed loaded-versus-reference pack voltage difference in millivolts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoltageDelta(i32);

impl VoltageDelta {
    /// Creates a voltage delta in millivolts.
    #[must_use]
    pub const fn from_millivolts(value: i32) -> Self {
        Self(value)
    }

    /// Returns the signed voltage delta in millivolts.
    #[must_use]
    pub const fn as_millivolts(self) -> i32 {
        self.0
    }
}

/// Effective pack resistance learned from observed voltage/current load steps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveResistance(u32);

impl EffectiveResistance {
    /// Creates an effective resistance in milliohms.
    #[must_use]
    pub const fn from_milliohms(value: u32) -> Self {
        Self(value)
    }

    /// Returns the effective resistance in milliohms.
    #[must_use]
    pub const fn as_milliohms(self) -> u32 {
        self.0
    }
}

/// Rust-owned recent voltage-sag evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoltageSagEstimate {
    /// Estimated loaded-minus-no-load pack voltage.
    pub delta: VoltageDelta,
    /// Latest observed pack current used to project sag.
    pub load_current: Measured<BatteryCurrent>,
    /// Effective pack resistance learned from observed load steps.
    pub effective_resistance: EffectiveResistance,
    /// Number of admitted load-step observations in the bounded model.
    pub observations: u16,
    /// Confidence in the sag evidence.
    pub confidence: EstimateConfidence,
    /// Timestamp at which this evidence was calculated.
    pub calculated_at: MonotonicTimestamp,
    /// Timestamp after which this evidence must be discarded.
    pub valid_until: MonotonicTimestamp,
}

impl VoltageSagEstimate {
    /// Creates typed sag evidence for consumption by energy estimates.
    #[must_use]
    pub const fn new(
        delta: VoltageDelta,
        load_current: Measured<BatteryCurrent>,
        effective_resistance: EffectiveResistance,
        observations: u16,
        confidence: EstimateConfidence,
        calculated_at: MonotonicTimestamp,
        valid_until: MonotonicTimestamp,
    ) -> Self {
        Self {
            delta,
            load_current,
            effective_resistance,
            observations,
            confidence,
            calculated_at,
            valid_until,
        }
    }
}

/// One measured pack-voltage/current observation for sag estimation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoltageSagInput {
    /// Host observation timestamp.
    pub at: MonotonicTimestamp,
    /// Observed pack voltage.
    pub voltage: Measured<Voltage>,
    /// Simultaneously observed pack current.
    pub battery_current: Measured<BatteryCurrent>,
    /// Freshness policy for the resulting evidence.
    pub freshness: TelemetryFreshness,
}

/// Durable learned pack resistance for one stable EUC identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoltageSagModel {
    /// Learned effective pack resistance.
    pub effective_resistance: EffectiveResistance,
    /// Number of admitted load steps.
    pub observations: u16,
    /// Whether every admitted step came from hardware-verified telemetry.
    pub hardware_verified: bool,
}

impl VoltageSagModel {
    /// Creates a persistable learned resistance model.
    #[must_use]
    pub const fn new(
        effective_resistance: EffectiveResistance,
        observations: u16,
        hardware_verified: bool,
    ) -> Self {
        Self {
            effective_resistance,
            observations,
            hardware_verified,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VoltageSagSample {
    at: MonotonicTimestamp,
    voltage: Measured<Voltage>,
    battery_current: Measured<BatteryCurrent>,
}

/// Bounded recent voltage-sag estimator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VoltageSagEstimator {
    last: Option<VoltageSagSample>,
    resistance_q8: u64,
    observations: u16,
    all_hardware_verified: bool,
    loaded_current_negative: Option<bool>,
}

impl VoltageSagEstimator {
    /// Creates an empty sag estimator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last: None,
            resistance_q8: 0,
            observations: 0,
            all_hardware_verified: false,
            loaded_current_negative: None,
        }
    }

    /// Clears recent sag evidence.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Clears only the transient observation pair, preserving learned resistance.
    pub fn reset_observations(&mut self) {
        self.last = None;
        self.loaded_current_negative = None;
    }

    /// Returns the durable learned resistance model, when available.
    #[must_use]
    pub fn model(&self) -> Option<VoltageSagModel> {
        (self.observations > 0).then(|| {
            VoltageSagModel::new(
                EffectiveResistance::from_milliohms(
                    u32::try_from(self.resistance_q8.saturating_add(128) / 256).unwrap_or(u32::MAX),
                ),
                self.observations,
                self.all_hardware_verified,
            )
        })
    }

    /// Restores a validated model belonging to the active EUC identity.
    #[must_use]
    pub fn restore_model(&mut self, model: VoltageSagModel) -> bool {
        let resistance = u64::from(model.effective_resistance.as_milliohms());
        if model.observations == 0
            || !(1..=MAX_EFFECTIVE_RESISTANCE_MILLIOHMS).contains(&resistance)
        {
            self.reset();
            return false;
        }

        self.resistance_q8 = resistance.saturating_mul(256);
        self.observations = model.observations;
        self.all_hardware_verified = model.hardware_verified;
        self.reset_observations();
        true
    }

    /// Learns from consecutive load steps and projects sag at the latest current.
    #[must_use]
    pub fn update(&mut self, input: VoltageSagInput) -> Option<VoltageSagEstimate> {
        if !input.voltage.verification.is_trusted()
            || !input.battery_current.verification.is_trusted()
            || input.voltage.quality == ValueQuality::Inferred
            || input.battery_current.quality == ValueQuality::Inferred
        {
            self.reset_observations();
            return None;
        }

        let sample = VoltageSagSample {
            at: input.at,
            voltage: input.voltage,
            battery_current: input.battery_current,
        };
        let current = u64::from(sample.battery_current.value.as_milliamps().unsigned_abs());
        let Some(previous) = self.last else {
            self.seed(sample);
            return (self.observations > 0 && current >= MIN_SAG_CURRENT_STEP_MILLIAMPS)
                .then(|| self.project(input, current))
                .flatten();
        };
        let elapsed = input.at.saturating_duration_since(previous.at);
        if input.at < previous.at
            || elapsed > input.freshness.max_age
            || elapsed.as_milliseconds() > MAX_SAG_LOAD_STEP_MILLISECONDS
            || previous.voltage.source != sample.voltage.source
            || previous.voltage.verification != sample.voltage.verification
            || previous.battery_current.source != sample.battery_current.source
            || previous.battery_current.verification != sample.battery_current.verification
        {
            self.reset_observations();
            self.seed(sample);
            return None;
        }

        let previous_current =
            u64::from(previous.battery_current.value.as_milliamps().unsigned_abs());
        let current_polarity = (current >= MIN_SAG_CURRENT_STEP_MILLIAMPS)
            .then_some(sample.battery_current.value.as_milliamps().is_negative());
        if current_polarity
            .zip(self.loaded_current_negative)
            .is_some_and(|(current, previous)| current != previous)
        {
            self.reset_observations();
            self.seed(sample);
            return None;
        }

        let current_delta = i128::from(current).saturating_sub(i128::from(previous_current));
        let voltage_delta = i128::from(sample.voltage.value.as_millivolts())
            .saturating_sub(i128::from(previous.voltage.value.as_millivolts()));
        let current_step = u64::try_from(current_delta.unsigned_abs()).unwrap_or(u64::MAX);
        let voltage_step = u64::try_from(voltage_delta.unsigned_abs()).unwrap_or(u64::MAX);
        let material_current_step = current_step >= MIN_SAG_CURRENT_STEP_MILLIAMPS;
        let material_voltage_step = voltage_step >= MIN_SAG_VOLTAGE_STEP_MILLIVOLTS;
        let opposing_step = (current_delta.is_positive() && voltage_delta.is_negative())
            || (current_delta.is_negative() && voltage_delta.is_positive());

        if material_current_step && material_voltage_step && !opposing_step {
            self.reset_observations();
            self.seed(sample);
            return None;
        }
        if material_current_step && material_voltage_step {
            self.learn(previous, sample, current_step, voltage_step);
        }

        self.last = Some(sample);
        if let Some(polarity) = current_polarity {
            self.loaded_current_negative = Some(polarity);
        }
        if self.observations == 0 || current < MIN_SAG_CURRENT_STEP_MILLIAMPS {
            return None;
        }

        self.project(input, current)
    }

    fn learn(
        &mut self,
        previous: VoltageSagSample,
        sample: VoltageSagSample,
        current_step: u64,
        voltage_step: u64,
    ) {
        let Some(resistance_milliohms) = voltage_step
            .saturating_mul(1_000)
            .saturating_add(current_step / 2)
            .checked_div(current_step)
        else {
            return;
        };
        if !(1..=MAX_EFFECTIVE_RESISTANCE_MILLIOHMS).contains(&resistance_milliohms) {
            return;
        }

        let resistance_q8 = resistance_milliohms.saturating_mul(256);
        self.resistance_q8 = if self.observations == 0 {
            resistance_q8
        } else if resistance_q8 >= self.resistance_q8 {
            self.resistance_q8
                .saturating_add((resistance_q8 - self.resistance_q8) / 4)
        } else {
            self.resistance_q8
                .saturating_sub((self.resistance_q8 - resistance_q8) / 4)
        };
        let pair_hardware_verified = previous.voltage.verification.is_hardware_verified()
            && previous.battery_current.verification.is_hardware_verified()
            && sample.voltage.verification.is_hardware_verified()
            && sample.battery_current.verification.is_hardware_verified();
        self.all_hardware_verified = if self.observations == 0 {
            pair_hardware_verified
        } else {
            self.all_hardware_verified && pair_hardware_verified
        };
        self.observations = self.observations.saturating_add(1);
    }

    fn project(&self, input: VoltageSagInput, current: u64) -> Option<VoltageSagEstimate> {
        let resistance_milliohms = self.resistance_q8.saturating_add(128) / 256;
        let delta_millivolts = current
            .saturating_mul(resistance_milliohms)
            .saturating_add(500)
            .checked_div(1_000)?;
        let delta_millivolts = i32::try_from(delta_millivolts).ok()?.saturating_neg();
        let confidence = match self.observations {
            0 | 1 => EstimateConfidence::Low,
            2 => EstimateConfidence::Medium,
            _ if self.all_hardware_verified => EstimateConfidence::High,
            _ => EstimateConfidence::Medium,
        };
        Some(VoltageSagEstimate::new(
            VoltageDelta::from_millivolts(delta_millivolts),
            input.battery_current,
            EffectiveResistance::from_milliohms(
                u32::try_from(resistance_milliohms).unwrap_or(u32::MAX),
            ),
            self.observations,
            confidence,
            input.at,
            input.at.saturating_add_duration(input.freshness.max_age),
        ))
    }

    fn seed(&mut self, sample: VoltageSagSample) {
        let current = u64::from(sample.battery_current.value.as_milliamps().unsigned_abs());
        self.last = Some(sample);
        self.loaded_current_negative = (current >= MIN_SAG_CURRENT_STEP_MILLIAMPS)
            .then_some(sample.battery_current.value.as_milliamps().is_negative());
    }
}

/// All typed evidence needed for one estimator update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargeEstimateInput {
    /// Active device/profile session.
    pub session: ChargeSessionIdentity,

    /// Active charge profile identity.
    pub profile: ChargeProfileIdentity,

    /// Host evaluation timestamp.
    pub at: MonotonicTimestamp,

    /// Timestamp at which the measured values were observed.
    pub observed_at: MonotonicTimestamp,

    /// Signed protocol battery current.
    pub battery_current: Option<Measured<BatteryCurrent>>,

    /// Explicit protocol charge state.
    pub charge_mode: Measured<ChargeMode>,

    /// Canonical pack-current direction.
    pub flow: Measured<ChargeFlow>,

    /// Reported or profile-derived SOC evidence.
    pub battery_level: BatteryLevelBasis,

    /// Usable capacity selected by the Rust domain layer.
    pub usable_capacity: UsablePackCapacity,

    /// Optional battery temperature evidence.
    pub battery_temperature: Option<Measured<Temperature>>,

    /// Optional recent sag evidence used to widen confidence bounds.
    pub voltage_sag: Option<VoltageSagEstimate>,

    /// Freshness policy for this sample.
    pub freshness: TelemetryFreshness,
}

/// Why a charge estimate is unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChargeEstimateUnavailableReason {
    /// The device is not explicitly charging.
    NotCharging,
    /// No battery current was provided.
    CurrentMissing,
    /// Current direction or charge semantics are not verified.
    CurrentDirectionUnverified,
    /// Current is zero or below the useful threshold.
    CurrentTooSmall,
    /// No valid SOC evidence was provided.
    BatteryLevelMissing,
    /// No verified usable capacity was provided.
    CapacityMissing,
    /// A required profile is missing or not verified.
    UnsupportedProfile,
    /// Current observations are too variable for the model.
    UnstableCurrent,
    /// A sample is older than the supplied freshness policy.
    StaleInput,
    /// Temperature is outside the supported conservative model.
    TemperatureOutOfModel,
    /// The pack is full or close enough to full that charging may be balancing.
    FullOrNearFull,
    /// Independent fields disagree about the charging state.
    ContradictoryInputs,
}

/// Why the estimator discarded its bounded accumulator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChargeEstimateResetReason {
    /// A new session identity was observed.
    SessionChanged,
    /// A session became terminal or stopped charging.
    ChargingStopped,
    /// A stale gap interrupted the sample sequence.
    StaleGap,
    /// A sample timestamp moved backwards.
    TimestampOrder,
    /// Current evidence changed provenance, verification, or polarity.
    CurrentEvidenceChanged,
    /// The usable pack profile changed.
    CapacityChanged,
    /// The selected charge profile changed.
    ProfileChanged,
    /// The caller explicitly reset the estimator.
    Manual,
}

/// Invariant or checked-arithmetic failures in the estimator.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ChargeEstimateError {
    /// The host supplied an impossible timestamp order.
    #[error("charge estimate timestamp order is invalid")]
    TimestampOrder,
    /// A checked duration calculation exceeded the typed duration range.
    #[error("charge estimate duration overflowed")]
    ArithmeticOverflow,
}

/// Compact summary of the admitted charging-current rate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurrentRateSummary {
    /// EWMA charging current.
    pub mean: BatteryCurrent,
    /// Lowest admitted charging current magnitude.
    pub minimum: BatteryCurrent,
    /// Highest admitted charging current magnitude.
    pub maximum: BatteryCurrent,
    /// Range divided by mean, in permille.
    pub variability_permille: u16,
}

impl CurrentRateSummary {
    /// Returns whether the bounded current window is stable.
    #[must_use]
    pub const fn is_stable(self) -> bool {
        self.variability_permille as u64 <= STABLE_VARIABILITY_PERMILLE
    }
}

/// The kind of time-to-full estimate returned to presentation layers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EstimateKind {
    /// Duration assumes the current rate remains unchanged.
    AtPresentCurrent,
    /// Duration integrates a verified charge profile.
    ProfileBackedTimeToFull,
    /// Duration uses a bounded taper model derived from live history.
    ObservedTaperTimeToFull,
}

/// A typed, uncertainty-aware time-to-full result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargeTimeEstimate {
    /// Conservative lower duration.
    pub lower: Duration,
    /// Expected duration at the admitted current rate.
    pub expected: Duration,
    /// Conservative upper duration.
    pub upper: Duration,
    /// Semantics of the estimate.
    pub kind: EstimateKind,
    /// Confidence after combining evidence and current stability.
    pub confidence: EstimateConfidence,
    /// Current-rate evidence used in the calculation.
    pub current_rate: CurrentRateSummary,
    /// SOC basis used in the calculation.
    pub battery_level_basis: BatteryLevelBasis,
    /// Capacity provenance used in the calculation.
    pub capacity_source: CapacitySource,

    /// Recent sag evidence used to widen confidence bounds.
    pub voltage_sag: Option<VoltageSagEstimate>,
    /// Host timestamp at which this result was calculated.
    pub calculated_at: MonotonicTimestamp,
    /// Timestamp after which this result must be treated as stale.
    pub valid_until: MonotonicTimestamp,
}

/// Current state of the charge estimator.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ChargeEstimateState {
    /// The estimator has valid input but not enough observation history yet.
    #[error("collecting charging samples")]
    CollectingSamples {
        /// Number of admitted samples.
        samples: u16,
        /// Duration covered by the admitted samples.
        observed_for: Duration,
    },
    /// A usable estimate is available.
    #[error("charge estimate available")]
    Available(ChargeTimeEstimate),
    /// The current input cannot produce an estimate.
    #[error("charge estimate unavailable: {reason:?}")]
    Unavailable {
        /// Reason the estimate was withheld.
        reason: ChargeEstimateUnavailableReason,
    },
    /// Telemetry freshness invalidated the current estimate.
    #[error("charge estimate is stale")]
    Stale,
    /// An invariant or arithmetic error occurred.
    #[error("charge estimate failed: {0}")]
    Failed(ChargeEstimateError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CurrentRateWindow {
    start: Option<MonotonicTimestamp>,
    last: Option<MonotonicTimestamp>,
    count: u16,
    mean_q8: i64,
    minimum: i64,
    maximum: i64,
    last_current: i64,
    variability_q8: i64,
}

impl CurrentRateWindow {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn observe(&mut self, at: MonotonicTimestamp, current: i64) {
        let current_q8 = current.saturating_mul(256);
        if self.count == 0 {
            self.start = Some(at);
            self.mean_q8 = current_q8;
            self.minimum = current;
            self.maximum = current;
        } else {
            let delta_q8 = current_q8.saturating_sub(self.mean_q8);
            self.mean_q8 = self
                .mean_q8
                .saturating_add(delta_q8 / (1_i64 << EWMA_SHIFT));
            let difference = current.saturating_sub(self.last_current).unsigned_abs();
            let difference_q8 = i64::try_from(difference)
                .unwrap_or(i64::MAX)
                .saturating_mul(256);
            self.variability_q8 = self.variability_q8.saturating_add(
                difference_q8.saturating_sub(self.variability_q8) / (1_i64 << EWMA_SHIFT),
            );
            self.minimum = self.minimum.min(current);
            self.maximum = self.maximum.max(current);
        }
        self.last_current = current;
        self.last = Some(at);
        self.count = self.count.saturating_add(1);
    }

    fn summary(self) -> Option<CurrentRateSummary> {
        if self.count == 0 || self.mean_q8 <= 0 {
            return None;
        }
        let mean = (self.mean_q8.saturating_add(128)) / 256;
        let variability = self
            .variability_q8
            .saturating_mul(1_000)
            .checked_div(self.mean_q8)
            .unwrap_or(i64::MAX)
            .clamp(0, i64::from(u16::MAX));
        Some(CurrentRateSummary {
            mean: BatteryCurrent::from_milliamps(i32::try_from(mean).ok()?),
            minimum: BatteryCurrent::from_milliamps(i32::try_from(self.minimum).ok()?),
            maximum: BatteryCurrent::from_milliamps(i32::try_from(self.maximum).ok()?),
            variability_permille: u16::try_from(variability).ok()?,
        })
    }
}

/// Stateful, bounded charging-time estimator owned by the Rust domain layer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChargeEstimator {
    session: Option<ChargeSessionIdentity>,
    window: CurrentRateWindow,
    last_current_negative: Option<bool>,
    last_current_source: Option<ValueSource>,
    last_current_verification: Option<VerificationStatus>,
    last_charge_mode_source: Option<ValueSource>,
    last_charge_mode_verification: Option<VerificationStatus>,
    profile: Option<ChargeProfileIdentity>,
    capacity: Option<UsablePackCapacity>,
    last_reset_reason: Option<ChargeEstimateResetReason>,
}

#[derive(Clone, Copy)]
struct ValidatedChargeSample {
    current: i64,
    current_negative: bool,
    battery_current: Measured<BatteryCurrent>,
    level_confidence: EstimateConfidence,
}

impl ChargeEstimator {
    /// Creates an empty estimator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            session: None,
            window: CurrentRateWindow {
                start: None,
                last: None,
                count: 0,
                mean_q8: 0,
                minimum: 0,
                maximum: 0,
                last_current: 0,
                variability_q8: 0,
            },
            last_current_negative: None,
            last_current_source: None,
            last_current_verification: None,
            last_charge_mode_source: None,
            last_charge_mode_verification: None,
            profile: None,
            capacity: None,
            last_reset_reason: None,
        }
    }

    /// Clears samples and records an explicit reset.
    pub fn reset(&mut self) {
        self.reset_with_reason(ChargeEstimateResetReason::Manual);
    }

    /// Returns the most recent reset reason, if the window has been reset.
    #[must_use]
    pub const fn last_reset_reason(&self) -> Option<ChargeEstimateResetReason> {
        self.last_reset_reason
    }

    /// Admits one typed sample and returns the current estimator state.
    #[must_use]
    pub fn update(&mut self, input: ChargeEstimateInput) -> ChargeEstimateState {
        if let Err(state) = self.prepare_identity(input) {
            return state;
        }
        let sample = match self.validate_evidence(input) {
            Ok(sample) => sample,
            Err(state) => return state,
        };
        if let Err(state) = self.prepare_sample(input, sample) {
            return state;
        }

        self.window.observe(input.observed_at, sample.current);
        self.last_current_negative = Some(sample.current_negative);
        self.last_current_source = Some(sample.battery_current.source);
        self.last_current_verification = Some(sample.battery_current.verification);
        self.last_charge_mode_source = Some(input.charge_mode.source);
        self.last_charge_mode_verification = Some(input.charge_mode.verification);
        self.capacity = Some(input.usable_capacity);

        self.current_state(input, sample.level_confidence)
    }

    fn prepare_identity(&mut self, input: ChargeEstimateInput) -> Result<(), ChargeEstimateState> {
        if input.at < input.observed_at {
            self.reset_with_reason(ChargeEstimateResetReason::TimestampOrder);
            return Err(ChargeEstimateState::Failed(
                ChargeEstimateError::TimestampOrder,
            ));
        }
        if input.at.saturating_duration_since(input.observed_at) > input.freshness.max_age {
            self.reset_with_reason(ChargeEstimateResetReason::StaleGap);
            return Err(ChargeEstimateState::Stale);
        }
        if self.session != Some(input.session) {
            if self.session.is_some() {
                self.reset_with_reason(ChargeEstimateResetReason::SessionChanged);
            }
            self.session = Some(input.session);
        }
        if self.profile.is_some_and(|profile| profile != input.profile) {
            self.reset_with_reason(ChargeEstimateResetReason::ProfileChanged);
        }
        self.profile = Some(input.profile);
        Ok(())
    }

    fn validate_evidence(
        &mut self,
        input: ChargeEstimateInput,
    ) -> Result<ValidatedChargeSample, ChargeEstimateState> {
        if !input.charge_mode.verification.is_trusted() || !input.charge_mode.value.is_active() {
            self.reset_with_reason(ChargeEstimateResetReason::ChargingStopped);
            return Err(unavailable(ChargeEstimateUnavailableReason::NotCharging));
        }
        if !input.flow.verification.is_trusted() {
            self.reset_with_reason(ChargeEstimateResetReason::ChargingStopped);
            return Err(unavailable(
                ChargeEstimateUnavailableReason::CurrentDirectionUnverified,
            ));
        }
        if !input.flow.value.is_charging() {
            self.reset_with_reason(ChargeEstimateResetReason::ChargingStopped);
            return Err(unavailable(
                ChargeEstimateUnavailableReason::ContradictoryInputs,
            ));
        }
        let Some(battery_current) = input.battery_current else {
            return Err(unavailable(ChargeEstimateUnavailableReason::CurrentMissing));
        };
        if !battery_current.verification.is_trusted() {
            return Err(unavailable(
                ChargeEstimateUnavailableReason::CurrentDirectionUnverified,
            ));
        }
        let current = i64::from(battery_current.value.as_milliamps()).unsigned_abs();
        let current = i64::try_from(current).unwrap_or(i64::MAX);
        if current < MIN_CHARGE_CURRENT_MILLIAMPS {
            self.reset_with_reason(ChargeEstimateResetReason::ChargingStopped);
            return Err(unavailable(
                ChargeEstimateUnavailableReason::CurrentTooSmall,
            ));
        }
        let current_negative = battery_current.value.as_milliamps().is_negative();
        if self
            .last_current_negative
            .is_some_and(|previous| previous != current_negative)
        {
            self.reset_with_reason(ChargeEstimateResetReason::CurrentEvidenceChanged);
            return Err(unavailable(
                ChargeEstimateUnavailableReason::ContradictoryInputs,
            ));
        }
        if !input.usable_capacity.verification.is_trusted()
            || input.usable_capacity.as_milliamp_hours() == 0
        {
            return Err(unavailable(
                ChargeEstimateUnavailableReason::CapacityMissing,
            ));
        }
        let level_confidence = validate_battery_level(input.battery_level)?;
        validate_battery_temperature(input.battery_temperature)?;
        Ok(ValidatedChargeSample {
            current,
            current_negative,
            battery_current,
            level_confidence,
        })
    }

    fn prepare_sample(
        &mut self,
        input: ChargeEstimateInput,
        sample: ValidatedChargeSample,
    ) -> Result<(), ChargeEstimateState> {
        if let Some(previous) = self.window.last {
            if input.observed_at < previous {
                self.reset_with_reason(ChargeEstimateResetReason::TimestampOrder);
                return Err(ChargeEstimateState::Failed(
                    ChargeEstimateError::TimestampOrder,
                ));
            }
            if input.observed_at.saturating_duration_since(previous) > input.freshness.max_age {
                self.reset_with_reason(ChargeEstimateResetReason::StaleGap);
                return Err(ChargeEstimateState::Stale);
            }
        }
        if self
            .last_current_source
            .is_some_and(|source| source != sample.battery_current.source)
            || self
                .last_current_verification
                .is_some_and(|verification| verification != sample.battery_current.verification)
            || self
                .last_charge_mode_source
                .is_some_and(|source| source != input.charge_mode.source)
            || self
                .last_charge_mode_verification
                .is_some_and(|verification| verification != input.charge_mode.verification)
        {
            self.reset_with_reason(ChargeEstimateResetReason::CurrentEvidenceChanged);
        }
        if self
            .capacity
            .is_some_and(|capacity| capacity != input.usable_capacity)
        {
            self.reset_with_reason(ChargeEstimateResetReason::CapacityChanged);
        }
        Ok(())
    }

    fn current_state(
        &self,
        input: ChargeEstimateInput,
        level_confidence: EstimateConfidence,
    ) -> ChargeEstimateState {
        let observed_for = input
            .observed_at
            .saturating_duration_since(self.window.start.unwrap_or(input.observed_at));
        if self.window.count < MIN_SAMPLES
            || observed_for.as_milliseconds() < MIN_OBSERVATION_MILLISECONDS
        {
            return ChargeEstimateState::CollectingSamples {
                samples: self.window.count,
                observed_for,
            };
        }
        let Some(current_rate) = self.window.summary() else {
            return ChargeEstimateState::Failed(ChargeEstimateError::ArithmeticOverflow);
        };
        if !current_rate.is_stable() {
            return unavailable(ChargeEstimateUnavailableReason::UnstableCurrent);
        }
        match calculate_estimate(input, current_rate, level_confidence) {
            Ok(estimate) => ChargeEstimateState::Available(estimate),
            Err(error) => ChargeEstimateState::Failed(error),
        }
    }

    fn reset_with_reason(&mut self, reason: ChargeEstimateResetReason) {
        self.window.reset();
        self.last_current_negative = None;
        self.last_current_source = None;
        self.last_current_verification = None;
        self.last_charge_mode_source = None;
        self.last_charge_mode_verification = None;
        self.profile = None;
        self.capacity = None;
        self.last_reset_reason = Some(reason);
    }
}

fn validate_battery_level(
    basis: BatteryLevelBasis,
) -> Result<EstimateConfidence, ChargeEstimateState> {
    let (level, confidence) = match basis {
        BatteryLevelBasis::Reported(level) => {
            if !level.verification.is_trusted() || level.quality != ValueQuality::Known {
                return Err(unavailable(
                    ChargeEstimateUnavailableReason::BatteryLevelMissing,
                ));
            }
            (level.value, EstimateConfidence::High)
        }
        BatteryLevelBasis::ProfileEstimated {
            level,
            profile,
            confidence,
        } => {
            if profile.get() == 0 || !level.verification.is_trusted() {
                return Err(unavailable(
                    ChargeEstimateUnavailableReason::UnsupportedProfile,
                ));
            }
            (level.value, confidence)
        }
        BatteryLevelBasis::Unavailable => {
            return Err(unavailable(
                ChargeEstimateUnavailableReason::BatteryLevelMissing,
            ));
        }
    };
    if level.as_percent() >= 99 {
        return Err(unavailable(ChargeEstimateUnavailableReason::FullOrNearFull));
    }
    Ok(confidence)
}

fn validate_battery_temperature(
    temperature: Option<Measured<Temperature>>,
) -> Result<(), ChargeEstimateState> {
    if temperature.is_some_and(|temperature| {
        !(-10_000..=50_000).contains(&temperature.value.as_millicelsius())
    }) {
        return Err(unavailable(
            ChargeEstimateUnavailableReason::TemperatureOutOfModel,
        ));
    }
    Ok(())
}

const fn unavailable(reason: ChargeEstimateUnavailableReason) -> ChargeEstimateState {
    ChargeEstimateState::Unavailable { reason }
}

fn calculate_estimate(
    input: ChargeEstimateInput,
    current_rate: CurrentRateSummary,
    level_confidence: EstimateConfidence,
) -> Result<ChargeTimeEstimate, ChargeEstimateError> {
    let missing_percent = u128::from(
        100_u8.saturating_sub(
            input
                .battery_level
                .level()
                .map_or(100, BatteryLevel::as_percent),
        ),
    );
    let remaining_milliamp_hours = u128::from(input.usable_capacity.as_milliamp_hours())
        .checked_mul(missing_percent)
        .and_then(|value| value.checked_add(99))
        .and_then(|value| value.checked_div(100))
        .ok_or(ChargeEstimateError::ArithmeticOverflow)?;
    let expected_milliseconds = duration_at_current(remaining_milliamp_hours, current_rate.mean)?;
    let expected = u64::try_from(expected_milliseconds)
        .map(Duration::from_milliseconds)
        .map_err(|_| ChargeEstimateError::ArithmeticOverflow)?;

    let mut widen_permille: u64 = 50;
    if matches!(
        input.battery_level,
        BatteryLevelBasis::ProfileEstimated { .. }
    ) {
        widen_permille = widen_permille.saturating_add(200);
    }
    if input.usable_capacity.source == CapacitySource::Estimated {
        widen_permille = widen_permille.saturating_add(150);
    }
    if let Some(sag) = input.voltage_sag {
        if sag.valid_until < input.at || sag.confidence == EstimateConfidence::Low {
            widen_permille = widen_permille.saturating_add(200);
        } else if sag.delta.as_millivolts().unsigned_abs() > 1_000 {
            widen_permille = widen_permille.saturating_add(100);
        }
    }
    widen_permille = widen_permille.saturating_add(
        u64::from(current_rate.variability_permille)
            .saturating_div(2)
            .min(200),
    );
    let lower_factor = 1_000_u64.saturating_sub(widen_permille.min(900));
    let upper_factor = 1_000_u64.saturating_add(widen_permille);
    let fastest_milliseconds = duration_at_current(remaining_milliamp_hours, current_rate.maximum)?;
    let slowest_milliseconds = duration_at_current(remaining_milliamp_hours, current_rate.minimum)?;
    let lower = scaled_duration(fastest_milliseconds, lower_factor)?;
    let upper = scaled_duration(slowest_milliseconds, upper_factor)?;

    let confidence = if level_confidence == EstimateConfidence::High
        && input.usable_capacity.source != CapacitySource::Estimated
        && input.usable_capacity.verification.is_hardware_verified()
        && input
            .voltage_sag
            .is_none_or(|sag| sag.confidence >= EstimateConfidence::Medium)
    {
        EstimateConfidence::High
    } else if level_confidence == EstimateConfidence::Low
        || input.usable_capacity.source == CapacitySource::Estimated
    {
        EstimateConfidence::Low
    } else {
        EstimateConfidence::Medium
    };
    let valid_until = input.at.saturating_add_duration(input.freshness.max_age);
    Ok(ChargeTimeEstimate {
        lower,
        expected,
        upper,
        kind: EstimateKind::AtPresentCurrent,
        confidence,
        current_rate,
        battery_level_basis: input.battery_level,
        capacity_source: input.usable_capacity.source,
        voltage_sag: input.voltage_sag,
        calculated_at: input.at,
        valid_until,
    })
}

fn duration_at_current(
    remaining_milliamp_hours: u128,
    current: BatteryCurrent,
) -> Result<u128, ChargeEstimateError> {
    let current_milliamps = u128::from(current.as_milliamps().unsigned_abs());
    remaining_milliamp_hours
        .checked_mul(MILLISECONDS_PER_MILLIAMP_HOUR)
        .and_then(|value| value.checked_add(current_milliamps.saturating_sub(1)))
        .and_then(|value| value.checked_div(current_milliamps))
        .ok_or(ChargeEstimateError::ArithmeticOverflow)
}

fn scaled_duration(value: u128, factor_permille: u64) -> Result<Duration, ChargeEstimateError> {
    let scaled = value
        .checked_mul(u128::from(factor_permille))
        .and_then(|value| value.checked_add(999))
        .and_then(|value| value.checked_div(1_000))
        .ok_or(ChargeEstimateError::ArithmeticOverflow)?;
    u64::try_from(scaled)
        .map(Duration::from_milliseconds)
        .map_err(|_| ChargeEstimateError::ArithmeticOverflow)
}

impl VerificationStatus {
    fn is_trusted(self) -> bool {
        matches!(
            self,
            Self::SourceVerified | Self::HardwareVerified | Self::SourceAndHardwareVerified
        )
    }

    fn is_hardware_verified(self) -> bool {
        matches!(
            self,
            Self::HardwareVerified | Self::SourceAndHardwareVerified
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn input(at: u64, current: i32, level: u8) -> ChargeEstimateInput {
        ChargeEstimateInput {
            session: ChargeSessionIdentity::new(1),
            profile: ChargeProfileIdentity::new(1),
            at: MonotonicTimestamp::new(at),
            observed_at: MonotonicTimestamp::new(at),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(current))),
            charge_mode: Measured::reported(ChargeMode::Charging),
            flow: Measured::reported(ChargeFlow::Charging),
            battery_level: BatteryLevelBasis::reported(Measured::reported(
                BatteryLevel::from_percent(level),
            )),
            usable_capacity: UsablePackCapacity::new(
                Capacity::from_milliamp_hours(10_000),
                CapacitySource::ProtocolProfile,
                VerificationStatus::SourceAndHardwareVerified,
            ),
            battery_temperature: None,
            voltage_sag: None,
            freshness: TelemetryFreshness::new(Duration::from_seconds(60)),
        }
    }

    fn available_estimate(mut sample: ChargeEstimateInput) -> ChargeTimeEstimate {
        let mut estimator = ChargeEstimator::new();
        for at in [0, 15_000, 30_000] {
            sample.at = MonotonicTimestamp::new(at);
            sample.observed_at = MonotonicTimestamp::new(at);
            if at == 30_000 {
                let ChargeEstimateState::Available(estimate) = estimator.update(sample) else {
                    panic!("stable charging samples should produce an estimate");
                };
                return estimate;
            }
            assert!(matches!(
                estimator.update(sample),
                ChargeEstimateState::CollectingSamples { .. }
            ));
        }
        unreachable!("the final sample returns an estimate")
    }

    #[test]
    fn exact_arithmetic_matches_capacity_soc_and_current_table() {
        let cases = [
            (10_000, 50, -2_000, 150),
            (2_000, 75, -1_000, 30),
            (5_000, 20, -2_500, 96),
        ];
        for (capacity, level, current, expected_minutes) in cases {
            let mut sample = input(0, current, level);
            sample.usable_capacity = UsablePackCapacity::new(
                Capacity::from_milliamp_hours(capacity),
                CapacitySource::ProtocolProfile,
                VerificationStatus::SourceAndHardwareVerified,
            );
            let estimate = available_estimate(sample);
            assert_eq!(estimate.expected.as_minutes(), expected_minutes);
            assert!(estimate.lower <= estimate.expected);
            assert!(estimate.expected <= estimate.upper);
            if (capacity, level, current) == (10_000, 50, -2_000) {
                assert_eq!(estimate.lower.as_milliseconds(), 8_550_000);
                assert_eq!(estimate.expected.as_milliseconds(), 9_000_000);
                assert_eq!(estimate.upper.as_milliseconds(), 9_450_000);
            }
        }
    }

    #[test]
    fn greater_soc_produces_no_longer_remaining_duration_at_fixed_current() {
        let lower_soc = available_estimate(input(0, -2_000, 20));
        let higher_soc = available_estimate(input(0, -2_000, 80));

        assert!(higher_soc.expected < lower_soc.expected);
        assert!(higher_soc.lower < lower_soc.lower);
        assert!(higher_soc.upper < lower_soc.upper);
    }

    #[test]
    fn uncertainty_widens_bounds_and_lowers_confidence() {
        let baseline = available_estimate(input(0, -2_000, 50));
        let mut uncertain_sample = input(0, -2_000, 50);
        uncertain_sample.battery_level = BatteryLevelBasis::profile_estimated(
            Measured::reported(BatteryLevel::from_percent(50)),
            ChargeProfileIdentity::new(1),
            EstimateConfidence::Low,
        );
        uncertain_sample.usable_capacity = UsablePackCapacity::new(
            Capacity::from_milliamp_hours(10_000),
            CapacitySource::Estimated,
            VerificationStatus::HardwareVerified,
        );
        let uncertain = available_estimate(uncertain_sample);

        assert!(uncertain.lower <= baseline.lower);
        assert!(uncertain.upper >= baseline.upper);
        assert_eq!(uncertain.confidence, EstimateConfidence::Low);
    }

    #[test]
    fn variable_but_stable_current_widens_bounds() {
        let baseline = available_estimate(input(0, -2_000, 50));
        let mut estimator = ChargeEstimator::new();
        let mut sample = input(0, -2_000, 50);
        let mut variable = None;
        for (at, current) in [(0, -2_000), (15_000, -2_100), (30_000, -2_000)] {
            sample.at = MonotonicTimestamp::new(at);
            sample.observed_at = MonotonicTimestamp::new(at);
            sample.battery_current =
                Some(Measured::reported(BatteryCurrent::from_milliamps(current)));
            variable = match estimator.update(sample) {
                ChargeEstimateState::Available(estimate) => Some(estimate),
                ChargeEstimateState::CollectingSamples { .. } => variable,
                _ => None,
            };
        }
        let variable = variable.expect("stable variable current should produce an estimate");

        assert!(variable.lower < baseline.lower);
        assert!(variable.upper > baseline.upper);
    }

    #[test]
    fn bounded_current_window_recovers_after_a_transient_ages_out() {
        let mut estimator = ChargeEstimator::new();
        let mut state = ChargeEstimateState::Unavailable {
            reason: ChargeEstimateUnavailableReason::UnstableCurrent,
        };

        for (index, current) in std::iter::once(-4_000)
            .chain(std::iter::repeat_n(-2_000, 20))
            .enumerate()
        {
            let at = u64::try_from(index).unwrap() * 15_000;
            state = estimator.update(input(at, current, 50));
        }

        assert!(matches!(state, ChargeEstimateState::Available(_)));
    }

    #[test]
    fn upper_bound_covers_the_slowest_observed_charge_rate() {
        let estimate = calculate_estimate(
            input(30_000, -1_000, 50),
            CurrentRateSummary {
                mean: BatteryCurrent::from_milliamps(1_000),
                minimum: BatteryCurrent::from_milliamps(800),
                maximum: BatteryCurrent::from_milliamps(1_000),
                variability_permille: 200,
            },
            EstimateConfidence::High,
        )
        .unwrap();

        assert!(estimate.upper >= Duration::from_seconds(22_500));
    }

    #[test]
    fn stable_current_becomes_available_after_observation_window() {
        let mut estimator = ChargeEstimator::new();
        assert!(matches!(
            estimator.update(input(0, -2_000, 50)),
            ChargeEstimateState::CollectingSamples { samples: 1, .. }
        ));
        assert!(matches!(
            estimator.update(input(15_000, -2_000, 50)),
            ChargeEstimateState::CollectingSamples { samples: 2, .. }
        ));
        let ChargeEstimateState::Available(estimate) = estimator.update(input(30_000, -2_000, 50))
        else {
            panic!("stable charging samples should produce an estimate");
        };
        assert_eq!(estimate.expected.as_minutes(), 150);
        assert!(estimate.lower < estimate.expected);
        assert!(estimate.upper > estimate.expected);
        assert_eq!(estimate.kind, EstimateKind::AtPresentCurrent);
    }

    #[test]
    fn charging_requires_verified_direction_and_explicit_mode() {
        let mut estimator = ChargeEstimator::new();
        let mut sample = input(0, -2_000, 50);
        sample.flow = Measured::estimated(ChargeFlow::Charging);
        assert_eq!(
            estimator.update(sample),
            ChargeEstimateState::Unavailable {
                reason: ChargeEstimateUnavailableReason::CurrentDirectionUnverified,
            }
        );
        let mut sample = input(1_000, -2_000, 50);
        sample.charge_mode = Measured::reported(ChargeMode::NotCharging);
        assert_eq!(
            estimator.update(sample),
            ChargeEstimateState::Unavailable {
                reason: ChargeEstimateUnavailableReason::NotCharging,
            }
        );
    }

    #[test]
    fn near_zero_current_is_unavailable() {
        for current in [0, 99] {
            let mut estimator = ChargeEstimator::new();
            assert_eq!(
                estimator.update(input(0, current, 50)),
                ChargeEstimateState::Unavailable {
                    reason: ChargeEstimateUnavailableReason::CurrentTooSmall,
                }
            );
            assert_eq!(
                estimator.last_reset_reason(),
                Some(ChargeEstimateResetReason::ChargingStopped)
            );
        }
    }

    #[test]
    fn current_polarity_change_resets_as_contradictory() {
        let mut estimator = ChargeEstimator::new();
        let _ = estimator.update(input(0, -2_000, 50));
        let _ = estimator.update(input(15_000, -2_000, 50));

        assert_eq!(
            estimator.update(input(30_000, 2_000, 50)),
            ChargeEstimateState::Unavailable {
                reason: ChargeEstimateUnavailableReason::ContradictoryInputs,
            }
        );
        assert_eq!(
            estimator.last_reset_reason(),
            Some(ChargeEstimateResetReason::CurrentEvidenceChanged)
        );
        assert!(matches!(
            estimator.update(input(45_000, 2_000, 50)),
            ChargeEstimateState::CollectingSamples { samples: 1, .. }
        ));
    }

    #[test]
    fn charge_mode_change_resets_the_observation_window() {
        let mut estimator = ChargeEstimator::new();
        let _ = estimator.update(input(0, -2_000, 50));

        let mut stopped = input(15_000, -2_000, 50);
        stopped.charge_mode = Measured::reported(ChargeMode::NotCharging);
        assert_eq!(
            estimator.update(stopped),
            ChargeEstimateState::Unavailable {
                reason: ChargeEstimateUnavailableReason::NotCharging,
            }
        );
        assert_eq!(
            estimator.last_reset_reason(),
            Some(ChargeEstimateResetReason::ChargingStopped)
        );
        assert!(matches!(
            estimator.update(input(30_000, -2_000, 50)),
            ChargeEstimateState::CollectingSamples { samples: 1, .. }
        ));
    }

    #[test]
    fn sag_estimator_learns_resistance_from_an_observed_load_step() {
        let mut estimator = VoltageSagEstimator::new();
        assert_eq!(
            estimator.update(VoltageSagInput {
                at: MonotonicTimestamp::new(0),
                voltage: Measured::reported(Voltage::from_millivolts(100_000)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(0)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            }),
            None
        );
        let evidence = estimator
            .update(VoltageSagInput {
                at: MonotonicTimestamp::new(1_000),
                voltage: Measured::reported(Voltage::from_millivolts(99_000)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(10_000)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
            .expect("an observed load step should produce sag");
        assert_eq!(evidence.delta.as_millivolts(), -1_000);
        assert_eq!(evidence.load_current.value.as_milliamps(), 10_000);
        assert_eq!(evidence.effective_resistance.as_milliohms(), 100);
        assert_eq!(evidence.observations, 1);
        assert_eq!(evidence.confidence, EstimateConfidence::Low);
        assert_eq!(evidence.valid_until.get(), 31_000);
    }

    #[test]
    fn sag_estimator_rejects_same_direction_and_stable_current_samples() {
        let mut estimator = VoltageSagEstimator::new();
        let observe = |estimator: &mut VoltageSagEstimator, at, voltage, current| {
            estimator.update(VoltageSagInput {
                at: MonotonicTimestamp::new(at),
                voltage: Measured::reported(Voltage::from_millivolts(voltage)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(current)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
        };
        assert_eq!(observe(&mut estimator, 0, 100_000, 0), None);
        assert_eq!(observe(&mut estimator, 1_000, 101_000, 10_000), None);
        assert_eq!(observe(&mut estimator, 2_000, 99_000, 10_000), None);
    }

    #[test]
    fn sag_estimator_learns_from_voltage_recovery_when_load_decreases() {
        let mut estimator = VoltageSagEstimator::new();
        let observe = |estimator: &mut VoltageSagEstimator, at, voltage, current| {
            estimator.update(VoltageSagInput {
                at: MonotonicTimestamp::new(at),
                voltage: Measured::reported(Voltage::from_millivolts(voltage)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(current)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
        };
        let _ = observe(&mut estimator, 0, 100_000, 0);
        let _ = observe(&mut estimator, 1_000, 99_000, 10_000);
        let evidence = observe(&mut estimator, 2_000, 99_800, 2_000)
            .expect("voltage recovery should update the resistance model");
        assert_eq!(evidence.delta.as_millivolts(), -200);
        assert_eq!(evidence.effective_resistance.as_milliohms(), 100);
        assert_eq!(evidence.observations, 2);
        assert_eq!(evidence.confidence, EstimateConfidence::Medium);
    }

    #[test]
    fn sag_estimator_preserves_the_learned_model_across_transient_resets() {
        let mut estimator = VoltageSagEstimator::new();
        let observe = |estimator: &mut VoltageSagEstimator, at, voltage, current| {
            estimator.update(VoltageSagInput {
                at: MonotonicTimestamp::new(at),
                voltage: Measured::reported(Voltage::from_millivolts(voltage)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(current)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
        };
        let _ = observe(&mut estimator, 0, 100_000, 0);
        let learned = observe(&mut estimator, 1_000, 99_000, 10_000)
            .expect("load step should learn a resistance model");

        estimator.reset_observations();
        assert_eq!(
            estimator.model(),
            Some(VoltageSagModel::new(
                learned.effective_resistance,
                learned.observations,
                true,
            ))
        );
        let restored = observe(&mut estimator, 60_000, 99_000, 10_000)
            .expect("the first fresh sample should reuse the learned resistance");
        assert_eq!(restored.delta.as_millivolts(), -1_000);
        assert_eq!(restored.observations, 1);
    }

    #[test]
    fn sag_estimator_restores_a_valid_per_device_model() {
        let model = VoltageSagModel::new(EffectiveResistance::from_milliohms(125), 7, true);
        let mut estimator = VoltageSagEstimator::new();

        assert!(estimator.restore_model(model));
        assert_eq!(estimator.model(), Some(model));
        estimator.reset_observations();
        assert_eq!(estimator.model(), Some(model));
        estimator.reset();
        assert_eq!(estimator.model(), None);
    }

    #[test]
    fn restored_sag_model_projects_from_the_first_current_sample() {
        let mut estimator = VoltageSagEstimator::new();
        assert!(estimator.restore_model(VoltageSagModel::new(
            EffectiveResistance::from_milliohms(100),
            8,
            true,
        )));

        let sag = estimator
            .update(VoltageSagInput {
                at: MonotonicTimestamp::new(1_000),
                voltage: Measured::reported(Voltage::from_millivolts(99_000)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(10_000)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
            .expect("a restored model and current are sufficient to project sag");

        assert_eq!(sag.delta.as_millivolts(), -1_000);
    }

    #[test]
    fn unlearned_sag_model_uses_connection_under_load_only_as_a_baseline() {
        let mut estimator = VoltageSagEstimator::new();
        let observe = |estimator: &mut VoltageSagEstimator, at, voltage, current| {
            estimator.update(VoltageSagInput {
                at: MonotonicTimestamp::new(at),
                voltage: Measured::reported(Voltage::from_millivolts(voltage)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(current)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
        };

        assert_eq!(observe(&mut estimator, 0, 99_000, 10_000), None);
        let sag = observe(&mut estimator, 1_000, 98_000, 20_000)
            .expect("the next load step should teach the model");
        assert_eq!(sag.effective_resistance.as_milliohms(), 100);
        assert_eq!(sag.delta.as_millivolts(), -2_000);
    }

    #[test]
    fn sag_estimator_rejects_invalid_persisted_models() {
        let mut estimator = VoltageSagEstimator::new();

        assert!(!estimator.restore_model(VoltageSagModel::new(
            EffectiveResistance::from_milliohms(0),
            1,
            true,
        )));
        assert!(!estimator.restore_model(VoltageSagModel::new(
            EffectiveResistance::from_milliohms(100),
            0,
            true,
        )));
        assert_eq!(estimator.model(), None);
    }

    #[test]
    fn sag_estimator_stale_and_out_of_order_samples_preserve_the_model() {
        let mut estimator = VoltageSagEstimator::new();
        let observe = |estimator: &mut VoltageSagEstimator, at, voltage, current| {
            estimator.update(VoltageSagInput {
                at: MonotonicTimestamp::new(at),
                voltage: Measured::reported(Voltage::from_millivolts(voltage)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(current)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
        };
        let _ = observe(&mut estimator, 0, 100_000, 0);
        let learned = observe(&mut estimator, 1_000, 99_000, 10_000)
            .expect("load step should learn a resistance model");
        let model = estimator.model();

        assert_eq!(observe(&mut estimator, 40_000, 99_000, 10_000), None);
        assert_eq!(estimator.model(), model);
        assert_eq!(
            observe(&mut estimator, 41_000, 99_000, 10_000)
                .expect("fresh observations should reuse the model")
                .effective_resistance,
            learned.effective_resistance
        );
        assert_eq!(observe(&mut estimator, 40_500, 99_000, 10_000), None);
        assert_eq!(estimator.model(), model);
    }

    #[test]
    fn sag_estimator_admits_higher_resistance_slowly_as_the_pack_ages() {
        let mut estimator = VoltageSagEstimator::new();
        let observe = |estimator: &mut VoltageSagEstimator, at, voltage, current| {
            estimator.update(VoltageSagInput {
                at: MonotonicTimestamp::new(at),
                voltage: Measured::reported(Voltage::from_millivolts(voltage)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(current)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
        };
        let _ = observe(&mut estimator, 0, 100_000, 0);
        let _ = observe(&mut estimator, 1_000, 99_000, 10_000);
        let _ = observe(&mut estimator, 2_000, 100_000, 0);
        let aged = observe(&mut estimator, 3_000, 98_000, 10_000)
            .expect("later higher resistance should update the model");

        assert_eq!(aged.effective_resistance.as_milliohms(), 125);
        assert_eq!(aged.observations, 3);
    }

    #[test]
    fn sag_estimator_does_not_treat_a_slow_change_as_an_instantaneous_load_step() {
        let mut estimator = VoltageSagEstimator::new();
        let observe = |estimator: &mut VoltageSagEstimator, at, voltage, current| {
            estimator.update(VoltageSagInput {
                at: MonotonicTimestamp::new(at),
                voltage: Measured::reported(Voltage::from_millivolts(voltage)),
                battery_current: Measured::reported(BatteryCurrent::from_milliamps(current)),
                freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
            })
        };

        assert_eq!(observe(&mut estimator, 0, 100_000, 0), None);
        assert_eq!(observe(&mut estimator, 10_000, 99_000, 10_000), None);
        assert_eq!(estimator.model(), None);
    }

    #[test]
    fn stale_gap_resets_the_window() {
        let mut estimator = ChargeEstimator::new();
        let _ = estimator.update(input(0, -2_000, 50));
        assert_eq!(
            estimator.update(input(61_000, -2_000, 50)),
            ChargeEstimateState::Stale
        );
        assert_eq!(
            estimator.last_reset_reason(),
            Some(ChargeEstimateResetReason::StaleGap)
        );
    }

    #[test]
    fn identity_and_profile_changes_reset_samples_before_recollecting() {
        let mut estimator = ChargeEstimator::new();
        let _ = estimator.update(input(0, -2_000, 50));
        let _ = estimator.update(input(15_000, -2_000, 50));
        assert!(matches!(
            estimator.update(input(30_000, -2_000, 50)),
            ChargeEstimateState::Available(_)
        ));

        let mut new_session = input(45_000, -2_000, 50);
        new_session.session = ChargeSessionIdentity::new(2);
        assert!(matches!(
            estimator.update(new_session),
            ChargeEstimateState::CollectingSamples { samples: 1, .. }
        ));
        assert_eq!(
            estimator.last_reset_reason(),
            Some(ChargeEstimateResetReason::SessionChanged)
        );

        let mut new_profile = input(60_000, -2_000, 50);
        new_profile.session = ChargeSessionIdentity::new(2);
        new_profile.profile = ChargeProfileIdentity::new(2);
        assert!(matches!(
            estimator.update(new_profile),
            ChargeEstimateState::CollectingSamples { samples: 1, .. }
        ));
        assert_eq!(
            estimator.last_reset_reason(),
            Some(ChargeEstimateResetReason::ProfileChanged)
        );
    }

    #[test]
    fn full_and_unstable_inputs_remain_unavailable() {
        let mut estimator = ChargeEstimator::new();
        assert_eq!(
            estimator.update(input(0, -2_000, 99)),
            ChargeEstimateState::Unavailable {
                reason: ChargeEstimateUnavailableReason::FullOrNearFull,
            }
        );

        let _ = estimator.update(input(0, -1_000, 50));
        let _ = estimator.update(input(15_000, -2_000, 50));
        assert_eq!(
            estimator.update(input(30_000, -1_000, 50)),
            ChargeEstimateState::Unavailable {
                reason: ChargeEstimateUnavailableReason::UnstableCurrent,
            }
        );
    }

    #[test]
    fn timestamp_order_returns_failure_and_clears_history() {
        let mut estimator = ChargeEstimator::new();
        let _ = estimator.update(input(10_000, -2_000, 50));
        let mut out_of_order = input(20_000, -2_000, 50);
        out_of_order.observed_at = MonotonicTimestamp::new(9_000);

        assert_eq!(
            estimator.update(out_of_order),
            ChargeEstimateState::Failed(ChargeEstimateError::TimestampOrder)
        );
        assert_eq!(
            estimator.last_reset_reason(),
            Some(ChargeEstimateResetReason::TimestampOrder)
        );
    }

    proptest! {
        #[test]
        fn generated_estimates_have_ordered_nonnegative_bounded_durations(
            capacity in 1_u32..=200_000,
            level in 0_u8..=98,
            current in 100_i32..=100_000,
        ) {
            let mut sample = input(0, -current, level);
            sample.usable_capacity = UsablePackCapacity::new(
                Capacity::from_milliamp_hours(capacity),
                CapacitySource::ProtocolProfile,
                VerificationStatus::SourceAndHardwareVerified,
            );
            let estimate = available_estimate(sample);

            prop_assert!(estimate.lower <= estimate.expected);
            prop_assert!(estimate.expected <= estimate.upper);
        }

        #[test]
        fn generated_fixed_current_estimates_are_monotonic_with_soc(
            capacity in 1_u32..=200_000,
            first_level in 0_u8..=98,
            second_level in 0_u8..=98,
            current in 100_i32..=100_000,
        ) {
            prop_assume!(first_level != second_level);
            let (lower_level, higher_level) = if first_level < second_level {
                (first_level, second_level)
            } else {
                (second_level, first_level)
            };
            let mut lower_soc = input(0, -current, lower_level);
            lower_soc.usable_capacity = UsablePackCapacity::new(
                Capacity::from_milliamp_hours(capacity),
                CapacitySource::ProtocolProfile,
                VerificationStatus::SourceAndHardwareVerified,
            );
            let mut higher_soc = lower_soc;
            higher_soc.battery_level = BatteryLevelBasis::reported(Measured::reported(
                BatteryLevel::from_percent(higher_level),
            ));

            let lower_estimate = available_estimate(lower_soc);
            let higher_estimate = available_estimate(higher_soc);

            prop_assert!(higher_estimate.lower <= lower_estimate.lower);
            prop_assert!(higher_estimate.expected <= lower_estimate.expected);
            prop_assert!(higher_estimate.upper <= lower_estimate.upper);
        }

        #[test]
        fn generated_uncertainty_never_narrows_estimate_bounds(
            capacity in 1_u32..=200_000,
            level in 0_u8..=98,
            current in 100_i32..=100_000,
        ) {
            let mut baseline_input = input(0, -current, level);
            baseline_input.usable_capacity = UsablePackCapacity::new(
                Capacity::from_milliamp_hours(capacity),
                CapacitySource::ProtocolProfile,
                VerificationStatus::SourceAndHardwareVerified,
            );
            let baseline = available_estimate(baseline_input);

            let mut uncertain_input = baseline_input;
            uncertain_input.battery_level = BatteryLevelBasis::profile_estimated(
                Measured::reported(BatteryLevel::from_percent(level)),
                ChargeProfileIdentity::new(1),
                EstimateConfidence::Low,
            );
            uncertain_input.usable_capacity = UsablePackCapacity::new(
                Capacity::from_milliamp_hours(capacity),
                CapacitySource::Estimated,
                VerificationStatus::SourceAndHardwareVerified,
            );
            let uncertain = available_estimate(uncertain_input);

            prop_assert!(uncertain.lower <= baseline.lower);
            prop_assert!(uncertain.upper >= baseline.upper);
            prop_assert!(uncertain.confidence <= baseline.confidence);
        }
    }
}
