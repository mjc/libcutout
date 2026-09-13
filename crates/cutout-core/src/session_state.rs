//! Rust-owned session-state root and typed state slices.

use crate::{
    BatteryPageMetadata, BatteryReadback, DeviceEvent, FirmwareInfo, GattFingerprint,
    MonotonicTimestamp, ParserDiagnostics, ProtocolFamily, RawTelemetryReadback, ReadOnlyResponse,
    RideSessionLifecycle, SessionOutput, TelemetryDelta, TelemetrySnapshot,
};
use arrayvec::ArrayVec;
use bytes::Bytes;

/// Rust-owned durable state for one `CutOut` mobile/device session.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CutoutSessionState {
    /// Attempt identity, deadline and protocol admission state.
    pub connection: crate::ConnectionAttemptLifecycle,

    /// Logical ride and Live Activity lifecycle state.
    pub ride_session: RideSessionLifecycle,

    /// Device identity state accumulated from discovery, protocol, and model evidence.
    pub identity: DeviceIdentityState,

    /// Telemetry state accumulated from ride, charge, raw telemetry, and BMS packets.
    pub telemetry: TelemetryState,

    /// Semantic setting observations and command lifecycle for this device.
    pub settings: crate::DeviceSettingsState,

    /// Semantic device action lifecycle and measured procedure progress.
    pub actions: crate::DeviceActionsState,

    /// Diagnostics accumulated from parser and protocol diagnostic events.
    pub diagnostics: SessionDiagnosticsState,
}

impl CutoutSessionState {
    /// Returns the current identity state without cloning the whole root.
    #[must_use]
    pub const fn identity(&self) -> &DeviceIdentityState {
        &self.identity
    }

    /// Returns mutable identity state to protocol decoders that contribute ordered evidence.
    #[must_use]
    pub const fn identity_mut(&mut self) -> &mut DeviceIdentityState {
        &mut self.identity
    }

    /// Returns the current discovery facts without cloning the whole root.
    #[must_use]
    pub const fn discovery(&self) -> &DiscoveryState {
        &self.identity.discovery
    }

    /// Returns the current telemetry state without cloning the whole root.
    #[must_use]
    pub const fn telemetry(&self) -> &TelemetryState {
        &self.telemetry
    }

    /// Returns the latest rolling ride/charge telemetry snapshot.
    #[must_use]
    pub const fn current_telemetry(&self) -> TelemetrySnapshot {
        self.telemetry.current
    }

    /// Returns cumulative parser diagnostics retained by the state root.
    #[must_use]
    pub const fn parser_diagnostics(&self) -> ParserDiagnostics {
        self.diagnostics.parser
    }

    /// Accumulates model/protocol identity evidence into the identity slice.
    pub fn observe_identity(&mut self, update: DeviceIdentityUpdate) {
        self.identity.apply_update(update);
    }

    /// Adds a discovery observation to the identity slice.
    pub fn observe_discovery(&mut self, observation: DiscoveryObservation) {
        self.identity.observe_discovery(observation);
    }

    /// Selects a discovered platform identifier for this session.
    pub fn select_discovered_platform(&mut self, platform_identifier: String) {
        self.identity
            .select_discovered_platform(platform_identifier);
    }

    /// Clears device-specific identity evidence while preserving the discovery inventory.
    pub fn reset_device_identity(&mut self) {
        let discovery = core::mem::take(&mut self.identity.discovery);
        self.identity = DeviceIdentityState {
            discovery,
            ..DeviceIdentityState::default()
        };
    }

    pub(crate) fn observe_outputs(&mut self, outputs: &[SessionOutput]) {
        for output in outputs {
            self.observe_output(output);
        }
    }

    /// Assigns an identity to a newly decoded cell-voltage page before any presentation
    /// aggregation can combine it with other pages.
    pub(crate) fn assign_bms_observation_event_sequence(&mut self, readback: &mut BatteryReadback) {
        self.telemetry
            .bms
            .assign_observation_event_sequence(readback);
    }

    fn observe_output(&mut self, output: &SessionOutput) {
        match output {
            SessionOutput::Event(event) => self.observe_event(event),
            SessionOutput::Transport(_) | SessionOutput::NotificationIngest(_) => {}
        }
    }

    fn observe_event(&mut self, event: &DeviceEvent) {
        match event {
            DeviceEvent::Telemetry(delta) => self.telemetry.observe_delta(*delta),
            DeviceEvent::ReadOnlyResponse(response) => self.observe_read_only_response(response),
            DeviceEvent::Diagnostics(diagnostics) => self.diagnostics.parser.merge(*diagnostics),
            DeviceEvent::LinkUp(_)
            | DeviceEvent::LinkDown
            | DeviceEvent::Tick { .. }
            | DeviceEvent::ControlRefusal(_)
            | DeviceEvent::DiagnosticError(_) => {}
        }
    }

    fn observe_read_only_response(&mut self, response: &ReadOnlyResponse) {
        match response {
            ReadOnlyResponse::Firmware(firmware) => self
                .identity
                .apply_update(DeviceIdentityUpdate::firmware(*firmware)),
            ReadOnlyResponse::Battery(readback) => self.telemetry.bms.observe_readback(readback),
            ReadOnlyResponse::RawTelemetry(readback) => self.telemetry.raw = readback.clone(),
            ReadOnlyResponse::Diagnostics(_)
            | ReadOnlyResponse::Settings(_)
            | ReadOnlyResponse::FaultHistory(_) => {}
        }
    }
}

/// Device model/protocol identity accumulated across many discovery and protocol events.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceIdentityState {
    /// Resolved protocol family, once enough evidence identifies it.
    pub protocol_family: Option<ProtocolFamily>,

    /// Resolved model name, once enough evidence identifies it.
    pub model: Option<String>,

    /// Discovery observations and selection facts from the mobile BLE stack.
    pub discovery: DiscoveryState,

    /// Firmware or protocol version readback, when reported.
    pub firmware: Option<FirmwareInfo>,

    /// Latest raw advertisement name retained as identity provenance.
    pub advertised_name: Option<AdvertisedName>,

    /// Current GATT fingerprint evidence.
    pub gatt: Vec<GattFingerprint>,

    /// Latest raw model banner retained as identity provenance.
    pub model_banner: Option<ModelBanner>,

    /// Latest raw firmware banner retained as identity provenance.
    pub firmware_banner: Option<Vec<u8>>,

    /// Latest raw IMU banner retained as identity provenance.
    pub imu_banner: Option<Vec<u8>>,

    /// Protocol-owned model identity evidence.
    pub protocol_model: ProtocolModelIdentityEvidence,

    /// Strong wire evidence reported incompatible protocol families.
    pub protocol_conflict: bool,

    pending_probe_started_at: [Option<MonotonicTimestamp>; PendingProbe::COUNT],

    /// Latest probe that did not produce a matching response.
    pub missing_probe_response: Option<PendingProbe>,

    /// Latest probe that produced malformed identity evidence.
    pub malformed_probe_response: Option<PendingProbe>,
}

impl DeviceIdentityState {
    /// Records an identity probe and the monotonic time at which it was written.
    pub fn observe_probe_write(&mut self, probe: PendingProbe, started_at: MonotonicTimestamp) {
        self.pending_probe_started_at[probe.index()].get_or_insert(started_at);
    }

    /// Clears one probe after its matching response arrives.
    pub fn observe_probe_response(&mut self, probe: PendingProbe) {
        self.pending_probe_started_at[probe.index()] = None;
    }

    /// Marks one pending probe as missing.
    pub fn observe_probe_timeout(&mut self, probe: PendingProbe) -> bool {
        if self.pending_probe_started_at[probe.index()]
            .take()
            .is_none()
        {
            return false;
        }
        self.missing_probe_response = Some(probe);
        true
    }

    /// Expires every probe strictly older than the response timeout.
    pub fn expire_pending_probes(
        &mut self,
        now: MonotonicTimestamp,
        timeout: crate::Duration,
    ) -> ArrayVec<PendingProbe, { PendingProbe::COUNT }> {
        let mut expired = ArrayVec::new();
        for probe in PendingProbe::ALL {
            let Some(started_at) = self.pending_probe_started_at[probe.index()] else {
                continue;
            };
            if now.saturating_duration_since(started_at) > timeout {
                let _ = self.observe_probe_timeout(probe);
                expired.push(probe);
            }
        }
        expired
    }

    /// Marks every outstanding probe as missing.
    pub fn mark_pending_probes_missing(
        &mut self,
    ) -> ArrayVec<PendingProbe, { PendingProbe::COUNT }> {
        let mut missing = ArrayVec::new();
        for probe in PendingProbe::ALL {
            if self.observe_probe_timeout(probe) {
                missing.push(probe);
            }
        }
        missing
    }

    /// Returns the next strict probe-expiration deadline.
    #[must_use]
    pub fn next_probe_expiry(&self, timeout: crate::Duration) -> Option<MonotonicTimestamp> {
        let delay = crate::Duration::from_milliseconds(timeout.as_milliseconds().saturating_add(1));
        self.pending_probe_started_at
            .iter()
            .flatten()
            .map(|started_at| started_at.saturating_add_duration(delay))
            .min()
    }

    fn apply_update(&mut self, update: DeviceIdentityUpdate) {
        self.protocol_family = self.protocol_family.or(update.protocol_family);
        self.model = self.model.take().or(update.model);
        self.firmware = self.firmware.or(update.firmware);
    }

    fn observe_discovery(&mut self, observation: DiscoveryObservation) {
        self.discovery.observe(observation);
    }

    fn select_discovered_platform(&mut self, platform_identifier: String) {
        self.discovery.select_platform(platform_identifier);
    }
}

/// Raw advertised-name bytes retained as device identity provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvertisedName(Bytes);

impl AdvertisedName {
    /// Copies borrowed advertised-name bytes into owned provenance.
    #[must_use]
    pub fn copy_from_slice(bytes: &[u8]) -> Self {
        Self(Bytes::copy_from_slice(bytes))
    }

    /// Returns the original advertised-name bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Returns the advertised name only when the bytes are valid UTF-8.
    #[must_use]
    pub fn get(&self) -> Option<&str> {
        core::str::from_utf8(self.as_bytes()).ok()
    }
}

/// Raw model-banner bytes retained as device identity provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelBanner(Bytes);

impl ModelBanner {
    /// Copies borrowed model-banner bytes into owned provenance.
    #[must_use]
    pub fn copy_from_slice(bytes: &[u8]) -> Self {
        Self(Bytes::copy_from_slice(bytes))
    }

    /// Returns the original model-banner bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Returns the model banner only when the bytes are valid banner text.
    #[must_use]
    pub fn get(&self) -> Option<&str> {
        core::str::from_utf8(self.as_bytes())
            .ok()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .filter(|model| {
                model
                    .bytes()
                    .all(|byte| matches!(byte, b'\n' | b'\r' | b'\t' | 0x20..=0x7e))
            })
    }
}

/// Protocol-native model identity decoded from protocol-owned bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolModelIdentity {
    /// Protocol family that owned and decoded the model id.
    pub family: ProtocolFamily,

    /// Protocol-native model id.
    pub model_id: u16,
}

/// Protocol-owned model identity evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProtocolModelIdentityEvidence {
    /// No protocol model id was present.
    #[default]
    Missing,

    /// A protocol-owned decoder produced a model id.
    ModelId(ProtocolModelIdentity),

    /// The bytes looked like protocol identity but were malformed.
    Malformed,
}

impl ProtocolModelIdentityEvidence {
    /// Creates protocol-owned model-id evidence.
    #[must_use]
    pub const fn model_id(family: ProtocolFamily, model_id: u16) -> Self {
        Self::ModelId(ProtocolModelIdentity { family, model_id })
    }
}

/// Identity probe correlation state retained by the Rust session root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingProbe {
    /// Begode `N` probe awaiting a model/name response.
    BegodeName,

    /// Begode `V` probe awaiting a firmware response.
    BegodeFirmware,

    /// Begode `M` probe awaiting an IMU response.
    BegodeImu,
}

impl PendingProbe {
    const ALL: [Self; 3] = [Self::BegodeName, Self::BegodeFirmware, Self::BegodeImu];
    const COUNT: usize = Self::ALL.len();

    const fn index(self) -> usize {
        match self {
            Self::BegodeName => 0,
            Self::BegodeFirmware => 1,
            Self::BegodeImu => 2,
        }
    }
}

/// Partial identity evidence update.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceIdentityUpdate {
    /// Protocol family evidence from discovery or protocol classification.
    pub protocol_family: Option<ProtocolFamily>,

    /// Model evidence from discovery, protocol identity, or registry resolution.
    pub model: Option<String>,

    /// Firmware or protocol version evidence.
    pub firmware: Option<FirmwareInfo>,
}

impl DeviceIdentityUpdate {
    /// Creates a firmware identity update.
    #[must_use]
    pub const fn firmware(firmware: FirmwareInfo) -> Self {
        Self {
            protocol_family: None,
            model: None,
            firmware: Some(firmware),
        }
    }
}

/// Device discovery state retained as identity evidence from the mobile BLE stack.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveryState {
    /// Discovery observations retained by platform identifier in last-observed order.
    pub observations: Vec<DiscoveryObservation>,

    /// Platform identifier selected for the current mobile session.
    pub selected_platform_identifier: Option<String>,
}

impl DiscoveryState {
    fn observe(&mut self, observation: DiscoveryObservation) {
        self.observations
            .retain(|existing| existing.platform_identifier != observation.platform_identifier);
        self.observations.push(observation);
    }

    fn select_platform(&mut self, platform_identifier: String) {
        self.selected_platform_identifier = Some(platform_identifier);
    }

    /// Returns picker candidates derived from retained discovery evidence.
    #[must_use]
    pub fn picker_candidates(&self) -> Vec<DiscoveryCandidateSnapshot> {
        self.observations
            .iter()
            .filter_map(DiscoveryCandidateSnapshot::from_observation)
            .collect()
    }

    /// Returns retained unrecognized peripherals only for explicit advanced capture selection.
    #[must_use]
    pub fn advanced_capture_candidates(&self) -> Vec<DiscoveryCandidateSnapshot> {
        self.observations
            .iter()
            .filter(|observation| {
                DiscoveryCandidateSnapshot::from_observation(observation).is_none()
            })
            .map(|observation| DiscoveryCandidateSnapshot {
                platform_identifier: observation.platform_identifier.clone(),
                display_name: observation
                    .advertised_name_text()
                    .unwrap_or("Unknown Bluetooth device")
                    .to_owned(),
                product_category: "Unknown device".to_owned(),
                evidence: "Retained Bluetooth advertisement".to_owned(),
                detail: "Capture only; protocol unknown".to_owned(),
                support: DiscoveryCandidateSupport::UnknownRecordable,
                connection_route: None,
                electric_unicycle_model: None,
            })
            .collect()
    }
}

/// A normalized 128-bit Bluetooth service UUID.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BluetoothServiceUuid([u8; 16]);

impl BluetoothServiceUuid {
    /// Serial service advertised by many EUC controllers.
    pub const EUC_SERIAL_FFE0: Self = Self::from_bluetooth16(0xffe0);

    /// Legacy VESC serial service.
    pub const VESC_SERIAL_FFF0: Self = Self::from_bluetooth16(0xfff0);

    /// Nordic UART service used by VESC controllers.
    pub const VESC_NORDIC_UART: Self = Self([
        0x6e, 0x40, 0x00, 0x01, 0xb5, 0xa3, 0xf3, 0x93, 0xe0, 0xa9, 0xe5, 0x0e, 0x24, 0xdc, 0xca,
        0x9e,
    ]);

    /// Normalizes a standard 16-bit service UUID into the Bluetooth base UUID.
    #[must_use]
    pub const fn from_bluetooth16(value: u16) -> Self {
        let [high, low] = value.to_be_bytes();
        Self([
            0x00, 0x00, high, low, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
            0x34, 0xfb,
        ])
    }

    /// Returns the normalized UUID bytes in network order.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Length supplied for a Bluetooth service UUID was not 16 bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidBluetoothServiceUuidLength(pub usize);

impl TryFrom<Vec<u8>> for BluetoothServiceUuid {
    type Error = InvalidBluetoothServiceUuidLength;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let length = bytes.len();
        <[u8; 16]>::try_from(bytes)
            .map(Self)
            .map_err(|_| InvalidBluetoothServiceUuidLength(length))
    }
}

/// Discovery facts observed for one platform peripheral.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryObservation {
    /// Stable platform identifier supplied by the mobile BLE stack.
    pub platform_identifier: String,

    /// Raw advertised-name bytes.
    pub advertised_name: Option<Vec<u8>>,

    /// Normalized advertised service UUIDs used as protocol evidence.
    pub advertised_service_uuids: Vec<BluetoothServiceUuid>,

    /// Manufacturer data summaries without retaining opaque payload bytes.
    pub manufacturer_data: Vec<DiscoveryManufacturerDataSummary>,

    /// Last observed RSSI in dBm.
    pub rssi_dbm: Option<i16>,
}

impl DiscoveryObservation {
    /// Returns advertised-name text only when the raw bytes are valid UTF-8.
    #[must_use]
    pub fn advertised_name_text(&self) -> Option<&str> {
        self.advertised_name
            .as_deref()
            .and_then(|bytes| core::str::from_utf8(bytes).ok())
    }
}

/// Summary of advertised manufacturer data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryManufacturerDataSummary {
    /// Bluetooth company identifier.
    pub company_identifier: u16,

    /// Opaque manufacturer payload length in bytes.
    pub payload_len: usize,
}

/// Picker candidate support derived from discovery evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryCandidateSupport {
    /// Candidate can be paired through the current mobile route.
    Supported,

    /// Candidate should be identified with a read-only probe before routing.
    ProbeRecommended,

    /// Candidate is relevant enough to capture but has no supported route yet.
    UnknownRecordable,

    /// Candidate category is known, but no route exists yet.
    KnownUnsupported,

    /// Candidate has multiple plausible identities or variants.
    Ambiguous,

    /// Candidate has contradictory identity evidence.
    Conflicting,

    /// Candidate is unrelated Bluetooth noise.
    RejectedNoise,

    /// Manual add / record placeholder until capture flow is available.
    ManualPlaceholder,

    /// Candidate is not currently supported.
    Unsupported,
}

/// Electric-unicycle route model derived from typed discovery evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryElectricUnicycleModel {
    /// NOSFET Aero session.
    Aero,

    /// Begode Falcon session.
    Falcon,
}

/// Picker connection route derived from typed discovery evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryConnectionRoute {
    /// Electric unicycle read-only session route.
    ElectricUnicycle,

    /// VESC/Onewheel read-only route.
    VescOnewheel,
}

/// Picker/discovery candidate derived from Rust-owned discovery evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryCandidateSnapshot {
    /// Stable platform identifier supplied by the mobile BLE stack.
    pub platform_identifier: String,

    /// User-facing display name derived from advertisement bytes.
    pub display_name: String,

    /// Product category derived from typed discovery evidence.
    pub product_category: String,

    /// Evidence label describing why this is a picker candidate.
    pub evidence: String,

    /// Detail text derived from registry/discovery evidence.
    pub detail: String,

    /// Candidate support state.
    pub support: DiscoveryCandidateSupport,

    /// Route to use when the candidate can be picked.
    pub connection_route: Option<DiscoveryConnectionRoute>,

    /// Electric-unicycle model hint for supported discovery routes.
    pub electric_unicycle_model: Option<DiscoveryElectricUnicycleModel>,
}

impl DiscoveryCandidateSnapshot {
    /// Classifies transport hints for probing without claiming a verified protocol.
    #[must_use]
    pub fn from_observation(observation: &DiscoveryObservation) -> Option<Self> {
        let display_name = observation
            .advertised_name_text()
            .unwrap_or("Unknown Bluetooth device");

        match (
            observation
                .advertised_service_uuids
                .contains(&BluetoothServiceUuid::EUC_SERIAL_FFE0),
            observation.advertised_service_uuids.iter().any(|uuid| {
                *uuid == BluetoothServiceUuid::VESC_SERIAL_FFF0
                    || *uuid == BluetoothServiceUuid::VESC_NORDIC_UART
            }),
        ) {
            (true, _) => Some(Self {
                platform_identifier: observation.platform_identifier.clone(),
                display_name: display_name.to_owned(),
                product_category: "Electric unicycle".to_owned(),
                evidence: "FFE0/FFE1 transport hint".to_owned(),
                detail: "Read-only protocol probe recommended".to_owned(),
                support: DiscoveryCandidateSupport::ProbeRecommended,
                connection_route: None,
                electric_unicycle_model: None,
            }),
            (false, true) => Some(Self {
                platform_identifier: observation.platform_identifier.clone(),
                display_name: display_name.to_owned(),
                product_category: "UART device".to_owned(),
                evidence: "FFF0/Nordic UART transport hint".to_owned(),
                detail: "Read-only protocol probe recommended".to_owned(),
                support: DiscoveryCandidateSupport::ProbeRecommended,
                connection_route: None,
                electric_unicycle_model: None,
            }),
            (false, false) => None,
        }
    }
}

/// Telemetry accumulated across ride, charge, raw telemetry, and BMS packets.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TelemetryState {
    /// Latest rolling ride/charge telemetry fields.
    pub current: TelemetrySnapshot,

    /// BMS or battery telemetry accumulated from one or more page packets.
    pub bms: BmsTelemetryState,

    /// Latest protocol-native raw telemetry readback.
    pub raw: RawTelemetryReadback,
}

impl TelemetryState {
    fn observe_delta(&mut self, delta: TelemetryDelta) {
        self.current.apply_delta(delta);
    }
}

/// BMS telemetry accumulated across one or more page packets.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BmsTelemetryState {
    /// Latest BMS or battery readback event.
    pub latest: BatteryReadback,

    /// Latest complete readback for each observed BMS page identity.
    ///
    /// Keeping the complete readback preserves decoder-assigned observation identity alongside
    /// its page payload after later BMS packets replace `latest`.
    pub pages: Vec<BatteryReadback>,

    /// Timestamped cell-page readbacks retained across complete page cycles.
    observation_history: Vec<BatteryReadback>,

    /// Sequence for raw cell-page events in this host session.
    next_observation_event_sequence: u64,
}

impl BmsTelemetryState {
    fn assign_observation_event_sequence(&mut self, readback: &mut BatteryReadback) {
        if readback.observed_at().is_none()
            || !matches!(
                readback.page(),
                Some(crate::BatteryPagePayload::CellVoltage(_))
            )
            || readback.observation_event_sequence().is_some()
        {
            return;
        }
        let sequence = self.next_observation_event_sequence;
        self.next_observation_event_sequence = self
            .next_observation_event_sequence
            .checked_add(1)
            .expect("BMS observation event sequence exhausted");
        *readback = readback.clone().with_observation_event_sequence(sequence);
    }

    /// Summarizes retained observations, not a simultaneous scan or physical pack topology.
    #[must_use]
    pub fn observation_summary(&self) -> crate::BmsObservationSummary {
        crate::BmsObservationSummary::from_readbacks(&self.observation_history)
    }

    /// Summarizes the latest retained temperature readings across source pages.
    #[must_use]
    pub fn temperature_summary(&self) -> crate::BmsTemperatureSummary {
        crate::BmsTemperatureSummary::from_readbacks(&self.pages)
    }

    fn observe_readback(&mut self, readback: &BatteryReadback) {
        self.latest = readback.clone();
        if readback.availability() != crate::BatteryReadbackAvailability::Available {
            self.pages.clear();
            self.observation_history.clear();
        }
        self.observe_page(readback);
    }

    fn observe_page(&mut self, readback: &BatteryReadback) {
        let Some(page) = readback.page() else {
            return;
        };
        let identity = page.page();
        self.pages.retain(|existing| {
            existing
                .page()
                .is_none_or(|existing_page| !same_bms_page(existing_page.page(), identity))
        });
        self.pages.push(readback.clone());
        if matches!(page, crate::BatteryPagePayload::CellVoltage(_))
            && readback.observed_at().is_some()
        {
            let matching_history_count = self
                .observation_history
                .iter()
                .filter(|existing| {
                    existing
                        .page()
                        .is_some_and(|existing| same_bms_page(existing.page(), identity))
                })
                .count();
            if matching_history_count == crate::BMS_OBSERVATION_HISTORY_CYCLES
                && let Some(position) = self.observation_history.iter().position(|existing| {
                    existing
                        .page()
                        .is_some_and(|existing| same_bms_page(existing.page(), identity))
                })
            {
                self.observation_history.remove(position);
            }
            self.observation_history.push(readback.clone());
            let page_count = self
                .pages
                .iter()
                .filter(|readback| {
                    matches!(
                        readback.page(),
                        Some(crate::BatteryPagePayload::CellVoltage(_))
                    )
                })
                .count();
            debug_assert!(
                self.observation_history.len() <= bms_observation_history_max(page_count)
            );
        }
    }
}

const fn bms_observation_history_max(cell_page_count: usize) -> usize {
    cell_page_count.saturating_mul(crate::BMS_OBSERVATION_HISTORY_CYCLES)
}

const fn same_bms_page(left: BatteryPageMetadata, right: BatteryPageMetadata) -> bool {
    left.selector.get() == right.selector.get()
        && match (left.tag, right.tag) {
            (Some(left), Some(right)) => left.get() == right.get(),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        }
}

/// Diagnostics accumulated across parser and diagnostic events.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionDiagnosticsState {
    /// Cumulative parser diagnostics retained by the state root.
    pub parser: ParserDiagnostics,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_service_uuid_normalizes_standard_and_custom_services() {
        assert_eq!(
            BluetoothServiceUuid::from_bluetooth16(0xffe0).as_bytes(),
            &[
                0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]
        );
        assert_eq!(
            BluetoothServiceUuid::try_from(vec![0; 15]),
            Err(InvalidBluetoothServiceUuidLength(15))
        );
    }

    fn discovery_observation(
        platform_identifier: &str,
        name: &[u8],
        services: Vec<BluetoothServiceUuid>,
        rssi_dbm: i16,
    ) -> DiscoveryObservation {
        DiscoveryObservation {
            platform_identifier: platform_identifier.to_owned(),
            advertised_name: Some(name.to_vec()),
            advertised_service_uuids: services,
            manufacturer_data: vec![DiscoveryManufacturerDataSummary {
                company_identifier: 0x004c,
                payload_len: 6,
            }],
            rssi_dbm: Some(rssi_dbm),
        }
    }

    #[test]
    fn identity_state_accumulates_protocol_and_model_evidence() {
        let mut state = CutoutSessionState::default();

        state.observe_identity(DeviceIdentityUpdate {
            protocol_family: Some(ProtocolFamily::BegodeGotway),
            ..DeviceIdentityUpdate::default()
        });
        state.observe_identity(DeviceIdentityUpdate {
            model: Some("Begode Falcon".to_owned()),
            ..DeviceIdentityUpdate::default()
        });

        assert_eq!(
            state.identity().protocol_family,
            Some(ProtocolFamily::BegodeGotway)
        );
        assert_eq!(state.identity().model.as_deref(), Some("Begode Falcon"));
    }

    fn cell_readback(
        selector: u8,
        first_observation_index: u16,
        millivolts: i32,
        observed_at_ms: u64,
    ) -> BatteryReadback {
        BatteryReadback::available(crate::BatteryPagePayload::cell_voltage(
            BatteryPageMetadata::cell_voltage(
                crate::ProtocolSelector::new(selector),
                crate::VerificationStatus::HardwareVerified,
            ),
            crate::BatteryInfo::default(),
            (0..15)
                .map(|_| crate::Voltage::from_millivolts(millivolts))
                .collect(),
        ))
        .with_first_observation_index(crate::BmsObservationIndex::new(first_observation_index))
        .with_observed_at(MonotonicTimestamp::new(observed_at_ms))
    }

    fn temperature_readback(selector: u8, values: &[i32]) -> BatteryReadback {
        BatteryReadback::available(crate::BatteryPagePayload::temperature_values(
            BatteryPageMetadata::temperature(
                crate::ProtocolSelector::new(selector),
                crate::VerificationStatus::HardwareVerified,
            ),
            crate::BatteryInfo::default(),
            std::array::from_fn(|index| {
                values.get(index).map(|value| {
                    crate::Measured::reported(crate::Temperature::from_millicelsius(*value))
                })
            }),
        ))
    }

    fn metadata_temperature_readback(selector: u8, value: i32) -> BatteryReadback {
        BatteryReadback::available(crate::BatteryPagePayload::raw(
            BatteryPageMetadata::metadata(
                crate::ProtocolSelector::new(selector),
                crate::VerificationStatus::HardwareVerified,
            ),
            crate::BatteryInfo {
                temperature: Some(crate::Measured::reported(
                    crate::Temperature::from_millicelsius(value),
                )),
                ..crate::BatteryInfo::default()
            },
        ))
    }

    #[test]
    fn bms_telemetry_retains_latest_temperatures_from_every_source_page() {
        let mut state = CutoutSessionState::default();
        for readback in [
            temperature_readback(3, &[60_000]),
            temperature_readback(7, &[25_000]),
            temperature_readback(3, &[61_000]),
            cell_readback(1, 0, 4_180, 1),
        ] {
            state.observe_read_only_response(&ReadOnlyResponse::Battery(readback));
        }

        let summary = state.telemetry.bms.temperature_summary();
        assert_eq!(
            summary.readings,
            vec![
                crate::Temperature::from_millicelsius(61_000),
                crate::Temperature::from_millicelsius(25_000),
            ]
        );
        assert_eq!(
            summary.highest_temperature,
            Some(crate::Temperature::from_millicelsius(61_000))
        );
    }

    #[test]
    fn bms_telemetry_retains_metadata_temperature_sources() {
        let mut state = CutoutSessionState::default();
        state.observe_read_only_response(&ReadOnlyResponse::Battery(
            metadata_temperature_readback(1, 32_000),
        ));

        let summary = state.telemetry.bms.temperature_summary();
        assert_eq!(
            summary.readings,
            vec![crate::Temperature::from_millicelsius(32_000)]
        );
        assert_eq!(
            summary.highest_temperature,
            Some(crate::Temperature::from_millicelsius(32_000))
        );
    }

    #[test]
    fn bms_telemetry_retains_protocol_assigned_observation_indices() {
        let mut state = CutoutSessionState::default();
        for readback in [
            cell_readback(5, 30, 3_850, 1),
            cell_readback(1, 0, 3_810, 2),
            cell_readback(6, 45, 3_860, 3),
            cell_readback(2, 15, 3_820, 4),
            cell_readback(2, 15, 3_825, 5),
            BatteryReadback::available(crate::BatteryPagePayload::temperature(
                BatteryPageMetadata::temperature(
                    crate::ProtocolSelector::new(3),
                    crate::VerificationStatus::HardwareVerified,
                ),
                crate::BatteryInfo::default(),
            )),
        ] {
            state.observe_read_only_response(&ReadOnlyResponse::Battery(readback));
        }

        let mut observed = std::collections::BTreeMap::new();
        for page in state
            .telemetry
            .bms
            .pages
            .iter()
            .filter_map(|readback| crate::BatteryReadbackDto::from(readback.clone()).page)
        {
            let Some(first_observation_index) = page.first_observation_index else {
                continue;
            };
            observed.extend(page.cell_voltages.into_iter().enumerate().map(
                |(local_index, voltage)| {
                    (
                        usize::from(first_observation_index) + local_index,
                        voltage.value,
                    )
                },
            ));
        }

        assert_eq!(observed.len(), 60);
        for (indices, millivolts) in [
            (0..15, 3_810),
            (15..30, 3_825),
            (30..45, 3_850),
            (45..60, 3_860),
        ] {
            for index in indices {
                assert_eq!(observed[&index], millivolts);
            }
        }
        assert!(matches!(
            state.telemetry.bms.latest.page(),
            Some(crate::BatteryPagePayload::Temperature(_))
        ));
        let summary = state.telemetry.bms.observation_summary();
        assert_eq!(summary.observed_count, 60);
        assert_eq!(
            summary.lowest_index,
            Some(crate::BmsObservationIndex::new(0))
        );
        assert_eq!(
            summary.highest_index,
            Some(crate::BmsObservationIndex::new(45))
        );
        assert_eq!(
            summary
                .voltage_spread
                .map(crate::VoltageDelta::as_millivolts),
            Some(50)
        );

        for observed_at_ms in 6..10 {
            state.observe_read_only_response(&ReadOnlyResponse::Battery(cell_readback(
                2,
                15,
                3_800,
                observed_at_ms,
            )));
        }
        let summary = state.telemetry.bms.observation_summary();
        assert_eq!(summary.observed_count, 60);
        assert_eq!(
            summary.lowest_index,
            Some(crate::BmsObservationIndex::new(15))
        );
        assert_eq!(
            summary
                .voltage_spread
                .map(crate::VoltageDelta::as_millivolts),
            Some(60)
        );

        state
            .observe_read_only_response(&ReadOnlyResponse::Battery(BatteryReadback::unavailable()));
        assert!(state.telemetry.bms.pages.is_empty());
        assert_eq!(
            state.telemetry.bms.observation_summary(),
            crate::BmsObservationSummary::default()
        );
    }

    #[test]
    fn bms_observation_summary_handles_empty_zero_ties_overflow_and_large_collections() {
        fn page(first: u16, values: &[i32]) -> BatteryReadback {
            BatteryReadback::available(crate::BatteryPagePayload::cell_voltage(
                BatteryPageMetadata::cell_voltage(
                    crate::ProtocolSelector::new(1),
                    crate::VerificationStatus::Unverified,
                ),
                crate::BatteryInfo::default(),
                values
                    .iter()
                    .copied()
                    .map(crate::Voltage::from_millivolts)
                    .collect(),
            ))
            .with_first_observation_index(BmsObservationIndex::new(first))
            .with_observed_at(MonotonicTimestamp::new(u64::from(first) + 1))
        }
        use crate::{BmsObservationIndex, BmsObservationSummary, VoltageDelta};
        assert_eq!(
            BmsObservationSummary::from_readbacks(&[page(0, &[]), BatteryReadback::unsupported()]),
            BmsObservationSummary::default()
        );
        let summary =
            BmsObservationSummary::from_readbacks(&[page(45, &[0, 4_200]), page(0, &[0, 4_200])]);
        assert_eq!(summary.observed_count, 4);
        assert_eq!(summary.lowest_index, Some(BmsObservationIndex::new(0)));
        assert_eq!(summary.highest_index, Some(BmsObservationIndex::new(1)));
        assert_eq!(
            summary.voltage_spread,
            Some(VoltageDelta::from_millivolts(4_200))
        );
        let summary = BmsObservationSummary::from_readbacks(&[page(0, &[i32::MIN, i32::MAX])]);
        assert_eq!(
            summary.voltage_spread,
            Some(VoltageDelta::from_millivolts(i32::MAX))
        );
        let summary = BmsObservationSummary::from_readbacks(&[page(0, &[0]), page(0, &[4_000])]);
        assert_eq!(summary.observed_count, 1);
        assert_eq!(
            summary.voltage_spread,
            Some(VoltageDelta::from_millivolts(0))
        );
        for count in [224_u16, 252] {
            let pages: Vec<_> = (0..count)
                .rev()
                .map(|index| page(index, &[4_000 + i32::from(index)]))
                .collect();
            let summary = BmsObservationSummary::from_readbacks(&pages);
            assert_eq!(summary.observed_count, u32::from(count));
            assert_eq!(summary.lowest_index, Some(BmsObservationIndex::new(0)));
            assert_eq!(
                summary.highest_index,
                Some(BmsObservationIndex::new(count - 1))
            );
        }
    }

    #[test]
    fn bms_summary_retains_raw_history_and_ignores_three_sample_voltage_pulses() {
        let mut state = CutoutSessionState::default();
        state.observe_read_only_response(&ReadOnlyResponse::Battery(
            cell_readback(1, 0, 4_177, 1)
                .with_observation_pack(crate::BmsPackIndex::new(0), crate::BmsCellIndex::new(0)),
        ));
        state.observe_read_only_response(&ReadOnlyResponse::Battery(
            cell_readback(2, 15, 4_193, 2)
                .with_observation_pack(crate::BmsPackIndex::new(0), crate::BmsCellIndex::new(15)),
        ));
        for (observed_at_ms, voltage) in (3..6).zip([4_209, 4_209, 4_209]) {
            state.observe_read_only_response(&ReadOnlyResponse::Battery(
                cell_readback(2, 15, voltage, observed_at_ms).with_observation_pack(
                    crate::BmsPackIndex::new(0),
                    crate::BmsCellIndex::new(15),
                ),
            ));
        }

        let summary = state.telemetry.bms.observation_summary();
        assert_eq!(
            summary.voltage_spread,
            Some(crate::VoltageDelta::from_millivolts(16))
        );
        assert_eq!(summary.observations.len(), 30);
        let pulsing = &summary.observations[15];
        assert_eq!(pulsing.voltage, crate::Voltage::from_millivolts(4_193));
        assert_eq!(
            pulsing.latest_voltage,
            crate::Voltage::from_millivolts(4_209)
        );
        assert_eq!(pulsing.samples.len(), 4);
        assert_eq!(pulsing.pack_index, Some(crate::BmsPackIndex::new(0)));
        assert_eq!(
            pulsing.pack_observation_index,
            Some(crate::BmsCellIndex::new(15))
        );

        state.observe_read_only_response(&ReadOnlyResponse::Battery(
            cell_readback(2, 15, 4_209, 6)
                .with_observation_pack(crate::BmsPackIndex::new(0), crate::BmsCellIndex::new(15)),
        ));
        assert_eq!(
            state.telemetry.bms.observation_summary().voltage_spread,
            Some(crate::VoltageDelta::from_millivolts(32))
        );
    }

    #[test]
    fn bms_history_scales_with_the_observed_page_cycle_and_requires_timestamps() {
        let mut state = CutoutSessionState::default();
        state.observe_read_only_response(&ReadOnlyResponse::Battery(BatteryReadback::available(
            crate::BatteryPagePayload::cell_voltage(
                BatteryPageMetadata::cell_voltage(
                    crate::ProtocolSelector::new(1),
                    crate::VerificationStatus::HardwareVerified,
                ),
                crate::BatteryInfo::default(),
                [crate::Voltage::from_millivolts(4_000)]
                    .into_iter()
                    .collect(),
            ),
        )));
        assert!(state.telemetry.bms.observation_history.is_empty());

        for cycle in 0_u64..8 {
            for (selector, first) in [(1, 0), (2, 15), (5, 30), (6, 45)] {
                state.observe_read_only_response(&ReadOnlyResponse::Battery(cell_readback(
                    selector,
                    first,
                    4_000 + i32::try_from(cycle).unwrap(),
                    cycle * 4 + u64::from(selector),
                )));
            }
        }

        assert_eq!(state.telemetry.bms.pages.len(), 4);
        assert_eq!(state.telemetry.bms.observation_history.len(), 4 * 7);
        let summary = state.telemetry.bms.observation_summary();
        assert_eq!(summary.observed_count, 60);
        assert!(
            summary
                .observations
                .iter()
                .all(|observation| observation.samples.len() == 7)
        );
        assert_eq!(
            summary.observations[0].samples[0].observed_at,
            MonotonicTimestamp::new(5)
        );

        for observed_at_ms in 100..120 {
            state.observe_read_only_response(&ReadOnlyResponse::Battery(cell_readback(
                2,
                15,
                4_010,
                observed_at_ms,
            )));
        }
        let summary = state.telemetry.bms.observation_summary();
        assert_eq!(summary.observed_count, 60);
        assert!(
            summary
                .observations
                .iter()
                .all(|observation| observation.samples.len() == 7)
        );
    }

    #[test]
    fn duplicate_probe_observation_preserves_the_original_deadline() {
        let mut identity = DeviceIdentityState::default();
        identity.observe_probe_write(PendingProbe::BegodeName, MonotonicTimestamp::new(1_000));

        identity.observe_probe_write(PendingProbe::BegodeName, MonotonicTimestamp::new(1_500));

        assert_eq!(
            identity.next_probe_expiry(crate::Duration::from_milliseconds(2_000)),
            Some(MonotonicTimestamp::new(3_001))
        );
    }

    #[test]
    fn identity_reset_preserves_discovery_and_clears_device_evidence() {
        let mut state = CutoutSessionState::default();
        state.observe_discovery(discovery_observation(
            "peripheral-a",
            b"GotWay_002441",
            vec![BluetoothServiceUuid::EUC_SERIAL_FFE0],
            -42,
        ));
        state.select_discovered_platform("peripheral-a".to_owned());
        state.identity.protocol_family = Some(ProtocolFamily::BegodeGotway);
        state.identity.model = Some("Falcon".to_owned());
        state
            .identity
            .observe_probe_write(PendingProbe::BegodeName, MonotonicTimestamp::new(42));

        state.reset_device_identity();

        assert_eq!(state.discovery().observations.len(), 1);
        assert_eq!(
            state.discovery().selected_platform_identifier.as_deref(),
            Some("peripheral-a")
        );
        assert_eq!(state.identity().protocol_family, None);
        assert_eq!(state.identity().model, None);
        assert_eq!(
            state
                .identity()
                .next_probe_expiry(crate::Duration::from_milliseconds(2_000)),
            None
        );
    }

    #[test]
    fn advanced_capture_retains_unknowns_without_adding_default_picker_noise() {
        let mut state = CutoutSessionState::default();
        state.observe_discovery(discovery_observation("unknown", b"My PEV", vec![], -42));
        state.observe_discovery(discovery_observation(
            "known-uart",
            b"UART",
            vec![BluetoothServiceUuid::VESC_NORDIC_UART],
            -40,
        ));
        let ordinary = state.discovery().picker_candidates();
        assert_eq!(ordinary.len(), 1);
        assert_eq!(ordinary[0].platform_identifier, "known-uart");
        let advanced = state.discovery().advanced_capture_candidates();
        assert_eq!(advanced.len(), 1);
        assert_eq!(advanced[0].platform_identifier, "unknown");
        assert_eq!(
            advanced[0].support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert!(advanced[0].connection_route.is_none());
        assert!(advanced[0].electric_unicycle_model.is_none());
        assert_eq!(state.discovery().observations.len(), 2);
    }

    #[test]
    fn identity_state_retains_latest_discovery_by_platform_identifier() {
        let mut state = CutoutSessionState::default();

        state.observe_discovery(discovery_observation(
            "peripheral-b",
            b"Later stale",
            vec![BluetoothServiceUuid::EUC_SERIAL_FFE0],
            -60,
        ));
        state.observe_discovery(discovery_observation(
            "peripheral-a",
            b"Old",
            vec![BluetoothServiceUuid::EUC_SERIAL_FFE0],
            -70,
        ));
        state.observe_discovery(discovery_observation(
            "peripheral-a",
            &[b'F', b'a', b'l', b'c', b'o', b'n', 0xff],
            vec![
                BluetoothServiceUuid::EUC_SERIAL_FFE0,
                BluetoothServiceUuid::from_bluetooth16(0x180f),
            ],
            -42,
        ));

        assert_eq!(state.identity().discovery.observations.len(), 2);
        assert_eq!(
            state.identity().discovery.observations[0].platform_identifier,
            "peripheral-b"
        );
        assert_eq!(
            state.identity().discovery.observations[1].platform_identifier,
            "peripheral-a"
        );
        assert_eq!(
            state.identity().discovery.observations[1]
                .advertised_name
                .as_deref(),
            Some(&[b'F', b'a', b'l', b'c', b'o', b'n', 0xff][..])
        );
        assert_eq!(
            state.identity().discovery.observations[1].advertised_name_text(),
            None
        );
        assert_eq!(
            state.identity().discovery.observations[1].advertised_service_uuids,
            [
                BluetoothServiceUuid::EUC_SERIAL_FFE0,
                BluetoothServiceUuid::from_bluetooth16(0x180f),
            ]
        );
        assert_eq!(
            state.identity().discovery.observations[1].rssi_dbm,
            Some(-42)
        );
    }

    #[test]
    fn advertisement_transport_hints_never_establish_ride_route() {
        for service in [
            BluetoothServiceUuid::EUC_SERIAL_FFE0,
            BluetoothServiceUuid::VESC_SERIAL_FFF0,
            BluetoothServiceUuid::VESC_NORDIC_UART,
        ] {
            let mut state = CutoutSessionState::default();
            state.observe_discovery(discovery_observation(
                "unverified",
                b"Claimed rideable",
                vec![service],
                -50,
            ));
            let candidates = state.discovery().picker_candidates();
            assert_eq!(candidates.len(), 1);
            assert_eq!(
                candidates[0].support,
                DiscoveryCandidateSupport::ProbeRecommended
            );
            assert_eq!(candidates[0].connection_route, None);
            assert_eq!(candidates[0].electric_unicycle_model, None);
        }
    }

    #[test]
    fn discovery_snapshot_projects_picker_candidates_from_identity_state() {
        let mut state = CutoutSessionState::default();

        state.observe_discovery(discovery_observation(
            "falcon-id",
            b"Begode Falcon",
            vec![BluetoothServiceUuid::EUC_SERIAL_FFE0],
            -50,
        ));
        state.observe_discovery(discovery_observation(
            "vesc-id",
            b"Floatwheel",
            vec![BluetoothServiceUuid::VESC_NORDIC_UART],
            -60,
        ));
        state.observe_discovery(discovery_observation(
            "unknown-euc-id",
            b"EUC-unknown",
            vec![BluetoothServiceUuid::EUC_SERIAL_FFE0],
            -55,
        ));
        state.observe_discovery(discovery_observation(
            "unknown-id",
            b"Keyboard",
            vec![BluetoothServiceUuid::from_bluetooth16(0x180f)],
            -65,
        ));
        state.select_discovered_platform("falcon-id".to_owned());

        let discovery = state.discovery();
        let picker_candidates = discovery.picker_candidates();

        assert_eq!(
            discovery.selected_platform_identifier.as_deref(),
            Some("falcon-id")
        );
        assert_eq!(picker_candidates.len(), 3);
        assert_eq!(picker_candidates[0].platform_identifier, "falcon-id");
        assert_eq!(
            picker_candidates[0].support,
            DiscoveryCandidateSupport::ProbeRecommended
        );
        assert_eq!(picker_candidates[0].electric_unicycle_model, None);
        assert_eq!(picker_candidates[0].connection_route, None);
        assert_eq!(picker_candidates[1].platform_identifier, "vesc-id");
        assert_eq!(
            picker_candidates[1].support,
            DiscoveryCandidateSupport::ProbeRecommended
        );
        assert_eq!(picker_candidates[1].connection_route, None);
        assert_eq!(picker_candidates[2].platform_identifier, "unknown-euc-id");
        assert_eq!(
            picker_candidates[2].support,
            DiscoveryCandidateSupport::ProbeRecommended
        );
        assert_eq!(
            picker_candidates[2].detail,
            "Read-only protocol probe recommended"
        );
        assert_eq!(picker_candidates[2].connection_route, None);
        assert_eq!(picker_candidates[2].electric_unicycle_model, None);
    }
}
