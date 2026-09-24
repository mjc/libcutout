//! Rust-owned projection of live rider metrics for mobile dashboards.

use crate::{
    BatteryCurrent, BatteryLevel, Distance, DutyCycle, Measured, Power, RideOperatingState,
    Temperature, Voltage,
};

const PWM_IDLE_DEADBAND_PERMILLE: u16 = 20;

/// Availability of one projected rider metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiderMetricValue<T> {
    /// The metric has a usable typed value.
    Available(T),
    /// The metric is supported but does not apply in the current operating state.
    NotApplicable,
    /// The metric is supported but no current value is available.
    Unavailable,
}

/// Origin and value of the projected electrical power metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiderPowerValue {
    /// Power calculated from pack voltage and battery current, including zero current.
    CalculatedPackCurrent(Power),
    /// Power reported directly by the active protocol.
    Reported(Power),
    /// Power supplied with its original source and quality metadata.
    Measured(Measured<Power>),
    /// No usable power value is available.
    Unavailable,
}

/// Typed temperatures retained for the thermal dashboard metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RiderThermalReadback {
    /// Hottest available temperature.
    pub maximum: Temperature,
    /// Controller temperature, when available.
    pub controller: Option<Temperature>,
    /// Motor temperature, when available.
    pub motor: Option<Temperature>,
    /// Battery temperature, when available.
    pub battery: Option<Temperature>,
}

/// One metric included in the main live rider dashboard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiderDashboardMetricDescriptor {
    /// Battery percentage with its original measurement provenance.
    BatteryLevel {
        /// Prefer a reported percentage, falling back to a produced estimate.
        reading: Option<Measured<BatteryLevel>>,
    },
    /// Rust-owned time-to-full estimator, shown only while charging.
    ChargeEstimate,
    /// Pack voltage, including explicit current unavailability.
    PackVoltage {
        /// Current pack voltage value.
        value: RiderMetricValue<Voltage>,
    },
    /// Electrical power with its selection semantics resolved.
    Power {
        /// Current power value and origin.
        value: RiderPowerValue,
    },
    /// Hottest temperature and component readback.
    Thermal {
        /// Current thermal value.
        value: RiderMetricValue<RiderThermalReadback>,
    },
    /// Remaining limp-home distance. This descriptor exists only with a producer value.
    LimpHomeRange {
        /// Produced remaining distance.
        value: Distance,
    },
}

/// One metric included in the live rider safety section.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiderSafetyMetricDescriptor {
    /// Remaining PWM duty headroom in permille.
    PwmHeadroom {
        /// Current headroom availability and value.
        value: RiderMetricValue<u16>,
    },
}

/// Typed inputs needed to project the live rider dashboard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RiderDashboardInput {
    /// Current operating state.
    pub operating_state: RideOperatingState,
    /// Battery percentage reported by the device.
    pub battery_level_reported: Option<Measured<BatteryLevel>>,
    /// Battery percentage estimated by the protocol or energy model.
    pub battery_level_estimated: Option<Measured<BatteryLevel>>,
    /// Latest pack voltage.
    pub voltage: Option<Measured<Voltage>>,
    /// Latest battery current.
    pub battery_current: Option<Measured<BatteryCurrent>>,
    /// Latest protocol-reported power.
    pub reported_power: Option<Measured<Power>>,
    /// Latest controller temperature.
    pub controller_temperature: Option<Measured<Temperature>>,
    /// Latest motor temperature.
    pub motor_temperature: Option<Measured<Temperature>>,
    /// Latest battery temperature.
    pub battery_temperature: Option<Measured<Temperature>>,
    /// Latest PWM duty.
    pub pwm: Option<Measured<DutyCycle>>,
    /// Produced limp-home distance, when a defined producer exists.
    pub limp_home_range: Option<Measured<Distance>>,
}

/// Ordered live rider metrics ready for a platform adapter to render.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiderDashboardProjection {
    /// Main dashboard metrics in display order.
    pub dashboard_metrics: Vec<RiderDashboardMetricDescriptor>,
    /// Safety metrics in display order.
    pub safety_metrics: Vec<RiderSafetyMetricDescriptor>,
}

impl RiderDashboardProjection {
    /// Projects supported live rider metrics and omits metrics without a producer.
    #[must_use]
    pub fn from_input(input: RiderDashboardInput) -> Self {
        let mut dashboard_metrics = vec![
            RiderDashboardMetricDescriptor::BatteryLevel {
                reading: input
                    .battery_level_reported
                    .or(input.battery_level_estimated),
            },
            RiderDashboardMetricDescriptor::PackVoltage {
                value: input
                    .voltage
                    .map_or(RiderMetricValue::Unavailable, |reading| {
                        RiderMetricValue::Available(reading.value)
                    }),
            },
            RiderDashboardMetricDescriptor::Power {
                value: power_value(input.voltage, input.battery_current, input.reported_power),
            },
            RiderDashboardMetricDescriptor::Thermal {
                value: thermal_value(
                    input.controller_temperature,
                    input.motor_temperature,
                    input.battery_temperature,
                ),
            },
        ];
        if input.operating_state == RideOperatingState::Charging {
            dashboard_metrics.insert(0, RiderDashboardMetricDescriptor::ChargeEstimate);
        }
        if let Some(range) = input.limp_home_range {
            dashboard_metrics
                .push(RiderDashboardMetricDescriptor::LimpHomeRange { value: range.value });
        }

        Self {
            dashboard_metrics,
            safety_metrics: vec![RiderSafetyMetricDescriptor::PwmHeadroom {
                value: pwm_headroom(input.operating_state, input.pwm),
            }],
        }
    }
}

fn power_value(
    voltage: Option<Measured<Voltage>>,
    battery_current: Option<Measured<BatteryCurrent>>,
    reported_power: Option<Measured<Power>>,
) -> RiderPowerValue {
    if let (Some(voltage), Some(current)) = (voltage, battery_current) {
        return RiderPowerValue::CalculatedPackCurrent(Power::from_pack_voltage_current(
            voltage.value,
            current.value,
        ));
    }
    reported_power.map_or(RiderPowerValue::Unavailable, RiderPowerValue::Measured)
}

fn thermal_value(
    controller: Option<Measured<Temperature>>,
    motor: Option<Measured<Temperature>>,
    battery: Option<Measured<Temperature>>,
) -> RiderMetricValue<RiderThermalReadback> {
    let maximum = [controller, motor, battery]
        .into_iter()
        .flatten()
        .map(|reading| reading.value)
        .max_by_key(|temperature| temperature.as_millicelsius());
    maximum.map_or(RiderMetricValue::Unavailable, |maximum| {
        RiderMetricValue::Available(RiderThermalReadback {
            maximum,
            controller: controller.map(|reading| reading.value),
            motor: motor.map(|reading| reading.value),
            battery: battery.map(|reading| reading.value),
        })
    })
}

fn pwm_headroom(
    operating_state: RideOperatingState,
    pwm: Option<Measured<DutyCycle>>,
) -> RiderMetricValue<u16> {
    let Some(pwm) = pwm else {
        return RiderMetricValue::Unavailable;
    };
    if !matches!(
        operating_state,
        RideOperatingState::Riding | RideOperatingState::Standing
    ) {
        return RiderMetricValue::NotApplicable;
    }

    let raw_used = pwm.value.as_permille().unsigned_abs().min(1_000);
    let used = if raw_used <= PWM_IDLE_DEADBAND_PERMILLE {
        0
    } else {
        raw_used
    };
    RiderMetricValue::Available(1_000 - used)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BatteryCurrent, Distance, DutyCycle, Measured, Power, RideOperatingState, Temperature,
        Voltage,
    };

    fn empty_input() -> RiderDashboardInput {
        RiderDashboardInput {
            operating_state: RideOperatingState::Unknown,
            battery_level_reported: None,
            battery_level_estimated: None,
            voltage: None,
            battery_current: None,
            reported_power: None,
            controller_temperature: None,
            motor_temperature: None,
            battery_temperature: None,
            pwm: None,
            limp_home_range: None,
        }
    }

    #[test]
    fn battery_percentage_preserves_reported_zero_and_estimated_provenance() {
        let estimated = Measured::estimated(BatteryLevel::from_percent(62));
        let reported = Measured::reported(BatteryLevel::from_percent(0));
        for (battery_level_reported, battery_level_estimated, expected) in [
            (None, None, None),
            (None, Some(estimated), Some(estimated)),
            (Some(reported), Some(estimated), Some(reported)),
        ] {
            let projection = RiderDashboardProjection::from_input(RiderDashboardInput {
                battery_level_reported,
                battery_level_estimated,
                ..empty_input()
            });
            assert_eq!(
                projection.dashboard_metrics.first(),
                Some(&RiderDashboardMetricDescriptor::BatteryLevel { reading: expected })
            );
        }
    }

    #[test]
    fn charge_time_is_only_offered_while_charging() {
        for operating_state in [
            RideOperatingState::Unknown,
            RideOperatingState::Parked,
            RideOperatingState::Standing,
            RideOperatingState::Riding,
            RideOperatingState::Charging,
        ] {
            let projection = RiderDashboardProjection::from_input(RiderDashboardInput {
                operating_state,
                ..empty_input()
            });
            assert_eq!(
                projection
                    .dashboard_metrics
                    .contains(&RiderDashboardMetricDescriptor::ChargeEstimate),
                operating_state == RideOperatingState::Charging,
                "{operating_state:?}"
            );
        }
    }

    #[test]
    fn projection_omits_metrics_without_a_producer() {
        let projection = RiderDashboardProjection::from_input(empty_input());

        assert_eq!(
            projection.dashboard_metrics,
            vec![
                RiderDashboardMetricDescriptor::BatteryLevel { reading: None },
                RiderDashboardMetricDescriptor::PackVoltage {
                    value: RiderMetricValue::Unavailable,
                },
                RiderDashboardMetricDescriptor::Power {
                    value: RiderPowerValue::Unavailable,
                },
                RiderDashboardMetricDescriptor::Thermal {
                    value: RiderMetricValue::Unavailable,
                },
            ]
        );
        assert_eq!(
            projection.safety_metrics,
            vec![RiderSafetyMetricDescriptor::PwmHeadroom {
                value: RiderMetricValue::Unavailable,
            }]
        );
    }

    #[test]
    fn projection_preserves_available_values_and_zero() {
        let projection = RiderDashboardProjection::from_input(RiderDashboardInput {
            operating_state: RideOperatingState::Riding,
            battery_level_reported: None,
            battery_level_estimated: None,
            voltage: Some(Measured::reported(Voltage::from_millivolts(60_000))),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(0))),
            reported_power: Some(Measured::reported(Power::from_milliwatts(0))),
            controller_temperature: Some(Measured::reported(Temperature::from_millicelsius(
                42_000,
            ))),
            motor_temperature: Some(Measured::reported(Temperature::from_millicelsius(54_000))),
            battery_temperature: None,
            pwm: Some(Measured::reported(DutyCycle::from_permille(1_000))),
            limp_home_range: Some(Measured::estimated(Distance::from_millimetres(22_852_500))),
        });

        assert_eq!(
            projection.dashboard_metrics,
            vec![
                RiderDashboardMetricDescriptor::BatteryLevel { reading: None },
                RiderDashboardMetricDescriptor::PackVoltage {
                    value: RiderMetricValue::Available(Voltage::from_millivolts(60_000)),
                },
                RiderDashboardMetricDescriptor::Power {
                    value: RiderPowerValue::CalculatedPackCurrent(Power::from_milliwatts(0)),
                },
                RiderDashboardMetricDescriptor::Thermal {
                    value: RiderMetricValue::Available(RiderThermalReadback {
                        maximum: Temperature::from_millicelsius(54_000),
                        controller: Some(Temperature::from_millicelsius(42_000)),
                        motor: Some(Temperature::from_millicelsius(54_000)),
                        battery: None,
                    }),
                },
                RiderDashboardMetricDescriptor::LimpHomeRange {
                    value: Distance::from_millimetres(22_852_500),
                },
            ]
        );
        assert_eq!(
            projection.safety_metrics,
            vec![RiderSafetyMetricDescriptor::PwmHeadroom {
                value: RiderMetricValue::Available(0),
            }]
        );
    }

    #[test]
    fn projection_calculates_pack_power_and_pwm_headroom() {
        let projection = RiderDashboardProjection::from_input(RiderDashboardInput {
            operating_state: RideOperatingState::Standing,
            voltage: Some(Measured::reported(Voltage::from_millivolts(60_000))),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(10_000))),
            reported_power: Some(Measured::reported(Power::from_milliwatts(900_000))),
            pwm: Some(Measured::reported(DutyCycle::from_permille(-450))),
            ..empty_input()
        });

        assert_eq!(
            projection.dashboard_metrics[2],
            RiderDashboardMetricDescriptor::Power {
                value: RiderPowerValue::CalculatedPackCurrent(Power::from_milliwatts(600_000)),
            }
        );
        assert_eq!(
            projection.safety_metrics,
            vec![RiderSafetyMetricDescriptor::PwmHeadroom {
                value: RiderMetricValue::Available(550),
            }]
        );
    }

    #[test]
    fn projection_preserves_fallback_power_provenance() {
        let calculated = Measured::calculated(Power::from_watts(3));
        let estimated = Measured::estimated(Power::from_watts(2));

        let calculated_projection = RiderDashboardProjection::from_input(RiderDashboardInput {
            reported_power: Some(calculated),
            ..empty_input()
        });
        let estimated_projection = RiderDashboardProjection::from_input(RiderDashboardInput {
            reported_power: Some(estimated),
            ..empty_input()
        });

        assert_eq!(
            calculated_projection.dashboard_metrics[2],
            RiderDashboardMetricDescriptor::Power {
                value: RiderPowerValue::Measured(calculated),
            }
        );
        assert_eq!(
            estimated_projection.dashboard_metrics[2],
            RiderDashboardMetricDescriptor::Power {
                value: RiderPowerValue::Measured(estimated),
            }
        );
    }

    #[test]
    fn pwm_headroom_is_not_applicable_outside_balancing_states() {
        for operating_state in [
            RideOperatingState::Unknown,
            RideOperatingState::Parked,
            RideOperatingState::Charging,
        ] {
            let projection = RiderDashboardProjection::from_input(RiderDashboardInput {
                operating_state,
                pwm: Some(Measured::reported(DutyCycle::from_permille(500))),
                ..empty_input()
            });

            assert_eq!(
                projection.safety_metrics,
                vec![RiderSafetyMetricDescriptor::PwmHeadroom {
                    value: RiderMetricValue::NotApplicable,
                }]
            );
        }
    }
}
