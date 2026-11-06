//! Battery estimation through the public Rust application owner.

use cutout_application::charging::estimate::{
    ChargeEstimatorApplication, ChargeProfile, EstimateInput, EstimateTelemetry,
};
use cutout_core::{
    BatteryCurrent, BatteryLevel, CapacitySource, ChargeEstimateState,
    ChargeEstimateUnavailableReason, ChargeMode, Duration, Measured, MonotonicTimestamp,
    PowerFlowDirection, RideOperatingState, Speed, VerificationStatus, Voltage,
};

fn sample(at: u64, voltage_at: u64, current_at: u64, voltage: i32, current: u32) -> EstimateInput {
    EstimateInput {
        at: MonotonicTimestamp::new(at),
        freshness: Duration::from_milliseconds(30_000),
        snapshot: EstimateTelemetry {
            at_ms: Some(MonotonicTimestamp::new(at)),
            battery_current_observed_at_ms: Some(MonotonicTimestamp::new(current_at)),
            voltage_observed_at_ms: Some(MonotonicTimestamp::new(voltage_at)),
            speed_observed_at_ms: Some(MonotonicTimestamp::new(at)),
            battery_current: Some(Measured::reported(BatteryCurrent::current_out(current))),
            voltage: Some(Measured::reported(Voltage::from_millivolts(voltage))),
            speed: Some(Speed::from_millimetres_per_second(1_000)),
            charge_mode: Some(Measured::reported(ChargeMode::NotCharging)),
            operating_state: RideOperatingState::Riding,
            power_flow: Some(PowerFlowDirection::Discharge),
            battery_level_reported: Some(Measured::reported(BatteryLevel::from_percent(50))),
            battery_level_estimated: None,
            battery_temperature: None,
        },
    }
}

#[test]
fn stale_voltage_cannot_pair_with_new_current_but_fresh_pairs_resume_learning() {
    let application = ChargeEstimatorApplication::new();
    assert_eq!(
        application
            .update(sample(1_000, 1_000, 1_000, 100_000, 0))
            .voltage_sag,
        None
    );
    assert_eq!(
        application
            .update(sample(7_000, 1_000, 7_000, 99_000, 10_000))
            .voltage_sag,
        None
    );
    assert_eq!(application.voltage_sag_model(), None);
    let _ = application.update(sample(8_000, 8_000, 8_000, 100_000, 0));
    let sag = application
        .update(sample(9_000, 9_000, 9_000, 99_000, 10_000))
        .voltage_sag
        .expect("fresh pair learns sag");
    assert_eq!(sag.delta.as_millivolts(), -1_000);
    assert_eq!(sag.observations, 1);
    let model = application.voltage_sag_model().expect("learned resistance");
    let restored = ChargeEstimatorApplication::new();
    assert!(restored.restore_voltage_sag_model(model));
    assert_eq!(restored.voltage_sag_model(), Some(model));
}

#[test]
fn incompatible_pack_replacement_clears_a_learned_model() {
    let application = ChargeEstimatorApplication::new();
    let profile = ChargeProfile {
        session_id: 1,
        profile_id: 7,
        capacity_milliamp_hours: 10_000,
        capacity_source: CapacitySource::HardwareMeasured,
        verification: VerificationStatus::HardwareVerified,
        charge_flow_verification: VerificationStatus::HardwareVerified,
    };
    application.configure_profile(profile);
    let _ = application.update(sample(0, 0, 0, 100_000, 0));
    let _ = application.update(sample(1_000, 1_000, 1_000, 99_000, 10_000));
    assert!(application.voltage_sag_model().is_some());
    application.configure_profile(ChargeProfile {
        capacity_milliamp_hours: 20_000,
        ..profile
    });
    assert_eq!(application.voltage_sag_model(), None);
}

#[test]
fn missing_capacity_remains_a_typed_unavailable_result() {
    assert_eq!(
        ChargeEstimatorApplication::new()
            .update(sample(0, 0, 0, 100_000, 0))
            .state,
        ChargeEstimateState::Unavailable {
            reason: ChargeEstimateUnavailableReason::CapacityMissing
        }
    );
}
