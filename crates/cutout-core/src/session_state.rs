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

    /// Firmware or protocol version readback, when reported.
    pub firmware: Option<FirmwareInfo>,
}

impl DeviceIdentityState {
    fn apply_update(&mut self, update: DeviceIdentityUpdate) {
        self.protocol_family = self.protocol_family.or(update.protocol_family);
        self.model = self.model.take().or(update.model);
        self.firmware = self.firmware.or(update.firmware);
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
}
