#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Bluetooth transport adapter scaffolding for Cutout.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    future::Future,
    pin::Pin,
    time::Duration,
};

use async_trait::async_trait;
use btleplug::{
    api::{
        Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _,
        PeripheralProperties, ScanFilter, Service, ValueNotification, WriteType,
    },
    platform::{Adapter, Manager},
};
use cutout_core::{
    DeviceCommand, DeviceEvent, DiagnosticError, FirmwareInfo, GattChannel, GattFingerprint,
    GattRoles, LinkInfo, ParserDiagnostics, PevcapCapture, PevcapHeader, PevcapHeaderError,
    PevcapRecord, ProtocolSession, ReadOnlyResponse, SessionInput, SessionOutput, SettingsReadback,
    TelemetryDelta, TelemetrySnapshot, TransportAction, VerificationStatus, WriteMode,
};
use cutout_protocols::{
    BEGODE_FALCON_REGISTRY_ENTRY, BegodeBanner, IdentityConfidence, IdentityEvidence,
    ProtocolFamilyClassification, ProtocolFamilyClassifier, StagedIdentityInput,
    StagedIdentityResolution, identify_model, parse_begode_ascii_banner,
};
use futures_util::{StreamExt, stream::Stream};
use thiserror::Error;
use tracing::{debug, info};
use uuid::Uuid;

const BATTERY_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb);
const BATTERY_LEVEL_UUID: Uuid = Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb);
const SHARED_FFE0_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
const TARGETED_SCAN_POLL_INTERVAL: Duration = Duration::from_millis(100);
const BACKEND_OPERATION_TIMEOUT: Duration = Duration::from_secs(10);

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

    /// Manufacturer data company ids and payload lengths advertised by the peripheral.
    pub manufacturer_data: Vec<ManufacturerDataSummary>,
}

/// Summary of advertised manufacturer data without retaining opaque payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManufacturerDataSummary {
    /// Bluetooth SIG company identifier.
    pub company_id: u16,

    /// Payload length in bytes.
    pub len: usize,
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
            manufacturer_data: manufacturer_data_summary(&properties.manufacturer_data),
        }
    }

    fn family_hints(&self) -> Vec<&'static str> {
        scan_family_hints(self.name.as_deref(), &self.advertised_services)
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
        write!(f, " services=[{}]", join_uuids(&self.advertised_services))?;
        write!(
            f,
            " manufacturer_data=[{}]",
            join_manufacturer_data(&self.manufacturer_data)
        )?;
        write!(f, " family_hints=[{}]", self.family_hints().join(","))
    }
}

fn manufacturer_data_summary(
    manufacturer_data: &std::collections::HashMap<u16, Vec<u8>>,
) -> Vec<ManufacturerDataSummary> {
    manufacturer_data
        .iter()
        .map(|(&company_id, bytes)| (company_id, bytes.len()))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .map(|(company_id, len)| ManufacturerDataSummary { company_id, len })
        .collect()
}

fn join_manufacturer_data(values: &[ManufacturerDataSummary]) -> String {
    values
        .iter()
        .map(|value| format!("{:04x}:{}b", value.company_id, value.len))
        .collect::<Vec<_>>()
        .join(",")
}

fn scan_family_hints(name: Option<&str>, advertised_services: &[Uuid]) -> Vec<&'static str> {
    let mut hints = Vec::new();
    if advertised_services.contains(&SHARED_FFE0_SERVICE_UUID) {
        hints.push("shared-ffe0-ffe1");
    }
    if name.is_some_and(|name| {
        name.contains("Aero") || name.contains("NOSFET") || name.starts_with("NF")
    }) {
        hints.push("name-nosfet-aero");
    }
    if name.is_some_and(|name| {
        name.contains("Falcon") || name.contains("Begode") || name.contains("Gotway")
    }) {
        hints.push("name-begode-falcon");
    }
    hints
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

    fn gatt_fingerprint(&self) -> GattFingerprint {
        GattFingerprint {
            service: gatt_channel_from_uuid(self.service_uuid),
            characteristic: gatt_channel_from_uuid(self.uuid),
            roles: gatt_roles_from_flags(self.properties),
            verification: VerificationStatus::HardwareVerified,
        }
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
    /// Returns observed GATT fingerprints for PEVCAP and registry evidence.
    #[must_use]
    pub fn gatt_fingerprints(&self) -> Vec<GattFingerprint> {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .map(CharacteristicSummary::gatt_fingerprint)
            .collect()
    }

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

    /// Selects a notify/indicate characteristic by UUID, or the first
    /// notify-capable candidate when no UUID is requested.
    #[must_use]
    pub fn select_notify_characteristic(
        &self,
        requested: Option<Uuid>,
    ) -> Option<&CharacteristicSummary> {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .find(|characteristic| {
                characteristic.can_notify()
                    && requested.is_none_or(|uuid| characteristic.uuid == uuid)
            })
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
    /// Protocol write actions emitted by the session before transport chunking.
    pub protocol_writes: usize,

    /// Transport writes executed through the bridge.
    pub writes: usize,

    /// Transport subscribe operations executed through the bridge.
    pub subscribes: usize,

    /// Notification payloads relayed into the session.
    pub notifications: usize,

    /// Total notification payload bytes relayed into the session.
    pub notification_bytes: usize,

    /// Length of the latest notification payload, if any were observed.
    pub latest_notification_len: Option<usize>,

    /// Semantic telemetry events emitted by the session.
    pub telemetry: usize,

    /// Latest semantic telemetry values emitted by the session.
    pub telemetry_snapshot: TelemetrySnapshot,

    /// Semantic read-only response events emitted by the session.
    pub read_only_responses: usize,

    /// Full read-only response payloads emitted by the session.
    pub read_only_response_events: Vec<ReadOnlyResponse>,

    /// Latest firmware readback emitted by the session.
    pub firmware: Option<FirmwareInfo>,

    /// Settings readbacks emitted by the session.
    pub settings: Vec<SettingsReadback>,

    /// Parser diagnostics events emitted by the session.
    pub diagnostics: usize,

    /// Aggregated parser diagnostic counters emitted by the session.
    pub diagnostics_snapshot: ParserDiagnostics,

    /// Detailed parser diagnostic errors emitted by the session.
    pub diagnostic_errors: Vec<DiagnosticError>,

    /// Staged identity resolution from non-actuating evidence.
    pub identity: Option<BridgeIdentityResolution>,

    /// Timestamped raw and processed telemetry events observed during the run.
    pub events: Vec<SessionBridgeEvent>,

    /// Transport disconnect operations executed through the bridge.
    pub disconnects: usize,
}

/// Staged identity resolution surfaced by a BTLE bridge run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeIdentityResolution {
    /// Resolved manufacturer, when model confidence was reached.
    pub manufacturer: Option<&'static str>,

    /// Resolved model, when model confidence was reached.
    pub model: Option<&'static str>,

    /// Confidence reported by staged identification.
    pub confidence: IdentityConfidence,

    /// Evidence that contributed to the decision.
    pub evidence: IdentityEvidence,
}

/// Timestamped raw or processed telemetry event emitted by the bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionBridgeEvent {
    /// Link-down event emitted by the protocol session after transport disconnect.
    LinkDown {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,
    },

    /// Raw notification payload received from BTLE.
    RawNotification {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Characteristic that emitted the notification.
        characteristic: Uuid,

        /// Service associated with the notification.
        service: Uuid,

        /// Notification payload length.
        len: usize,
    },

    /// Decoded telemetry emitted by the protocol session.
    ProcessedTelemetry {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Telemetry delta emitted by the protocol session.
        delta: TelemetryDelta,
    },

    /// Parser diagnostics emitted by the protocol session.
    Diagnostics {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Parser diagnostic counters emitted at this timestamp.
        diagnostics: ParserDiagnostics,
    },

    /// Detailed parser diagnostic error emitted by the protocol session.
    DiagnosticError {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Detailed parser error emitted at this timestamp.
        error: DiagnosticError,
    },
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

    /// Link-down event observed after the session requested disconnect.
    LinkDown {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,
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
            Self::LinkDown { monotonic_ms } => {
                write!(f, "link_down t_ms={monotonic_ms}")
            }
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

/// Reconnect capture plus per-link connection metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReconnectingSessionCapture {
    /// Aggregate records and report across connected links.
    pub capture: SessionCapture,

    /// Ordered diagnostics for each connected link attempt.
    pub attempts: Vec<ReconnectAttemptReport>,
}

/// Diagnostics captured for one reconnect link attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectAttemptReport {
    /// One-based link attempt number.
    pub attempt: usize,

    /// Connection summary observed for this link attempt.
    pub summary: ConnectionSummary,

    /// Bridge counters and lifecycle events observed during this link attempt.
    pub report: SessionBridgeReport,
}

/// Protocol-agnostic raw notification record captured from a subscribed
/// characteristic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawNotificationRecord {
    /// Monotonic capture time relative to the start of the raw subscription.
    pub monotonic_ms: u64,

    /// Characteristic UUID that emitted the notification.
    pub characteristic: Uuid,

    /// Service UUID associated with the notification.
    pub service: Uuid,

    /// Exact notification bytes.
    pub bytes: Vec<u8>,
}

/// Caller-supplied metadata for converting a live BTLE session capture into
/// PEVCAP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapSessionMetadata<'a> {
    /// Wall-clock capture start time in Unix milliseconds.
    pub wall_clock_start_unix_ms: u64,

    /// Platform identifier recorded by the capture producer.
    pub platform_id: &'a str,

    /// Version of the Cutout library or binary that produced the capture.
    pub library_version: &'a str,

    /// Registry hash used while producing the capture.
    pub registry_hash: [u8; 32],

    /// Resolved identity used while producing the capture, when known.
    pub resolved_identity: Option<cutout_core::PevcapResolvedIdentity>,

    /// Human annotations attached to the capture.
    pub annotations: &'a [&'a str],
}

impl SessionCapture {
    /// Converts this live BTLE bridge capture into a PEVCAP envelope.
    ///
    /// Link and subscribe records are represented in the PEVCAP header or GATT
    /// metadata; outbound writes and inbound notifications become ordered
    /// PEVCAP transport records.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapHeaderError`] if observed metadata exceeds PEVCAP
    /// bounds.
    pub fn to_pevcap(
        &self,
        summary: &ConnectionSummary,
        metadata: PevcapSessionMetadata<'_>,
    ) -> Result<PevcapCapture, PevcapHeaderError> {
        let advertised_services = summary
            .observation
            .advertised_services
            .iter()
            .copied()
            .map(gatt_channel_from_uuid)
            .collect::<Vec<_>>();
        let gatt_fingerprints = summary.gatt_fingerprints();
        let write_limit = self.records.iter().find_map(|record| match record {
            SessionCaptureRecord::Link { max_write_len, .. } => *max_write_len,
            SessionCaptureRecord::LinkDown { .. }
            | SessionCaptureRecord::Subscribe { .. }
            | SessionCaptureRecord::Write { .. }
            | SessionCaptureRecord::Notification { .. } => None,
        });
        let header = PevcapHeader::new(
            metadata.wall_clock_start_unix_ms,
            metadata.platform_id,
            write_limit,
            &advertised_services,
            &gatt_fingerprints,
            metadata.resolved_identity,
            metadata.library_version,
            metadata.registry_hash,
            metadata.annotations,
        )?;
        let records = self
            .records
            .iter()
            .filter_map(session_record_to_pevcap_record)
            .collect();

        Ok(PevcapCapture::new(header, records))
    }
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

    /// The discovered GATT tree did not contain a writable session endpoint.
    #[error("bridge needs a writable session endpoint")]
    MissingSessionEndpoint,
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

/// Host boundary that can create fresh connected peripherals for reconnecting sessions.
#[async_trait]
pub trait ReconnectingSessionHost: Send {
    /// Connected peripheral type returned for each link attempt.
    type Peripheral: SessionPeripheral + Sync;

    /// Connects and discovers the next link attempt.
    async fn connect(&mut self) -> Result<(Self::Peripheral, ConnectionSummary), BtleError>;
}

/// Production reconnect host backed by btleplug target scanning and discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BtleplugReconnectHost {
    target: ConnectionTarget,
    scan_for: Duration,
}

impl BtleplugReconnectHost {
    /// Creates a reconnect host that reuses the same target and scan duration
    /// for every link attempt.
    #[must_use]
    pub const fn new(target: ConnectionTarget, scan_for: Duration) -> Self {
        Self { target, scan_for }
    }

    /// Returns the target reused for each reconnect attempt.
    #[must_use]
    pub const fn target(&self) -> &ConnectionTarget {
        &self.target
    }

    /// Returns the scan duration reused for each reconnect attempt.
    #[must_use]
    pub const fn scan_for(&self) -> Duration {
        self.scan_for
    }
}

#[async_trait]
impl ReconnectingSessionHost for BtleplugReconnectHost {
    type Peripheral = btleplug::platform::Peripheral;

    async fn connect(&mut self) -> Result<(Self::Peripheral, ConnectionSummary), BtleError> {
        let connected = connect_and_discover(&self.target, self.scan_for).await?;
        Ok((connected.peripheral, connected.summary))
    }
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

    /// The underlying BTLE stack did not finish an operation in time.
    #[error("bluetooth operation timed out: {operation} after {after:?}")]
    OperationTimedOut {
        /// Operation that timed out.
        operation: &'static str,

        /// Timeout duration.
        after: Duration,
    },

    /// Error reported by the session bridge.
    #[error(transparent)]
    Bridge(#[from] SessionBridgeError),
}

impl BtleError {
    /// Returns a concise user-facing hint for common desktop BLE failures.
    #[must_use]
    pub const fn diagnostic_hint(&self) -> &'static str {
        match self {
            Self::NoAdapterAvailable => {
                "enable Bluetooth, grant Bluetooth permission to this terminal, and verify the OS exposes an adapter"
            }
            Self::NoPeripheralMatched => {
                "power on the device, keep it nearby, increase --seconds, or use --name-contains/--identifier to narrow selection"
            }
            Self::OperationTimedOut { .. } => {
                "retry the operation, move closer to the device, and check whether another app is holding the BLE connection"
            }
            Self::Bridge(SessionBridgeError::MissingSessionEndpoint) => {
                "inspect GATT services and select a device exposing a writable and notify-capable session characteristic"
            }
            Self::Bridge(SessionBridgeError::MissingNotifyEndpoint { .. }) => {
                "inspect GATT services and select a notify-capable characteristic for the session channel"
            }
            Self::Bridge(SessionBridgeError::UnexpectedChannel { .. }) => {
                "report this protocol binding mismatch with the selected profile, GATT inventory, and capture logs"
            }
            Self::Backend(_) => {
                "check OS Bluetooth permissions, adapter state, and whether another app is already connected"
            }
        }
    }
}

/// Scans for peripherals and returns what was observed.
///
/// # Errors
///
/// Returns [`BtleError::NoAdapterAvailable`] when the platform exposes no
/// adapters, or [`BtleError::Backend`] when the BTLE backend reports a failure.
pub async fn scan_peripherals(scan_for: Duration) -> Result<Vec<PeripheralObservation>, BtleError> {
    let adapter = first_adapter().await?;
    backend_call("start scan", adapter.start_scan(ScanFilter::default())).await?;
    tokio::time::sleep(scan_for).await;
    let observations = collect_observations(&adapter).await?;
    let _ = backend_call("stop scan", adapter.stop_scan()).await;
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
    backend_call("start scan", adapter.start_scan(ScanFilter::default())).await?;

    let peripheral = wait_for_scan_match(scan_for, TARGETED_SCAN_POLL_INTERVAL, || {
        find_peripheral(&adapter, target)
    })
    .await;
    let _ = backend_call("stop scan", adapter.stop_scan()).await;
    let peripheral = peripheral?;

    backend_call("connect peripheral", peripheral.connect()).await?;
    backend_call("discover services", peripheral.discover_services()).await?;

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

    let value = backend_call(
        "read battery level",
        peripheral.read(&characteristic_from_summary(characteristic)),
    )
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
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    drive_session_with_commands(
        peripheral,
        session,
        channel,
        summary,
        endpoints,
        notification_window,
        &[],
    )
    .await
}

/// Drives a protocol session against a connected peripheral and explicit commands.
///
/// Commands are injected after link setup/subscription processing and before
/// the passive notification window, so any resulting writes are captured as
/// ordinary session transport actions.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe, write, or
/// notification streaming fails.
pub async fn drive_session_with_commands<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
    commands: &[DeviceCommand],
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    drive_session_inner(
        peripheral,
        session,
        DriveSessionConfig {
            channel,
            summary,
            endpoints,
            notification_window,
            commands,
            provisional_writes: false,
            monotonic_start: 0,
        },
        None,
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
    summary: &ConnectionSummary,
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
        DriveSessionConfig {
            channel,
            summary,
            endpoints,
            notification_window,
            commands: &[],
            provisional_writes,
            monotonic_start: 0,
        },
        Some(&mut records),
    )
    .await?;

    Ok(SessionCapture { records, report })
}

/// Captures a protocol session while injecting explicit read-only commands.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe, write, or
/// notification streaming fails.
pub async fn capture_session_with_commands<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
    commands: &[DeviceCommand],
) -> Result<SessionCapture, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let mut records = Vec::new();
    let report = drive_session_inner(
        peripheral,
        session,
        DriveSessionConfig {
            channel,
            summary,
            endpoints,
            notification_window,
            commands,
            provisional_writes: false,
            monotonic_start: 0,
        },
        Some(&mut records),
    )
    .await?;
    Ok(SessionCapture { records, report })
}

/// Captures a session across reconnect attempts supplied by a host boundary.
///
/// The host owns platform-specific connect/discover work. This bridge repeats
/// one bounded session run only when the previous link intentionally
/// disconnected.
///
/// # Errors
///
/// Returns the underlying host, transport, or bridge error from any link
/// attempt.
pub async fn capture_reconnecting_session<H, S>(
    host: &mut H,
    session: &mut S,
    channel: GattChannel,
    notification_window: Duration,
    max_links: usize,
    provisional_writes: bool,
) -> Result<SessionCapture, BtleError>
where
    H: ReconnectingSessionHost,
    S: ProtocolSession + Send,
{
    Ok(capture_reconnecting_session_with_commands(
        host,
        session,
        channel,
        notification_window,
        max_links,
        provisional_writes,
        &[],
    )
    .await?
    .capture)
}

/// Captures a reconnecting session and preserves per-link connection summaries.
///
/// # Errors
///
/// Returns the underlying host, transport, or bridge error from any link
/// attempt.
pub async fn capture_reconnecting_session_with_summaries<H, S>(
    host: &mut H,
    session: &mut S,
    channel: GattChannel,
    notification_window: Duration,
    max_links: usize,
    provisional_writes: bool,
) -> Result<ReconnectingSessionCapture, BtleError>
where
    H: ReconnectingSessionHost,
    S: ProtocolSession + Send,
{
    capture_reconnecting_session_with_commands(
        host,
        session,
        channel,
        notification_window,
        max_links,
        provisional_writes,
        &[],
    )
    .await
}

/// Captures a reconnecting session, sending explicit commands on the first link only.
///
/// Commands are intentionally not replayed after reconnect. A link loss cancels
/// any in-flight response expectation for those commands while allowing the
/// read-only session to resume passive subscription on a fresh link.
///
/// # Errors
///
/// Returns the underlying host, transport, or bridge error from any link
/// attempt.
pub async fn capture_reconnecting_session_with_commands<H, S>(
    host: &mut H,
    session: &mut S,
    channel: GattChannel,
    notification_window: Duration,
    max_links: usize,
    provisional_writes: bool,
    commands: &[DeviceCommand],
) -> Result<ReconnectingSessionCapture, BtleError>
where
    H: ReconnectingSessionHost,
    S: ProtocolSession + Send,
{
    let mut reconnecting_capture = ReconnectingSessionCapture::default();
    let mut monotonic_start = 0;

    for attempt in 1..=max_links {
        let (peripheral, summary) = host.connect().await?;
        let endpoints = summary
            .select_session_endpoints()
            .ok_or(SessionBridgeError::MissingSessionEndpoint)?;
        let mut records = Vec::new();
        let report = drive_session_inner(
            &peripheral,
            session,
            DriveSessionConfig {
                channel,
                summary: &summary,
                endpoints,
                notification_window,
                commands: if attempt == 1 { commands } else { &[] },
                provisional_writes,
                monotonic_start,
            },
            Some(&mut records),
        )
        .await?;
        monotonic_start = records
            .iter()
            .map(session_record_monotonic_ms)
            .max()
            .unwrap_or(monotonic_start)
            .saturating_add(1);
        merge_session_report(&mut reconnecting_capture.capture.report, report.clone());
        let should_reconnect = report.disconnects > 0
            && records
                .iter()
                .any(|record| matches!(record, SessionCaptureRecord::LinkDown { .. }));
        reconnecting_capture.attempts.push(ReconnectAttemptReport {
            attempt,
            summary,
            report,
        });
        reconnecting_capture.capture.records.extend(records);
        if !should_reconnect {
            break;
        }
    }

    Ok(reconnecting_capture)
}

/// Subscribes to a notify/indicate characteristic and records raw notification chunks.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe or
/// notification streaming fails.
pub async fn capture_raw_notifications<P>(
    peripheral: &P,
    characteristic: &CharacteristicSummary,
    notification_window: Duration,
) -> Result<Vec<RawNotificationRecord>, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
{
    let characteristic = characteristic_from_summary(characteristic);
    peripheral.subscribe(&characteristic).await?;
    if notification_window.is_zero() {
        return Ok(Vec::new());
    }

    let mut notifications = peripheral.notifications().await?;
    let started_at = tokio::time::Instant::now();
    let deadline = started_at + notification_window;
    let mut records = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, notifications.next()).await {
            Ok(Some(notification)) if notification.uuid == characteristic.uuid => {
                records.push(RawNotificationRecord {
                    monotonic_ms: u64::try_from(started_at.elapsed().as_millis())
                        .unwrap_or(u64::MAX),
                    characteristic: notification.uuid,
                    service: notification.service_uuid,
                    bytes: notification.value,
                });
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }

    Ok(records)
}

struct DriveSessionConfig<'a> {
    channel: GattChannel,
    summary: &'a ConnectionSummary,
    endpoints: SessionEndpoints<'a>,
    notification_window: Duration,
    commands: &'a [DeviceCommand],
    provisional_writes: bool,
    monotonic_start: u64,
}

async fn drive_session_inner<P, S>(
    peripheral: &P,
    session: &mut S,
    config: DriveSessionConfig<'_>,
    mut capture: Option<&mut Vec<SessionCaptureRecord>>,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    info!(
        window_ms = config.notification_window.as_millis(),
        channel = ?config.channel,
        "session bridge drive inner entered"
    );
    let mut report = SessionBridgeReport::default();
    let identity_context = IdentityContext::new(config.summary);
    let mut identity_state = IdentityState::default();
    update_identity_report(&mut report, &identity_context, &identity_state);
    let bindings = BridgeBindings {
        write_characteristic: characteristic_from_summary(config.endpoints.write),
        notify_characteristic: config.endpoints.notify.map(characteristic_from_summary),
    };
    let mut outputs = Vec::new();
    let mut monotonic_ms = config.monotonic_start;

    process_link_up_outputs(
        LinkUpContext {
            peripheral,
            channel: config.channel,
            bindings: &bindings,
            report: &mut report,
            capture: capture.as_deref_mut(),
            provisional_writes: config.provisional_writes,
        },
        session,
        &mut outputs,
        monotonic_ms,
    )
    .await?;

    for command in config.commands {
        monotonic_ms += 1;
        session.handle(SessionInput::Command(*command), &mut outputs);
        process_session_outputs(
            SessionOutputContext {
                peripheral,
                channel: config.channel,
                write_characteristic: &bindings.write_characteristic,
                notify_characteristic: bindings.notify_characteristic.as_ref(),
                report: &mut report,
                capture: capture.as_deref_mut(),
                provisional_writes: config.provisional_writes,
            },
            session,
            &mut outputs,
            monotonic_ms,
        )
        .await?;
    }

    monotonic_ms += 1;
    session.handle(SessionInput::Tick { monotonic_ms }, &mut outputs);
    process_session_outputs(
        SessionOutputContext {
            peripheral,
            channel: config.channel,
            write_characteristic: &bindings.write_characteristic,
            notify_characteristic: bindings.notify_characteristic.as_ref(),
            report: &mut report,
            capture: capture.as_deref_mut(),
            provisional_writes: config.provisional_writes,
        },
        session,
        &mut outputs,
        monotonic_ms,
    )
    .await?;

    if config.notification_window.is_zero() || bindings.notify_characteristic.is_none() {
        return Ok(report);
    }

    process_notification_window(
        NotificationLoopContext {
            peripheral,
            channel: config.channel,
            bindings: &bindings,
            identity_context: &identity_context,
            identity_state: &mut identity_state,
            report: &mut report,
            capture,
            provisional_writes: config.provisional_writes,
        },
        session,
        &mut outputs,
        &mut monotonic_ms,
        config.notification_window,
    )
    .await?;

    Ok(report)
}

struct BridgeBindings {
    write_characteristic: Characteristic,
    notify_characteristic: Option<Characteristic>,
}

struct LinkUpContext<'a, P: ?Sized> {
    peripheral: &'a P,
    channel: GattChannel,
    bindings: &'a BridgeBindings,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    provisional_writes: bool,
}

async fn process_link_up_outputs<P, S>(
    mut context: LinkUpContext<'_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: u64,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let max_write_len = Some(context.peripheral.mtu());

    info!("session bridge link-up handling starting");
    session.handle(
        SessionInput::LinkUp(LinkInfo {
            monotonic_ms,
            max_write_len,
        }),
        outputs,
    );
    info!(
        outputs = outputs.len(),
        "session bridge link-up handling completed"
    );
    if let Some(records) = context.capture.as_deref_mut() {
        records.push(SessionCaptureRecord::Link {
            monotonic_ms,
            max_write_len,
        });
    }
    info!(
        outputs = outputs.len(),
        "session bridge initial output processing starting"
    );
    process_session_outputs(
        SessionOutputContext {
            peripheral: context.peripheral,
            channel: context.channel,
            write_characteristic: &context.bindings.write_characteristic,
            notify_characteristic: context.bindings.notify_characteristic.as_ref(),
            report: context.report,
            capture: context.capture.as_deref_mut(),
            provisional_writes: context.provisional_writes,
        },
        session,
        outputs,
        monotonic_ms,
    )
    .await?;
    info!("session bridge initial output processing completed");

    Ok(())
}

struct NotificationLoopContext<'a, P: ?Sized> {
    peripheral: &'a P,
    channel: GattChannel,
    bindings: &'a BridgeBindings,
    identity_context: &'a IdentityContext<'a>,
    identity_state: &'a mut IdentityState,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    provisional_writes: bool,
}

async fn process_notification_window<P, S>(
    mut context: NotificationLoopContext<'_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: &mut u64,
    notification_window: Duration,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    info!(
        window_ms = notification_window.as_millis(),
        "session notification window starting"
    );
    info!("session notifications stream await starting");
    let mut notifications = context.peripheral.notifications().await?;
    info!("session notifications stream await completed");
    let deadline = tokio::time::Instant::now() + notification_window;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        debug!(
            remaining_ms = remaining.as_millis(),
            "session notification next await starting"
        );
        match tokio::time::timeout(remaining, notifications.next()).await {
            Ok(Some(notification)) => {
                info!(
                    uuid = %notification.uuid,
                    service = %notification.service_uuid,
                    len = notification.value.len(),
                    "session notification next await completed"
                );
                *monotonic_ms += 1;
                if let Some(records) = context.capture.as_deref_mut() {
                    records.push(SessionCaptureRecord::Notification {
                        monotonic_ms: *monotonic_ms,
                        characteristic: notification.uuid,
                        service: notification.service_uuid,
                        bytes: notification.value.clone(),
                    });
                }
                record_raw_notification(context.report, *monotonic_ms, &notification);
                context.identity_state.observe(&notification.value);
                update_identity_report(
                    context.report,
                    context.identity_context,
                    context.identity_state,
                );
                session.handle(
                    SessionInput::Notification {
                        channel: context.channel,
                        bytes: &notification.value,
                        monotonic_ms: *monotonic_ms,
                    },
                    outputs,
                );
                process_session_outputs(
                    SessionOutputContext {
                        peripheral: context.peripheral,
                        channel: context.channel,
                        write_characteristic: &context.bindings.write_characteristic,
                        notify_characteristic: context.bindings.notify_characteristic.as_ref(),
                        report: context.report,
                        capture: context.capture.as_deref_mut(),
                        provisional_writes: context.provisional_writes,
                    },
                    session,
                    outputs,
                    *monotonic_ms,
                )
                .await?;
                context.report.notifications += 1;
                context.report.notification_bytes += notification.value.len();
                context.report.latest_notification_len = Some(notification.value.len());
            }
            Ok(None) => {
                debug!("session notification stream ended");
                break;
            }
            Err(_) => {
                debug!("session notification window elapsed");
                break;
            }
        }
    }
    debug!(
        notifications = context.report.notifications,
        notification_bytes = context.report.notification_bytes,
        latest_notification_len = ?context.report.latest_notification_len,
        "session notification window completed"
    );

    Ok(())
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

fn session_record_to_pevcap_record(record: &SessionCaptureRecord) -> Option<PevcapRecord> {
    match record {
        SessionCaptureRecord::Link {
            monotonic_ms,
            max_write_len,
        } => Some(PevcapRecord::link_up(*monotonic_ms, *max_write_len)),
        SessionCaptureRecord::LinkDown { monotonic_ms } => {
            Some(PevcapRecord::link_down(*monotonic_ms))
        }
        SessionCaptureRecord::Write {
            monotonic_ms,
            characteristic,
            mode,
            bytes,
            provisional: _,
        } => Some(PevcapRecord::outbound_write(
            *monotonic_ms,
            gatt_channel_from_uuid(*characteristic),
            write_mode_from_btle(*mode),
            bytes.clone(),
        )),
        SessionCaptureRecord::Notification {
            monotonic_ms,
            characteristic,
            service,
            bytes,
        } => Some(PevcapRecord::inbound_notification(
            *monotonic_ms,
            gatt_channel_from_uuid(*characteristic),
            gatt_channel_from_uuid(*service),
            bytes.clone(),
        )),
        SessionCaptureRecord::Subscribe { .. } => None,
    }
}

const fn session_record_monotonic_ms(record: &SessionCaptureRecord) -> u64 {
    match record {
        SessionCaptureRecord::Link { monotonic_ms, .. }
        | SessionCaptureRecord::LinkDown { monotonic_ms }
        | SessionCaptureRecord::Subscribe { monotonic_ms, .. }
        | SessionCaptureRecord::Write { monotonic_ms, .. }
        | SessionCaptureRecord::Notification { monotonic_ms, .. } => *monotonic_ms,
    }
}

fn merge_session_report(into: &mut SessionBridgeReport, report: SessionBridgeReport) {
    into.protocol_writes += report.protocol_writes;
    into.writes += report.writes;
    into.subscribes += report.subscribes;
    into.notifications += report.notifications;
    into.notification_bytes += report.notification_bytes;
    if report.latest_notification_len.is_some() {
        into.latest_notification_len = report.latest_notification_len;
    }
    into.telemetry += report.telemetry;
    into.telemetry_snapshot = report.telemetry_snapshot;
    into.read_only_responses += report.read_only_responses;
    into.read_only_response_events
        .extend(report.read_only_response_events);
    if report.firmware.is_some() {
        into.firmware = report.firmware;
    }
    into.settings.extend(report.settings);
    into.diagnostics += report.diagnostics;
    into.diagnostics_snapshot.merge(report.diagnostics_snapshot);
    into.diagnostic_errors.extend(report.diagnostic_errors);
    if report.identity.is_some() {
        into.identity = report.identity;
    }
    into.events.extend(report.events);
    into.disconnects += report.disconnects;
}

const fn write_mode_from_btle(mode: WriteType) -> WriteMode {
    match mode {
        WriteType::WithResponse => WriteMode::WithResponse,
        WriteType::WithoutResponse => WriteMode::WithoutResponse,
    }
}

fn gatt_roles_from_flags(flags: CharPropFlags) -> GattRoles {
    let mut roles = GattRoles::empty();
    if flags.contains(CharPropFlags::READ) {
        roles = roles.with_read();
    }
    if flags.contains(CharPropFlags::WRITE) {
        roles = roles.with_write();
    }
    if flags.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) {
        roles = roles.with_write_without_response();
    }
    if flags.contains(CharPropFlags::NOTIFY) {
        roles = roles.with_notify();
    }
    if flags.contains(CharPropFlags::INDICATE) {
        roles = roles.with_indicate();
    }
    roles
}

struct IdentityContext<'a> {
    advertised_name: Option<&'a str>,
    gatt: Vec<GattFingerprint>,
}

impl<'a> IdentityContext<'a> {
    fn new(summary: &'a ConnectionSummary) -> Self {
        Self {
            advertised_name: summary.observation.name.as_deref(),
            gatt: summary.gatt_fingerprints(),
        }
    }
}

struct IdentityState {
    stream_family: ProtocolFamilyClassification,
    banner_model: Option<String>,
}

impl Default for IdentityState {
    fn default() -> Self {
        Self {
            stream_family: ProtocolFamilyClassification::Pending,
            banner_model: None,
        }
    }
}

impl IdentityState {
    fn observe(&mut self, bytes: &[u8]) {
        if !matches!(
            self.stream_family,
            ProtocolFamilyClassification::Known(
                cutout_protocols::DeviceFamily::BegodeFalcon
                    | cutout_protocols::DeviceFamily::NosfetAero
            )
        ) {
            let classification = ProtocolFamilyClassifier::classify(bytes);
            if classification != ProtocolFamilyClassification::Unknown {
                self.stream_family = classification;
            }
        }

        if let Some(BegodeBanner::ModelName(model)) = parse_begode_ascii_banner(bytes) {
            self.banner_model = Some(model.to_owned());
        }
    }

    fn banner(&self) -> Option<BegodeBanner<'_>> {
        self.banner_model.as_deref().map(BegodeBanner::ModelName)
    }
}

fn update_identity_report(
    report: &mut SessionBridgeReport,
    context: &IdentityContext<'_>,
    state: &IdentityState,
) {
    let resolution = identify_model(
        StagedIdentityInput {
            advertised_name: context.advertised_name,
            gatt: &context.gatt,
            stream_family: state.stream_family,
            banner: state.banner(),
        },
        &[&BEGODE_FALCON_REGISTRY_ENTRY],
    );
    report.identity = bridge_identity_resolution(resolution);
}

fn bridge_identity_resolution(
    resolution: StagedIdentityResolution,
) -> Option<BridgeIdentityResolution> {
    (resolution.confidence != IdentityConfidence::NoMatch).then(|| {
        let model = resolution.model;
        BridgeIdentityResolution {
            manufacturer: model.map(|entry| entry.manufacturer),
            model: model.map(|entry| entry.model),
            confidence: resolution.confidence,
            evidence: resolution.evidence,
        }
    })
}

fn record_raw_notification(
    report: &mut SessionBridgeReport,
    monotonic_ms: u64,
    notification: &ValueNotification,
) {
    report.events.push(SessionBridgeEvent::RawNotification {
        monotonic_ms,
        characteristic: notification.uuid,
        service: notification.service_uuid,
        len: notification.value.len(),
    });
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

async fn process_session_outputs<P, S>(
    mut context: SessionOutputContext<'_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: u64,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let mut pending = outputs.drain(..).collect::<VecDeque<_>>();
    while let Some(output) = pending.pop_front() {
        match output {
            SessionOutput::Transport(TransportAction::Subscribe { channel: observed }) => {
                info!(
                    expected = ?context.channel,
                    observed = ?observed,
                    monotonic_ms,
                    "session bridge processing subscribe output"
                );
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
                info!(
                    characteristic = %notify_characteristic.uuid,
                    service = %notify_characteristic.service_uuid,
                    monotonic_ms,
                    "session subscribe await starting"
                );
                context.peripheral.subscribe(notify_characteristic).await?;
                info!(
                    characteristic = %notify_characteristic.uuid,
                    service = %notify_characteristic.service_uuid,
                    monotonic_ms,
                    "session subscribe await completed"
                );
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
                context.report.protocol_writes += 1;
                let write_limit = usize::from(context.peripheral.mtu()).max(1);
                for chunk in bytes.as_slice().chunks(write_limit) {
                    context
                        .peripheral
                        .write(context.write_characteristic, chunk, write_type)
                        .await?;
                    if let Some(records) = context.capture.as_deref_mut() {
                        records.push(SessionCaptureRecord::Write {
                            monotonic_ms,
                            characteristic: context.write_characteristic.uuid,
                            mode: write_type,
                            bytes: chunk.to_vec(),
                            provisional: context.provisional_writes,
                        });
                    }
                    context.report.writes += 1;
                }
            }
            SessionOutput::Transport(TransportAction::Disconnect) => {
                context.peripheral.disconnect().await?;
                if let Some(records) = context.capture.as_deref_mut() {
                    records.push(SessionCaptureRecord::LinkDown { monotonic_ms });
                }
                context.report.disconnects += 1;
                session.handle(SessionInput::LinkDown, outputs);
                pending.extend(outputs.drain(..));
            }
            SessionOutput::Event(event) => {
                process_device_event(context.report, event, monotonic_ms);
            }
        }
    }
    Ok(())
}

fn process_device_event(report: &mut SessionBridgeReport, event: DeviceEvent, monotonic_ms: u64) {
    match event {
        DeviceEvent::NotificationReceived { .. }
        | DeviceEvent::LinkUp(_)
        | DeviceEvent::Tick { .. }
        | DeviceEvent::ControlRefusal(_) => {}
        DeviceEvent::LinkDown => {
            report
                .events
                .push(SessionBridgeEvent::LinkDown { monotonic_ms });
        }
        DeviceEvent::Telemetry(delta) => {
            report.telemetry += 1;
            report.telemetry_snapshot.apply_delta(delta);
            report.events.push(SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms,
                delta,
            });
        }
        DeviceEvent::ReadOnlyResponse(response) => {
            report.read_only_responses += 1;
            report.read_only_response_events.push(response);
            match response {
                ReadOnlyResponse::Firmware(firmware) => {
                    report.firmware = Some(firmware);
                }
                ReadOnlyResponse::Settings(settings) => {
                    report.settings.push(settings);
                }
                ReadOnlyResponse::Battery(_)
                | ReadOnlyResponse::Diagnostics(_)
                | ReadOnlyResponse::RawTelemetry(_) => {}
            }
        }
        DeviceEvent::Diagnostics(diagnostics) => {
            report.diagnostics += 1;
            report.diagnostics_snapshot.merge(diagnostics);
            report.events.push(SessionBridgeEvent::Diagnostics {
                monotonic_ms,
                diagnostics,
            });
        }
        DeviceEvent::DiagnosticError(error) => {
            report.diagnostic_errors.push(error);
            report.events.push(SessionBridgeEvent::DiagnosticError {
                monotonic_ms,
                error,
            });
        }
    }
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
    let mut adapters = backend_call("list adapters", manager.adapters()).await?;
    adapters.pop().ok_or(BtleError::NoAdapterAvailable)
}

async fn collect_observations(adapter: &Adapter) -> Result<Vec<PeripheralObservation>, BtleError> {
    let mut observations = Vec::new();
    for peripheral in backend_call("list peripherals", adapter.peripherals()).await? {
        if let Some(properties) =
            backend_call("read peripheral properties", peripheral.properties()).await?
        {
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
    let Some(properties) =
        backend_call("read peripheral properties", peripheral.properties()).await?
    else {
        return Ok(PeripheralObservation {
            identifier: peripheral.id().to_string(),
            address: None,
            name: None,
            rssi: None,
            advertised_services: Vec::new(),
            manufacturer_data: Vec::new(),
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
    for peripheral in backend_call("list peripherals", adapter.peripherals()).await? {
        let Some(properties) =
            backend_call("read peripheral properties", peripheral.properties()).await?
        else {
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

async fn backend_call<T, F>(operation: &'static str, future: F) -> Result<T, BtleError>
where
    F: Future<Output = Result<T, btleplug::Error>>,
{
    tokio::time::timeout(BACKEND_OPERATION_TIMEOUT, future)
        .await
        .map_err(|_| BtleError::OperationTimedOut {
            operation,
            after: BACKEND_OPERATION_TIMEOUT,
        })?
        .map_err(BtleError::Backend)
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
        DeviceCommand, DeviceEvent, FirmwareInfo, GattChannel, Measured, ParserDiagnostics,
        PevcapDirection, PevcapResolvedIdentity, ProtocolFamily, ProtocolSession, RawFieldValue,
        ReadOnlyResponse, SessionInput, SessionOutput, SettingsEntry, SettingsReadback,
        TelemetryDelta, TransportAction, ValueQuality, ValueSource, VerificationStatus,
        VerifiedValue, WriteMode,
    };
    use cutout_protocols::IdentityConfidence;
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
            manufacturer_data: Vec::new(),
        };

        assert!(target.matches(&observation));
    }

    #[test]
    fn btleplug_reconnect_host_reuses_target_and_scan_duration() {
        let target = crate::ConnectionTarget {
            address: None,
            identifier: Some("corebluetooth-id".to_owned()),
            name_contains: Some("NF2557".to_owned()),
        };

        let host = crate::BtleplugReconnectHost::new(target.clone(), Duration::from_secs(7));

        assert_eq!(host.target(), &target);
        assert_eq!(host.scan_for(), Duration::from_secs(7));
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
            manufacturer_data: Vec::new(),
        };

        assert!(target.matches(&observation));
    }

    #[test]
    fn peripheral_observation_renders_manufacturer_data_without_payload_bytes() {
        let observation = crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: None,
            name: Some("Generic".to_owned()),
            rssi: Some(-60),
            advertised_services: vec![],
            manufacturer_data: vec![
                crate::ManufacturerDataSummary {
                    company_id: 0x004c,
                    len: 6,
                },
                crate::ManufacturerDataSummary {
                    company_id: 0x000f,
                    len: 2,
                },
            ],
        };

        assert_eq!(
            observation.to_string(),
            "id=peripheral-id name=Generic rssi=-60 services=[] manufacturer_data=[004c:6b,000f:2b] family_hints=[]"
        );
    }

    #[test]
    fn peripheral_observation_renders_family_hints_as_non_final_evidence() {
        let observation = crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: None,
            name: Some("NF2557".to_owned()),
            rssi: None,
            advertised_services: vec![Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb)],
            manufacturer_data: Vec::new(),
        };

        assert_eq!(
            observation.to_string(),
            "id=peripheral-id name=NF2557 services=[0000ffe0-0000-1000-8000-00805f9b34fb] manufacturer_data=[] family_hints=[shared-ffe0-ffe1,name-nosfet-aero]"
        );
    }

    #[test]
    fn operation_timeout_error_names_the_backend_operation() {
        let error = crate::BtleError::OperationTimedOut {
            operation: "start scan",
            after: Duration::from_secs(10),
        };

        assert_eq!(
            error.to_string(),
            "bluetooth operation timed out: start scan after 10s"
        );
    }

    #[test]
    fn btle_errors_expose_actionable_desktop_diagnostic_hints() {
        let no_adapter = crate::BtleError::NoAdapterAvailable;
        assert_eq!(
            no_adapter.diagnostic_hint(),
            "enable Bluetooth, grant Bluetooth permission to this terminal, and verify the OS exposes an adapter"
        );

        let no_match = crate::BtleError::NoPeripheralMatched;
        assert_eq!(
            no_match.diagnostic_hint(),
            "power on the device, keep it nearby, increase --seconds, or use --name-contains/--identifier to narrow selection"
        );

        let timed_out = crate::BtleError::OperationTimedOut {
            operation: "discover services",
            after: Duration::from_secs(5),
        };
        assert_eq!(
            timed_out.diagnostic_hint(),
            "retry the operation, move closer to the device, and check whether another app is holding the BLE connection"
        );

        let missing_endpoint =
            crate::BtleError::Bridge(crate::SessionBridgeError::MissingSessionEndpoint);
        assert_eq!(
            missing_endpoint.diagnostic_hint(),
            "inspect GATT services and select a device exposing a writable and notify-capable session characteristic"
        );
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
                manufacturer_data: Vec::new(),
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
    fn connection_summary_selects_explicit_notify_characteristic() {
        let requested = Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb);
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: None,
                name: Some("Raw device".to_owned()),
                rssi: None,
                advertised_services: vec![],
                manufacturer_data: Vec::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE_WITHOUT_RESPONSE,
                    },
                    crate::CharacteristicSummary {
                        uuid: requested,
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        };

        assert_eq!(
            summary
                .select_notify_characteristic(Some(requested))
                .map(|characteristic| characteristic.uuid),
            Some(requested)
        );
    }

    #[test]
    fn connection_summary_rejects_explicit_non_notify_characteristic() {
        let requested = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: None,
                name: Some("Raw device".to_owned()),
                rssi: None,
                advertised_services: vec![],
                manufacturer_data: Vec::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: requested,
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE_WITHOUT_RESPONSE,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        };

        assert!(
            summary
                .select_notify_characteristic(Some(requested))
                .is_none()
        );
    }

    #[tokio::test]
    async fn raw_notification_capture_subscribes_and_filters_selected_characteristic() {
        let service = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
        let selected = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
        let other = Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb);
        let peripheral = RecordingPeripheral::with_notifications(vec![
            ValueNotification {
                uuid: other,
                service_uuid: service,
                value: vec![0x99],
            },
            ValueNotification {
                uuid: selected,
                service_uuid: service,
                value: vec![0x01, 0x02, 0x03],
            },
        ]);
        let characteristic = crate::CharacteristicSummary {
            uuid: selected,
            service_uuid: service,
            properties: CharPropFlags::NOTIFY,
        };

        let records = crate::capture_raw_notifications(
            &peripheral,
            &characteristic,
            Duration::from_millis(5),
        )
        .await
        .expect("raw notification capture succeeds");

        assert_eq!(
            peripheral
                .subscribes
                .lock()
                .expect("subscribe log")
                .as_slice(),
            &[selected]
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].characteristic, selected);
        assert_eq!(records[0].service, service);
        assert_eq!(records[0].bytes, [0x01, 0x02, 0x03]);
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
                manufacturer_data: Vec::new(),
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
                manufacturer_data: Vec::new(),
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
    fn session_capture_converts_to_pevcap_with_summary_metadata() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "cb-uuid".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: Some(-67),
                advertised_services: vec![Uuid::from_u128(
                    0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
                )],
                manufacturer_data: Vec::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }],
            }],
        };
        let capture = crate::SessionCapture {
            records: vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(23),
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 1,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteType::WithoutResponse,
                    bytes: b"N".to_vec(),
                    provisional: false,
                },
                crate::SessionCaptureRecord::Notification {
                    monotonic_ms: 3,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    bytes: b"NAME=NF2557".to_vec(),
                },
                crate::SessionCaptureRecord::LinkDown { monotonic_ms: 4 },
            ],
            report: crate::SessionBridgeReport::default(),
        };

        let pevcap = capture
            .to_pevcap(
                &summary,
                crate::PevcapSessionMetadata {
                    wall_clock_start_unix_ms: 1_725_000_123_456,
                    platform_id: "darwin",
                    library_version: "0.1.0",
                    registry_hash: [0x42; 32],
                    resolved_identity: Some(PevcapResolvedIdentity {
                        protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
                        model: Some(VerifiedValue {
                            value: "NOSFET Aero".to_owned(),
                            verification: VerificationStatus::Inferred,
                        }),
                        firmware: None,
                    }),
                    annotations: &["live aero"],
                },
            )
            .expect("session capture converts to PEVCAP");

        assert_eq!(pevcap.header.wall_clock_start_unix_ms, 1_725_000_123_456);
        assert_eq!(pevcap.header.platform_id, "darwin");
        assert_eq!(pevcap.header.write_limit, Some(23));
        assert_eq!(pevcap.header.advertised_services.len(), 1);
        assert_eq!(pevcap.header.gatt_fingerprints.len(), 1);
        assert_eq!(
            pevcap
                .header
                .resolved_identity
                .as_ref()
                .and_then(|identity| identity.model.as_ref().map(|model| model.value.as_str())),
            Some("NOSFET Aero")
        );
        assert_eq!(pevcap.records.len(), 4);
        assert_eq!(pevcap.records[0].direction, PevcapDirection::LinkUp);
        assert_eq!(pevcap.records[0].monotonic_ms, 0);
        assert_eq!(pevcap.records[0].link_max_write_len, Some(23));
        assert_eq!(pevcap.records[1].direction, PevcapDirection::Outbound);
        assert_eq!(
            pevcap.records[1].write_mode,
            Some(WriteMode::WithoutResponse)
        );
        assert_eq!(pevcap.records[1].bytes, b"N");
        assert_eq!(pevcap.records[2].direction, PevcapDirection::Inbound);
        assert_eq!(
            pevcap.records[2].service,
            pevcap.header.advertised_services.first().copied()
        );
        assert_eq!(pevcap.records[2].bytes, b"NAME=NF2557");
        assert_eq!(pevcap.records[3].direction, PevcapDirection::LinkDown);
        assert_eq!(pevcap.records[3].monotonic_ms, 4);
    }

    #[test]
    fn session_capture_pevcap_conversion_preserves_write_response_mode() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("Begode_Falcon".to_owned()),
                rssi: None,
                advertised_services: vec![],
                manufacturer_data: Vec::new(),
            },
            services: vec![],
        };
        let capture = crate::SessionCapture {
            records: vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: None,
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 1,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteType::WithResponse,
                    bytes: vec![0x01, 0x02],
                    provisional: true,
                },
            ],
            report: crate::SessionBridgeReport::default(),
        };

        let pevcap = capture
            .to_pevcap(
                &summary,
                crate::PevcapSessionMetadata {
                    wall_clock_start_unix_ms: 1,
                    platform_id: "test",
                    library_version: "0.1.0",
                    registry_hash: [0; 32],
                    resolved_identity: None,
                    annotations: &[],
                },
            )
            .expect("session capture converts to PEVCAP");

        assert_eq!(pevcap.header.write_limit, None);
        assert_eq!(pevcap.records.len(), 2);
        assert_eq!(pevcap.records[0].direction, PevcapDirection::LinkUp);
        assert_eq!(pevcap.records[1].write_mode, Some(WriteMode::WithResponse));
        assert_eq!(pevcap.records[1].bytes, [0x01, 0x02]);
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
                manufacturer_data: Vec::new(),
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
                manufacturer_data: Vec::new(),
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
    async fn drive_session_reports_hints_only_identity_from_name_and_shared_gatt() {
        let peripheral = RecordingPeripheral::default();
        let mut session = SubscribeOnlySession;
        let summary = begode_falcon_summary("Falcon");

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes(
                *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
            ),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
        )
        .await
        .expect("bridge reports staged identity hints");

        let identity = report.identity.expect("identity hints are reported");
        assert_eq!(identity.confidence, IdentityConfidence::HintsOnly);
        assert_eq!(identity.model, None);
        assert_eq!(identity.manufacturer, None);
        assert!(identity.evidence.has_advertised_name_hint());
        assert!(identity.evidence.has_gatt_hint());
        assert_eq!(peripheral.writes.lock().expect("write log").len(), 0);
    }

    #[test]
    fn identity_context_preserves_advertised_name_and_gatt_roles() {
        let summary = begode_falcon_summary("Falcon");

        let context = crate::IdentityContext::new(&summary);

        assert_eq!(context.advertised_name, Some("Falcon"));
        assert_eq!(context.gatt.len(), 1);
        assert_eq!(
            context.gatt[0].service,
            GattChannel::from_bytes(
                *Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
            )
        );
        assert!(context.gatt[0].roles.supports_write_without_response());
        assert!(context.gatt[0].roles.supports_notify());
    }

    #[tokio::test]
    async fn drive_session_resolves_falcon_after_family_and_name_banner_notifications() {
        let peripheral = RecordingPeripheral::with_notifications(vec![
            ValueNotification {
                uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                value: vec![0x55, 0xaa, 0, 0],
            },
            ValueNotification {
                uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                value: b"NAME=Falcon".to_vec(),
            },
        ]);
        let mut session = SubscribeOnlySession;
        let summary = begode_falcon_summary("Begode_Falcon");

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes(
                *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
            ),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
        )
        .await
        .expect("bridge resolves Falcon identity");

        let identity = report.identity.expect("model identity is reported");
        assert_eq!(identity.confidence, IdentityConfidence::Model);
        assert_eq!(identity.manufacturer, Some("Begode"));
        assert_eq!(identity.model, Some("Falcon"));
        assert!(identity.evidence.has_passive_family_match());
        assert!(identity.evidence.has_banner_model_match());
        assert_eq!(report.notifications, 2);
        assert_eq!(peripheral.writes.lock().expect("write log").len(), 0);
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
                manufacturer_data: Vec::new(),
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
            &summary,
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
        let summary = shared_write_notify_summary("NOSFET Aero");

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
        )
        .await
        .expect("bridge consumes notifications");

        assert_eq!(report.notifications, 1);
        assert_eq!(report.notification_bytes, 2);
        assert_eq!(report.latest_notification_len, Some(2));
        assert_eq!(
            *session
                .last_notification_channel
                .lock()
                .expect("notification channel"),
            Some(GattChannel::from_bytes([0xA1; 16]))
        );
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
        assert_eq!(report.diagnostics_snapshot.malformed_frames, 1);
        assert_eq!(
            report.diagnostic_errors.as_slice(),
            &[cutout_core::DiagnosticError::from_parser_error(
                cutout_core::ParserError::MalformedFrame
            )]
        );
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::RawNotification {
                monotonic_ms: 2,
                len: 2,
                ..
            }
        )));
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: 2,
                ..
            }
        )));
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::Diagnostics {
                monotonic_ms: 2,
                diagnostics,
            } if diagnostics.malformed_frames == 1
        )));
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::DiagnosticError {
                monotonic_ms: 2,
                error,
            } if error.kind == cutout_core::DiagnosticErrorKind::MalformedFrame
        )));
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
        let summary = shared_write_notify_summary("NOSFET Aero");

        let capture = crate::capture_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
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

    #[tokio::test]
    async fn capture_session_with_commands_records_command_writes_before_tick() {
        let peripheral = RecordingPeripheral::default();
        let mut session = CommandWriteSession;
        let summary = begode_falcon_summary("Begode_Falcon");

        let capture = crate::capture_session_with_commands(
            &peripheral,
            &mut session,
            GattChannel::from_bytes(
                *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
            ),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
            &[
                DeviceCommand::RequestIdentity,
                DeviceCommand::RequestFirmwareInfo,
            ],
        )
        .await
        .expect("capture records explicit command writes");

        assert_eq!(capture.report.writes, 2);
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
                    mode: WriteType::WithoutResponse,
                    bytes: b"N".to_vec(),
                    provisional: false,
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteType::WithoutResponse,
                    bytes: b"V".to_vec(),
                    provisional: false,
                },
            ]
        );
    }

    #[tokio::test]
    async fn capture_session_chunks_writes_by_negotiated_write_limit() {
        let peripheral = RecordingPeripheral::with_mtu(4);
        let mut session = LargeWriteSession;
        let summary = shared_write_notify_summary("NOSFET Aero");

        let capture = crate::capture_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
            false,
        )
        .await
        .expect("capture chunks oversized bridge writes");

        assert_eq!(capture.report.protocol_writes, 1);
        assert_eq!(capture.report.writes, 3);
        assert_eq!(
            peripheral.writes.lock().expect("write log").as_slice(),
            &[
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"0123".to_vec(),
                    WriteType::WithoutResponse,
                ),
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"4567".to_vec(),
                    WriteType::WithoutResponse,
                ),
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"89".to_vec(),
                    WriteType::WithoutResponse,
                ),
            ]
        );
        let writes: Vec<_> = capture
            .records
            .iter()
            .filter_map(|record| match record {
                crate::SessionCaptureRecord::Write { bytes, mode, .. } => {
                    Some((bytes.as_slice(), *mode))
                }
                crate::SessionCaptureRecord::Link { .. }
                | crate::SessionCaptureRecord::LinkDown { .. }
                | crate::SessionCaptureRecord::Subscribe { .. }
                | crate::SessionCaptureRecord::Notification { .. } => None,
            })
            .collect();
        assert_eq!(
            writes,
            vec![
                (b"0123".as_slice(), WriteType::WithoutResponse),
                (b"4567".as_slice(), WriteType::WithoutResponse),
                (b"89".as_slice(), WriteType::WithoutResponse),
            ]
        );
    }

    #[tokio::test]
    async fn drive_session_feeds_link_down_after_intentional_disconnect() {
        let peripheral = RecordingPeripheral::default();
        let mut session = DisconnectOnTickSession::default();
        let summary = shared_write_notify_summary("NOSFET Aero");

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
        )
        .await
        .expect("bridge handles intentional disconnect");

        assert_eq!(report.disconnects, 1);
        assert_eq!(
            report.events.as_slice(),
            &[crate::SessionBridgeEvent::LinkDown { monotonic_ms: 1 }]
        );
        assert_eq!(*session.link_down_count.lock().expect("count"), 1);
        assert_eq!(*peripheral.disconnects.lock().expect("disconnect log"), 1);
    }

    #[tokio::test]
    async fn capture_session_records_link_down_after_intentional_disconnect() {
        let peripheral = RecordingPeripheral::default();
        let mut session = DisconnectOnTickSession::default();
        let summary = shared_write_notify_summary("NOSFET Aero");

        let capture = crate::capture_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
            false,
        )
        .await
        .expect("capture records intentional disconnect");

        assert_eq!(
            capture.records,
            vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(185),
                },
                crate::SessionCaptureRecord::LinkDown { monotonic_ms: 1 },
            ]
        );
        assert_eq!(capture.report.disconnects, 1);
    }

    #[tokio::test]
    async fn capture_reconnecting_session_restores_subscription_after_disconnect() {
        let first = RecordingPeripheral::default();
        let second = RecordingPeripheral::default();
        let mut host = FakeReconnectHost::new(vec![first.clone(), second.clone()]);
        let mut session = ReconnectOnceSession::default();

        let reconnecting_capture = crate::capture_reconnecting_session_with_summaries(
            &mut host,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            Duration::ZERO,
            2,
            false,
        )
        .await
        .expect("fake host reconnects once");
        let capture = reconnecting_capture.capture;

        assert_eq!(host.connects, 2);
        assert_eq!(reconnecting_capture.attempts.len(), 2);
        assert_eq!(reconnecting_capture.attempts[0].attempt, 1);
        assert_eq!(reconnecting_capture.attempts[0].report.subscribes, 1);
        assert_eq!(reconnecting_capture.attempts[0].report.disconnects, 1);
        assert_eq!(reconnecting_capture.attempts[1].attempt, 2);
        assert_eq!(reconnecting_capture.attempts[1].report.subscribes, 1);
        assert_eq!(reconnecting_capture.attempts[1].report.disconnects, 0);
        assert_eq!(
            reconnecting_capture.attempts[0]
                .summary
                .observation
                .name
                .as_deref(),
            Some("NOSFET Aero")
        );
        assert_eq!(
            reconnecting_capture.attempts[1]
                .summary
                .observation
                .name
                .as_deref(),
            Some("NOSFET Aero")
        );
        assert_eq!(capture.report.subscribes, 2);
        assert_eq!(capture.report.disconnects, 1);
        assert_eq!(*session.link_ups.lock().expect("link ups"), 2);
        assert_eq!(*session.link_downs.lock().expect("link downs"), 1);
        assert_eq!(first.subscribes.lock().expect("first subscribes").len(), 1);
        assert_eq!(
            second.subscribes.lock().expect("second subscribes").len(),
            1
        );
        assert_eq!(*first.disconnects.lock().expect("first disconnects"), 1);
        assert_eq!(*second.disconnects.lock().expect("second disconnects"), 0);
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
                crate::SessionCaptureRecord::LinkDown { monotonic_ms: 1 },
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 2,
                    max_write_len: Some(185),
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
            ]
        );
    }

    #[tokio::test]
    async fn capture_reconnecting_session_cancels_commands_after_reconnect() {
        let first = RecordingPeripheral::default();
        let second = RecordingPeripheral::default();
        let mut host = FakeReconnectHost::new(vec![first.clone(), second.clone()]);
        let mut session = CommandThenDisconnectSession::default();

        let capture = crate::capture_reconnecting_session_with_commands(
            &mut host,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            Duration::ZERO,
            2,
            false,
            &[
                DeviceCommand::RequestIdentity,
                DeviceCommand::RequestFirmwareInfo,
            ],
        )
        .await
        .expect("fake host reconnects after first-link commands");

        assert_eq!(capture.attempts.len(), 2);
        assert_eq!(capture.attempts[0].report.writes, 2);
        assert_eq!(capture.attempts[1].report.writes, 0);
        assert_eq!(
            first.writes.lock().expect("first writes").as_slice(),
            &[
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"N".to_vec(),
                    WriteType::WithoutResponse,
                ),
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"V".to_vec(),
                    WriteType::WithoutResponse,
                ),
            ]
        );
        assert!(second.writes.lock().expect("second writes").is_empty());
        assert_eq!(
            second.subscribes.lock().expect("second subscribes").len(),
            1
        );
    }

    #[derive(Default)]
    struct BridgeSession {
        notification_count: Arc<Mutex<usize>>,
        last_notification_channel: Arc<Mutex<Option<GattChannel>>>,
    }

    struct SubscribeOnlySession;

    impl ProtocolSession for SubscribeOnlySession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            if matches!(input, SessionInput::LinkUp(_)) {
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: GattChannel::from_bytes(
                        *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                    ),
                }));
            }
        }
    }

    struct CommandWriteSession;

    #[derive(Default)]
    struct CommandThenDisconnectSession {
        link_ups: usize,
    }

    struct LargeWriteSession;

    #[derive(Default)]
    struct DisconnectOnTickSession {
        link_down_count: Arc<Mutex<usize>>,
    }

    #[derive(Default)]
    struct ReconnectOnceSession {
        link_ups: Arc<Mutex<usize>>,
        link_downs: Arc<Mutex<usize>>,
    }

    struct FakeReconnectHost {
        peripherals: std::collections::VecDeque<RecordingPeripheral>,
        connects: usize,
    }

    impl FakeReconnectHost {
        fn new(peripherals: Vec<RecordingPeripheral>) -> Self {
            Self {
                peripherals: peripherals.into(),
                connects: 0,
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::ReconnectingSessionHost for FakeReconnectHost {
        type Peripheral = RecordingPeripheral;

        async fn connect(
            &mut self,
        ) -> Result<(Self::Peripheral, crate::ConnectionSummary), crate::BtleError> {
            self.connects += 1;
            self.peripherals
                .pop_front()
                .map(|peripheral| (peripheral, shared_write_notify_summary("NOSFET Aero")))
                .ok_or(crate::BtleError::NoPeripheralMatched)
        }
    }

    impl ProtocolSession for CommandWriteSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(_) => {
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes(
                            *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                        ),
                    }));
                }
                SessionInput::Command(command) => {
                    let bytes = match command {
                        DeviceCommand::RequestIdentity => b"N".as_slice(),
                        DeviceCommand::RequestFirmwareInfo => b"V".as_slice(),
                        _ => return,
                    };
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes(
                            *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                        ),
                        bytes: cutout_core::WritePayload::try_from_slice(bytes)
                            .expect("fixture payload fits"),
                        mode: WriteMode::WithoutResponse,
                    }));
                }
                SessionInput::LinkDown
                | SessionInput::Tick { .. }
                | SessionInput::Notification { .. } => {}
            }
        }
    }

    impl ProtocolSession for LargeWriteSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            if matches!(input, SessionInput::Tick { .. }) {
                output.push(SessionOutput::Transport(TransportAction::Write {
                    channel: GattChannel::from_bytes([0xA1; 16]),
                    bytes: cutout_core::WritePayload::try_from_slice(b"0123456789")
                        .expect("fixture payload fits"),
                    mode: WriteMode::WithoutResponse,
                }));
            }
        }
    }

    impl ProtocolSession for CommandThenDisconnectSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(_) => {
                    self.link_ups += 1;
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                    }));
                }
                SessionInput::Command(command) => {
                    let bytes = match command {
                        DeviceCommand::RequestIdentity => b"N".as_slice(),
                        DeviceCommand::RequestFirmwareInfo => b"V".as_slice(),
                        _ => return,
                    };
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                        bytes: cutout_core::WritePayload::try_from_slice(bytes)
                            .expect("fixture payload fits"),
                        mode: WriteMode::WithoutResponse,
                    }));
                }
                SessionInput::Tick { .. } => {
                    if self.link_ups == 1 {
                        output.push(SessionOutput::Transport(TransportAction::Disconnect));
                    }
                }
                SessionInput::LinkDown | SessionInput::Notification { .. } => {}
            }
        }
    }

    impl ProtocolSession for DisconnectOnTickSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::Tick { .. } => {
                    output.push(SessionOutput::Transport(TransportAction::Disconnect));
                }
                SessionInput::LinkDown => {
                    *self.link_down_count.lock().expect("link down count") += 1;
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown));
                }
                SessionInput::LinkUp(_)
                | SessionInput::Command(_)
                | SessionInput::Notification { .. } => {}
            }
        }
    }

    impl ProtocolSession for ReconnectOnceSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(_) => {
                    let mut link_ups = self.link_ups.lock().expect("link ups");
                    *link_ups += 1;
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                    }));
                }
                SessionInput::Tick { .. } => {
                    if *self.link_ups.lock().expect("link ups") == 1 {
                        output.push(SessionOutput::Transport(TransportAction::Disconnect));
                    }
                }
                SessionInput::LinkDown => {
                    *self.link_downs.lock().expect("link downs") += 1;
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown));
                }
                SessionInput::Command(_) | SessionInput::Notification { .. } => {}
            }
        }
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
                SessionInput::Notification { channel, .. } => {
                    *self
                        .notification_count
                        .lock()
                        .expect("notification counter") += 1;
                    *self
                        .last_notification_channel
                        .lock()
                        .expect("notification channel") = Some(channel);
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
                    output.push(SessionOutput::Event(DeviceEvent::DiagnosticError(
                        cutout_core::DiagnosticError::from_parser_error(
                            cutout_core::ParserError::MalformedFrame,
                        ),
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
        disconnects: Arc<Mutex<usize>>,
        mtu: u16,
    }

    impl Default for RecordingPeripheral {
        fn default() -> Self {
            Self {
                subscribes: Arc::new(Mutex::new(Vec::new())),
                writes: Arc::new(Mutex::new(Vec::new())),
                notifications: Arc::new(Mutex::new(Vec::new())),
                disconnects: Arc::new(Mutex::new(0)),
                mtu: 185,
            }
        }
    }

    impl RecordingPeripheral {
        fn with_mtu(mtu: u16) -> Self {
            Self {
                mtu,
                ..Self::default()
            }
        }

        fn with_notification(notification: ValueNotification) -> Self {
            Self::with_notifications(vec![notification])
        }

        fn with_notifications(notifications: Vec<ValueNotification>) -> Self {
            Self {
                notifications: Arc::new(Mutex::new(notifications)),
                ..Self::default()
            }
        }
    }

    fn begode_falcon_summary(name: &str) -> crate::ConnectionSummary {
        crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some(name.to_owned()),
                rssi: Some(-42),
                advertised_services: vec![Uuid::from_u128(
                    0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
                )],
                manufacturer_data: Vec::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }],
            }],
        }
    }

    fn shared_write_notify_summary(name: &str) -> crate::ConnectionSummary {
        crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some(name.to_owned()),
                rssi: Some(-42),
                advertised_services: Vec::new(),
                manufacturer_data: Vec::new(),
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
        }
    }

    #[async_trait::async_trait]
    impl crate::SessionPeripheral for RecordingPeripheral {
        fn mtu(&self) -> u16 {
            self.mtu
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
            *self.disconnects.lock().expect("disconnect log") += 1;
            Ok(())
        }
    }
}
