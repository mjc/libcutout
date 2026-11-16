//! Fail-closed Backfire telemetry.
//!
//! Admission requires the telemetry characteristic, a 25-byte frame, the archived
//! header, and a CRC-16/MODBUS residue of zero. The archived controller exposes a
//! write characteristic, but no write payload is pinned, so this session never writes.

use cutout_core::{
    BatteryLevel, Capabilities, DeviceEvent, Distance, GattChannel, GattFingerprint, HostSession,
    Measured, MonotonicTimestamp, ProtocolSession, SessionInput, SessionInputDto, SessionOutput,
    TelemetryDelta, TransportAction, ValueQuality, ValueSource, VerificationStatus, Voltage,
};

use crate::DeviceSessionStep;

/// Service `0000f1f0-0000-1000-8000-00805f9b34fb`.
pub const BACKFIRE_SERVICE_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xf1, 0xf0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// Write characteristic `0000f1f1-0000-1000-8000-00805f9b34fb`.
///
/// Present in the archived controller and intentionally unused. No payload is pinned.
pub const BACKFIRE_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xf1, 0xf1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// Notify and read characteristic `0000f1f2-0000-1000-8000-00805f9b34fb`.
pub const BACKFIRE_TELEMETRY_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xf1, 0xf2, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

const FRAME_LEN: usize = 25;
const BODY_LEN: usize = 23;
const HEADER: u8 = 0xac;
const LENGTH_BYTE: u8 = 0x19;
const MODE_INDEX: usize = 4;
const BATTERY_INDEX: usize = 5;
const VOLTAGE_INDEX: usize = 10;
const TRIP_INDEX: usize = 16;
const ODOMETER_INDEX: usize = 18;
const MILLIMETRES_PER_HECTOMETRE: u64 = 100_000;

/// Ride mode byte from one archived Backfire report.
///
/// This is not a shared ride-operating mode. Bytes other than the three archived
/// values stay unspecified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackfireRideMode {
    /// Byte `0x01`.
    Economy,
    /// Byte `0x02`.
    Speed,
    /// Byte `0x03`.
    Turbo,
    /// Any other mode byte.
    Unspecified,
}

impl BackfireRideMode {
    const fn from_byte(byte: u8) -> Self {
        match byte {
            1 => Self::Economy,
            2 => Self::Speed,
            3 => Self::Turbo,
            _ => Self::Unspecified,
        }
    }
}

/// Fields pinned by the archived 25-byte screenshot.
///
/// Speed is absent. Bytes 6–9 are not decoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackfireReport {
    mode: BackfireRideMode,
    battery: BatteryLevel,
    pack_voltage: Voltage,
    trip: Distance,
    odometer: Distance,
}

impl BackfireReport {
    /// Decodes one complete telemetry notification.
    ///
    /// Returns `None` unless the buffer is exactly 25 bytes, starts with `AC`,
    /// carries length byte `0x19`, has a CRC-16/MODBUS residue of zero, and a
    /// battery percent in `0..=100`.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != FRAME_LEN {
            return None;
        }
        let header = *bytes.first()?;
        let length = *bytes.get(2)?;
        if header != HEADER || length != LENGTH_BYTE {
            return None;
        }
        let body = bytes.get(..BODY_LEN)?;
        let stored = stored_crc(bytes)?;
        if crc16_modbus(body) != stored {
            return None;
        }
        let mode = BackfireRideMode::from_byte(*bytes.get(MODE_INDEX)?);
        let battery = BatteryLevel::try_from_percent(*bytes.get(BATTERY_INDEX)?)?;
        let pack_voltage = Voltage::from_millivolts(i32::from(be_u16(bytes, VOLTAGE_INDEX)?));
        let trip = Distance::from_millimetres(
            u64::from(be_u16(bytes, TRIP_INDEX)?).saturating_mul(MILLIMETRES_PER_HECTOMETRE),
        );
        let odometer = Distance::from_millimetres(
            u64::from(be_u24(bytes, ODOMETER_INDEX)?).saturating_mul(MILLIMETRES_PER_HECTOMETRE),
        );
        Some(Self {
            mode,
            battery,
            pack_voltage,
            trip,
            odometer,
        })
    }

    fn telemetry(self, at_ms: MonotonicTimestamp) -> TelemetryDelta {
        let mut delta = TelemetryDelta::empty(at_ms);
        delta.voltage = Some(source_verified(self.pack_voltage));
        delta.distance = Some(source_verified(self.odometer));
        delta.trip_distance = Some(source_verified(self.trip));
        delta.battery_level_reported = Some(source_verified(self.battery));
        delta
    }
}

/// Notify characteristics a host may subscribe to before any telemetry frame arrives.
///
/// Only the telemetry characteristic with notify support is returned. The write
/// characteristic is never selected.
#[must_use]
pub fn passive_observation_channels(fingerprints: &[GattFingerprint]) -> Vec<GattChannel> {
    let mut channels = Vec::new();
    for fingerprint in fingerprints {
        if fingerprint.characteristic == BACKFIRE_TELEMETRY_CHANNEL
            && fingerprint.roles.supports_notify()
            && !channels.contains(&fingerprint.characteristic)
        {
            channels.push(fingerprint.characteristic);
        }
    }
    channels
}

/// Passive Backfire session. Link-up subscribes to telemetry and never writes.
#[derive(Debug)]
pub(crate) struct BackfireSession {
    host: HostSession<BackfireReactor>,
}

impl BackfireSession {
    pub(crate) fn new() -> Self {
        Self {
            host: HostSession::new(BackfireReactor),
        }
    }

    pub(crate) fn ingest_typed(&mut self, input: &SessionInputDto) -> DeviceSessionStep {
        if let SessionInputDto::CommandAt { monotonic_ms, .. } = input {
            self.host.ingest(SessionInput::Tick {
                monotonic_ms: MonotonicTimestamp::new(monotonic_ms.milliseconds),
            });
        }
        self.host.ingest(input.as_session_input());
        DeviceSessionStep::drain(&mut self.host, input, Capabilities::default())
    }

    pub(crate) fn current_snapshot(&self) -> cutout_core::TelemetrySnapshotDto {
        self.host.current_snapshot().into()
    }

    pub(crate) fn diagnostics(&self) -> cutout_core::ParserDiagnosticsDto {
        self.host.diagnostics().into()
    }
}

#[derive(Debug, Default)]
struct BackfireReactor;

impl ProtocolSession for BackfireReactor {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(link) => {
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(link)));
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: BACKFIRE_TELEMETRY_CHANNEL,
                }));
            }
            SessionInput::LinkDown => {
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Tick { monotonic_ms } => {
                output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
            }
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => {
                if channel != BACKFIRE_TELEMETRY_CHANNEL {
                    return;
                }
                let Some(report) = BackfireReport::decode(bytes) else {
                    return;
                };
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    report.telemetry(monotonic_ms),
                )));
            }
            SessionInput::Command(_) => {}
        }
    }
}

/// CRC-16/MODBUS: init `0xFFFF`, reflected polynomial `0xA001`, xorout `0x0000`.
fn crc16_modbus(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0xffff, |crc, byte| {
        (0..8).fold(crc ^ u16::from(*byte), |crc, _| {
            if crc & 1 == 1 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            }
        })
    })
}

fn stored_crc(bytes: &[u8]) -> Option<u16> {
    let suffix = bytes.get(BODY_LEN..FRAME_LEN)?;
    let [low, high]: [u8; 2] = suffix.try_into().ok()?;
    Some(u16::from_le_bytes([low, high]))
}

fn be_u16(bytes: &[u8], start: usize) -> Option<u16> {
    let end = start.checked_add(2)?;
    let slice = bytes.get(start..end)?;
    let [high, low]: [u8; 2] = slice.try_into().ok()?;
    Some(u16::from_be_bytes([high, low]))
}

fn be_u24(bytes: &[u8], start: usize) -> Option<u32> {
    let end = start.checked_add(3)?;
    let slice = bytes.get(start..end)?;
    let [first, second, third]: [u8; 3] = slice.try_into().ok()?;
    Some(u32::from_be_bytes([0, first, second, third]))
}

fn source_verified<T>(value: T) -> Measured<T> {
    Measured {
        value,
        source: ValueSource::Reported,
        quality: ValueQuality::Known,
        verification: VerificationStatus::SourceVerified,
    }
}

#[cfg(test)]
mod tests {
    use cutout_core::{
        ControlRefusalReason, CutoutSessionState, DeviceCommandDto, DeviceEvent, GattRoles,
        MonotonicMillisDto, ProtocolFamily, SessionInputDto, SessionOutput, TransportAction,
        ValueQualityDto, ValueSourceDto, VehicleKind, VerificationStatusDto,
    };

    use super::*;
    use crate::{
        DeviceDetectionEvent, DeviceDetectionResolution, DeviceDetectionSession, DeviceSession,
        ProtocolFamilyState,
    };

    /// Archived stationary screenshot. CRC covers bytes 0–22 and is stored little-endian.
    const SCREENSHOT: [u8; 25] = [
        0xac, 0x00, 0x19, 0x01, 0x01, 0x46, 0x00, 0x00, 0x00, 0x00, 0xb8, 0xa7, 0x00, 0x07, 0x00,
        0x00, 0x00, 0x01, 0x00, 0x03, 0x6e, 0x80, 0x00, 0xf4, 0x88,
    ];

    /// Speculative 20-byte parser sample. It conflicts with the screenshot and is not a frame.
    const LEGACY_TWENTY: [u8; 20] = [
        0xac, 0x06, 0x19, 0x01, 0x01, 0x55, 0x50, 0xbc, 0x50, 0xcc, 0xbc, 0xe0, 0x00, 0x24, 0x00,
        0x00, 0x00, 0x03, 0x00, 0x00,
    ];

    fn with_crc(body: [u8; 23]) -> [u8; 25] {
        let crc = crc16_modbus(&body).to_le_bytes();
        let mut frame = [0_u8; 25];
        frame[..23].copy_from_slice(&body);
        frame[23] = crc[0];
        frame[24] = crc[1];
        frame
    }

    fn veteran_frame() -> [u8; 42] {
        let mut frame = [0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&60_000_u16.to_be_bytes());
        frame
    }

    fn observe(event: DeviceDetectionEvent<'_>) -> DeviceDetectionResolution {
        let mut state = CutoutSessionState::default();
        let mut detector = DeviceDetectionSession::default();
        detector.observe(&mut state, event)
    }

    #[test]
    fn modbus_check_value_and_screenshot_residue_match_the_archive() {
        assert_eq!(crc16_modbus(b"123456789"), 0x4b37);
        assert_eq!(crc16_modbus(&SCREENSHOT[..23]), 0x88f4);
        assert_eq!(crc16_modbus(&SCREENSHOT), 0);
    }

    #[test]
    fn screenshot_publishes_pinned_fields_and_leaves_speed_unread() {
        let report = BackfireReport::decode(&SCREENSHOT).expect("screenshot frame");
        assert_eq!(report.mode, BackfireRideMode::Economy);
        assert_eq!(report.battery.as_percent(), 70);
        assert_eq!(report.pack_voltage.as_millivolts(), 47_271);
        assert_eq!(report.trip.as_millimetres(), 100_000);
        assert_eq!(report.odometer.as_millimetres(), 87_800_000);

        let mut session = admitted_session();
        let _ = session.ingest_typed(&notification(BACKFIRE_TELEMETRY_CHANNEL, &SCREENSHOT));
        let snapshot = session.current_snapshot();
        assert_eq!(snapshot.speed, None);
        assert_eq!(snapshot.operating_mode, None);
        assert_eq!(reading_percent(snapshot.battery_level_reported), Some(70));
        assert_eq!(reading_millivolts(snapshot.voltage), Some(47_271));
        assert_eq!(reading_millimetres(snapshot.trip_distance), Some(100_000));
        assert_eq!(reading_millimetres(snapshot.distance), Some(87_800_000));
        assert_eq!(
            snapshot
                .battery_level_reported
                .map(|reading| reading.verification),
            Some(VerificationStatusDto::SourceVerified)
        );
        assert_eq!(
            snapshot.voltage.map(|reading| reading.source),
            Some(ValueSourceDto::Reported)
        );
        assert_eq!(
            snapshot.voltage.map(|reading| reading.quality),
            Some(ValueQualityDto::Known)
        );
    }

    #[test]
    fn nonzero_speed_bytes_still_leave_speed_absent() {
        let mut body = [0_u8; 23];
        body.copy_from_slice(&SCREENSHOT[..23]);
        body[6] = 0x12;
        body[7] = 0x34;
        body[8] = 0x56;
        body[9] = 0x78;
        let frame = with_crc(body);
        assert!(BackfireReport::decode(&frame).is_some());
        let mut session = admitted_session();
        let _ = session.ingest_typed(&notification(BACKFIRE_TELEMETRY_CHANNEL, &frame));
        assert_eq!(session.current_snapshot().speed, None);
    }

    #[test]
    fn rejected_frames_do_not_decode() {
        assert!(BackfireReport::decode(&LEGACY_TWENTY).is_none());
        assert!(BackfireReport::decode(&SCREENSHOT[..24]).is_none());
        let mut bad_crc = SCREENSHOT;
        bad_crc[24] ^= 0xff;
        assert!(BackfireReport::decode(&bad_crc).is_none());
        let mut bad_header = SCREENSHOT;
        bad_header[0] = 0xab;
        let header_crc = crc16_modbus(&bad_header[..23]).to_le_bytes();
        bad_header[23..].copy_from_slice(&header_crc);
        assert!(BackfireReport::decode(&bad_header).is_none());
        let mut bad_length = SCREENSHOT;
        bad_length[2] = 0x18;
        let length_crc = crc16_modbus(&bad_length[..23]).to_le_bytes();
        bad_length[23..].copy_from_slice(&length_crc);
        assert!(BackfireReport::decode(&bad_length).is_none());
        let mut over_percent = [0_u8; 23];
        over_percent.copy_from_slice(&SCREENSHOT[..23]);
        over_percent[5] = 101;
        assert!(BackfireReport::decode(&with_crc(over_percent)).is_none());
    }

    #[test]
    fn other_mode_bytes_stay_on_the_report() {
        let mut speed = [0_u8; 23];
        speed.copy_from_slice(&SCREENSHOT[..23]);
        speed[4] = 2;
        assert_eq!(
            BackfireReport::decode(&with_crc(speed))
                .expect("speed mode")
                .mode,
            BackfireRideMode::Speed
        );
        let mut turbo = [0_u8; 23];
        turbo.copy_from_slice(&SCREENSHOT[..23]);
        turbo[4] = 3;
        assert_eq!(
            BackfireReport::decode(&with_crc(turbo))
                .expect("turbo mode")
                .mode,
            BackfireRideMode::Turbo
        );
        let mut other = [0_u8; 23];
        other.copy_from_slice(&SCREENSHOT[..23]);
        other[4] = 9;
        assert_eq!(
            BackfireReport::decode(&with_crc(other))
                .expect("unspecified mode")
                .mode,
            BackfireRideMode::Unspecified
        );
    }

    #[test]
    fn link_up_subscribes_to_telemetry_and_a_command_does_not_write() {
        let mut session = admitted_session();
        let step = session.ingest_typed(&SessionInputDto::LinkUp {
            monotonic_ms: MonotonicMillisDto { milliseconds: 1 },
            max_write_len: None,
        });
        let subscribed: Vec<_> = step
            .outputs
            .iter()
            .filter_map(|output| match output {
                SessionOutput::Transport(TransportAction::Subscribe { channel }) => Some(*channel),
                _ => None,
            })
            .collect();
        assert_eq!(subscribed, vec![BACKFIRE_TELEMETRY_CHANNEL]);
        assert!(
            subscribed
                .iter()
                .all(|channel| *channel != BACKFIRE_WRITE_CHANNEL)
        );
        assert!(
            step.outputs
                .iter()
                .any(|output| { matches!(output, SessionOutput::Event(DeviceEvent::LinkUp(_))) })
        );
        assert!(step.outputs.iter().all(|output| {
            !matches!(
                output,
                SessionOutput::Transport(TransportAction::Write { .. })
            )
        }));

        let command = session.ingest_typed(&SessionInputDto::Command(
            DeviceCommandDto::RequestTelemetry,
        ));
        assert_eq!(
            command.error.map(|refusal| refusal.reason),
            Some(ControlRefusalReason::UnsupportedCommand)
        );
        assert!(command.outputs.iter().all(|output| {
            !matches!(
                output,
                SessionOutput::Transport(TransportAction::Write { .. })
            )
        }));
    }

    #[test]
    fn wrong_channel_and_bad_frame_leave_the_snapshot_unchanged() {
        let mut session = admitted_session();
        let before = session.current_snapshot();
        let _ = session.ingest_typed(&notification(BACKFIRE_WRITE_CHANNEL, &SCREENSHOT));
        let _ = session.ingest_typed(&notification(BACKFIRE_TELEMETRY_CHANNEL, &LEGACY_TWENTY));
        assert_eq!(session.current_snapshot(), before);
    }

    #[test]
    fn telemetry_channel_admits_backfire_without_a_catalog_model() {
        let resolution = observe(DeviceDetectionEvent::NotificationOnChannel {
            channel: BACKFIRE_TELEMETRY_CHANNEL,
            bytes: &SCREENSHOT,
        });
        assert_eq!(resolution.protocol, ProtocolFamilyState::Backfire);
        assert!(!resolution.awaits_passive_evidence());
        assert_eq!(resolution.staged.model, None);
        let session = DeviceSession::from_detection(&resolution).expect("family session");
        assert_eq!(session.identity().protocol, ProtocolFamily::Backfire);
        assert_eq!(session.identity().vehicle_kind(), VehicleKind::Eskate);
        assert_eq!(session.identity().model, None);
    }

    #[test]
    fn uuid_name_and_channel_less_bytes_do_not_admit_backfire() {
        let channel_less = observe(DeviceDetectionEvent::Notification { bytes: &SCREENSHOT });
        assert_ne!(channel_less.protocol, ProtocolFamilyState::Backfire);
        assert!(DeviceSession::from_detection(&channel_less).is_none());

        let write_channel = observe(DeviceDetectionEvent::NotificationOnChannel {
            channel: BACKFIRE_WRITE_CHANNEL,
            bytes: &SCREENSHOT,
        });
        assert_ne!(write_channel.protocol, ProtocolFamilyState::Backfire);

        let service_only = observe(DeviceDetectionEvent::Gatt {
            gatt: &[fingerprint(
                BACKFIRE_SERVICE_CHANNEL,
                BACKFIRE_TELEMETRY_CHANNEL,
                GattRoles::empty().with_notify(),
            )],
        });
        assert_ne!(service_only.protocol, ProtocolFamilyState::Backfire);

        let named = observe(DeviceDetectionEvent::Advertisement {
            name: Some(b"BF_Zealot"),
        });
        assert_ne!(named.protocol, ProtocolFamilyState::Backfire);
    }

    #[test]
    fn a_later_distinct_family_conflicts() {
        let mut state = CutoutSessionState::default();
        let mut detector = DeviceDetectionSession::default();
        let _ = detector.observe(
            &mut state,
            DeviceDetectionEvent::NotificationOnChannel {
                channel: BACKFIRE_TELEMETRY_CHANNEL,
                bytes: &SCREENSHOT,
            },
        );
        let resolution = detector.observe(
            &mut state,
            DeviceDetectionEvent::Notification {
                bytes: &veteran_frame(),
            },
        );
        assert_eq!(resolution.protocol, ProtocolFamilyState::Conflict);
        assert!(DeviceSession::from_detection(&resolution).is_none());
    }

    #[test]
    fn passive_channels_require_notify_on_the_telemetry_characteristic() {
        let telemetry = fingerprint(
            BACKFIRE_SERVICE_CHANNEL,
            BACKFIRE_TELEMETRY_CHANNEL,
            GattRoles::empty().with_notify(),
        );
        let write = fingerprint(
            BACKFIRE_SERVICE_CHANNEL,
            BACKFIRE_WRITE_CHANNEL,
            GattRoles::empty().with_write(),
        );
        let indicate = fingerprint(
            BACKFIRE_SERVICE_CHANNEL,
            BACKFIRE_TELEMETRY_CHANNEL,
            GattRoles::empty().with_indicate(),
        );
        let other = fingerprint(
            BACKFIRE_SERVICE_CHANNEL,
            BACKFIRE_SERVICE_CHANNEL,
            GattRoles::empty().with_notify(),
        );
        assert_eq!(
            passive_observation_channels(&[telemetry, telemetry, write, indicate, other]),
            vec![BACKFIRE_TELEMETRY_CHANNEL]
        );
        assert_eq!(passive_observation_channels(&[write, indicate]), Vec::new());
    }

    fn admitted_session() -> DeviceSession {
        let resolution = observe(DeviceDetectionEvent::NotificationOnChannel {
            channel: BACKFIRE_TELEMETRY_CHANNEL,
            bytes: &SCREENSHOT,
        });
        DeviceSession::from_detection(&resolution).expect("admitted backfire")
    }

    fn notification(channel: GattChannel, bytes: &[u8]) -> SessionInputDto {
        SessionInputDto::Notification {
            channel: channel.as_bytes(),
            bytes: bytes.to_vec(),
            monotonic_ms: MonotonicMillisDto { milliseconds: 10 },
        }
    }

    fn fingerprint(
        service: GattChannel,
        characteristic: GattChannel,
        roles: GattRoles,
    ) -> cutout_core::GattFingerprint {
        cutout_core::GattFingerprint {
            service,
            characteristic,
            roles,
            verification: cutout_core::VerificationStatus::SourceVerified,
        }
    }

    fn reading_percent(reading: Option<cutout_core::BatteryLevelReadingDto>) -> Option<u8> {
        reading.map(|reading| reading.value)
    }

    fn reading_millivolts(reading: Option<cutout_core::VoltageReadingDto>) -> Option<i32> {
        reading.map(|reading| reading.value)
    }

    fn reading_millimetres(reading: Option<cutout_core::DistanceReadingDto>) -> Option<u64> {
        reading.map(|reading| reading.value)
    }
}
