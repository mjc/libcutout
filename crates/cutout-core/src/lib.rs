#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Core types and setup scaffolding for Cutout.

/// Monotonic timestamp in milliseconds, supplied by the host.
pub type MonotonicMillis = u64;

/// Transport-independent identifier for a GATT characteristic or endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GattChannel([u8; 16]);

impl GattChannel {
    /// Creates a channel identifier from its 16-byte representation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the channel identifier as raw bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Host-observed link details supplied when a transport connects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkInfo {
    /// Host monotonic connection timestamp.
    pub monotonic_ms: MonotonicMillis,

    /// Maximum write payload length reported by the host, when known.
    pub max_write_len: Option<u16>,
}

/// Command requested by the host application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceCommand {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request a telemetry update.
    RequestTelemetry,
}

/// Transport write behavior requested by a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    /// Write with transport-level acknowledgement.
    WithResponse,

    /// Write without transport-level acknowledgement.
    WithoutResponse,
}

/// Where a measured value came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueSource {
    /// Value was reported directly by the device.
    Reported,

    /// Value was calculated by Cutout from other known values.
    Calculated,

    /// Value was estimated by Cutout from incomplete evidence.
    Estimated,
}

/// Confidence or usability of a measured value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueQuality {
    /// Value is directly supported by observed protocol data.
    Known,

    /// Value is inferred from partial, model-specific, or less direct evidence.
    Inferred,
}

/// A value with source and quality metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Measured<T> {
    /// Fixed-unit value.
    pub value: T,

    /// Source of the value.
    pub source: ValueSource,

    /// Quality of the value.
    pub quality: ValueQuality,
}

impl<T> Measured<T> {
    /// Creates a known value reported directly by the device.
    #[must_use]
    pub const fn reported(value: T) -> Self {
        Self {
            value,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
        }
    }
}

/// Partial telemetry update from a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryDelta {
    /// Host monotonic timestamp for this update.
    pub at_ms: MonotonicMillis,

    /// Reported or calculated speed in millimeters per second.
    pub speed_mm_s: Option<Measured<i32>>,

    /// Reported or measured input voltage in millivolts.
    pub voltage_mv: Option<Measured<i32>>,

    /// Battery/input current in milliamps.
    pub battery_current_ma: Option<Measured<i32>>,

    /// Motor/phase current in milliamps.
    pub motor_current_ma: Option<Measured<i32>>,

    /// Electrical power in milliwatts.
    pub power_mw: Option<Measured<i64>>,

    /// Controller temperature in millicelsius.
    pub controller_temperature_mc: Option<Measured<i32>>,

    /// Motor temperature in millicelsius.
    pub motor_temperature_mc: Option<Measured<i32>>,

    /// Battery temperature in millicelsius.
    pub battery_temperature_mc: Option<Measured<i32>>,

    /// PWM duty in permille.
    pub pwm_permille: Option<Measured<i16>>,

    /// Total or trip distance in millimeters.
    pub distance_mm: Option<Measured<u64>>,

    /// Pitch in millidegrees.
    pub pitch_mdeg: Option<Measured<i32>>,

    /// Roll in millidegrees.
    pub roll_mdeg: Option<Measured<i32>>,

    /// Battery percentage reported by the device.
    pub battery_percent_reported: Option<Measured<u8>>,

    /// Battery percentage estimated by Cutout.
    pub battery_percent_estimated: Option<Measured<u8>>,
}

impl TelemetryDelta {
    /// Creates an empty telemetry delta at a timestamp.
    #[must_use]
    pub const fn empty(at_ms: MonotonicMillis) -> Self {
        Self {
            at_ms,
            speed_mm_s: None,
            voltage_mv: None,
            battery_current_ma: None,
            motor_current_ma: None,
            power_mw: None,
            controller_temperature_mc: None,
            motor_temperature_mc: None,
            battery_temperature_mc: None,
            pwm_permille: None,
            distance_mm: None,
            pitch_mdeg: None,
            roll_mdeg: None,
            battery_percent_reported: None,
            battery_percent_estimated: None,
        }
    }
}

/// Aggregated latest-known telemetry snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TelemetrySnapshot {
    /// Timestamp of the latest applied delta.
    pub at_ms: Option<MonotonicMillis>,

    /// Latest known speed in millimeters per second.
    pub speed_mm_s: Option<Measured<i32>>,

    /// Latest known input voltage in millivolts.
    pub voltage_mv: Option<Measured<i32>>,

    /// Latest known battery/input current in milliamps.
    pub battery_current_ma: Option<Measured<i32>>,

    /// Latest known motor/phase current in milliamps.
    pub motor_current_ma: Option<Measured<i32>>,

    /// Latest known electrical power in milliwatts.
    pub power_mw: Option<Measured<i64>>,

    /// Latest known controller temperature in millicelsius.
    pub controller_temperature_mc: Option<Measured<i32>>,

    /// Latest known motor temperature in millicelsius.
    pub motor_temperature_mc: Option<Measured<i32>>,

    /// Latest known battery temperature in millicelsius.
    pub battery_temperature_mc: Option<Measured<i32>>,

    /// Latest known PWM duty in permille.
    pub pwm_permille: Option<Measured<i16>>,

    /// Latest known total or trip distance in millimeters.
    pub distance_mm: Option<Measured<u64>>,

    /// Latest known pitch in millidegrees.
    pub pitch_mdeg: Option<Measured<i32>>,

    /// Latest known roll in millidegrees.
    pub roll_mdeg: Option<Measured<i32>>,

    /// Latest known battery percentage reported by the device.
    pub battery_percent_reported: Option<Measured<u8>>,

    /// Latest known battery percentage estimated by Cutout.
    pub battery_percent_estimated: Option<Measured<u8>>,
}

impl TelemetrySnapshot {
    /// Applies a partial telemetry update, preserving fields absent from it.
    pub fn apply_delta(&mut self, delta: TelemetryDelta) {
        self.at_ms = Some(delta.at_ms);

        if delta.speed_mm_s.is_some() {
            self.speed_mm_s = delta.speed_mm_s;
        }
        if delta.voltage_mv.is_some() {
            self.voltage_mv = delta.voltage_mv;
        }
        if delta.battery_current_ma.is_some() {
            self.battery_current_ma = delta.battery_current_ma;
        }
        if delta.motor_current_ma.is_some() {
            self.motor_current_ma = delta.motor_current_ma;
        }
        if delta.power_mw.is_some() {
            self.power_mw = delta.power_mw;
        }
        if delta.controller_temperature_mc.is_some() {
            self.controller_temperature_mc = delta.controller_temperature_mc;
        }
        if delta.motor_temperature_mc.is_some() {
            self.motor_temperature_mc = delta.motor_temperature_mc;
        }
        if delta.battery_temperature_mc.is_some() {
            self.battery_temperature_mc = delta.battery_temperature_mc;
        }
        if delta.pwm_permille.is_some() {
            self.pwm_permille = delta.pwm_permille;
        }
        if delta.distance_mm.is_some() {
            self.distance_mm = delta.distance_mm;
        }
        if delta.pitch_mdeg.is_some() {
            self.pitch_mdeg = delta.pitch_mdeg;
        }
        if delta.roll_mdeg.is_some() {
            self.roll_mdeg = delta.roll_mdeg;
        }
        if delta.battery_percent_reported.is_some() {
            self.battery_percent_reported = delta.battery_percent_reported;
        }
        if delta.battery_percent_estimated.is_some() {
            self.battery_percent_estimated = delta.battery_percent_estimated;
        }
    }
}

/// Input supplied to a protocol session by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionInput<'a> {
    /// The underlying transport link is available.
    LinkUp(LinkInfo),

    /// The underlying transport link is no longer available.
    LinkDown,

    /// Notification bytes received from a transport endpoint.
    Notification {
        /// Transport endpoint that produced the bytes.
        channel: GattChannel,

        /// Borrowed notification payload for this reactor step.
        bytes: &'a [u8],

        /// Host monotonic receive timestamp.
        monotonic_ms: MonotonicMillis,
    },

    /// Timer tick supplied by the host.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicMillis,
    },

    /// Command requested by the host application.
    Command(DeviceCommand),
}

/// Action a host transport must perform for a protocol session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportAction {
    /// Subscribe to notifications from a transport endpoint.
    Subscribe {
        /// Transport endpoint to subscribe to.
        channel: GattChannel,
    },

    /// Write bytes to a transport endpoint.
    Write {
        /// Transport endpoint to write to.
        channel: GattChannel,

        /// Owned bytes to write after this reactor step.
        bytes: Vec<u8>,

        /// Transport write behavior.
        mode: WriteMode,
    },

    /// Disconnect the underlying transport.
    Disconnect,
}

/// Semantic event emitted by a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    /// Link-up event accepted by the session.
    LinkUp(LinkInfo),

    /// Link-down event accepted by the session.
    LinkDown,

    /// Notification metadata accepted by the session.
    NotificationReceived {
        /// Transport endpoint that produced the bytes.
        channel: GattChannel,

        /// Host monotonic receive timestamp.
        monotonic_ms: MonotonicMillis,

        /// Number of notification bytes observed.
        len: usize,
    },

    /// Tick event accepted by the session.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicMillis,
    },

    /// Telemetry update emitted by a protocol session.
    Telemetry(TelemetryDelta),
}

/// Output emitted by a protocol session for the host to drain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionOutput {
    /// Transport action to execute outside the protocol engine.
    Transport(TransportAction),

    /// Semantic event to report to the application.
    Event(DeviceEvent),
}

/// Synchronous protocol reactor.
pub trait ProtocolSession {
    /// Handles one input and appends any resulting outputs.
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>);
}

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-core"
}

#[cfg(test)]
mod tests {
    use super::crate_name;
    use crate::{
        DeviceCommand, DeviceEvent, GattChannel, LinkInfo, Measured, ProtocolSession, SessionInput,
        SessionOutput, TelemetryDelta, TelemetrySnapshot, TransportAction, ValueQuality,
        ValueSource, WriteMode,
    };

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-core");
    }

    #[derive(Default)]
    struct EchoSession {
        last_notification_len: usize,
        link_is_up: bool,
    }

    impl ProtocolSession for EchoSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(info) => {
                    self.link_is_up = true;
                    output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
                }
                SessionInput::LinkDown => {
                    self.link_is_up = false;
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown));
                }
                SessionInput::Notification {
                    bytes,
                    channel,
                    monotonic_ms,
                } => {
                    self.last_notification_len = bytes.len();
                    output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                        channel,
                        monotonic_ms,
                        len: bytes.len(),
                    }));
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                }
                SessionInput::Command(DeviceCommand::RequestTelemetry) => {
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes([1; 16]),
                        bytes: b"telemetry".to_vec(),
                        mode: WriteMode::WithResponse,
                    }));
                }
                SessionInput::Command(DeviceCommand::RequestIdentity) => {
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([2; 16]),
                    }));
                }
            }
        }
    }

    #[test]
    fn drives_a_session_without_runtime_or_ble_stack() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();
        let link = LinkInfo {
            monotonic_ms: 10,
            max_write_len: Some(185),
        };

        session.handle(SessionInput::LinkUp(link), &mut output);

        assert!(session.link_is_up);
        assert_eq!(
            output,
            vec![SessionOutput::Event(DeviceEvent::LinkUp(link))]
        );
    }

    #[test]
    fn passes_notification_bytes_through_borrowed_input() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();
        let channel = GattChannel::from_bytes([0xfe; 16]);

        session.handle(
            SessionInput::Notification {
                channel,
                bytes: &[0xdc, 0x5a, 0x5c],
                monotonic_ms: 20,
            },
            &mut output,
        );

        assert_eq!(session.last_notification_len, 3);
        assert_eq!(
            output,
            vec![SessionOutput::Event(DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms: 20,
                len: 3
            })]
        );
    }

    #[test]
    fn hosts_can_drain_owned_actions_after_each_input() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::RequestTelemetry),
            &mut output,
        );
        let drained = core::mem::take(&mut output);

        assert!(output.is_empty());
        assert_eq!(
            drained,
            vec![SessionOutput::Transport(TransportAction::Write {
                channel: GattChannel::from_bytes([1; 16]),
                bytes: b"telemetry".to_vec(),
                mode: WriteMode::WithResponse,
            })]
        );
    }

    #[test]
    fn telemetry_delta_updates_only_present_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        let first = TelemetryDelta {
            at_ms: 100,
            speed_mm_s: Some(Measured::reported(1_500)),
            voltage_mv: Some(Measured::reported(81_000)),
            battery_current_ma: Some(Measured::reported(-2_000)),
            ..TelemetryDelta::empty(100)
        };
        let second = TelemetryDelta {
            at_ms: 150,
            motor_temperature_mc: Some(Measured::reported(42_500)),
            ..TelemetryDelta::empty(150)
        };

        snapshot.apply_delta(first);
        snapshot.apply_delta(second);

        assert_eq!(snapshot.at_ms, Some(150));
        assert_eq!(snapshot.speed_mm_s, Some(Measured::reported(1_500)));
        assert_eq!(snapshot.voltage_mv, Some(Measured::reported(81_000)));
        assert_eq!(
            snapshot.motor_temperature_mc,
            Some(Measured::reported(42_500))
        );
    }

    #[test]
    fn zero_measurement_is_not_unknown() {
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(TelemetryDelta {
            at_ms: 200,
            speed_mm_s: Some(Measured::reported(0)),
            battery_current_ma: Some(Measured::reported(0)),
            ..TelemetryDelta::empty(200)
        });

        assert_eq!(snapshot.speed_mm_s, Some(Measured::reported(0)));
        assert_eq!(snapshot.battery_current_ma, Some(Measured::reported(0)));
        assert_eq!(snapshot.motor_current_ma, None);
    }

    #[test]
    fn telemetry_keeps_distinct_current_temperature_and_estimate_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        let estimated_percent = Measured {
            value: 76,
            source: ValueSource::Estimated,
            quality: ValueQuality::Inferred,
        };

        snapshot.apply_delta(TelemetryDelta {
            at_ms: 300,
            battery_current_ma: Some(Measured::reported(-1_200)),
            motor_current_ma: Some(Measured::reported(3_400)),
            controller_temperature_mc: Some(Measured::reported(35_000)),
            motor_temperature_mc: Some(Measured::reported(45_000)),
            battery_temperature_mc: Some(Measured::reported(31_000)),
            battery_percent_reported: Some(Measured::reported(80)),
            battery_percent_estimated: Some(estimated_percent),
            ..TelemetryDelta::empty(300)
        });

        assert_eq!(
            snapshot.battery_current_ma,
            Some(Measured::reported(-1_200))
        );
        assert_eq!(snapshot.motor_current_ma, Some(Measured::reported(3_400)));
        assert_eq!(
            snapshot.controller_temperature_mc,
            Some(Measured::reported(35_000))
        );
        assert_eq!(
            snapshot.motor_temperature_mc,
            Some(Measured::reported(45_000))
        );
        assert_eq!(
            snapshot.battery_temperature_mc,
            Some(Measured::reported(31_000))
        );
        assert_eq!(
            snapshot.battery_percent_reported,
            Some(Measured::reported(80))
        );
        assert_eq!(snapshot.battery_percent_estimated, Some(estimated_percent));
    }

    #[test]
    fn telemetry_delta_can_be_emitted_as_device_event() {
        let delta = TelemetryDelta {
            at_ms: 400,
            distance_mm: Some(Measured::reported(12_345)),
            ..TelemetryDelta::empty(400)
        };

        assert_eq!(
            DeviceEvent::Telemetry(delta),
            DeviceEvent::Telemetry(TelemetryDelta {
                at_ms: 400,
                distance_mm: Some(Measured::reported(12_345)),
                ..TelemetryDelta::empty(400)
            })
        );
    }
}
