//! Semantic settings state shared by device sessions and native clients.

use std::collections::BTreeMap;

use crate::{
    ControlRefusalReason, Duration, MonotonicTimestamp, SETTING_CONFIRMATION_TIMEOUT,
    SettingCommandStatus, SettingState, SettingValue, SettingValueSource,
};

/// Stable semantic identity, independent of protocol fields or model names.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SettingId {
    /// Main headlight state.
    Headlight,
    /// Independently controlled high beam.
    HighBeam,
    /// Speed that triggers tilt-back.
    TiltbackSpeed,
    /// PWM utilization that triggers tilt-back, or explicitly disabled.
    PwmTiltback,
    /// Continuous pedal hardness.
    PedalHardness,
    /// Wheel display brightness.
    DisplayBrightness,
    /// Units used by the wheel display.
    DisplayUnits,
    /// Beeper loudness measured as a percentage.
    BeeperVolumePercent,
    /// Dynamic assist strength.
    DynamicAssist,
    /// Pedal-dip compensation strength.
    PedalDipCompensation,
    /// Maximum permitted lateral tilt.
    LateralTiltLimit,
    /// Reported-voltage correction.
    VoltageCorrection,
    /// Raw charge-related diagnostic with unresolved physical meaning.
    ChargeLimitDiagnostic,
    /// High-speed operating mode.
    HighSpeedMode,
    /// Low-battery operating mode.
    LowBatteryMode,
    /// Transport operating mode.
    TransportMode,
    /// Audible speed-alarm threshold.
    SpeedAlarmThreshold,
    /// Fore/aft pedal angle.
    PedalAngle,
    /// Discrete riding response preset.
    RidingPreset,
    /// Brake overpressure alarm threshold.
    BrakeOverpressureAlarm,
    /// Discrete pedal mode; not assumed equivalent to a riding preset.
    PedalMode,
    /// Discrete roll-angle policy.
    RollAngleMode,
    /// Enabled speed alarm stages or alternative alarm policy.
    SpeedAlarmMode,
    /// Wheel maximum speed setting.
    MaximumSpeed,
    /// Discrete beeper level; not a percentage.
    BeeperVolumeLevel,
    /// Device-defined lighting pattern.
    LightingPattern,
    /// Acceleration-assist enablement.
    AccelerationAssist,
    /// Independently controlled taillight.
    Taillight,
}

/// A descriptor's semantic value, never a raw field identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceSettingValue {
    /// Explicit on/off value; absence of readback is represented separately.
    Boolean(bool),
    /// Fixed-point quantity using the descriptor's unit and decimal precision.
    Number(i32),
    /// Opaque option identifier from the descriptor's choices, not a wire byte.
    Choice(u16),
    /// Explicitly disabled functionality, distinct from zero and unknown.
    Disabled,
}

/// Result of submitting a validated setting through the device session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingSubmissionOutcome {
    /// The session accepted the request for transport.
    Accepted,
    /// The session refused the request before transport.
    Refused(ControlRefusalReason),
    /// The session or transport failed to submit the request.
    Failed,
}

/// Immutable lifecycle/evidence projection used by every setting presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceSettingSnapshot {
    /// Stable semantic identity.
    pub id: SettingId,
    /// Most recent observed value with its original provenance.
    pub current: Option<SettingValue<DeviceSettingValue>>,
    /// Last request, kept separate from device readback.
    pub requested: Option<DeviceSettingValue>,
    /// Rust-owned confirmation state.
    pub status: SettingCommandStatus,
    /// Age of the current observation; absent when no value has been observed.
    pub age: Option<Duration>,
    /// Reason for the last refused request.
    pub refusal: Option<ControlRefusalReason>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SettingRecord {
    state: SettingState<DeviceSettingValue>,
    observed_at: Option<MonotonicTimestamp>,
    confirmation_supported: bool,
}

impl Default for SettingRecord {
    fn default() -> Self {
        Self {
            state: SettingState::unknown(),
            observed_at: None,
            confirmation_supported: true,
        }
    }
}

/// One semantic settings state owner per device session.
///
/// Protocol adapters normalize observations before updating this owner. The
/// device session must validate identity, capabilities and authorization before
/// transport; recording an outcome here does not authorize a command.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceSettingsState {
    records: BTreeMap<SettingId, SettingRecord>,
}

impl DeviceSettingsState {
    /// Records ordered readback without treating old values as confirmation.
    pub fn observe(
        &mut self,
        id: SettingId,
        value: DeviceSettingValue,
        source: SettingValueSource,
        observed_at: MonotonicTimestamp,
    ) {
        let record = self.records.entry(id).or_default();
        if record
            .observed_at
            .is_some_and(|latest| observed_at < latest)
            || matches!(record.state, SettingState::Pending { submitted_at, .. } if observed_at < submitted_at)
        {
            return;
        }
        record.state.observe(value, source, observed_at);
        record.observed_at = Some(observed_at);
    }

    /// Records the outcome of this request, including failures before a write.
    pub fn submission(
        &mut self,
        id: SettingId,
        requested: DeviceSettingValue,
        outcome: SettingSubmissionOutcome,
        confirmation_supported: bool,
        submitted_at: MonotonicTimestamp,
    ) {
        let record = self.records.entry(id).or_default();
        record.confirmation_supported = confirmation_supported;
        record.state.submit(requested, submitted_at);
        match outcome {
            SettingSubmissionOutcome::Accepted => {}
            SettingSubmissionOutcome::Refused(reason) => record.state.refuse(reason),
            SettingSubmissionOutcome::Failed => record.state.fail(),
        }
    }

    /// Advances confirmation deadlines for readable settings only.
    pub fn tick(&mut self, now: MonotonicTimestamp) {
        for record in self.records.values_mut() {
            if record.confirmation_supported {
                record
                    .state
                    .timeout_if_elapsed(now, SETTING_CONFIRMATION_TIMEOUT);
            }
        }
    }

    /// Invalidates all observations and pending work on a device boundary.
    pub fn disconnect(&mut self) {
        self.records.clear();
    }

    /// Projects observed and requested values without inventing defaults.
    #[must_use]
    pub fn snapshot(&self, now: MonotonicTimestamp) -> Vec<DeviceSettingSnapshot> {
        self.records
            .iter()
            .map(|(&id, record)| DeviceSettingSnapshot {
                id,
                current: record.state.current_readback(),
                requested: record.state.requested_value(),
                status: record
                    .state
                    .command_status(now, record.confirmation_supported),
                age: record
                    .observed_at
                    .map(|observed| now.saturating_duration_since(observed)),
                refusal: match record.state {
                    SettingState::Refused { reason, .. } => Some(reason),
                    _ => None,
                },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ControlRefusalReason, MonotonicTimestamp, SettingCommandStatus, SettingValueSource,
    };

    fn time(value: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::new(value)
    }

    #[test]
    fn unknown_and_disabled_are_distinct_from_zero_and_false() {
        let mut settings = DeviceSettingsState::default();
        assert!(settings.snapshot(time(0)).is_empty());
        settings.observe(
            SettingId::PwmTiltback,
            DeviceSettingValue::Disabled,
            SettingValueSource::LiveReadback,
            time(1),
        );
        settings.observe(
            SettingId::Headlight,
            DeviceSettingValue::Boolean(false),
            SettingValueSource::LiveReadback,
            time(1),
        );
        let snapshot = settings.snapshot(time(11));
        assert_eq!(snapshot.len(), 2);
        let pwm = snapshot
            .iter()
            .find(|entry| entry.id == SettingId::PwmTiltback)
            .unwrap();
        assert_eq!(pwm.current.unwrap().value, DeviceSettingValue::Disabled);
        assert_eq!(pwm.age.unwrap().as_milliseconds(), 10);
        assert_eq!(pwm.status, SettingCommandStatus::Idle);
    }

    #[test]
    fn a_refused_request_preserves_current_and_a_retry_can_confirm() {
        let mut settings = DeviceSettingsState::default();
        let id = SettingId::Headlight;
        settings.observe(
            id,
            DeviceSettingValue::Boolean(false),
            SettingValueSource::LiveReadback,
            time(10),
        );
        settings.submission(
            id,
            DeviceSettingValue::Boolean(true),
            SettingSubmissionOutcome::Refused(ControlRefusalReason::Busy),
            true,
            time(20),
        );
        let snapshot = settings.snapshot(time(25));
        assert_eq!(
            snapshot[0].current.unwrap().value,
            DeviceSettingValue::Boolean(false)
        );
        assert_eq!(
            snapshot[0].requested,
            Some(DeviceSettingValue::Boolean(true))
        );
        assert_eq!(snapshot[0].status, SettingCommandStatus::Refused);
        assert_eq!(snapshot[0].refusal, Some(ControlRefusalReason::Busy));
        settings.submission(
            id,
            DeviceSettingValue::Boolean(true),
            SettingSubmissionOutcome::Accepted,
            true,
            time(30),
        );
        settings.observe(
            id,
            DeviceSettingValue::Boolean(true),
            SettingValueSource::LiveReadback,
            time(29),
        );
        assert_eq!(
            settings.snapshot(time(31))[0].status,
            SettingCommandStatus::WaitingForConfirmation
        );
        settings.observe(
            id,
            DeviceSettingValue::Boolean(true),
            SettingValueSource::LiveReadback,
            time(32),
        );
        assert_eq!(
            settings.snapshot(time(32))[0].status,
            SettingCommandStatus::Confirmed
        );
    }

    #[test]
    fn unconfirmed_writes_do_not_become_current_or_time_out() {
        let mut settings = DeviceSettingsState::default();
        settings.submission(
            SettingId::MaximumSpeed,
            DeviceSettingValue::Number(30),
            SettingSubmissionOutcome::Accepted,
            false,
            time(10),
        );
        settings.tick(time(5_000));
        let snapshot = settings.snapshot(time(5_000));
        assert_eq!(
            snapshot[0].status,
            SettingCommandStatus::SentWithoutConfirmation
        );
        assert_eq!(snapshot[0].current, None);
        assert_eq!(snapshot[0].age, None);
        assert_eq!(snapshot[0].requested, Some(DeviceSettingValue::Number(30)));
    }

    #[test]
    fn failed_retry_records_the_new_request_and_disconnect_clears_everything() {
        let mut settings = DeviceSettingsState::default();
        settings.submission(
            SettingId::MaximumSpeed,
            DeviceSettingValue::Number(30),
            SettingSubmissionOutcome::Accepted,
            false,
            time(10),
        );
        settings.submission(
            SettingId::MaximumSpeed,
            DeviceSettingValue::Number(40),
            SettingSubmissionOutcome::Failed,
            false,
            time(20),
        );
        let snapshot = settings.snapshot(time(20));
        assert_eq!(snapshot[0].requested, Some(DeviceSettingValue::Number(40)));
        assert_eq!(snapshot[0].status, SettingCommandStatus::Failed);
        settings.disconnect();
        assert!(settings.snapshot(time(30)).is_empty());
    }

    #[test]
    fn out_of_order_readback_cannot_replace_newer_state_or_refresh_its_age() {
        let mut settings = DeviceSettingsState::default();
        settings.observe(
            SettingId::Headlight,
            DeviceSettingValue::Boolean(true),
            SettingValueSource::LiveReadback,
            time(20),
        );
        settings.observe(
            SettingId::Headlight,
            DeviceSettingValue::Boolean(false),
            SettingValueSource::LiveReadback,
            time(10),
        );
        let snapshot = settings.snapshot(time(30));
        assert_eq!(
            snapshot[0].current.unwrap().value,
            DeviceSettingValue::Boolean(true)
        );
        assert_eq!(snapshot[0].age.unwrap().as_milliseconds(), 10);
    }
}
