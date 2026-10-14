use cutout_core::{
    BatteryCurrent, BatteryCurrentDirection, BatteryCurrentPolarity, Power, PowerFlowDirection,
    ReportedBatteryCurrent, RideOperatingState, Voltage,
};

#[test]
fn opposite_wire_polarities_have_one_physical_direction() {
    let positive_out = ReportedBatteryCurrent::from_milliamps(2_000);
    let negative_out = ReportedBatteryCurrent::from_milliamps(-2_000);
    assert_eq!(
        positive_out.normalize(BatteryCurrentPolarity::PositiveIsOut),
        BatteryCurrent::current_out(2_000)
    );
    assert_eq!(
        negative_out.normalize(BatteryCurrentPolarity::PositiveIsIn),
        BatteryCurrent::current_out(2_000)
    );
    assert_eq!(
        positive_out.normalize(BatteryCurrentPolarity::PositiveIsIn),
        BatteryCurrent::current_in(2_000)
    );
    assert_eq!(
        negative_out.normalize(BatteryCurrentPolarity::PositiveIsOut),
        BatteryCurrent::current_in(2_000)
    );
}

#[test]
fn unknown_direction_and_integer_extremes_preserve_magnitude() {
    let raw = ReportedBatteryCurrent::from_milliamps(i32::MIN);
    assert_eq!(raw.as_milliamps(), i32::MIN);
    let unknown = raw.normalize(BatteryCurrentPolarity::Unknown);
    assert_eq!(unknown.direction(), BatteryCurrentDirection::Unknown);
    assert_eq!(unknown.magnitude_milliamps(), 2_147_483_648);
    assert_eq!(BatteryCurrent::current_in(0), BatteryCurrent::Zero);
    assert_eq!(BatteryCurrent::current_out(0), BatteryCurrent::Zero);
    assert_eq!(BatteryCurrent::unknown(0), BatteryCurrent::Zero);
    assert_ne!(BatteryCurrent::unknown(1), BatteryCurrent::Zero);
}

#[test]
fn direction_is_independent_of_charge_context() {
    let incoming = BatteryCurrent::current_in(2_000);
    assert_eq!(
        incoming.power_flow(RideOperatingState::Charging),
        PowerFlowDirection::Charging
    );
    assert_eq!(
        incoming.power_flow(RideOperatingState::Riding),
        PowerFlowDirection::Regeneration
    );
    for state in [
        RideOperatingState::Parked,
        RideOperatingState::Standing,
        RideOperatingState::Unknown,
    ] {
        assert_eq!(incoming.power_flow(state), PowerFlowDirection::Unknown);
        assert_eq!(
            BatteryCurrent::current_out(500).power_flow(state),
            PowerFlowDirection::Discharge
        );
    }
    assert_eq!(
        BatteryCurrent::unknown(2_000).power_flow(RideOperatingState::Charging),
        PowerFlowDirection::Unknown
    );
    assert_eq!(
        BatteryCurrent::Zero.power_flow(RideOperatingState::Riding),
        PowerFlowDirection::Zero
    );
}

#[test]
fn power_needs_direction_and_uses_pack_boundary_convention() {
    let voltage = Voltage::from_volts(100);
    assert_eq!(
        Power::from_pack_voltage_current(voltage, BatteryCurrent::current_out(2_000)),
        Some(Power::from_watts(200))
    );
    assert_eq!(
        Power::from_pack_voltage_current(voltage, BatteryCurrent::current_in(2_000)),
        Some(Power::from_watts(-200))
    );
    assert_eq!(
        Power::from_pack_voltage_current(voltage, BatteryCurrent::unknown(2_000)),
        None
    );
    assert_eq!(
        Power::from_pack_voltage_current(voltage, BatteryCurrent::Zero),
        Some(Power::from_watts(0))
    );
}

#[test]
fn ffi_roundtrip_preserves_direction_magnitude_and_evidence() {
    use cutout_core::{
        BatteryCurrentDto, BatteryCurrentReadingDto, Measured, ValueQuality, ValueQualityDto,
        ValueSource, ValueSourceDto, VerificationStatus, VerificationStatusDto,
    };
    for value in [
        BatteryCurrent::Zero,
        BatteryCurrent::current_in(u32::MAX),
        BatteryCurrent::current_out(1_234),
        BatteryCurrent::unknown(5_678),
    ] {
        let reading = Measured {
            value,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::SourceVerified,
        };
        let dto = BatteryCurrentReadingDto::from(reading);
        assert_eq!(BatteryCurrent::from(dto.value), value);
        assert_eq!(dto.source, ValueSourceDto::Reported);
        assert_eq!(dto.quality, ValueQualityDto::Known);
        assert_eq!(dto.verification, VerificationStatusDto::SourceVerified);
    }
    for input in [
        BatteryCurrentDto::In { milliamps: 0 },
        BatteryCurrentDto::Out { milliamps: 0 },
        BatteryCurrentDto::Unknown { milliamps: 0 },
    ] {
        assert_eq!(BatteryCurrent::from(input), BatteryCurrent::Zero);
    }
}

#[test]
fn retained_current_has_its_own_clock_and_stale_flow_is_unknown() {
    use cutout_core::{Measured, MonotonicTimestamp, TelemetryDelta, TelemetrySnapshot};
    let ms = MonotonicTimestamp::new;
    let mut snapshot = TelemetrySnapshot::default();
    snapshot.apply_delta(TelemetryDelta {
        voltage: Some(Measured::reported(Voltage::from_volts(100))),
        battery_current: Some(Measured::reported(BatteryCurrent::current_out(2_000))),
        ..TelemetryDelta::empty(ms(100))
    });
    for (at, expected) in [
        (5_100, PowerFlowDirection::Discharge),
        (5_101, PowerFlowDirection::Unknown),
    ] {
        snapshot.apply_delta(TelemetryDelta::empty(ms(at)));
        assert_eq!(snapshot.battery_current_observed_at_ms, Some(ms(100)));
        assert_eq!(snapshot.voltage_observed_at_ms, Some(ms(100)));
        assert_eq!(
            snapshot.battery_current.unwrap().value.power_flow_fresh(
                RideOperatingState::Riding,
                snapshot.battery_current_observed_at_ms,
                snapshot.at_ms
            ),
            expected
        );
    }
    for at in [None, Some(ms(5_102))] {
        assert_eq!(
            BatteryCurrent::current_in(2_000).power_flow_fresh(
                RideOperatingState::Charging,
                at,
                Some(ms(5_101))
            ),
            PowerFlowDirection::Unknown
        );
    }
}
