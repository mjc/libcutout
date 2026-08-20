//! Rust-owned session-state root and typed state slices.

use crate::{
    BatteryPageMetadata, BatteryPagePayload, BatteryReadback, DeviceEvent, FirmwareInfo,
    GattFingerprint, MonotonicTimestamp, ParserDiagnostics, ProtocolFamily, RawTelemetryReadback,
    ReadOnlyResponse, RideSessionLifecycle, SessionOutput, TelemetryDelta, TelemetrySnapshot,
};
use arrayvec::ArrayVec;
use bytes::Bytes;

/// Rust-owned durable state for one `CutOut` mobile/device session.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CutoutSessionState {
    /// Logical ride and Live Activity lifecycle state.
    pub ride_session: RideSessionLifecycle,

    /// Device identity state accumulated from discovery, protocol, and model evidence.
    pub identity: DeviceIdentityState,

    /// Telemetry state accumulated from ride, charge, raw telemetry, and BMS packets.
    pub telemetry: TelemetryState,

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
}

/// Discovery facts observed for one platform peripheral.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryObservation {
    /// Stable platform identifier supplied by the mobile BLE stack.
    pub platform_identifier: String,

    /// Raw advertised-name bytes.
    pub advertised_name: Option<Vec<u8>>,

    /// Advertised 16-bit service UUID values relevant to picker routing.
    pub advertised_service_uuids: Vec<u16>,

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

    /// Candidate can use a read-only test route but is not confirmed identity.
    ProvisionalRoute,

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
    fn from_observation(observation: &DiscoveryObservation) -> Option<Self> {
        let display_name = observation
            .advertised_name_text()
            .unwrap_or("Unknown Bluetooth device");
        let lower_name = display_name.to_ascii_lowercase();

        match (
            observation.advertised_service_uuids.contains(&0xffe0),
            observation.advertised_service_uuids.contains(&0xfff0)
                || lower_name.contains("vesc")
                || lower_name.contains("focer")
                || lower_name.contains("onewheel")
                || lower_name.contains("floatwheel"),
        ) {
            (true, _) => Some(match discovery_electric_unicycle_model(&lower_name) {
                Some(model) => Self {
                    platform_identifier: observation.platform_identifier.clone(),
                    display_name: display_name.to_owned(),
                    product_category: "Electric unicycle".to_owned(),
                    evidence: "advertisement hint".to_owned(),
                    detail: discovery_electric_unicycle_detail(model).to_owned(),
                    support: DiscoveryCandidateSupport::ProvisionalRoute,
                    connection_route: Some(DiscoveryConnectionRoute::ElectricUnicycle),
                    electric_unicycle_model: Some(model),
                },
                None => Self {
                    platform_identifier: observation.platform_identifier.clone(),
                    display_name: display_name.to_owned(),
                    product_category: "Electric unicycle".to_owned(),
                    evidence: "FFE0/FFE1 transport hint".to_owned(),
                    detail: "Read-only probe recommended".to_owned(),
                    support: DiscoveryCandidateSupport::ProbeRecommended,
                    connection_route: None,
                    electric_unicycle_model: None,
                },
            }),
            (false, true) => Some(Self {
                platform_identifier: observation.platform_identifier.clone(),
                display_name: display_name.to_owned(),
                product_category: "VESC Onewheel".to_owned(),
                evidence: "VESC advertisement hint".to_owned(),
                detail: "VESC read-only route".to_owned(),
                support: DiscoveryCandidateSupport::ProvisionalRoute,
                connection_route: Some(DiscoveryConnectionRoute::VescOnewheel),
                electric_unicycle_model: None,
            }),
            (false, false) => None,
        }
    }
}

fn discovery_electric_unicycle_model(lower_name: &str) -> Option<DiscoveryElectricUnicycleModel> {
    match lower_name {
        name if ["falcon", "begode", "gotway"]
            .into_iter()
            .any(|needle| name.contains(needle)) =>
        {
            Some(DiscoveryElectricUnicycleModel::Falcon)
        }
        name if ["aero", "nosfet", "veteran"]
            .into_iter()
            .any(|needle| name.contains(needle))
            || name.starts_with("nf") =>
        {
            Some(DiscoveryElectricUnicycleModel::Aero)
        }
        _ => None,
    }
}

fn discovery_electric_unicycle_detail(model: DiscoveryElectricUnicycleModel) -> &'static str {
    match model {
        DiscoveryElectricUnicycleModel::Falcon => "Falcon provisional route",
        DiscoveryElectricUnicycleModel::Aero => "Aero provisional route",
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

    /// Latest page payload for each observed BMS page identity.
    pub pages: Vec<BatteryPagePayload>,
}

impl BmsTelemetryState {
    fn observe_readback(&mut self, readback: &BatteryReadback) {
        self.latest = readback.clone();
        readback
            .page()
            .into_iter()
            .for_each(|page| self.observe_page(page));
    }

    fn observe_page(&mut self, page: &BatteryPagePayload) {
        let identity = page.page();
        self.pages
            .retain(|existing| !same_bms_page(existing.page(), identity));
        self.pages.push(page.clone());
    }
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

    fn discovery_observation(
        platform_identifier: &str,
        name: &[u8],
        services: Vec<u16>,
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
            vec![0xffe0],
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
    fn identity_state_retains_latest_discovery_by_platform_identifier() {
        let mut state = CutoutSessionState::default();

        state.observe_discovery(discovery_observation(
            "peripheral-b",
            b"Later stale",
            vec![0xffe0],
            -60,
        ));
        state.observe_discovery(discovery_observation(
            "peripheral-a",
            b"Old",
            vec![0xffe0],
            -70,
        ));
        state.observe_discovery(discovery_observation(
            "peripheral-a",
            &[b'F', b'a', b'l', b'c', b'o', b'n', 0xff],
            vec![0xffe0, 0x180f],
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
            [0xffe0, 0x180f]
        );
        assert_eq!(
            state.identity().discovery.observations[1].rssi_dbm,
            Some(-42)
        );
    }

    #[test]
    fn discovery_snapshot_projects_picker_candidates_from_identity_state() {
        let mut state = CutoutSessionState::default();

        state.observe_discovery(discovery_observation(
            "falcon-id",
            b"Begode Falcon",
            vec![0xffe0],
            -50,
        ));
        state.observe_discovery(discovery_observation(
            "vesc-id",
            b"Floatwheel",
            vec![0xfff0],
            -60,
        ));
        state.observe_discovery(discovery_observation(
            "unknown-euc-id",
            b"EUC-unknown",
            vec![0xffe0],
            -55,
        ));
        state.observe_discovery(discovery_observation(
            "unknown-id",
            b"Keyboard",
            vec![0x180f],
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
            DiscoveryCandidateSupport::ProvisionalRoute
        );
        assert_eq!(
            picker_candidates[0].electric_unicycle_model,
            Some(DiscoveryElectricUnicycleModel::Falcon)
        );
        assert_eq!(
            picker_candidates[0].connection_route,
            Some(DiscoveryConnectionRoute::ElectricUnicycle)
        );
        assert_eq!(picker_candidates[1].platform_identifier, "vesc-id");
        assert_eq!(
            picker_candidates[1].support,
            DiscoveryCandidateSupport::ProvisionalRoute
        );
        assert_eq!(
            picker_candidates[1].connection_route,
            Some(DiscoveryConnectionRoute::VescOnewheel)
        );
        assert_eq!(picker_candidates[2].platform_identifier, "unknown-euc-id");
        assert_eq!(
            picker_candidates[2].support,
            DiscoveryCandidateSupport::ProbeRecommended
        );
        assert_eq!(picker_candidates[2].detail, "Read-only probe recommended");
        assert_eq!(picker_candidates[2].connection_route, None);
        assert_eq!(picker_candidates[2].electric_unicycle_model, None);
    }
}
