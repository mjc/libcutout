//! Rust-owned session-state root and typed state slices.

use crate::{
    BatteryPageMetadata, BatteryPagePayload, BatteryReadback, DeviceEvent, FirmwareInfo,
    ParserDiagnostics, ProtocolFamily, RawTelemetryReadback, ReadOnlyResponse, SessionOutput,
    TelemetryDelta, TelemetrySnapshot,
};

/// Rust-owned durable state for one `CutOut` mobile/device session.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CutoutSessionState {
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

    pub(crate) fn observe_outputs(&mut self, outputs: &[SessionOutput]) {
        outputs
            .iter()
            .for_each(|output| self.observe_output(output));
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
            ReadOnlyResponse::RawTelemetry(readback) => self.telemetry.raw = *readback,
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
}

impl DeviceIdentityState {
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

    /// Candidate is relevant enough to capture but has no supported route yet.
    UnknownRecordable,

    /// Candidate category is known, but no route exists yet.
    KnownUnsupported,

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
                    support: DiscoveryCandidateSupport::Supported,
                    electric_unicycle_model: Some(model),
                },
                None => Self {
                    platform_identifier: observation.platform_identifier.clone(),
                    display_name: display_name.to_owned(),
                    product_category: "Electric unicycle".to_owned(),
                    evidence: "FFE0/FFE1 transport hint".to_owned(),
                    detail: "Model not confirmed".to_owned(),
                    support: DiscoveryCandidateSupport::UnknownRecordable,
                    electric_unicycle_model: None,
                },
            }),
            (false, true) => Some(Self {
                platform_identifier: observation.platform_identifier.clone(),
                display_name: display_name.to_owned(),
                product_category: "VESC Onewheel".to_owned(),
                evidence: "VESC advertisement hint".to_owned(),
                detail: "Not yet supported".to_owned(),
                support: DiscoveryCandidateSupport::KnownUnsupported,
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
            DiscoveryCandidateSupport::Supported
        );
        assert_eq!(
            picker_candidates[0].electric_unicycle_model,
            Some(DiscoveryElectricUnicycleModel::Falcon)
        );
        assert_eq!(picker_candidates[1].platform_identifier, "vesc-id");
        assert_eq!(
            picker_candidates[1].support,
            DiscoveryCandidateSupport::KnownUnsupported
        );
        assert_eq!(picker_candidates[2].platform_identifier, "unknown-euc-id");
        assert_eq!(
            picker_candidates[2].support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(picker_candidates[2].detail, "Model not confirmed");
        assert_eq!(picker_candidates[2].electric_unicycle_model, None);
    }
}
