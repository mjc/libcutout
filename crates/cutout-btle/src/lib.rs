#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Bluetooth transport adapter scaffolding for Cutout.

use std::{collections::BTreeSet, fmt, future::Future, pin::Pin, time::Duration};

use async_trait::async_trait;
use btleplug::{
    api::{
        Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _,
        PeripheralProperties, ScanFilter, Service, ValueNotification, WriteType,
    },
    platform::{Adapter, Manager},
};
use cutout_core::{
    DeviceEvent, FirmwareInfo, GattChannel, LinkInfo, ProtocolSession, ReadOnlyResponse,
    SessionInput, SessionOutput, SettingsReadback, TelemetrySnapshot, TransportAction, WriteMode,
};
use futures_util::{StreamExt, stream::Stream};
use thiserror::Error;
use uuid::Uuid;

const BATTERY_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb);
const BATTERY_LEVEL_UUID: Uuid = Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb);
const TARGETED_SCAN_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-btle"
}

/// A peripheral observation gathered from a scan or connection pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeripheralObservation {
    /// Platform-specific peripheral identifier.
    pub identifier: String,

    /// Bluetooth address when the platform exposes one.
    pub address: Option<String>,

    /// Peripheral local name, if one was advertised.
    pub name: Option<String>,

    /// Received signal strength, if the platform exposed it.
    pub rssi: Option<i16>,

    /// Advertised service UUIDs, if the peripheral exposed them.
    pub advertised_services: Vec<Uuid>,
}

impl PeripheralObservation {
    fn from_peripheral(
        peripheral: &btleplug::platform::Peripheral,
        properties: &PeripheralProperties,
    ) -> Self {
        Self {
            identifier: peripheral.id().to_string(),
            address: normalize_address(properties.address.to_string()),
            name: properties.local_name.clone(),
            rssi: properties.rssi,
            advertised_services: properties.services.clone(),
        }
    }
}

impl fmt::Display for PeripheralObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(address) = &self.address {
            write!(f, "{address}")?;
        } else {
            write!(f, "id={}", self.identifier)?;
        }
        write!(f, " name={}", self.name.as_deref().unwrap_or("<none>"))?;
        if let Some(rssi) = self.rssi {
            write!(f, " rssi={rssi}")?;
        }
        write!(f, " services=[{}]", join_uuids(&self.advertised_services))
    }
}

/// Target used to select a peripheral from scan results.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionTarget {
    /// Match against the peripheral address, when provided.
    pub address: Option<String>,

    /// Match against the platform-specific peripheral identifier.
    pub identifier: Option<String>,

    /// Match against the peripheral local name, when provided.
    pub name_contains: Option<String>,
}

impl ConnectionTarget {
    /// Returns whether an observation matches this target.
    #[must_use]
    pub fn matches(&self, observation: &PeripheralObservation) -> bool {
        let address_matches = self
            .address
            .as_ref()
            .is_none_or(|address| observation.address.as_deref() == Some(address.as_str()));
        let identifier_matches = self
            .identifier
            .as_ref()
            .is_none_or(|identifier| observation.identifier == *identifier);
        let name_matches = self.name_contains.as_ref().is_none_or(|needle| {
            observation
                .name
                .as_deref()
                .is_some_and(|name| name.contains(needle))
        });

        address_matches && identifier_matches && name_matches
    }
}

/// Summary of a successful connection/discovery pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacteristicSummary {
    /// Characteristic UUID.
    pub uuid: Uuid,

    /// Owning service UUID.
    pub service_uuid: Uuid,

    /// GATT characteristic properties.
    pub properties: CharPropFlags,
}

impl CharacteristicSummary {
    /// Returns whether this characteristic can accept a write.
    #[must_use]
    pub fn can_write(&self) -> bool {
        self.properties
            .intersects(CharPropFlags::WRITE | CharPropFlags::WRITE_WITHOUT_RESPONSE)
    }

    /// Returns whether this characteristic can be read.
    #[must_use]
    pub fn can_read(&self) -> bool {
        self.properties.contains(CharPropFlags::READ)
    }

    /// Returns whether this characteristic can notify or indicate.
    #[must_use]
    pub fn can_notify(&self) -> bool {
        self.properties
            .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
    }
}

/// Service-level summary of a discovered peripheral.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceSummary {
    /// Service UUID.
    pub uuid: Uuid,

    /// Whether this is a primary service.
    pub primary: bool,

    /// Discovered characteristics for the service.
    pub characteristics: Vec<CharacteristicSummary>,
}

impl ServiceSummary {
    fn from_service(service: &Service) -> Self {
        Self {
            uuid: service.uuid,
            primary: service.primary,
            characteristics: service
                .characteristics
                .iter()
                .map(CharacteristicSummary::from_characteristic)
                .collect(),
        }
    }
}

impl CharacteristicSummary {
    const fn from_characteristic(characteristic: &Characteristic) -> Self {
        Self {
            uuid: characteristic.uuid,
            service_uuid: characteristic.service_uuid,
            properties: characteristic.properties,
        }
    }
}

/// Summary of a successful connection/discovery pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionSummary {
    /// Selected peripheral observation.
    pub observation: PeripheralObservation,

    /// Discovered GATT services and characteristics.
    pub services: Vec<ServiceSummary>,
}

impl fmt::Display for ConnectionSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "connected {}", self.observation)?;
        for service in &self.services {
            writeln!(
                f,
                "service {} primary={} characteristics=[{}]",
                service.uuid,
                service.primary,
                service
                    .characteristics
                    .iter()
                    .map(|characteristic| {
                        format!(
                            "{} props={:?}",
                            characteristic.uuid, characteristic.properties
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
        }
        Ok(())
    }
}

impl ConnectionSummary {
    /// Selects the standard BLE Battery Level characteristic when present.
    #[must_use]
    pub fn battery_level_characteristic(&self) -> Option<&CharacteristicSummary> {
        self.services
            .iter()
            .find(|service| service.uuid == BATTERY_SERVICE_UUID)
            .and_then(|service| {
                service.characteristics.iter().find(|characteristic| {
                    characteristic.uuid == BATTERY_LEVEL_UUID && characteristic.can_read()
                })
            })
    }

    /// Returns characteristics that can accept writes.
    #[must_use]
    pub fn write_candidates(&self) -> Vec<&CharacteristicSummary> {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .filter(|characteristic| characteristic.can_write())
            .collect()
    }

    /// Returns characteristics that can notify or indicate.
    #[must_use]
    pub fn notify_candidates(&self) -> Vec<&CharacteristicSummary> {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .filter(|characteristic| characteristic.can_notify())
            .collect()
    }

    /// Selects session endpoints from the discovered tree.
    #[must_use]
    pub fn select_session_endpoints(&self) -> Option<SessionEndpoints<'_>> {
        let write = self.write_candidates().into_iter().next()?;
        let notify = self
            .services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .find(|characteristic| {
                characteristic.service_uuid == write.service_uuid && characteristic.can_notify()
            })
            .or_else(|| {
                self.services
                    .iter()
                    .flat_map(|service| service.characteristics.iter())
                    .find(|characteristic| characteristic.can_notify())
            });

        Some(SessionEndpoints { write, notify })
    }
}

/// A connected peripheral paired with its discovered GATT tree.
#[derive(Clone, Debug)]
pub struct ConnectedPeripheral {
    /// Connected peripheral handle that remains live for the bridge.
    pub peripheral: btleplug::platform::Peripheral,

    /// Discovered services and characteristics for the connected peripheral.
    pub summary: ConnectionSummary,
}

/// Selected endpoints for a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionEndpoints<'a> {
    /// Writable characteristic selected for request writes.
    pub write: &'a CharacteristicSummary,

    /// Notification-capable characteristic, if one was selected.
    pub notify: Option<&'a CharacteristicSummary>,
}

/// Report produced by a protocol bridge run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionBridgeReport {
    /// Transport writes executed through the bridge.
    pub writes: usize,

    /// Transport subscribe operations executed through the bridge.
    pub subscribes: usize,

    /// Notification payloads relayed into the session.
    pub notifications: usize,

    /// Semantic telemetry events emitted by the session.
    pub telemetry: usize,

    /// Latest semantic telemetry values emitted by the session.
    pub telemetry_snapshot: TelemetrySnapshot,

    /// Semantic read-only response events emitted by the session.
    pub read_only_responses: usize,

    /// Latest firmware readback emitted by the session.
    pub firmware: Option<FirmwareInfo>,

    /// Settings readbacks emitted by the session.
    pub settings: Vec<SettingsReadback>,

    /// Parser diagnostics events emitted by the session.
    pub diagnostics: usize,

    /// Transport disconnect operations executed through the bridge.
    pub disconnects: usize,
}

/// Captured bridge records suitable for live BTLE evidence files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionCaptureRecord {
    /// Link-up metadata observed before protocol outputs were processed.
    Link {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Negotiated maximum write length, when known.
        max_write_len: Option<u16>,
    },

    /// Notification subscription issued by the session.
    Subscribe {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Concrete characteristic subscribed through BTLE.
        characteristic: Uuid,
    },

    /// Outbound write issued by the session.
    Write {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Concrete characteristic written through BTLE.
        characteristic: Uuid,

        /// BTLE write mode used by the bridge.
        mode: WriteType,

        /// Exact bytes sent to the characteristic.
        bytes: Vec<u8>,

        /// Whether the bytes come from provisional protocol encoders.
        provisional: bool,
    },

    /// Inbound notification observed from the device.
    Notification {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Characteristic that emitted the notification.
        characteristic: Uuid,

        /// Service associated with the notification.
        service: Uuid,

        /// Exact bytes received from the device.
        bytes: Vec<u8>,
    },
}

impl fmt::Display for SessionCaptureRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Link {
                monotonic_ms,
                max_write_len,
            } => write!(
                f,
                "link t_ms={monotonic_ms} max_write_len={}",
                max_write_len.map_or_else(|| "<unknown>".to_owned(), |value| value.to_string())
            ),
            Self::Subscribe {
                monotonic_ms,
                characteristic,
            } => write!(
                f,
                "subscribe t_ms={monotonic_ms} characteristic={characteristic}"
            ),
            Self::Write {
                monotonic_ms,
                characteristic,
                mode,
                bytes,
                provisional,
            } => write!(
                f,
                "write t_ms={monotonic_ms} characteristic={characteristic} mode={} bytes={} provisional={provisional}",
                format_write_type(*mode),
                encode_hex(bytes)
            ),
            Self::Notification {
                monotonic_ms,
                characteristic,
                service,
                bytes,
            } => write!(
                f,
                "notification t_ms={monotonic_ms} characteristic={characteristic} service={service} bytes={}",
                encode_hex(bytes)
            ),
        }
    }
}

/// Captured records plus bridge counters from a session run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionCapture {
    /// Records emitted during the bridge run.
    pub records: Vec<SessionCaptureRecord>,

    /// Aggregate bridge counters.
    pub report: SessionBridgeReport,
}

/// Errors surfaced while bridging protocol outputs to BTLE operations.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum SessionBridgeError {
    /// Session output referenced a channel that does not match the selected binding.
    #[error("bridge saw channel {observed:?} but expected {expected:?}")]
    UnexpectedChannel {
        /// Expected binding channel.
        expected: GattChannel,

        /// Observed channel from the session output.
        observed: GattChannel,
    },

    /// The session requested notification subscription but no notify endpoint was selected.
    #[error("bridge needs a notify-capable endpoint for channel {channel:?}")]
    MissingNotifyEndpoint {
        /// Abstract protocol channel requested for subscription.
        channel: GattChannel,
    },
}

/// Minimal BTLE operations required by the protocol bridge.
#[async_trait]
pub trait SessionPeripheral: Send + Sync {
    /// Returns the negotiated MTU for the connected peripheral.
    fn mtu(&self) -> u16;

    /// Subscribes to notifications on the selected endpoint.
    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), BtleError>;

    /// Writes a payload to the selected endpoint.
    async fn write(
        &self,
        characteristic: &Characteristic,
        bytes: &[u8],
        mode: WriteType,
    ) -> Result<(), BtleError>;

    /// Returns the notification stream for the connected peripheral.
    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>, BtleError>;

    /// Disconnects the peripheral.
    async fn disconnect(&self) -> Result<(), BtleError>;
}

fn join_uuids(values: &[Uuid]) -> String {
    values
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalize_address(address: String) -> Option<String> {
    if address == "00:00:00:00:00:00" {
        None
    } else {
        Some(address)
    }
}

/// Errors surfaced by the BTLE adapter.
#[derive(Debug, Error)]
pub enum BtleError {
    /// No Bluetooth adapters were reported by the platform.
    #[error("no bluetooth adapters were found")]
    NoAdapterAvailable,

    /// No peripheral matched the requested target.
    #[error("no peripheral matched the requested target")]
    NoPeripheralMatched,

    /// Error reported by the underlying BTLE stack.
    #[error(transparent)]
    Backend(#[from] btleplug::Error),

    /// Error reported by the session bridge.
    #[error(transparent)]
    Bridge(#[from] SessionBridgeError),
}

/// Scans for peripherals and returns what was observed.
///
/// # Errors
///
/// Returns [`BtleError::NoAdapterAvailable`] when the platform exposes no
/// adapters, or [`BtleError::Backend`] when the BTLE backend reports a failure.
pub async fn scan_peripherals(scan_for: Duration) -> Result<Vec<PeripheralObservation>, BtleError> {
    let adapter = first_adapter().await?;
    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(scan_for).await;
    let observations = collect_observations(&adapter).await?;
    let _ = adapter.stop_scan().await;
    Ok(observations)
}

/// Connects to the first peripheral matching the target and returns a summary.
///
/// # Errors
///
/// Returns [`BtleError::NoAdapterAvailable`] if no adapter is present,
/// [`BtleError::NoPeripheralMatched`] if scan results do not satisfy the
/// target, or [`BtleError::Backend`] if the BTLE backend fails.
pub async fn connect_and_discover(
    target: &ConnectionTarget,
    scan_for: Duration,
) -> Result<ConnectedPeripheral, BtleError> {
    let adapter = first_adapter().await?;
    adapter.start_scan(ScanFilter::default()).await?;

    let peripheral = wait_for_scan_match(scan_for, TARGETED_SCAN_POLL_INTERVAL, || {
        find_peripheral(&adapter, target)
    })
    .await;
    let _ = adapter.stop_scan().await;
    let peripheral = peripheral?;

    peripheral.connect().await?;
    peripheral.discover_services().await?;

    let observation = observation_from_peripheral(&peripheral).await?;
    let services = peripheral
        .services()
        .into_iter()
        .map(|service| ServiceSummary::from_service(&service))
        .collect();

    Ok(ConnectedPeripheral {
        peripheral,
        summary: ConnectionSummary {
            observation,
            services,
        },
    })
}

/// Reads the standard BLE Battery Level characteristic from a connected peripheral.
///
/// Returns `Ok(None)` when the device does not expose a readable Battery Level
/// characteristic or when the characteristic returns an empty payload.
///
/// # Errors
///
/// Returns [`BtleError::Backend`] if the underlying BTLE stack fails the read.
pub async fn read_battery_level(
    peripheral: &btleplug::platform::Peripheral,
    summary: &ConnectionSummary,
) -> Result<Option<u8>, BtleError> {
    let Some(characteristic) = summary.battery_level_characteristic() else {
        return Ok(None);
    };

    let value = peripheral
        .read(&characteristic_from_summary(characteristic))
        .await?;
    Ok(value.first().copied().map(|percent| percent.min(100)))
}

/// Drives a protocol session against the selected BTLE endpoints.
///
/// # Errors
///
/// Returns [`BtleError::Bridge`] when a session output references a channel
/// that does not match the selected binding, or when the session asks for
/// subscription but no notify-capable endpoint was selected.
pub async fn drive_session<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    drive_session_inner(
        peripheral,
        session,
        channel,
        endpoints,
        notification_window,
        None,
        false,
    )
    .await
}

/// Captures a protocol session against the selected BTLE endpoints.
///
/// # Errors
///
/// Returns [`BtleError::Bridge`] when a session output references a channel
/// that does not match the selected binding, or when the session asks for
/// subscription but no notify-capable endpoint was selected.
pub async fn capture_session<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
    provisional_writes: bool,
) -> Result<SessionCapture, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let mut records = Vec::new();
    let report = drive_session_inner(
        peripheral,
        session,
        channel,
        endpoints,
        notification_window,
        Some(&mut records),
        provisional_writes,
    )
    .await?;
    Ok(SessionCapture { records, report })
}

async fn drive_session_inner<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
    mut capture: Option<&mut Vec<SessionCaptureRecord>>,
    provisional_writes: bool,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let mut report = SessionBridgeReport::default();
    let write_characteristic = characteristic_from_summary(endpoints.write);
    let notify_characteristic = endpoints.notify.map(characteristic_from_summary);
    let mut outputs = Vec::new();
    let mut monotonic_ms = 0;
    let max_write_len = Some(peripheral.mtu());

    session.handle(
        SessionInput::LinkUp(LinkInfo {
            monotonic_ms,
            max_write_len,
        }),
        &mut outputs,
    );
    if let Some(records) = capture.as_deref_mut() {
        records.push(SessionCaptureRecord::Link {
            monotonic_ms,
            max_write_len,
        });
    }
    process_session_outputs(
        SessionOutputContext {
            peripheral,
            channel,
            write_characteristic: &write_characteristic,
            notify_characteristic: notify_characteristic.as_ref(),
            report: &mut report,
            capture: capture.as_deref_mut(),
            provisional_writes,
        },
        &mut outputs,
        monotonic_ms,
    )
    .await?;

    monotonic_ms += 1;
    session.handle(SessionInput::Tick { monotonic_ms }, &mut outputs);
    process_session_outputs(
        SessionOutputContext {
            peripheral,
            channel,
            write_characteristic: &write_characteristic,
            notify_characteristic: notify_characteristic.as_ref(),
            report: &mut report,
            capture: capture.as_deref_mut(),
            provisional_writes,
        },
        &mut outputs,
        monotonic_ms,
    )
    .await?;

    if notification_window.is_zero() || notify_characteristic.is_none() {
        return Ok(report);
    }

    let mut notifications = peripheral.notifications().await?;
    let deadline = tokio::time::Instant::now() + notification_window;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, notifications.next()).await {
            Ok(Some(notification)) => {
                monotonic_ms += 1;
                if let Some(records) = capture.as_deref_mut() {
                    records.push(SessionCaptureRecord::Notification {
                        monotonic_ms,
                        characteristic: notification.uuid,
                        service: notification.service_uuid,
                        bytes: notification.value.clone(),
                    });
                }
                session.handle(
                    SessionInput::Notification {
                        channel: gatt_channel_from_uuid(notification.uuid),
                        bytes: &notification.value,
                        monotonic_ms,
                    },
                    &mut outputs,
                );
                process_session_outputs(
                    SessionOutputContext {
                        peripheral,
                        channel,
                        write_characteristic: &write_characteristic,
                        notify_characteristic: notify_characteristic.as_ref(),
                        report: &mut report,
                        capture: capture.as_deref_mut(),
                        provisional_writes,
                    },
                    &mut outputs,
                    monotonic_ms,
                )
                .await?;
                report.notifications += 1;
            }
            Ok(None) | Err(_) => break,
        }
    }

    Ok(report)
}

const fn characteristic_from_summary(summary: &CharacteristicSummary) -> Characteristic {
    Characteristic {
        uuid: summary.uuid,
        service_uuid: summary.service_uuid,
        properties: summary.properties,
        descriptors: BTreeSet::new(),
    }
}

const fn gatt_channel_from_uuid(uuid: Uuid) -> GattChannel {
    GattChannel::from_bytes(*uuid.as_bytes())
}

struct SessionOutputContext<'a, P: ?Sized> {
    peripheral: &'a P,
    channel: GattChannel,
    write_characteristic: &'a Characteristic,
    notify_characteristic: Option<&'a Characteristic>,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    provisional_writes: bool,
}

async fn process_session_outputs<P>(
    mut context: SessionOutputContext<'_, P>,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: u64,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
{
    for output in outputs.drain(..) {
        match output {
            SessionOutput::Transport(TransportAction::Subscribe { channel: observed }) => {
                if observed != context.channel {
                    return Err(SessionBridgeError::UnexpectedChannel {
                        expected: context.channel,
                        observed,
                    }
                    .into());
                }
                let Some(notify_characteristic) = context.notify_characteristic else {
                    return Err(SessionBridgeError::MissingNotifyEndpoint {
                        channel: context.channel,
                    }
                    .into());
                };
                context.peripheral.subscribe(notify_characteristic).await?;
                if let Some(records) = context.capture.as_deref_mut() {
                    records.push(SessionCaptureRecord::Subscribe {
                        monotonic_ms,
                        characteristic: notify_characteristic.uuid,
                    });
                }
                context.report.subscribes += 1;
            }
            SessionOutput::Transport(TransportAction::Write {
                channel: observed,
                bytes,
                mode,
            }) => {
                if observed != context.channel {
                    return Err(SessionBridgeError::UnexpectedChannel {
                        expected: context.channel,
                        observed,
                    }
                    .into());
                }
                let write_type = match mode {
                    WriteMode::WithResponse => WriteType::WithResponse,
                    WriteMode::WithoutResponse => WriteType::WithoutResponse,
                };
                context
                    .peripheral
                    .write(context.write_characteristic, bytes.as_slice(), write_type)
                    .await?;
                if let Some(records) = context.capture.as_deref_mut() {
                    records.push(SessionCaptureRecord::Write {
                        monotonic_ms,
                        characteristic: context.write_characteristic.uuid,
                        mode: write_type,
                        bytes: bytes.as_slice().to_vec(),
                        provisional: context.provisional_writes,
                    });
                }
                context.report.writes += 1;
            }
            SessionOutput::Transport(TransportAction::Disconnect) => {
                context.peripheral.disconnect().await?;
                context.report.disconnects += 1;
            }
            SessionOutput::Event(
                DeviceEvent::NotificationReceived { .. }
                | DeviceEvent::LinkUp(_)
                | DeviceEvent::LinkDown
                | DeviceEvent::Tick { .. },
            ) => {}
            SessionOutput::Event(DeviceEvent::Telemetry(delta)) => {
                context.report.telemetry += 1;
                context.report.telemetry_snapshot.apply_delta(delta);
            }
            SessionOutput::Event(DeviceEvent::ReadOnlyResponse(response)) => {
                context.report.read_only_responses += 1;
                match response {
                    ReadOnlyResponse::Firmware(firmware) => {
                        context.report.firmware = Some(firmware);
                    }
                    ReadOnlyResponse::Settings(settings) => {
                        context.report.settings.push(settings);
                    }
                    ReadOnlyResponse::Battery(_) | ReadOnlyResponse::Diagnostics(_) => {}
                }
            }
            SessionOutput::Event(DeviceEvent::Diagnostics(_)) => {
                context.report.diagnostics += 1;
            }
        }
    }
    Ok(())
}

fn format_write_type(mode: WriteType) -> &'static str {
    match mode {
        WriteType::WithResponse => "with-response",
        WriteType::WithoutResponse => "without-response",
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

#[async_trait]
impl SessionPeripheral for btleplug::platform::Peripheral {
    fn mtu(&self) -> u16 {
        btleplug::api::Peripheral::mtu(self)
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), BtleError> {
        btleplug::api::Peripheral::subscribe(self, characteristic)
            .await
            .map_err(BtleError::from)
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        bytes: &[u8],
        mode: WriteType,
    ) -> Result<(), BtleError> {
        btleplug::api::Peripheral::write(self, characteristic, bytes, mode)
            .await
            .map_err(BtleError::from)
    }

    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>, BtleError> {
        btleplug::api::Peripheral::notifications(self)
            .await
            .map_err(BtleError::from)
    }

    async fn disconnect(&self) -> Result<(), BtleError> {
        btleplug::api::Peripheral::disconnect(self)
            .await
            .map_err(BtleError::from)
    }
}

async fn first_adapter() -> Result<Adapter, BtleError> {
    let manager = Manager::new().await?;
    let mut adapters = manager.adapters().await?;
    adapters.pop().ok_or(BtleError::NoAdapterAvailable)
}

async fn collect_observations(adapter: &Adapter) -> Result<Vec<PeripheralObservation>, BtleError> {
    let mut observations = Vec::new();
    for peripheral in adapter.peripherals().await? {
        if let Some(properties) = peripheral.properties().await? {
            observations.push(PeripheralObservation::from_peripheral(
                &peripheral,
                &properties,
            ));
        }
    }
    Ok(observations)
}

async fn observation_from_peripheral(
    peripheral: &btleplug::platform::Peripheral,
) -> Result<PeripheralObservation, BtleError> {
    let Some(properties) = peripheral.properties().await? else {
        return Ok(PeripheralObservation {
            identifier: peripheral.id().to_string(),
            address: None,
            name: None,
            rssi: None,
            advertised_services: Vec::new(),
        });
    };
    Ok(PeripheralObservation::from_peripheral(
        peripheral,
        &properties,
    ))
}

async fn find_peripheral(
    adapter: &Adapter,
    target: &ConnectionTarget,
) -> Result<btleplug::platform::Peripheral, BtleError> {
    for peripheral in adapter.peripherals().await? {
        let Some(properties) = peripheral.properties().await? else {
            continue;
        };
        let observation = PeripheralObservation::from_peripheral(&peripheral, &properties);
        if target.matches(&observation) {
            return Ok(peripheral);
        }
    }
    Err(BtleError::NoPeripheralMatched)
}

async fn wait_for_scan_match<T, F, Fut>(
    scan_for: Duration,
    poll_interval: Duration,
    mut find: F,
) -> Result<T, BtleError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, BtleError>>,
{
    let started = tokio::time::Instant::now();
    let deadline = started + scan_for;

    loop {
        match find().await {
            Ok(value) => return Ok(value),
            Err(BtleError::NoPeripheralMatched) if tokio::time::Instant::now() < deadline => {
                let next_poll = (tokio::time::Instant::now() + poll_interval).min(deadline);
                tokio::time::sleep_until(next_poll).await;
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        pin::Pin,
        sync::{Arc, Mutex},
        time::{Duration, Instant as StdInstant},
    };

    use btleplug::api::{CharPropFlags, Characteristic, ValueNotification, WriteType};
    use cutout_core::{
        DeviceEvent, FirmwareInfo, GattChannel, Measured, ParserDiagnostics, ProtocolSession,
        RawFieldValue, ReadOnlyResponse, SessionInput, SessionOutput, SettingsEntry,
        SettingsReadback, TelemetryDelta, TransportAction, ValueQuality, ValueSource,
        VerificationStatus, WriteMode,
    };
    use futures_util::stream;
    use uuid::Uuid;

    use super::crate_name;

    type WriteRecord = (Uuid, Vec<u8>, WriteType);
    type WriteLog = Arc<Mutex<Vec<WriteRecord>>>;
    type NotificationLog = Arc<Mutex<Vec<ValueNotification>>>;

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-btle");
    }

    #[test]
    fn connection_target_matches_on_address_and_name() {
        let target = crate::ConnectionTarget {
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            identifier: None,
            name_contains: Some("Aero".to_owned()),
        };
        let observation = crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: vec![],
        };

        assert!(target.matches(&observation));
    }

    #[tokio::test]
    async fn targeted_scan_wait_returns_as_soon_as_match_is_found() {
        let started = StdInstant::now();
        let mut attempts = 0_u8;

        let result = crate::wait_for_scan_match(
            Duration::from_millis(200),
            Duration::from_millis(5),
            || {
                attempts += 1;
                async move {
                    if attempts == 1 {
                        Err(crate::BtleError::NoPeripheralMatched)
                    } else {
                        Ok("matched")
                    }
                }
            },
        )
        .await
        .expect("second poll matches");

        assert_eq!(result, "matched");
        assert_eq!(attempts, 2);
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "targeted scan should not wait for the full scan period after a match"
        );
    }

    #[tokio::test]
    async fn targeted_scan_wait_times_out_without_match() {
        let started = StdInstant::now();
        let mut attempts = 0_u8;

        let result = crate::wait_for_scan_match::<(), _, _>(
            Duration::from_millis(15),
            Duration::from_millis(5),
            || {
                attempts += 1;
                async { Err(crate::BtleError::NoPeripheralMatched) }
            },
        )
        .await;

        assert!(matches!(result, Err(crate::BtleError::NoPeripheralMatched)));
        assert!(attempts >= 2);
        assert!(started.elapsed() >= Duration::from_millis(15));
    }

    #[tokio::test]
    async fn targeted_scan_wait_returns_non_match_errors_immediately() {
        let started = StdInstant::now();
        let mut attempts = 0_u8;

        let result = crate::wait_for_scan_match::<(), _, _>(
            Duration::from_millis(200),
            Duration::from_millis(5),
            || {
                attempts += 1;
                async {
                    Err(crate::BtleError::Bridge(
                        crate::SessionBridgeError::MissingNotifyEndpoint {
                            channel: GattChannel::from_bytes([0x44; 16]),
                        },
                    ))
                }
            },
        )
        .await;

        assert!(matches!(result, Err(crate::BtleError::Bridge(_))));
        assert_eq!(attempts, 1);
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn connection_target_matches_on_platform_identifier() {
        let target = crate::ConnectionTarget {
            address: None,
            identifier: Some("cb-uuid-1234".to_owned()),
            name_contains: None,
        };
        let observation = crate::PeripheralObservation {
            identifier: "cb-uuid-1234".to_owned(),
            address: None,
            name: Some("NF2557".to_owned()),
            rssi: Some(-42),
            advertised_services: vec![],
        };

        assert!(target.matches(&observation));
    }

    #[test]
    fn connection_summary_renders_services_and_characteristics() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                }],
            }],
        };

        assert!(summary.to_string().contains("AA:BB:CC:DD:EE:FF"));
        assert!(summary.to_string().contains("NOSFET Aero"));
        assert!(summary.to_string().contains("ffe0"));
        assert!(summary.to_string().contains("ffe1"));
    }

    #[test]
    fn connection_summary_selects_standard_battery_level_characteristic() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::READ,
                }],
            }],
        };

        assert_eq!(
            summary
                .battery_level_characteristic()
                .map(|characteristic| characteristic.uuid),
            Some(Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb))
        );
    }

    #[test]
    fn connection_summary_uses_identifier_when_address_is_unavailable() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "cb-uuid-1234".to_owned(),
                address: None,
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            services: vec![],
        };

        assert!(summary.to_string().contains("id=cb-uuid-1234"));
        assert!(summary.to_string().contains("name=NOSFET Aero"));
    }

    #[test]
    fn capture_record_formats_write_bytes_with_provenance() {
        let record = crate::SessionCaptureRecord::Write {
            monotonic_ms: 7,
            characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            mode: WriteType::WithoutResponse,
            bytes: vec![0x01, 0x23, 0xab, 0xcd],
            provisional: true,
        };

        assert_eq!(
            record.to_string(),
            "write t_ms=7 characteristic=0000ffe1-0000-1000-8000-00805f9b34fb mode=without-response bytes=0123abcd provisional=true"
        );
    }

    #[test]
    fn capture_record_formats_notification_bytes_with_service() {
        let record = crate::SessionCaptureRecord::Notification {
            monotonic_ms: 11,
            characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            bytes: vec![0xde, 0xad, 0xbe, 0xef],
        };

        assert_eq!(
            record.to_string(),
            "notification t_ms=11 characteristic=0000ffe1-0000-1000-8000-00805f9b34fb service=0000ffe0-0000-1000-8000-00805f9b34fb bytes=deadbeef"
        );
    }

    #[test]
    fn connection_summary_finds_write_and_notify_candidates() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::READ,
                    },
                ],
            }],
        };

        assert_eq!(summary.write_candidates().len(), 1);
        assert_eq!(summary.notify_candidates().len(), 1);
    }

    #[test]
    fn connection_summary_selects_session_endpoints() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::INDICATE,
                    },
                ],
            }],
        };

        let endpoints = summary
            .select_session_endpoints()
            .expect("summary has a writable characteristic");
        assert_eq!(
            endpoints.write.uuid,
            Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb)
        );
        assert_eq!(
            endpoints
                .notify
                .expect("summary has a notify-capable characteristic")
                .uuid,
            Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb)
        );
    }

    #[tokio::test]
    async fn drive_session_subscribes_and_writes_matching_transport_channels() {
        let peripheral = RecordingPeripheral::default();
        let mut session = BridgeSession::default();
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        };

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
        )
        .await
        .expect("bridge accepts matching transport outputs");

        assert_eq!(report.writes, 1);
        assert_eq!(report.subscribes, 1);
        assert_eq!(report.notifications, 0);
        assert_eq!(
            peripheral
                .subscribes
                .lock()
                .expect("subscribe log")
                .as_slice(),
            &[Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb)]
        );
        assert_eq!(
            peripheral.writes.lock().expect("write log").as_slice(),
            &[(
                Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                b"bridge:write".to_vec(),
                WriteType::WithResponse,
            )]
        );
    }

    #[tokio::test]
    async fn drive_session_relays_notifications_back_into_session() {
        let peripheral = RecordingPeripheral::with_notification(ValueNotification {
            uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
            service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            value: vec![0x13, 0x37],
        });
        let mut session = BridgeSession::default();
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        };

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
        )
        .await
        .expect("bridge consumes notifications");

        assert_eq!(report.notifications, 1);
        assert_eq!(report.telemetry, 1);
        assert_eq!(report.read_only_responses, 2);
        assert_eq!(
            report.telemetry_snapshot.speed_mm_s,
            Some(Measured::reported(1_200))
        );
        assert_eq!(
            report.telemetry_snapshot.voltage_mv,
            Some(Measured::reported(84_200))
        );
        assert_eq!(
            report.telemetry_snapshot.battery_percent_estimated,
            Some(Measured::estimated(61))
        );
        assert_eq!(
            report.firmware.expect("firmware response").firmware_major,
            Some(Measured::reported(43))
        );
        assert_eq!(
            report.settings.first().expect("settings response").entries[0]
                .expect("settings entry")
                .field,
            RawFieldValue::new(0x0014, 30)
        );
        assert_eq!(report.diagnostics, 1);
        assert_eq!(*session.notification_count.lock().expect("count"), 1);
    }

    #[tokio::test]
    async fn capture_session_records_subscribe_write_and_notification_bytes() {
        let peripheral = RecordingPeripheral::with_notification(ValueNotification {
            uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
            service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            value: vec![0x13, 0x37],
        });
        let mut session = BridgeSession::default();
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        };

        let capture = crate::capture_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
            true,
        )
        .await
        .expect("capture consumes bridge outputs");

        assert_eq!(capture.report.notifications, 1);
        assert_eq!(
            capture.records,
            vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(185),
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 0,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 1,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteType::WithResponse,
                    bytes: b"bridge:write".to_vec(),
                    provisional: true,
                },
                crate::SessionCaptureRecord::Notification {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                    service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    bytes: vec![0x13, 0x37],
                },
            ]
        );
    }

    #[derive(Default)]
    struct BridgeSession {
        notification_count: Arc<Mutex<usize>>,
    }

    impl ProtocolSession for BridgeSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(_) => {
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                    }));
                }
                SessionInput::Tick { .. } => {
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                        bytes: cutout_core::WritePayload::try_from_slice(b"bridge:write")
                            .expect("fixture payload fits"),
                        mode: WriteMode::WithResponse,
                    }));
                }
                SessionInput::Notification { .. } => {
                    *self
                        .notification_count
                        .lock()
                        .expect("notification counter") += 1;
                    output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                        monotonic_ms: 0,
                        len: 2,
                    }));
                    output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                        TelemetryDelta {
                            speed_mm_s: Some(Measured::reported(1_200)),
                            voltage_mv: Some(Measured::reported(84_200)),
                            battery_percent_estimated: Some(Measured::estimated(61)),
                            ..TelemetryDelta::empty(0)
                        },
                    )));
                    output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                        ReadOnlyResponse::Firmware(FirmwareInfo {
                            firmware_major: Some(Measured::reported(43)),
                            ..FirmwareInfo::default()
                        }),
                    )));
                    output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                        ReadOnlyResponse::Settings(SettingsReadback {
                            entries: [
                                Some(SettingsEntry {
                                    field: RawFieldValue::new(0x0014, 30),
                                    source: ValueSource::Reported,
                                    quality: ValueQuality::Known,
                                    verification: VerificationStatus::HardwareVerified,
                                }),
                                None,
                                None,
                                None,
                            ],
                        }),
                    )));
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        ParserDiagnostics {
                            malformed_frames: 1,
                            ..ParserDiagnostics::default()
                        },
                    )));
                }
                SessionInput::LinkDown | SessionInput::Command(_) => {}
            }
        }
    }

    #[derive(Clone, Debug)]
    struct RecordingPeripheral {
        subscribes: Arc<Mutex<Vec<Uuid>>>,
        writes: WriteLog,
        notifications: NotificationLog,
    }

    impl Default for RecordingPeripheral {
        fn default() -> Self {
            Self {
                subscribes: Arc::new(Mutex::new(Vec::new())),
                writes: Arc::new(Mutex::new(Vec::new())),
                notifications: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl RecordingPeripheral {
        fn with_notification(notification: ValueNotification) -> Self {
            Self {
                notifications: Arc::new(Mutex::new(vec![notification])),
                ..Self::default()
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::SessionPeripheral for RecordingPeripheral {
        fn mtu(&self) -> u16 {
            185
        }

        async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), crate::BtleError> {
            self.subscribes
                .lock()
                .expect("subscribe log")
                .push(characteristic.uuid);
            Ok(())
        }

        async fn write(
            &self,
            characteristic: &Characteristic,
            bytes: &[u8],
            mode: WriteType,
        ) -> Result<(), crate::BtleError> {
            self.writes.lock().expect("write log").push((
                characteristic.uuid,
                bytes.to_vec(),
                mode,
            ));
            Ok(())
        }

        async fn notifications(
            &self,
        ) -> Result<Pin<Box<dyn stream::Stream<Item = ValueNotification> + Send>>, crate::BtleError>
        {
            let notifications = self.notifications.lock().expect("notification log").clone();
            Ok(Box::pin(stream::iter(notifications)))
        }

        async fn disconnect(&self) -> Result<(), crate::BtleError> {
            Ok(())
        }
    }
}
