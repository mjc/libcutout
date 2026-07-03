use std::{
    fmt, fs,
    future::{Future, poll_fn},
    marker::PhantomData,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use cutout_btle::{
    BridgeIdentityResolution, BtleError, BtleplugReconnectHost, ConnectedPeripheral,
    ConnectionTarget, DiagnosticEventCount, DisconnectCount, MonotonicMs, NotificationCount,
    NotificationPayloadTotal, NotificationWindow, PevcapSessionMetadata, ProtocolWriteCount,
    RawNotificationRecord, ReadOnlyResponseCount, ReconnectAttemptReport, ScanWindow,
    SessionBridgeEvent, SessionBridgeReport, SessionCapture, SessionCaptureRecord,
    SessionEndpoints, SessionPeripheral, SubscribeCount, TelemetryEventCount, TransportWriteCount,
    WriteProvenance, capture_raw_notifications, capture_reconnecting_session_with_commands,
    capture_session_with_commands, connect_and_discover, drive_session,
    drive_session_with_commands, read_battery_level, scan_peripherals,
};
use cutout_core::{
    BatteryPageKind, BatteryPagePayload, BatteryReadback, BatteryReadbackAvailability,
    CaptureDistribution, CaptureEvidence, CapturePrivacy, CaptureSessionLabel,
    CatalogModelResolution, CommandKind, DeviceCommand, DeviceEvent, DiagnosticError,
    DiagnosticErrorKind, DiagnosticSnapshot, FirmwareInfo, GattChannel, HostSession, Measured,
    ModelCatalog, MonotonicTimestamp, NotificationByteLen, ParserDiagnostics, PevcapCapture,
    PevcapDirection, PevcapEncoding, PevcapHeader, PevcapRecord, PevcapResolvedIdentity,
    ProtocolFamily, ReadOnlyResponse, ReplayChunkComparison, SessionKey, SessionOutput,
    SettingsReadback, SettingsReadbackAvailability, TelemetrySnapshot, TransportWriteLimit,
    ValueQuality, ValueSource, VerificationStatus, VerifiedValue, WallClockUnixTimestamp,
};
#[cfg(test)]
use cutout_protocols::VETERAN_DATA_CHANNEL;
use cutout_protocols::{
    BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_FALCON_SESSION_KEY, BegodeBmsSummary,
    BegodeCapacityEvidence, BegodeCapacitySelection, BegodeFrameParseResult,
    BegodeFrameReassembler, BegodePackEvidenceConsistency, BegodePackLayoutEvidence,
    BegodePackLayoutSelection, BegodeVoltageEvidence, BegodeVoltageProfileSelection, MODEL_CATALOG,
    NOSFET_AERO_SESSION_KEY, RegisteredReadOnlySession,
    begode_falcon_read_only_session_with_voltage_profile, find_session_registration,
    select_begode_pack_capacity_from_annotations, select_begode_pack_layout_from_annotations,
    select_begode_pack_voltage_profile, select_begode_pack_voltage_profile_from_annotations,
    validate_begode_pack_evidence,
};
use tracing::{debug, info};

use crate::cli::{
    CaptureArgs, Cli, Command, DashboardArgs, PevcapArgs, PevcapCommand, PevcapConvertArgs,
    PevcapFormat, RawSubscribeArgs, ReadProbe, SessionProfile, TargetedScanArgs,
};
use crate::dashboard::{
    DashboardCaptureProvenance, DashboardState, DashboardUpdate, firmware_summary_string,
    run_dashboard_with_updates,
};
use crate::logging::install_dashboard_log_sink;
use crate::validation::render_validation_report;

const DASHBOARD_LIVE_WINDOW: Duration = Duration::from_millis(500);
const DASHBOARD_BATTERY_REFRESH_EVERY: u64 = 10;

/// Executes a parsed CLI invocation.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error when scanning, connecting,
/// discovery, or protocol session bridging fails.
pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Scan(args) => scan(args.seconds()).await?,
        Command::Connect(args) => connect(args, SessionMode::Drive).await?,
        Command::SubscribeRaw(args) => subscribe_raw(args).await?,
        Command::Capture(args) => capture(args).await?,
        Command::Validation => print!("{}", render_validation_report()),
        Command::Pevcap(args) => pevcap(args)?,
        Command::Dashboard(args) => dashboard(args).await?,
    }

    Ok(())
}

fn pevcap(args: PevcapArgs) -> Result<()> {
    match args.command {
        PevcapCommand::Convert(args) => pevcap_convert(&args)?,
        PevcapCommand::Replay(args) => pevcap_replay(&args)?,
    }

    Ok(())
}

fn pevcap_convert(args: &PevcapConvertArgs) -> Result<()> {
    let input = fs::read(&args.input)?;
    let output = convert_pevcap_bytes(&input, args.input_format, args.output_format)?;
    fs::write(&args.output, output)?;
    info!(
        input = %args.input.display(),
        input_format = ?args.input_format,
        output = %args.output.display(),
        output_format = ?args.output_format,
        "converted pevcap"
    );
    Ok(())
}

fn convert_pevcap_bytes(
    input: &[u8],
    input_format: PevcapFormat,
    output_format: PevcapFormat,
) -> Result<Vec<u8>> {
    let capture = PevcapCapture::decode(input, pevcap_encoding(input_format));
    Ok(capture?.encode(pevcap_encoding(output_format))?)
}

#[cfg(test)]
fn speed(value: i32) -> Measured<cutout_core::Speed> {
    Measured::reported(cutout_core::Speed::from_millimetres_per_second(value))
}

#[cfg(test)]
fn voltage(value: i32) -> Measured<cutout_core::Voltage> {
    Measured::reported(cutout_core::Voltage::from_millivolts(value))
}

#[cfg(test)]
fn battery_current(value: i32) -> Measured<cutout_core::BatteryCurrent> {
    Measured::reported(cutout_core::BatteryCurrent::from_milliamps(value))
}

#[cfg(test)]
fn temperature(value: i32) -> Measured<cutout_core::Temperature> {
    Measured::reported(cutout_core::Temperature::from_millicelsius(value))
}

#[cfg(test)]
fn power(value: i64) -> Measured<cutout_core::Power> {
    Measured::calculated(cutout_core::Power::from_milliwatts(value))
}

#[cfg(test)]
fn duty_cycle_permille(value: i16) -> Measured<cutout_core::DutyCycle> {
    Measured::reported(cutout_core::DutyCycle::from_permille(value))
}

#[cfg(test)]
fn distance(value: u64) -> Measured<cutout_core::Distance> {
    Measured::reported(cutout_core::Distance::from_millimetres(value))
}

#[cfg(test)]
fn angle_mdeg(value: i32) -> Measured<cutout_core::Angle> {
    Measured::reported(cutout_core::Angle::from_millidegrees(value))
}

#[cfg(test)]
fn level_estimated(value: u8) -> Measured<cutout_core::BatteryLevel> {
    Measured::estimated(cutout_core::BatteryLevel::from_percent(value))
}

const fn pevcap_encoding(format: PevcapFormat) -> PevcapEncoding {
    match format {
        PevcapFormat::Jsonl => PevcapEncoding::Jsonl,
        PevcapFormat::Binary => PevcapEncoding::Binary,
    }
}

fn pevcap_replay(args: &crate::cli::PevcapReplayArgs) -> Result<()> {
    let input = fs::read(&args.input)?;
    let capture = PevcapCapture::decode(&input, pevcap_encoding(args.input_format))?;
    let report = replay_pevcap_capture(
        &capture,
        selected_pevcap_replay_profile(&capture, args.profile)?,
    )?;
    info!("{}", render_pevcap_replay_report(&report));
    if args.read_only_jsonl {
        for line in render_read_only_responses_jsonl(&report.read_only_response_events) {
            info!("{}", line?);
        }
    }
    if args.diagnostics_jsonl {
        for line in render_diagnostic_snapshots_jsonl(&report.diagnostic_snapshots) {
            info!("{}", line?);
        }
        for line in render_diagnostic_errors_jsonl(&report.diagnostic_errors) {
            info!("{}", line?);
        }
    }
    if let Some(telemetry) = render_telemetry_snapshot(&report.telemetry_snapshot) {
        info!("{telemetry}");
    }
    if let Some(firmware) = render_firmware_info(report.firmware) {
        info!("{firmware}");
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PevcapReplayReport {
    replay_records: ReplayRecordCount,
    outputs: ReplayOutputCount,
    telemetry: ReplayTelemetryCount,
    read_only_responses: ReplayReadOnlyResponseCount,
    diagnostics: ReplayDiagnosticCount,
    arbitrary_chunk_plan_len: ReplayChunkPlanLen,
    chunk_one_byte_matches: bool,
    chunk_arbitrary_matches: bool,
    telemetry_snapshot: TelemetrySnapshot,
    firmware: Option<FirmwareInfo>,
    capacity: BegodeCapacitySelection,
    layout: BegodePackLayoutSelection,
    pack_evidence_consistency: Option<BegodePackEvidenceConsistency>,
    diagnostic_snapshots: Vec<DiagnosticSnapshot>,
    diagnostic_errors: Vec<DiagnosticError>,
    read_only_response_events: Vec<ReadOnlyResponse>,
    events: Vec<SessionBridgeEvent>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReplayRecordCountTag;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReplayOutputCountTag;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReplayTelemetryCountTag;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReplayReadOnlyResponseCountTag;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReplayDiagnosticCountTag;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReplayChunkPlanLenTag;

type ReplayRecordCount = ReplayCount<ReplayRecordCountTag>;
type ReplayOutputCount = ReplayCount<ReplayOutputCountTag>;
type ReplayTelemetryCount = ReplayCount<ReplayTelemetryCountTag>;
type ReplayReadOnlyResponseCount = ReplayCount<ReplayReadOnlyResponseCountTag>;
type ReplayDiagnosticCount = ReplayCount<ReplayDiagnosticCountTag>;
type ReplayChunkPlanLen = ReplayCount<ReplayChunkPlanLenTag>;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct JsonSequence(usize);

impl JsonSequence {
    const fn new(value: usize) -> Self {
        Self(value)
    }

    const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReplayCount<Tag> {
    value: usize,
    tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Default for ReplayCount<Tag> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<Tag> ReplayCount<Tag> {
    const fn new(value: usize) -> Self {
        Self {
            value,
            tag: PhantomData,
        }
    }

    const fn get(self) -> usize {
        self.value
    }

    const fn increment(self) -> Self {
        Self::new(self.value.saturating_add(1))
    }
}

impl<Tag> fmt::Display for ReplayCount<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

fn replay_pevcap_capture(
    capture: &PevcapCapture,
    profile: SelectedSessionProfile,
) -> Result<PevcapReplayReport> {
    let mut report = if profile.is_falcon() {
        replay_pevcap_with_session(capture, falcon_replay_session(capture)?)?
    } else {
        replay_pevcap_with_session(capture, profile.session_registration()?.construct())?
    };
    report.capacity =
        select_begode_pack_capacity_from_annotations(capture.header.annotations.iter());
    report.layout = select_begode_pack_layout_from_annotations(capture.header.annotations.iter());
    if profile.is_falcon() {
        report.pack_evidence_consistency = Some(validate_begode_pack_evidence(
            select_falcon_replay_voltage_profile(capture),
            report.capacity,
            report.layout,
        ));
    }
    Ok(report)
}

fn falcon_replay_session(capture: &PevcapCapture) -> Result<RegisteredReadOnlySession> {
    match select_falcon_replay_voltage_profile(capture) {
        BegodeVoltageProfileSelection::Selected(profile) => Ok(
            begode_falcon_read_only_session_with_voltage_profile(profile),
        ),
        BegodeVoltageProfileSelection::Missing => {
            bail!("Falcon PEVCAP replay requires explicit Falcon battery voltage evidence")
        }
        BegodeVoltageProfileSelection::Conflicting => {
            bail!("Falcon PEVCAP replay has conflicting Falcon battery voltage evidence")
        }
    }
}

fn select_falcon_replay_voltage_profile(capture: &PevcapCapture) -> BegodeVoltageProfileSelection {
    let bms_evidence = falcon_replay_bms_voltage_evidence(capture);
    match select_begode_pack_voltage_profile_from_annotations(capture.header.annotations.iter()) {
        BegodeVoltageProfileSelection::Conflicting => BegodeVoltageProfileSelection::Conflicting,
        BegodeVoltageProfileSelection::Selected(profile) => select_begode_pack_voltage_profile(
            core::iter::once(profile_evidence(profile)).chain(bms_evidence),
        ),
        BegodeVoltageProfileSelection::Missing => select_begode_pack_voltage_profile(bms_evidence),
    }
}

fn profile_evidence(profile: cutout_protocols::BegodePackVoltageProfile) -> BegodeVoltageEvidence {
    match profile {
        cutout_protocols::BegodePackVoltageProfile::Begode84VFullCharge => {
            BegodeVoltageEvidence::VoltageClass84V
        }
        cutout_protocols::BegodePackVoltageProfile::Begode100VFullCharge => {
            BegodeVoltageEvidence::VoltageClass100V
        }
    }
}

fn falcon_replay_bms_voltage_evidence(capture: &PevcapCapture) -> Vec<BegodeVoltageEvidence> {
    falcon_bms_voltage_evidence_from_records(capture.records.iter().filter_map(|record| {
        if record.direction == PevcapDirection::Inbound
            && record.characteristic == BEGODE_DATA_CHANNEL
        {
            Some((record.bytes.as_ref(), record.monotonic_ms))
        } else {
            None
        }
    }))
}

fn falcon_capture_bms_voltage_evidence(capture: &SessionCapture) -> Vec<BegodeVoltageEvidence> {
    falcon_bms_voltage_evidence_from_records(capture.records.iter().filter_map(|record| {
        let SessionCaptureRecord::Notification {
            monotonic_ms,
            characteristic,
            bytes,
            ..
        } = record
        else {
            return None;
        };
        if GattChannel::from_uuid(*characteristic) == BEGODE_DATA_CHANNEL {
            Some((bytes.as_raw_bytes(), monotonic_ms.into_core()))
        } else {
            None
        }
    }))
}

fn falcon_bms_voltage_evidence_from_records<'a>(
    records: impl IntoIterator<Item = (&'a [u8], MonotonicTimestamp)>,
) -> Vec<BegodeVoltageEvidence> {
    let mut reassembler = BegodeFrameReassembler::default();
    let mut evidence = Vec::new();
    for (bytes, monotonic_ms) in records {
        for byte in bytes {
            let Ok(BegodeFrameParseResult::Complete(frame)) =
                reassembler.feed_byte_result_at(*byte, monotonic_ms)
            else {
                continue;
            };
            if let Ok(summary) = BegodeBmsSummary::decode(&frame) {
                evidence.push(BegodeVoltageEvidence::ObservedPackVoltage(
                    summary.pack_voltage,
                ));
            }
        }
    }
    evidence
}

fn replay_pevcap_with_session<S>(capture: &PevcapCapture, session: S) -> Result<PevcapReplayReport>
where
    S: Clone + cutout_core::ProtocolSession,
{
    let comparison_session = session.clone();
    let mut host = HostSession::new(session);
    let mut outputs = Vec::with_capacity(capture.replay_input_count());
    capture.replay_into_host(&mut host, &mut outputs);

    let replay_records = ReplayRecordCount::new(capture.replay_input_count());
    let arbitrary_chunks = capture.arbitrary_notification_chunk_lengths();
    let arbitrary_chunk_plan_len = ReplayChunkPlanLen::new(arbitrary_chunks.len());
    let comparison = capture
        .compare_replay_chunks(|| comparison_session.clone(), &arbitrary_chunks)
        .with_context(|| {
            format!(
                "PEVCAP replay chunk comparison failed \
                 replay_records={} arbitrary_chunk_plan_len={}; \
                 inspect capture chunking and decoder output retention",
                replay_records.get(),
                arbitrary_chunk_plan_len.get()
            )
        })?;

    Ok(summarize_pevcap_replay(
        replay_records,
        arbitrary_chunk_plan_len,
        &outputs,
        comparison,
    ))
}

fn summarize_pevcap_replay(
    replay_records: ReplayRecordCount,
    arbitrary_chunk_plan_len: ReplayChunkPlanLen,
    outputs: &[SessionOutput],
    chunk_comparison: ReplayChunkComparison,
) -> PevcapReplayReport {
    let mut report = PevcapReplayReport {
        replay_records,
        outputs: ReplayOutputCount::new(outputs.len()),
        telemetry: ReplayTelemetryCount::default(),
        read_only_responses: ReplayReadOnlyResponseCount::default(),
        diagnostics: ReplayDiagnosticCount::default(),
        arbitrary_chunk_plan_len,
        chunk_one_byte_matches: chunk_comparison.one_byte_matches,
        chunk_arbitrary_matches: chunk_comparison.arbitrary_matches,
        telemetry_snapshot: TelemetrySnapshot::default(),
        firmware: None,
        capacity: BegodeCapacitySelection::Missing,
        layout: BegodePackLayoutSelection::Missing,
        pack_evidence_consistency: None,
        diagnostic_snapshots: Vec::new(),
        diagnostic_errors: Vec::new(),
        read_only_response_events: Vec::new(),
        events: Vec::new(),
    };

    for output in outputs {
        match output {
            SessionOutput::Event(DeviceEvent::Telemetry(delta)) => {
                report.telemetry = report.telemetry.increment();
                report.telemetry_snapshot.apply_delta(*delta);
            }
            SessionOutput::Event(DeviceEvent::ReadOnlyResponse(response)) => {
                report.read_only_responses = report.read_only_responses.increment();
                report.read_only_response_events.push(*response);
                if let ReadOnlyResponse::Firmware(firmware) = response {
                    report.firmware = Some(*firmware);
                }
            }
            SessionOutput::Event(DeviceEvent::Diagnostics(diagnostics)) => {
                report.diagnostics = report.diagnostics.increment();
                report
                    .diagnostic_snapshots
                    .push(DiagnosticSnapshot::from_parser_diagnostics(*diagnostics));
            }
            SessionOutput::Event(DeviceEvent::DiagnosticError(error)) => {
                report.diagnostic_errors.push(*error);
            }
            SessionOutput::NotificationIngest(outcome) => {
                report.events.push(SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: notification_ingest_monotonic_ms(outcome),
                    outcome: outcome.clone(),
                });
            }
            SessionOutput::Transport(_)
            | SessionOutput::Event(
                DeviceEvent::LinkUp(_)
                | DeviceEvent::LinkDown
                | DeviceEvent::Tick { .. }
                | DeviceEvent::ControlRefusal(_),
            ) => {}
        }
    }

    report
}

fn notification_ingest_monotonic_ms(
    outcome: &cutout_core::NotificationIngestOutcome,
) -> MonotonicMs {
    MonotonicMs::new(match outcome {
        cutout_core::NotificationIngestOutcome::SemanticEvents { notification, .. }
        | cutout_core::NotificationIngestOutcome::ParserDiagnostic { notification, .. }
        | cutout_core::NotificationIngestOutcome::KnownReserved { notification, .. }
        | cutout_core::NotificationIngestOutcome::ParserGap { notification, .. }
        | cutout_core::NotificationIngestOutcome::BufferedFragment(notification) => {
            notification.monotonic_ms.get()
        }
        cutout_core::NotificationIngestOutcome::Ignored { evidence, .. } => {
            evidence.monotonic_ms.get()
        }
    })
}

fn dashboard_state_from_aero_pevcap(capture: &PevcapCapture) -> Result<DashboardState> {
    let report = replay_pevcap_capture(capture, selected_aero_session_profile())?;
    if !(report.chunk_one_byte_matches && report.chunk_arbitrary_matches) {
        bail!("Aero PEVCAP replay chunks did not produce equivalent dashboard state");
    }

    let mut state = DashboardState::empty();
    state.provenance = Some(pevcap_dashboard_provenance(capture));
    state
        .device
        .identifier
        .clone_from(&capture.header.platform_id);
    "replayed".clone_into(&mut state.device.connection_state);
    if let Some(identity) = &capture.header.resolved_identity {
        if let Some(model) = &identity.model {
            state.device.model.clone_from(&model.value);
        }
        if let Some(firmware) = &identity.firmware {
            state.device.firmware.clone_from(&firmware.value);
        }
        if matches!(
            identity.protocol_family,
            Some(ProtocolFamily::VeteranLeaperkimNosfet)
        ) {
            "NOSFET".clone_into(&mut state.device.make);
        }
    }

    state.apply_session_report(&SessionBridgeReport {
        protocol_writes: ProtocolWriteCount::default(),
        writes: TransportWriteCount::default(),
        subscribes: SubscribeCount::default(),
        notifications: replay_notification_count(capture),
        notification_bytes: replay_notification_bytes(capture),
        latest_notification_len: latest_replay_notification_len(capture),
        telemetry: TelemetryEventCount::from_events(report.telemetry.get()),
        telemetry_snapshot: report.telemetry_snapshot,
        read_only_responses: ReadOnlyResponseCount::from_events(report.read_only_responses.get()),
        read_only_response_events: report.read_only_response_events,
        firmware: report.firmware,
        settings: Vec::new(),
        diagnostics: DiagnosticEventCount::from_events(report.diagnostics.get()),
        diagnostics_snapshot: ParserDiagnostics::default(),
        diagnostic_errors: report.diagnostic_errors,
        identity: None,
        events: report.events,
        disconnects: DisconnectCount::default(),
    });
    if state.device.firmware == "unknown" {
        if let Some(firmware) = state.read_only.firmware {
            state.device.firmware = firmware_summary_string(firmware);
        }
    }
    state.capture_provenance = Some(DashboardCaptureProvenance::from_pevcap_header(
        &capture.header,
        &state.device.model,
        &state.device.firmware,
    ));
    Ok(state)
}

fn pevcap_dashboard_provenance(capture: &PevcapCapture) -> String {
    if capture.header.annotations.is_empty() {
        return format!("pevcap replay platform={}", capture.header.platform_id);
    }

    format!(
        "pevcap replay platform={} annotations={}",
        capture.header.platform_id,
        AnnotationList(capture.header.annotations.as_slice())
    )
}

struct AnnotationList<'annotations>(&'annotations [String]);

impl fmt::Display for AnnotationList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut annotations = self.0.iter();
        let Some(first) = annotations.next() else {
            return Ok(());
        };
        f.write_str(first)?;
        for annotation in annotations {
            f.write_str(",")?;
            f.write_str(annotation)?;
        }
        Ok(())
    }
}

struct OptionalUuid(Option<uuid::Uuid>);

impl fmt::Display for OptionalUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(uuid) => uuid.fmt(f),
            None => f.write_str("none"),
        }
    }
}

fn replay_notification_count(capture: &PevcapCapture) -> NotificationCount {
    capture
        .records
        .iter()
        .filter(|record| record.direction == PevcapDirection::Inbound)
        .fold(NotificationCount::default(), |count, _| count.increment())
}

fn replay_notification_bytes(capture: &PevcapCapture) -> NotificationPayloadTotal {
    capture
        .records
        .iter()
        .filter(|record| record.direction == PevcapDirection::Inbound)
        .map(|record| NotificationByteLen::from_bytes(record.bytes.len()))
        .fold(NotificationPayloadTotal::default(), |total, len| {
            total.saturating_add(NotificationPayloadTotal::from_bytes(len.as_bytes()))
        })
}

fn latest_replay_notification_len(capture: &PevcapCapture) -> Option<NotificationByteLen> {
    capture
        .records
        .iter()
        .rev()
        .find(|record| record.direction == PevcapDirection::Inbound)
        .map(|record| NotificationByteLen::from_bytes(record.bytes.len()))
}

fn render_diagnostic_snapshots_jsonl(
    snapshots: &[DiagnosticSnapshot],
) -> impl Iterator<Item = Result<String, serde_json::Error>> + '_ {
    snapshots.iter().enumerate().map(|(sequence, snapshot)| {
        render_diagnostic_snapshot_jsonl(JsonSequence::new(sequence), *snapshot)
    })
}

fn render_diagnostic_errors_jsonl(
    errors: &[DiagnosticError],
) -> impl Iterator<Item = Result<String, serde_json::Error>> + '_ {
    errors
        .iter()
        .enumerate()
        .map(|(sequence, error)| render_diagnostic_error_jsonl(JsonSequence::new(sequence), *error))
}

fn render_diagnostic_snapshot_jsonl(
    sequence: JsonSequence,
    snapshot: DiagnosticSnapshot,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&serde_json::json!({
        "type": "diagnostic_snapshot",
        "sequence": sequence.get(),
        "dropped_bytes": snapshot.dropped_bytes.as_bytes(),
        "resyncs": snapshot.resyncs.as_events(),
        "bad_checksums": snapshot.bad_checksums.as_events(),
        "timeouts": snapshot.timeouts.as_events(),
        "oversized_frames": snapshot.oversized_frames.as_events(),
        "malformed_frames": snapshot.malformed_frames.as_events(),
        "unmatched_replies": snapshot.unmatched_replies.as_events(),
    }))
}

fn render_diagnostic_error_jsonl(
    sequence: JsonSequence,
    error: DiagnosticError,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&serde_json::json!({
        "type": "diagnostic_error",
        "sequence": sequence.get(),
        "kind": diagnostic_error_kind_name(error.kind),
        "claimed_len": error.claimed_len.map(cutout_core::ParserFrameLen::get),
        "max_len": error.max_len.map(cutout_core::ParserFrameLen::get),
        "elapsed_ms": error.elapsed_ms.map(MonotonicTimestamp::get),
        "timeout_ms": error.timeout_ms.map(MonotonicTimestamp::get),
    }))
}

const fn diagnostic_error_kind_name(kind: DiagnosticErrorKind) -> &'static str {
    match kind {
        DiagnosticErrorKind::OversizedFrame => "oversized_frame",
        DiagnosticErrorKind::BadChecksum => "bad_checksum",
        DiagnosticErrorKind::MalformedFrame => "malformed_frame",
        DiagnosticErrorKind::Timeout => "timeout",
        DiagnosticErrorKind::UnmatchedReply => "unmatched_reply",
    }
}

fn render_pevcap_replay_report(report: &PevcapReplayReport) -> PevcapReplayReportLine<'_> {
    PevcapReplayReportLine(report)
}

struct PevcapReplayReportLine<'a>(&'a PevcapReplayReport);

impl fmt::Display for PevcapReplayReportLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let report = self.0;
        write!(
            f,
            "pevcap replay records={} outputs={} telemetry={} read_only_responses={} diagnostics={} arbitrary_chunk_plan_len={} chunk_one_byte_matches={} chunk_arbitrary_matches={}",
            report.replay_records,
            report.outputs,
            report.telemetry,
            report.read_only_responses,
            report.diagnostics,
            report.arbitrary_chunk_plan_len,
            report.chunk_one_byte_matches,
            report.chunk_arbitrary_matches
        )?;
        write_capacity_evidence(f, report.capacity)?;
        write_layout_evidence(f, report.layout)?;
        write_pack_evidence_consistency(f, report.pack_evidence_consistency)
    }
}

fn write_pack_evidence_consistency(
    output: &mut fmt::Formatter<'_>,
    consistency: Option<BegodePackEvidenceConsistency>,
) -> fmt::Result {
    match consistency {
        None | Some(BegodePackEvidenceConsistency::Consistent) => Ok(()),
        Some(BegodePackEvidenceConsistency::Incomplete) => {
            output.write_str(" pack_evidence_incomplete=true")
        }
        Some(BegodePackEvidenceConsistency::Inconsistent) => {
            output.write_str(" pack_evidence_inconsistent=true")
        }
    }
}

fn write_capacity_evidence(
    output: &mut fmt::Formatter<'_>,
    capacity: BegodeCapacitySelection,
) -> fmt::Result {
    match capacity {
        BegodeCapacitySelection::Missing => Ok(()),
        BegodeCapacitySelection::Conflicting => output.write_str(" capacity_conflict=true"),
        BegodeCapacitySelection::Selected(evidence) => {
            write_selected_capacity_evidence(output, evidence)
        }
    }
}

fn write_selected_capacity_evidence(
    output: &mut fmt::Formatter<'_>,
    evidence: BegodeCapacityEvidence,
) -> fmt::Result {
    if let Some(nominal_capacity) = evidence.nominal_capacity {
        write!(
            output,
            " capacity_nominal_mah={}",
            nominal_capacity.as_milliamp_hours()
        )?;
    }
    if let Some(reported_energy) = evidence.reported_energy {
        write!(
            output,
            " capacity_reported_wh={}",
            reported_energy.as_watt_hours()
        )?;
    }
    Ok(())
}

fn write_layout_evidence(
    output: &mut fmt::Formatter<'_>,
    layout: BegodePackLayoutSelection,
) -> fmt::Result {
    match layout {
        BegodePackLayoutSelection::Missing => Ok(()),
        BegodePackLayoutSelection::Conflicting => output.write_str(" layout_conflict=true"),
        BegodePackLayoutSelection::Selected(evidence) => {
            write_selected_layout_evidence(output, evidence)
        }
    }
}

fn write_selected_layout_evidence(
    output: &mut fmt::Formatter<'_>,
    evidence: BegodePackLayoutEvidence,
) -> fmt::Result {
    if let Some(cell_model) = evidence.cell_model {
        write!(output, " layout_cell_model={}", cell_model.label())?;
    }
    if let Some(series_cells) = evidence.series_cells {
        write!(output, " layout_series_cells={series_cells}")?;
    }
    if let Some(parallel_count) = evidence.parallel_count {
        write!(output, " layout_parallel_count={parallel_count}")?;
    }
    Ok(())
}

async fn dashboard(args: DashboardArgs) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let _log_guard = install_dashboard_log_sink(tx.clone());

    if args.demo {
        return run_dashboard_with_updates(DashboardState::demo(args.device.as_deref()), &rx);
    }

    if let Some(path) = args.pevcap.as_ref() {
        let input = fs::read(path)?;
        let capture = PevcapCapture::decode(&input, pevcap_encoding(args.pevcap_format))?;
        return run_dashboard_with_updates(dashboard_state_from_aero_pevcap(&capture)?, &rx);
    }

    let target = dashboard_live_target(&args)?;
    info!(
        device = target.name_contains.as_deref().unwrap_or("<none>"),
        seconds = args.seconds(),
        "scanning for dashboard device"
    );
    let connection = connect_and_discover(&target, ScanWindow::from_secs(args.seconds())).await?;
    info!(
        observation = %connection.summary.observation,
        "connected dashboard device"
    );
    let mut state = DashboardState::live_connected(&target, &connection.summary);
    match read_battery_level(&connection.peripheral, &connection.summary).await? {
        Some(percent) => {
            info!(
                percent = cutout_core::PercentQuantity::as_percent(percent),
                "read dashboard battery level"
            );
            state.apply_battery_level(cutout_core::PercentQuantity::as_percent(percent));
        }
        None => {
            info!("dashboard battery level unavailable from standard BLE characteristic");
        }
    }
    run_live_dashboard(state, connection, tx, rx)
}

fn dashboard_live_target(args: &DashboardArgs) -> Result<ConnectionTarget> {
    let Some(device) = args.device.clone() else {
        anyhow::bail!("dashboard requires --demo or --device to start");
    };

    Ok(ConnectionTarget {
        address: None,
        identifier: None,
        name_contains: Some(device),
    })
}

fn run_live_dashboard(
    state: DashboardState,
    connection: ConnectedPeripheral,
    tx: mpsc::Sender<DashboardUpdate>,
    rx: mpsc::Receiver<DashboardUpdate>,
) -> Result<()> {
    info!(
        observation = %connection.summary.observation,
        "starting dashboard live runner"
    );
    run_live_dashboard_with(
        state,
        tx,
        rx,
        move |tx| run_dashboard_live_updates(connection, tx),
        |state, rx| run_dashboard_with_updates(state, &rx),
    )
}

fn run_live_dashboard_with<Start, Fut, Run>(
    state: DashboardState,
    tx: mpsc::Sender<DashboardUpdate>,
    rx: mpsc::Receiver<DashboardUpdate>,
    start_live_updates: Start,
    run_terminal: Run,
) -> Result<()>
where
    Start: FnOnce(mpsc::Sender<DashboardUpdate>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
    Run: FnOnce(DashboardState, mpsc::Receiver<DashboardUpdate>) -> Result<()> + Send + 'static,
{
    let live_thread = start_dashboard_live_thread(start_live_updates, tx)?;

    info!("running dashboard terminal while live update thread is active");
    let dashboard_result = run_terminal(state, rx);
    live_thread.shutdown();
    dashboard_result
}

struct LiveDashboardThread {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    thread: thread::JoinHandle<()>,
}

impl LiveDashboardThread {
    fn shutdown(self) {
        info!("dashboard live update thread shutdown requested");
        let _ = self.shutdown_tx.send(());
        match self.thread.join() {
            Ok(()) => info!("dashboard live update thread joined"),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}

fn start_dashboard_live_thread<Start, Fut>(
    start_live_updates: Start,
    tx: mpsc::Sender<DashboardUpdate>,
) -> Result<LiveDashboardThread>
where
    Start: FnOnce(mpsc::Sender<DashboardUpdate>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (started_tx, started_rx) = mpsc::channel();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    info!("dashboard live update thread spawning");
    let thread = thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
            .enable_time()
            .worker_threads(2)
            .thread_name("cutout-dashboard-live")
            .build()
        else {
            let _ = started_tx.send(Err(anyhow::anyhow!(
                "dashboard live update runtime failed to build"
            )));
            return;
        };

        runtime.block_on(async move {
            let mut started_tx = Some(started_tx);
            let mut live_updates = Box::pin(start_live_updates(tx));
            let live_updates = poll_fn(move |context| {
                if let Some(started_tx) = started_tx.take() {
                    let _ = started_tx.send(Ok(()));
                    info!("dashboard live update task entered in dedicated runtime");
                }
                live_updates.as_mut().poll(context)
            });
            tokio::pin!(live_updates);
            tokio::select! {
                () = &mut live_updates => {
                    info!("dashboard live update task finished");
                }
                _ = shutdown_rx => {
                    info!("dashboard live update task dropping after shutdown");
                }
            }
        });
    });
    started_rx.recv()??;
    info!("dashboard live update thread started before terminal");

    Ok(LiveDashboardThread {
        shutdown_tx,
        thread,
    })
}

async fn run_dashboard_live_updates(
    connection: ConnectedPeripheral,
    tx: mpsc::Sender<DashboardUpdate>,
) {
    info!("dashboard live update task entered");
    info!("dashboard live update selecting session endpoints");
    if connection.summary.select_session_endpoints().is_none() {
        debug!("dashboard live update aborted: no session endpoints");
        let _ = tx.send(DashboardUpdate::Log {
            level: "warn".to_owned(),
            message: "dashboard session endpoints unavailable".to_owned(),
        });
        return;
    }
    info!("dashboard live update selected session endpoints");

    let selected_session = match dashboard_session_profile_from_summary(&connection.summary) {
        Ok(selected_session) => selected_session,
        Err(error) => {
            let _ = tx.send(DashboardUpdate::Log {
                level: "error".to_owned(),
                message: error.to_string(),
            });
            return;
        }
    };
    info!(
        session = %selected_session.session_key().as_str(),
        "dashboard live update constructing registered read-only session"
    );
    let registration = match selected_session.session_registration() {
        Ok(registration) => registration,
        Err(error) => {
            let _ = tx.send(DashboardUpdate::Log {
                level: "error".to_owned(),
                message: error.to_string(),
            });
            return;
        }
    };
    let mut session = registration.construct();
    info!(
        session = %selected_session.session_key().as_str(),
        "dashboard live update constructed registered read-only session"
    );
    let mut iteration = 0_u64;
    debug!("dashboard live update checking battery refresh capability");
    let refresh_battery = connection.summary.battery_level_characteristic().is_some();
    info!(
        refresh_battery,
        "dashboard live update battery refresh policy"
    );

    loop {
        if !run_dashboard_live_iteration(
            &connection,
            &tx,
            &mut session,
            registration.data_channel,
            iteration,
            refresh_battery,
        )
        .await
        {
            return;
        }

        iteration = iteration.wrapping_add(1);
    }
}

async fn run_dashboard_live_iteration(
    connection: &ConnectedPeripheral,
    tx: &mpsc::Sender<DashboardUpdate>,
    session: &mut RegisteredReadOnlySession,
    data_channel: GattChannel,
    iteration: u64,
    refresh_battery: bool,
) -> bool {
    info!(iteration, "dashboard live update loop tick");
    if refresh_battery && iteration % DASHBOARD_BATTERY_REFRESH_EVERY == 0 {
        if !refresh_dashboard_battery(connection, tx, iteration).await {
            return false;
        }
    } else if !refresh_battery && iteration == 0 {
        debug!("dashboard battery refresh skipped: no standard battery characteristic");
    }

    let Some(endpoints) = connection.summary.select_session_endpoints() else {
        debug!(
            iteration,
            "dashboard live update stopped: session endpoints disappeared"
        );
        return false;
    };
    debug!(
        iteration,
        write = %endpoints.write.uuid,
        notify = %OptionalUuid(endpoints.notify.map(|characteristic| characteristic.uuid)),
        window_ms = DASHBOARD_LIVE_WINDOW.as_millis(),
        "dashboard drive_session starting"
    );
    info!(iteration, "dashboard awaiting drive_session");
    match drive_session(
        &connection.peripheral,
        session,
        data_channel,
        &connection.summary,
        endpoints,
        DASHBOARD_LIVE_WINDOW.into(),
    )
    .await
    {
        Ok(report) => send_dashboard_session_report(tx, iteration, report),
        Err(error) => retry_after_dashboard_session_error(tx, iteration, error).await,
    }
}

fn send_dashboard_session_report(
    tx: &mpsc::Sender<DashboardUpdate>,
    iteration: u64,
    report: SessionBridgeReport,
) -> bool {
    info!(
        iteration,
        notifications = report.notifications.as_events(),
        read_only_responses = report.read_only_responses.as_events(),
        telemetry = report.telemetry.as_events(),
        "dashboard drive_session returned"
    );
    debug!(
        iteration,
        subscribes = report.subscribes.as_events(),
        notifications = report.notifications.as_events(),
        notification_bytes = report.notification_bytes.as_bytes(),
        telemetry = report.telemetry.as_events(),
        read_only_responses = report.read_only_responses.as_events(),
        diagnostics = report.diagnostics.as_events(),
        latest_notification_len = ?report.latest_notification_len,
        "dashboard drive_session completed"
    );
    if tx
        .send(DashboardUpdate::SessionReport(Box::new(report)))
        .is_err()
    {
        debug!(iteration, "dashboard receiver closed after session report");
        return false;
    }
    true
}

async fn retry_after_dashboard_session_error(
    tx: &mpsc::Sender<DashboardUpdate>,
    iteration: u64,
    error: BtleError,
) -> bool {
    debug!(iteration, %error, "dashboard drive_session failed");
    if tx
        .send(DashboardUpdate::Log {
            level: "warn".to_owned(),
            message: format!("dashboard session update failed, retrying: {error}"),
        })
        .is_err()
    {
        return false;
    }
    tokio::time::sleep(DASHBOARD_LIVE_WINDOW).await;
    true
}

async fn refresh_dashboard_battery(
    connection: &ConnectedPeripheral,
    tx: &mpsc::Sender<DashboardUpdate>,
    iteration: u64,
) -> bool {
    info!(iteration, "dashboard battery refresh starting");
    match read_battery_level(&connection.peripheral, &connection.summary).await {
        Ok(Some(percent)) => {
            info!(
                iteration,
                percent = cutout_core::PercentQuantity::as_percent(percent),
                "dashboard battery refresh succeeded"
            );
            tx.send(DashboardUpdate::BatteryLevel(
                cutout_core::PercentQuantity::as_percent(percent),
            ))
            .is_ok()
        }
        Ok(None) => {
            info!(iteration, "dashboard battery refresh unavailable");
            true
        }
        Err(error) => {
            debug!(iteration, %error, "dashboard battery refresh failed");
            tx.send(DashboardUpdate::Log {
                level: "warn".to_owned(),
                message: format!("dashboard battery refresh failed: {error}"),
            })
            .is_ok()
        }
    }
}

async fn scan(seconds: u64) -> Result<(), BtleError> {
    for observation in scan_peripherals(ScanWindow::from_secs(seconds)).await? {
        info!("{observation}");
    }
    Ok(())
}

async fn subscribe_raw(args: RawSubscribeArgs) -> Result<()> {
    let seconds = args.seconds();
    let requested = args.characteristic;
    let annotations = raw_capture_annotations(&args);
    let RawSubscribeArgs {
        target,
        pevcap_output,
        pevcap_format,
        ..
    } = args;
    let output = pevcap_output.map(|path| (path, pevcap_format));
    let connection = connect_and_discover(&target.into(), ScanWindow::from_secs(seconds)).await?;
    info!("{}", connection.summary);
    let Some(characteristic) = connection.summary.select_notify_characteristic(requested) else {
        if let Some(uuid) = requested {
            bail!("no notify/indicate characteristic matched {uuid}");
        }
        bail!("no notify/indicate characteristics discovered");
    };
    info!(
        characteristic = %characteristic.uuid,
        service = %characteristic.service_uuid,
        window_seconds = seconds,
        "raw subscribe"
    );
    let records = capture_raw_notifications(
        &connection.peripheral,
        characteristic,
        NotificationWindow::from_secs(seconds),
    )
    .await?;
    match output {
        Some((path, format)) => {
            let annotation_refs: Vec<&str> = annotations.iter().map(String::as_str).collect();
            let bytes = encode_raw_capture_pevcap(
                &records,
                &connection.summary,
                Some(connection.peripheral.mtu()),
                format,
                capture_wall_clock_unix_ms(),
                annotation_refs.as_slice(),
            )?;
            fs::write(&path, bytes)?;
            info!(path = %path.display(), format = ?format, "wrote raw pevcap");
        }
        None => {
            for record in records {
                info!(
                    t_ms = record.monotonic_ms.get(),
                    characteristic = %record.characteristic,
                    service = %record.service,
                    bytes = %encode_hex(record.bytes.as_raw_bytes()),
                    "raw-notification"
                );
            }
        }
    }
    Ok(())
}

fn raw_capture_annotations(args: &RawSubscribeArgs) -> Vec<String> {
    [
        args.capture_label
            .map(CaptureSessionLabel::from)
            .map(CaptureSessionLabel::annotation),
        args.capture_privacy
            .map(CapturePrivacy::from)
            .map(CapturePrivacy::annotation),
        args.capture_evidence
            .map(CaptureEvidence::from)
            .map(CaptureEvidence::annotation),
        args.capture_distribution
            .map(CaptureDistribution::from)
            .map(CaptureDistribution::annotation),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

async fn connect(args: TargetedScanArgs, mode: SessionMode) -> Result<()> {
    let seconds = args.seconds();
    let requested_profile = args.profile();
    let commands = read_probe_commands(args.probes());
    let diagnostics_jsonl = args.diagnostics_jsonl();
    let read_only_jsonl = args.read_only_jsonl();
    let connection =
        connect_and_discover(&args.into_target(), ScanWindow::from_secs(seconds)).await?;
    let resolution =
        selected_session_resolution_for_summary(requested_profile, &connection.summary)?;

    info!("{}", connection.summary);
    if let Some(endpoints) = connection.summary.select_session_endpoints() {
        print_session_endpoints(endpoints);
        mode.run(
            &connection,
            endpoints,
            resolution.selected_session,
            SessionRunOptions {
                commands: &commands,
                window: NotificationWindow::from_secs(seconds),
                diagnostics_jsonl,
                read_only_jsonl,
                resolved_identity: resolution.resolved_identity,
            },
        )
        .await?;
    }

    Ok(())
}

async fn capture(args: CaptureArgs) -> Result<()> {
    let annotations = capture_annotations(&args);
    if args.reconnect_attempts.has_multiple_links() {
        let output = capture_output(args.pevcap_output.clone(), args.pevcap_format, annotations);
        return capture_reconnecting(args, output).await;
    }
    let output = capture_output(args.pevcap_output, args.pevcap_format, annotations);
    connect(args.target, SessionMode::Capture { output }).await
}

fn capture_output(
    pevcap_output: Option<std::path::PathBuf>,
    pevcap_format: PevcapFormat,
    annotations: Vec<String>,
) -> CaptureOutput {
    pevcap_output.map_or(CaptureOutput::Text, |path| CaptureOutput::Pevcap {
        path,
        format: pevcap_format,
        annotations,
    })
}

async fn capture_reconnecting(args: CaptureArgs, output: CaptureOutput) -> Result<()> {
    let seconds = args.target.seconds();
    let requested_profile = args.target.profile();
    let target = args.target.clone().into_target();
    let commands = read_probe_commands(args.target.probes());
    let diagnostics_jsonl = args.target.diagnostics_jsonl();
    let read_only_jsonl = args.target.read_only_jsonl();
    let mut host = BtleplugReconnectHost::new(target, ScanWindow::from_secs(seconds));
    let resolution = selected_session_resolution_for_target(requested_profile, host.target())?;
    let registration = resolution.selected_session.session_registration()?;
    let mut session = registration.construct();
    let reconnecting_capture = capture_reconnecting_session_with_commands(
        &mut host,
        &mut session,
        registration.data_channel,
        NotificationWindow::from_secs(seconds),
        args.reconnect_attempts,
        WriteProvenance::Stable,
        &commands,
    )
    .await?;
    let summary = merge_reconnect_summaries(
        reconnecting_capture
            .attempts
            .iter()
            .map(|attempt| &attempt.summary),
    )
    .ok_or(BtleError::NoPeripheralMatched)?;
    let summary_resolution = selected_session_resolution_for_summary(requested_profile, &summary)?;
    let resolved_identity = resolution
        .resolved_identity
        .or(summary_resolution.resolved_identity);
    write_or_print_capture(
        reconnecting_capture.capture,
        &summary,
        &output,
        resolution.selected_session,
        diagnostics_jsonl,
        read_only_jsonl,
        resolved_identity,
    )?;
    print_reconnect_attempt_diagnostics_jsonl(&reconnecting_capture.attempts, diagnostics_jsonl)?;
    Ok(())
}

fn merge_reconnect_summaries<'a>(
    summaries: impl IntoIterator<Item = &'a cutout_btle::ConnectionSummary>,
) -> Option<cutout_btle::ConnectionSummary> {
    let mut summaries = summaries.into_iter();
    let mut merged = summaries.next().cloned()?;
    for summary in summaries {
        for service in &summary.observation.advertised_services {
            if !merged.observation.advertised_services.contains(service) {
                merged.observation.advertised_services.push(*service);
            }
        }
        for service in &summary.services {
            if let Some(existing) = merged
                .services
                .iter_mut()
                .find(|existing| existing.uuid == service.uuid)
            {
                for characteristic in &service.characteristics {
                    if !existing.characteristics.iter().any(|existing| {
                        existing.uuid == characteristic.uuid
                            && existing.service_uuid == characteristic.service_uuid
                    }) {
                        existing.characteristics.push(characteristic.clone());
                    }
                }
            } else {
                merged.services.push(service.clone());
            }
        }
    }
    Some(merged)
}

fn capture_annotations(args: &CaptureArgs) -> Vec<String> {
    [
        args.capture_label
            .map(CaptureSessionLabel::from)
            .map(CaptureSessionLabel::annotation),
        args.capture_privacy
            .map(CapturePrivacy::from)
            .map(CapturePrivacy::annotation),
        args.capture_evidence
            .map(CaptureEvidence::from)
            .map(CaptureEvidence::annotation),
        args.capture_distribution
            .map(CaptureDistribution::from)
            .map(CaptureDistribution::annotation),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SessionMode {
    Drive,
    Capture { output: CaptureOutput },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CaptureOutput {
    Text,
    Pevcap {
        path: std::path::PathBuf,
        format: PevcapFormat,
        annotations: Vec<String>,
    },
}

#[derive(Clone, Debug)]
struct SessionRunOptions<'a> {
    commands: &'a [DeviceCommand],
    window: NotificationWindow,
    diagnostics_jsonl: bool,
    read_only_jsonl: bool,
    resolved_identity: Option<PevcapResolvedIdentity>,
}

impl SessionMode {
    async fn run(
        self,
        connection: &ConnectedPeripheral,
        endpoints: SessionEndpoints<'_>,
        profile: SelectedSessionProfile,
        options: SessionRunOptions<'_>,
    ) -> Result<()> {
        let registration = profile.session_registration()?;
        let binding = SessionBinding::new(registration.data_channel, profile);
        self.run_with_session(
            connection,
            endpoints,
            registration.construct(),
            binding,
            options,
        )
        .await
    }

    async fn run_with_session<S>(
        self,
        connection: &ConnectedPeripheral,
        endpoints: SessionEndpoints<'_>,
        mut session: S,
        binding: SessionBinding,
        options: SessionRunOptions<'_>,
    ) -> Result<()>
    where
        S: cutout_core::ProtocolSession + Send,
    {
        match self {
            Self::Drive => {
                let report = drive_session_with_commands(
                    &connection.peripheral,
                    &mut session,
                    binding.channel,
                    &connection.summary,
                    endpoints,
                    options.window,
                    options.commands,
                )
                .await?;
                print_session_report(&report);
                print_session_read_only_jsonl(&report, options.read_only_jsonl)?;
                print_session_diagnostics_jsonl(&report, options.diagnostics_jsonl)?;
            }
            Self::Capture { output } => {
                let capture = capture_session_with_commands(
                    &connection.peripheral,
                    &mut session,
                    binding.channel,
                    &connection.summary,
                    endpoints,
                    options.window,
                    options.commands,
                )
                .await?;
                write_or_print_capture(
                    capture,
                    &connection.summary,
                    &output,
                    binding.profile,
                    options.diagnostics_jsonl,
                    options.read_only_jsonl,
                    options.resolved_identity,
                )?;
            }
        }
        Ok(())
    }
}

fn read_probe_commands(probes: &[ReadProbe]) -> Vec<DeviceCommand> {
    probes.iter().copied().map(read_probe_command).collect()
}

const fn read_probe_command(probe: ReadProbe) -> DeviceCommand {
    match probe {
        ReadProbe::Identity => DeviceCommand::RequestIdentity,
        ReadProbe::Firmware => DeviceCommand::RequestFirmwareInfo,
        ReadProbe::Telemetry => DeviceCommand::RequestTelemetry,
        ReadProbe::Battery => DeviceCommand::RequestBatteryInfo,
        ReadProbe::Diagnostics => DeviceCommand::RequestDiagnostics,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectedSessionProfile(SessionKey);

impl SelectedSessionProfile {
    fn from_session_key(key: SessionKey) -> Option<Self> {
        if find_session_registration(key).is_some() {
            Some(Self(key))
        } else {
            None
        }
    }

    const fn session_key(self) -> SessionKey {
        self.0
    }

    fn session_registration(self) -> Result<&'static cutout_protocols::SessionRegistration> {
        session_registration_by_key(self.session_key())
    }

    fn is_falcon(self) -> bool {
        self.0 == BEGODE_FALCON_SESSION_KEY
    }
}

fn session_registration_by_key(
    key: SessionKey,
) -> Result<&'static cutout_protocols::SessionRegistration> {
    find_session_registration(key).ok_or_else(|| {
        anyhow::anyhow!("selected session registration is missing: {}", key.as_str())
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SessionBinding {
    channel: GattChannel,
    profile: SelectedSessionProfile,
}

impl SessionBinding {
    const fn new(channel: GattChannel, profile: SelectedSessionProfile) -> Self {
        Self { channel, profile }
    }
}

const fn selected_session_profile(profile: SessionProfile) -> SelectedSessionProfile {
    match profile {
        SessionProfile::Auto | SessionProfile::Aero => {
            SelectedSessionProfile(NOSFET_AERO_SESSION_KEY)
        }
        SessionProfile::Falcon => SelectedSessionProfile(BEGODE_FALCON_SESSION_KEY),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionResolution {
    selected_session: SelectedSessionProfile,
    resolved_identity: Option<PevcapResolvedIdentity>,
    source: SessionResolutionSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SessionResolutionSource {
    Explicit(SessionProfile),
    AdvertisedName(String),
    Fallback,
}

fn selected_session_resolution_for_summary(
    profile: SessionProfile,
    summary: &cutout_btle::ConnectionSummary,
) -> Result<SessionResolution> {
    match profile {
        SessionProfile::Aero | SessionProfile::Falcon => Ok(SessionResolution {
            selected_session: selected_session_profile(profile),
            resolved_identity: pevcap_identity_for_profile(profile),
            source: SessionResolutionSource::Explicit(profile),
        }),
        SessionProfile::Auto => auto_session_resolution(summary.observation.name.as_deref()),
    }
}

fn selected_session_resolution_for_target(
    profile: SessionProfile,
    target: &ConnectionTarget,
) -> Result<SessionResolution> {
    match profile {
        SessionProfile::Aero | SessionProfile::Falcon => Ok(SessionResolution {
            selected_session: selected_session_profile(profile),
            resolved_identity: pevcap_identity_for_profile(profile),
            source: SessionResolutionSource::Explicit(profile),
        }),
        SessionProfile::Auto => auto_session_resolution(target.name_contains.as_deref()),
    }
}

fn auto_session_resolution(name: Option<&str>) -> Result<SessionResolution> {
    let Some(name) = name else {
        return Ok(SessionResolution {
            selected_session: selected_aero_session_profile(),
            resolved_identity: None,
            source: SessionResolutionSource::Fallback,
        });
    };

    match ModelCatalog::new(&MODEL_CATALOG).resolve_advertised_name(name) {
        CatalogModelResolution::Matched(entry) => {
            let selected_session = selected_session_profile_for_catalog_entry(entry)?;
            Ok(SessionResolution {
                selected_session,
                resolved_identity: Some(pevcap_identity_for_catalog_entry(entry)),
                source: SessionResolutionSource::AdvertisedName(name.to_owned()),
            })
        }
        CatalogModelResolution::NoMatch => Ok(SessionResolution {
            selected_session: selected_aero_session_profile(),
            resolved_identity: None,
            source: SessionResolutionSource::Fallback,
        }),
        CatalogModelResolution::Ambiguous => {
            bail!(
                "auto session resolution found ambiguous catalog entries for advertised name {name}"
            )
        }
    }
}

const fn selected_aero_session_profile() -> SelectedSessionProfile {
    selected_session_profile(SessionProfile::Aero)
}

#[cfg(test)]
const fn selected_falcon_session_profile() -> SelectedSessionProfile {
    selected_session_profile(SessionProfile::Falcon)
}

fn selected_pevcap_replay_profile(
    capture: &PevcapCapture,
    profile: SessionProfile,
) -> Result<SelectedSessionProfile> {
    match profile {
        SessionProfile::Aero | SessionProfile::Falcon => Ok(selected_session_profile(profile)),
        SessionProfile::Auto => auto_pevcap_replay_profile(capture),
    }
}

fn auto_pevcap_replay_profile(capture: &PevcapCapture) -> Result<SelectedSessionProfile> {
    let Some(identity) = capture.header.resolved_identity.as_ref() else {
        bail!("PEVCAP replay --profile auto requires resolved model identity metadata");
    };
    let Some(family) = identity.protocol_family else {
        bail!("PEVCAP replay --profile auto requires resolved protocol family metadata");
    };
    let Some(model) = identity.model.as_ref() else {
        bail!("PEVCAP replay --profile auto requires resolved model metadata");
    };

    match ModelCatalog::new(&MODEL_CATALOG).resolve_display_model(family, model.value.as_str()) {
        CatalogModelResolution::Matched(entry) => selected_session_profile_for_catalog_entry(entry),
        CatalogModelResolution::NoMatch => {
            bail!(
                "PEVCAP replay --profile auto found no catalog entry for resolved model {}",
                model.value
            )
        }
        CatalogModelResolution::Ambiguous => {
            bail!(
                "PEVCAP replay --profile auto found ambiguous catalog entries for resolved model {}",
                model.value
            )
        }
    }
}

fn selected_session_profile_for_catalog_entry(
    entry: &cutout_core::ModelCatalogEntry,
) -> Result<SelectedSessionProfile> {
    let Some(session) = entry.registration.session else {
        bail!(
            "catalog entry {} has no session registration",
            entry.registry.model
        );
    };
    SelectedSessionProfile::from_session_key(session).ok_or_else(|| {
        anyhow::anyhow!(
            "catalog entry {} uses unsupported CLI session registration: {}",
            entry.registry.model,
            session.as_str()
        )
    })
}

fn dashboard_session_profile_from_summary(
    summary: &cutout_btle::ConnectionSummary,
) -> Result<SelectedSessionProfile> {
    let Some(name) = summary.observation.name.as_deref() else {
        bail!("dashboard cannot resolve a session profile from unnamed device evidence");
    };

    match ModelCatalog::new(&MODEL_CATALOG).resolve_advertised_name(name) {
        CatalogModelResolution::Matched(entry) => selected_session_profile_for_catalog_entry(entry),
        CatalogModelResolution::NoMatch => {
            bail!("dashboard cannot resolve a session profile from device evidence: {name}")
        }
        CatalogModelResolution::Ambiguous => {
            bail!("dashboard found ambiguous catalog entries for advertised name {name}")
        }
    }
}

fn pevcap_identity_for_profile(profile: SessionProfile) -> Option<PevcapResolvedIdentity> {
    match profile {
        SessionProfile::Auto => None,
        SessionProfile::Aero => Some(PevcapResolvedIdentity {
            protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
            model: Some(VerifiedValue {
                value: "NOSFET Aero".to_owned(),
                verification: VerificationStatus::Inferred,
            }),
            firmware: None,
        }),
        SessionProfile::Falcon => Some(PevcapResolvedIdentity {
            protocol_family: Some(ProtocolFamily::BegodeGotway),
            model: Some(VerifiedValue {
                value: "Begode Falcon".to_owned(),
                verification: VerificationStatus::Inferred,
            }),
            firmware: None,
        }),
    }
}

fn pevcap_identity_for_catalog_entry(
    entry: &cutout_core::ModelCatalogEntry,
) -> PevcapResolvedIdentity {
    PevcapResolvedIdentity {
        protocol_family: Some(entry.registry.protocol_family),
        model: Some(VerifiedValue {
            value: entry.registry.model.as_str().to_owned(),
            verification: VerificationStatus::Inferred,
        }),
        firmware: None,
    }
}

fn print_session_endpoints(endpoints: SessionEndpoints<'_>) {
    info!(
        write = %endpoints.write.uuid,
        notify = %OptionalUuid(endpoints.notify.map(|notify| notify.uuid)),
        "session endpoints"
    );
}

fn print_capture(
    capture: SessionCapture,
    diagnostics_jsonl: bool,
    read_only_jsonl: bool,
) -> Result<()> {
    for record in capture.records {
        info!("{record}");
    }
    print_session_report(&capture.report);
    print_session_read_only_jsonl(&capture.report, read_only_jsonl)?;
    print_session_diagnostics_jsonl(&capture.report, diagnostics_jsonl)?;
    Ok(())
}

fn write_or_print_capture(
    capture: SessionCapture,
    summary: &cutout_btle::ConnectionSummary,
    output: &CaptureOutput,
    profile: SelectedSessionProfile,
    diagnostics_jsonl: bool,
    read_only_jsonl: bool,
    resolved_identity: Option<PevcapResolvedIdentity>,
) -> Result<()> {
    match output {
        CaptureOutput::Text => {
            print_capture(capture, diagnostics_jsonl, read_only_jsonl)?;
            Ok(())
        }
        CaptureOutput::Pevcap {
            path,
            format,
            annotations,
        } => {
            let report = capture.report.clone();
            let annotation_refs: Vec<&str> = annotations.iter().map(String::as_str).collect();
            let bytes = encode_session_capture_pevcap(
                &capture,
                summary,
                *format,
                capture_wall_clock_unix_ms(),
                profile,
                resolved_identity,
                annotation_refs.as_slice(),
            )?;
            fs::write(path, bytes)?;
            info!(path = %path.display(), format = ?format, "wrote pevcap");
            print_session_report(&report);
            print_session_read_only_jsonl(&report, read_only_jsonl)?;
            print_session_diagnostics_jsonl(&report, diagnostics_jsonl)?;
            Ok(())
        }
    }
}

fn append_falcon_capture_resolver_context(
    capture: &SessionCapture,
    annotations: &[&str],
    resolver_evidence: &mut Vec<String>,
    resolver_warnings: &mut Vec<String>,
) {
    let capacity = select_begode_pack_capacity_from_annotations(annotations.iter().copied());
    let layout = select_begode_pack_layout_from_annotations(annotations.iter().copied());
    let bms_voltage_evidence = falcon_capture_bms_voltage_evidence(capture);
    if let Some(voltage) = bms_voltage_evidence
        .iter()
        .find_map(|evidence| match evidence {
            BegodeVoltageEvidence::ObservedPackVoltage(voltage) => Some(*voltage),
            BegodeVoltageEvidence::VoltageClass84V | BegodeVoltageEvidence::VoltageClass100V => {
                None
            }
        })
    {
        resolver_evidence.push(format!("bms_voltage={}", voltage.get()));
    }
    let voltage_profile =
        match select_begode_pack_voltage_profile_from_annotations(annotations.iter().copied()) {
            BegodeVoltageProfileSelection::Conflicting => {
                BegodeVoltageProfileSelection::Conflicting
            }
            BegodeVoltageProfileSelection::Selected(profile) => select_begode_pack_voltage_profile(
                core::iter::once(profile_evidence(profile)).chain(bms_voltage_evidence),
            ),
            BegodeVoltageProfileSelection::Missing => {
                select_begode_pack_voltage_profile(bms_voltage_evidence)
            }
        };
    match voltage_profile {
        BegodeVoltageProfileSelection::Missing => {
            resolver_warnings.push("missing_falcon_battery_voltage_evidence".to_owned());
        }
        BegodeVoltageProfileSelection::Conflicting => {
            resolver_warnings.push("conflicting_falcon_battery_voltage_evidence".to_owned());
        }
        BegodeVoltageProfileSelection::Selected(_) => {}
    }

    match capacity {
        BegodeCapacitySelection::Missing => {
            resolver_warnings.push("missing_falcon_battery_capacity_evidence".to_owned());
        }
        BegodeCapacitySelection::Conflicting => {
            resolver_warnings.push("conflicting_falcon_battery_capacity_evidence".to_owned());
        }
        BegodeCapacitySelection::Selected(_) => {}
    }

    match layout {
        BegodePackLayoutSelection::Missing => {
            resolver_warnings.push("missing_falcon_battery_layout_evidence".to_owned());
        }
        BegodePackLayoutSelection::Conflicting => {
            resolver_warnings.push("conflicting_falcon_battery_layout_evidence".to_owned());
        }
        BegodePackLayoutSelection::Selected(_) => {}
    }

    match validate_begode_pack_evidence(voltage_profile, capacity, layout) {
        BegodePackEvidenceConsistency::Inconsistent => {
            resolver_warnings.push("falcon_battery_evidence_inconsistent".to_owned());
        }
        BegodePackEvidenceConsistency::Incomplete => {
            resolver_warnings.push("falcon_battery_evidence_incomplete".to_owned());
        }
        BegodePackEvidenceConsistency::Consistent => {}
    }
}

fn encode_session_capture_pevcap(
    capture: &SessionCapture,
    summary: &cutout_btle::ConnectionSummary,
    format: PevcapFormat,
    wall_clock_start_unix_ms: WallClockUnixTimestamp,
    profile: SelectedSessionProfile,
    resolved_identity: Option<PevcapResolvedIdentity>,
    annotations: &[&str],
) -> Result<Vec<u8>> {
    let mut capture_annotations = Vec::with_capacity(annotations.len() + 1);
    capture_annotations.push("cutout-cli capture");
    capture_annotations.extend_from_slice(annotations);
    let mut resolver_evidence = Vec::new();
    resolver_evidence.push(format!(
        "selected_session_key={}",
        profile.session_key().as_str()
    ));
    if let Some(identity) = resolved_identity.as_ref() {
        if let Some(protocol_family) = identity.protocol_family {
            resolver_evidence.push(format!("resolved_protocol_family={protocol_family:?}"));
        }
        if let Some(model) = identity.model.as_ref() {
            resolver_evidence.push(format!("resolved_model={}", model.value));
        }
        if let Some(firmware) = identity.firmware.as_ref() {
            resolver_evidence.push(format!("resolved_firmware={}", firmware.value));
        }
    }
    let mut resolver_warnings = Vec::new();
    if profile.is_falcon() {
        append_falcon_capture_resolver_context(
            capture,
            annotations,
            &mut resolver_evidence,
            &mut resolver_warnings,
        );
    }
    let resolver_evidence_refs = resolver_evidence
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let resolver_warnings_refs = resolver_warnings
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let pevcap = capture.to_pevcap(
        summary,
        PevcapSessionMetadata {
            wall_clock_start_unix_ms,
            platform_id: std::env::consts::OS,
            library_version: env!("CARGO_PKG_VERSION"),
            registry_hash: cutout_core::registry_entries_hash(&[&BEGODE_FALCON_REGISTRY_ENTRY]),
            selected_session_key: Some(profile.session_key().as_str()),
            resolved_identity,
            resolver_evidence: resolver_evidence_refs.as_slice(),
            resolver_warnings: resolver_warnings_refs.as_slice(),
            annotations: capture_annotations.as_slice(),
        },
    )?;
    Ok(pevcap.encode(pevcap_encoding(format))?)
}

fn encode_raw_capture_pevcap(
    records: &[RawNotificationRecord],
    summary: &cutout_btle::ConnectionSummary,
    write_limit: Option<u16>,
    format: PevcapFormat,
    wall_clock_start_unix_ms: WallClockUnixTimestamp,
    annotations: &[&str],
) -> Result<Vec<u8>> {
    let write_limit = write_limit.map(TransportWriteLimit::from_bytes);
    let advertised_services = summary
        .observation
        .advertised_services
        .iter()
        .copied()
        .map(gatt_channel_from_uuid)
        .collect::<Vec<_>>();
    let gatt_fingerprints = summary.gatt_fingerprints();
    let mut pevcap_records = Vec::with_capacity(records.len().saturating_add(2));
    pevcap_records.push(PevcapRecord::link_up(
        MonotonicTimestamp::new(0),
        write_limit,
    ));
    pevcap_records.extend(records.iter().map(|record| {
        PevcapRecord::inbound_notification(
            MonotonicTimestamp::new(record.monotonic_ms.get()),
            gatt_channel_from_uuid(record.characteristic),
            gatt_channel_from_uuid(record.service),
            record.bytes.to_raw_bytes(),
        )
    }));
    pevcap_records.push(PevcapRecord::link_down(MonotonicTimestamp::new(
        records
            .last()
            .map_or(MonotonicMs::default(), |record| record.monotonic_ms)
            .get(),
    )));
    let mut capture_annotations = Vec::with_capacity(annotations.len() + 1);
    capture_annotations.push("cutout-cli subscribe-raw");
    capture_annotations.extend_from_slice(annotations);
    let header = PevcapHeader::new(
        wall_clock_start_unix_ms,
        std::env::consts::OS,
        write_limit,
        &advertised_services,
        &gatt_fingerprints,
        None,
        None,
        env!("CARGO_PKG_VERSION"),
        cutout_core::registry_entries_hash(&[&BEGODE_FALCON_REGISTRY_ENTRY]),
        capture_annotations.as_slice(),
    )?;
    Ok(PevcapCapture::new(header, pevcap_records).encode(pevcap_encoding(format))?)
}

fn gatt_channel_from_uuid(uuid: uuid::Uuid) -> GattChannel {
    GattChannel::from_uuid(uuid)
}

fn capture_wall_clock_unix_ms() -> WallClockUnixTimestamp {
    WallClockUnixTimestamp::from_milliseconds(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
            }),
    )
}

fn print_session_report(report: &SessionBridgeReport) {
    info!(
        protocol_writes = report.protocol_writes.as_events(),
        writes = report.writes.as_events(),
        subscribes = report.subscribes.as_events(),
        notifications = report.notifications.as_events(),
        telemetry = report.telemetry.as_events(),
        read_only_responses = report.read_only_responses.as_events(),
        diagnostics = report.diagnostics.as_events(),
        disconnects = report.disconnects.as_events(),
        "session report"
    );
    if let Some(telemetry) = render_telemetry_snapshot(&report.telemetry_snapshot) {
        info!("{telemetry}");
    }
    if let Some(firmware) = render_firmware_info(report.firmware) {
        info!("{firmware}");
    }
    if let Some(identity) = render_identity(report) {
        info!("{identity}");
    }
    for settings in render_settings_readbacks(&report.settings) {
        info!("{settings}");
    }
}

fn print_session_diagnostics_jsonl(
    report: &SessionBridgeReport,
    enabled: bool,
) -> Result<(), serde_json::Error> {
    if enabled {
        info!("{}", render_session_diagnostics_jsonl(report)?);
        for line in render_diagnostic_errors_jsonl(&report.diagnostic_errors) {
            info!("{}", line?);
        }
    }
    Ok(())
}

fn print_reconnect_attempt_diagnostics_jsonl(
    attempts: &[ReconnectAttemptReport],
    enabled: bool,
) -> Result<(), serde_json::Error> {
    if enabled {
        for attempt in attempts {
            info!("{}", render_reconnect_attempt_diagnostics_jsonl(attempt)?);
        }
    }
    Ok(())
}

fn print_session_read_only_jsonl(
    report: &SessionBridgeReport,
    enabled: bool,
) -> Result<(), serde_json::Error> {
    if enabled {
        for line in render_read_only_responses_jsonl(&report.read_only_response_events) {
            info!("{}", line?);
        }
    }
    Ok(())
}

fn render_session_diagnostics_jsonl(
    report: &SessionBridgeReport,
) -> Result<String, serde_json::Error> {
    let diagnostics = DiagnosticSnapshot::from_parser_diagnostics(report.diagnostics_snapshot);
    serde_json::to_string(&serde_json::json!({
        "type": "diagnostic_snapshot",
        "sequence": 0,
        "protocol_writes": report.protocol_writes.as_events(),
        "writes": report.writes.as_events(),
        "dropped_bytes": diagnostics.dropped_bytes.as_bytes(),
        "resyncs": diagnostics.resyncs.as_events(),
        "bad_checksums": diagnostics.bad_checksums.as_events(),
        "timeouts": diagnostics.timeouts.as_events(),
        "oversized_frames": diagnostics.oversized_frames.as_events(),
        "malformed_frames": diagnostics.malformed_frames.as_events(),
        "unmatched_replies": diagnostics.unmatched_replies.as_events(),
    }))
}

fn render_reconnect_attempt_diagnostics_jsonl(
    attempt: &ReconnectAttemptReport,
) -> Result<String, serde_json::Error> {
    let diagnostics =
        DiagnosticSnapshot::from_parser_diagnostics(attempt.report.diagnostics_snapshot);
    serde_json::to_string(&serde_json::json!({
        "type": "reconnect_attempt",
        "attempt": attempt.attempt.get(),
        "identifier": attempt.summary.observation.identifier,
        "name": attempt.summary.observation.name,
        "rssi": attempt.summary.observation.rssi.map(cutout_core::SignalStrength::as_dbm),
        "protocol_writes": attempt.report.protocol_writes.as_events(),
        "writes": attempt.report.writes.as_events(),
        "subscribes": attempt.report.subscribes.as_events(),
        "notifications": attempt.report.notifications.as_events(),
        "telemetry": attempt.report.telemetry.as_events(),
        "read_only_responses": attempt.report.read_only_responses.as_events(),
        "diagnostics": attempt.report.diagnostics.as_events(),
        "disconnects": attempt.report.disconnects.as_events(),
        "dropped_bytes": diagnostics.dropped_bytes.as_bytes(),
        "resyncs": diagnostics.resyncs.as_events(),
        "bad_checksums": diagnostics.bad_checksums.as_events(),
        "timeouts": diagnostics.timeouts.as_events(),
        "oversized_frames": diagnostics.oversized_frames.as_events(),
        "malformed_frames": diagnostics.malformed_frames.as_events(),
        "unmatched_replies": diagnostics.unmatched_replies.as_events(),
    }))
}

fn render_read_only_responses_jsonl(
    responses: &[ReadOnlyResponse],
) -> impl Iterator<Item = Result<String, serde_json::Error>> + '_ {
    responses.iter().enumerate().map(|(sequence, response)| {
        render_read_only_response_jsonl(JsonSequence::new(sequence), *response)
    })
}

fn render_read_only_response_jsonl(
    sequence: JsonSequence,
    response: ReadOnlyResponse,
) -> Result<String, serde_json::Error> {
    match response {
        ReadOnlyResponse::Battery(payload) => render_battery_response_jsonl(sequence, payload),
        ReadOnlyResponse::Firmware(firmware) => serde_json::to_string(&serde_json::json!({
            "type": "read_only_response",
            "sequence": sequence.get(),
            "command_kind": command_kind_name(response.command_kind()),
            "response": "firmware",
            "firmware_major": measured_u16_json(firmware.firmware_major),
            "firmware_minor": measured_u16_json(firmware.firmware_minor),
            "firmware_patch": measured_u16_json(firmware.firmware_patch),
            "protocol_version": measured_u16_json(firmware.protocol_version),
            "build_id": raw_field_json(firmware.build_id),
        })),
        ReadOnlyResponse::Settings(settings) => serde_json::to_string(&serde_json::json!({
            "type": "read_only_response",
            "sequence": sequence.get(),
            "command_kind": command_kind_name(response.command_kind()),
            "response": "settings",
            "availability": settings_readback_availability_name(settings.availability),
            "entries": settings.entries.into_iter().flatten().map(settings_entry_json).collect::<Vec<_>>(),
        })),
        ReadOnlyResponse::FaultHistory(fault_history) => {
            serde_json::to_string(&serde_json::json!({
                "type": "read_only_response",
                "sequence": sequence.get(),
                "command_kind": command_kind_name(response.command_kind()),
                "response": "fault_history",
                "availability": fault_history_availability_name(fault_history.availability),
                "last_fault": fault_history.last_fault.map(fault_history_entry_json),
                "since_distance_mm": fault_history.since_distance.map(|distance| distance.value.as_millimetres()),
            }))
        }
        ReadOnlyResponse::Diagnostics(diagnostics) => serde_json::to_string(&serde_json::json!({
            "type": "read_only_response",
            "sequence": sequence.get(),
            "command_kind": command_kind_name(response.command_kind()),
            "response": "diagnostics",
            "details": diagnostics.details.into_iter().flatten().map(diagnostic_detail_json).collect::<Vec<_>>(),
        })),
        ReadOnlyResponse::RawTelemetry(raw) => serde_json::to_string(&serde_json::json!({
            "type": "read_only_response",
            "sequence": sequence.get(),
            "command_kind": command_kind_name(response.command_kind()),
            "response": "raw_telemetry",
            "fields": raw.fields.into_iter().flatten().map(|field| raw_field_json(Some(field))).collect::<Vec<_>>(),
        })),
    }
}

fn render_battery_response_jsonl(
    sequence: JsonSequence,
    readback: BatteryReadback,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&serde_json::json!({
        "type": "read_only_response",
        "sequence": sequence.get(),
        "command_kind": command_kind_name(CommandKind::RequestBatteryInfo),
        "response": "battery",
        "availability": battery_readback_availability_name(readback.availability),
        "page": readback.page.map(battery_page_json),
    }))
}

fn battery_page_json(payload: BatteryPagePayload) -> serde_json::Value {
    let page = payload.page();
    serde_json::json!({
        "selector": page.selector.get(),
        "side": battery_page_side_name(page.kind, page.selector.get()),
        "kind": battery_page_kind_name(page.kind),
        "verification": verification_status_name(page.verification),
        "battery": battery_info_json(payload),
        "temperatures": battery_temperature_values_json(payload),
    })
}

fn battery_temperature_values_json(payload: BatteryPagePayload) -> serde_json::Value {
    match payload {
        BatteryPagePayload::Temperature(_) => serde_json::json!(
            payload
                .temperatures()
                .into_iter()
                .map(measured_i32_json)
                .collect::<Vec<_>>()
        ),
        BatteryPagePayload::CellVoltage(_) | BatteryPagePayload::Raw(_) => serde_json::Value::Null,
    }
}

fn battery_info_json(payload: BatteryPagePayload) -> serde_json::Value {
    let battery = payload.battery();
    serde_json::json!({
        "voltage": measured_i32_json(battery.voltage.map(|measured| {
            measured.map_value(cutout_core::Voltage::as_millivolts)
        })),
        "current": measured_i32_json(battery.current.map(|measured| {
            measured.map_value(cutout_core::BatteryCurrent::as_milliamps)
        })),
        "bms_pack_current_0": bms_pack_current_json(
            payload.bms_pack_currents(),
            cutout_core::BmsPackCurrents::current_0,
        ),
        "bms_pack_current_1": bms_pack_current_json(
            payload.bms_pack_currents(),
            cutout_core::BmsPackCurrents::current_1,
        ),
        "level_reported": measured_u8_json(battery.level_reported.map(|measured| {
            measured.map_value(cutout_core::PercentQuantity::as_percent)
        })),
        "level_estimated": measured_u8_json(battery.level_estimated.map(|measured| {
            measured.map_value(cutout_core::PercentQuantity::as_percent)
        })),
        "temperature": measured_i32_json(battery.temperature.map(|measured| {
            measured.map_value(cutout_core::Temperature::as_millicelsius)
        })),
        "raw_state": raw_field_json(battery.raw_state),
    })
}

fn bms_pack_current_json(
    currents: Option<cutout_core::BmsPackCurrents>,
    select: impl FnOnce(cutout_core::BmsPackCurrents) -> cutout_core::BatteryCurrent,
) -> serde_json::Value {
    currents.map_or(serde_json::Value::Null, |currents| {
        measured_json_parts(
            i64::from(select(currents).as_milliamps()),
            currents.source,
            currents.quality,
            currents.verification,
        )
    })
}

fn measured_i32_json(measured: Option<Measured<i32>>) -> serde_json::Value {
    measured.map_or(serde_json::Value::Null, |measured| {
        serde_json::json!(measured_json_parts(
            i64::from(measured.value),
            measured.source,
            measured.quality,
            measured.verification
        ))
    })
}

fn measured_u8_json(measured: Option<Measured<u8>>) -> serde_json::Value {
    measured.map_or(serde_json::Value::Null, |measured| {
        serde_json::json!(measured_json_parts(
            u64::from(measured.value),
            measured.source,
            measured.quality,
            measured.verification
        ))
    })
}

fn measured_u16_json(measured: Option<Measured<u16>>) -> serde_json::Value {
    measured.map_or(serde_json::Value::Null, |measured| {
        serde_json::json!(measured_json_parts(
            u64::from(measured.value),
            measured.source,
            measured.quality,
            measured.verification
        ))
    })
}

fn measured_json_parts<T>(
    value: T,
    source: ValueSource,
    quality: ValueQuality,
    verification: VerificationStatus,
) -> serde_json::Value
where
    T: Into<serde_json::Value>,
{
    serde_json::json!({
        "value": value.into(),
        "source": value_source_name(source),
        "quality": value_quality_name(quality),
        "verification": verification_status_name(verification),
    })
}

fn raw_field_json(field: Option<cutout_core::RawFieldValue>) -> serde_json::Value {
    field.map_or(serde_json::Value::Null, |field| {
        serde_json::json!({
            "id": field.id,
            "value": field.value,
        })
    })
}

fn settings_entry_json(entry: cutout_core::SettingsEntry) -> serde_json::Value {
    serde_json::json!({
        "field": raw_field_json(Some(entry.field)),
        "source": value_source_name(entry.source),
        "quality": value_quality_name(entry.quality),
        "verification": verification_status_name(entry.verification),
    })
}

fn fault_history_entry_json(entry: cutout_core::FaultHistoryEntry) -> serde_json::Value {
    serde_json::json!({
        "code": raw_field_json(Some(entry.code)),
        "source": value_source_name(entry.source),
        "quality": value_quality_name(entry.quality),
        "verification": verification_status_name(entry.verification),
    })
}

fn diagnostic_detail_json(detail: cutout_core::DiagnosticDetail) -> serde_json::Value {
    serde_json::json!({
        "field": raw_field_json(Some(detail.field)),
        "severity": diagnostic_severity_name(detail.severity),
        "quality": value_quality_name(detail.quality),
        "verification": verification_status_name(detail.verification),
    })
}

const fn command_kind_name(kind: CommandKind) -> &'static str {
    match kind {
        CommandKind::RequestIdentity => "request_identity",
        CommandKind::RequestTelemetry => "request_telemetry",
        CommandKind::RequestFirmwareInfo => "request_firmware_info",
        CommandKind::RequestBatteryInfo => "request_battery_info",
        CommandKind::RequestDiagnostics => "request_diagnostics",
        CommandKind::RequestSettings => "request_settings",
        CommandKind::SetLights => "set_lights",
        CommandKind::SoundHorn => "sound_horn",
        CommandKind::SetRawMotorCurrent => "set_raw_motor_current",
    }
}

const fn battery_page_kind_name(kind: BatteryPageKind) -> &'static str {
    match kind {
        BatteryPageKind::Metadata => "metadata",
        BatteryPageKind::CellVoltage => "cell_voltage",
        BatteryPageKind::Temperature => "temperature",
        BatteryPageKind::Raw => "raw",
    }
}

fn battery_page_side_name(kind: BatteryPageKind, selector: u8) -> Option<&'static str> {
    if kind != BatteryPageKind::Temperature {
        return None;
    }

    match selector {
        3 => Some("left"),
        7 => Some("right"),
        _ => None,
    }
}

const fn value_source_name(source: ValueSource) -> &'static str {
    match source {
        ValueSource::Reported => "reported",
        ValueSource::Calculated => "calculated",
        ValueSource::Estimated => "estimated",
    }
}

const fn value_quality_name(quality: ValueQuality) -> &'static str {
    match quality {
        ValueQuality::Known => "known",
        ValueQuality::Inferred => "inferred",
    }
}

const fn verification_status_name(verification: VerificationStatus) -> &'static str {
    match verification {
        VerificationStatus::Unverified => "unverified",
        VerificationStatus::Inferred => "inferred",
        VerificationStatus::SourceVerified => "source_verified",
        VerificationStatus::HardwareVerified => "hardware_verified",
        VerificationStatus::SourceAndHardwareVerified => "source_and_hardware_verified",
    }
}

const fn diagnostic_severity_name(severity: cutout_core::DiagnosticSeverity) -> &'static str {
    match severity {
        cutout_core::DiagnosticSeverity::Info => "info",
        cutout_core::DiagnosticSeverity::Warning => "warning",
        cutout_core::DiagnosticSeverity::Error => "error",
    }
}

fn render_identity(report: &SessionBridgeReport) -> Option<IdentityLine<'_>> {
    let identity = report.identity.as_ref()?;
    Some(IdentityLine(identity))
}

struct IdentityLine<'a>(&'a BridgeIdentityResolution);

impl fmt::Display for IdentityLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let identity = self.0;
        write!(
            f,
            "identity confidence={:?} manufacturer={} model={} advertised_name_hint={} gatt_hint={} passive_family={} banner_model={}",
            identity.confidence,
            identity.manufacturer.unwrap_or("<unknown>"),
            identity.model.unwrap_or("<unknown>"),
            identity.evidence.has_advertised_name_hint(),
            identity.evidence.has_gatt_hint(),
            identity.evidence.has_passive_family_match(),
            identity.evidence.has_banner_model_match()
        )
    }
}

fn render_telemetry_snapshot(snapshot: &TelemetrySnapshot) -> Option<TelemetrySnapshotLine> {
    TelemetrySnapshotLine(*snapshot)
        .has_fields()
        .then_some(TelemetrySnapshotLine(*snapshot))
}

fn render_firmware_info(firmware: Option<FirmwareInfo>) -> Option<FirmwareLine> {
    let firmware = firmware?;
    FirmwareLine(firmware)
        .has_fields()
        .then_some(FirmwareLine(firmware))
}

fn render_settings_readbacks(
    settings: &[SettingsReadback],
) -> impl Iterator<Item = SettingsLine> + '_ {
    settings
        .iter()
        .copied()
        .filter(|settings| SettingsLine(*settings).has_fields())
        .map(SettingsLine)
}

struct TelemetrySnapshotLine(TelemetrySnapshot);

impl TelemetrySnapshotLine {
    const fn has_fields(self) -> bool {
        let snapshot = self.0;
        snapshot.speed.is_some()
            || snapshot.voltage.is_some()
            || snapshot.battery_current.is_some()
            || snapshot.motor_current.is_some()
            || snapshot.power.is_some()
            || snapshot.controller_temperature.is_some()
            || snapshot.motor_temperature.is_some()
            || snapshot.battery_temperature.is_some()
            || snapshot.pwm.is_some()
            || snapshot.distance.is_some()
            || snapshot.pitch.is_some()
            || snapshot.roll.is_some()
            || snapshot.battery_level_reported.is_some()
            || snapshot.battery_level_estimated.is_some()
    }
}

impl fmt::Display for TelemetrySnapshotLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = self.0;
        let mut fields = CommandFieldWriter::new(f, "telemetry");
        fields.write_measured("speed", snapshot.speed)?;
        fields.write_measured("voltage", snapshot.voltage)?;
        fields.write_measured("battery_current", snapshot.battery_current)?;
        fields.write_measured("motor_current", snapshot.motor_current)?;
        fields.write_measured("power", snapshot.power)?;
        fields.write_measured("controller_temperature", snapshot.controller_temperature)?;
        fields.write_measured("motor_temperature", snapshot.motor_temperature)?;
        fields.write_measured("battery_temperature", snapshot.battery_temperature)?;
        fields.write_measured("pwm", snapshot.pwm)?;
        fields.write_measured("distance", snapshot.distance)?;
        fields.write_measured("pitch", snapshot.pitch)?;
        fields.write_measured("roll", snapshot.roll)?;
        fields.write_measured("battery_level_reported", snapshot.battery_level_reported)?;
        fields.write_measured("battery_level_estimated", snapshot.battery_level_estimated)
    }
}

struct FirmwareLine(FirmwareInfo);

impl FirmwareLine {
    const fn has_fields(self) -> bool {
        self.0.firmware_major.is_some()
            || self.0.firmware_minor.is_some()
            || self.0.firmware_patch.is_some()
            || self.0.build_id.is_some()
    }
}

impl fmt::Display for FirmwareLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let firmware = self.0;
        let mut fields = CommandFieldWriter::new(f, "firmware");
        fields.write_measured("firmware_major", firmware.firmware_major)?;
        fields.write_measured("firmware_minor", firmware.firmware_minor)?;
        fields.write_measured("firmware_patch", firmware.firmware_patch)?;
        if let Some(build_id) = firmware.build_id {
            fields.write_raw_field(build_id)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct SettingsLine(SettingsReadback);

impl SettingsLine {
    fn has_fields(self) -> bool {
        self.0.availability != SettingsReadbackAvailability::Available
            || self.0.entries.into_iter().any(|entry| entry.is_some())
    }
}

impl fmt::Display for SettingsLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fields = CommandFieldWriter::new(f, "settings");
        if self.0.availability != SettingsReadbackAvailability::Available {
            fields.write_str_field(
                "availability",
                settings_readback_availability_name(self.0.availability),
            )?;
        }
        for entry in self.0.entries.into_iter().flatten() {
            fields.write_raw_field(entry.field)?;
        }
        Ok(())
    }
}

fn settings_readback_availability_name(availability: SettingsReadbackAvailability) -> &'static str {
    match availability {
        SettingsReadbackAvailability::Available => "available",
        SettingsReadbackAvailability::Unavailable => "unavailable",
        SettingsReadbackAvailability::Unsupported => "unsupported",
    }
}

fn battery_readback_availability_name(availability: BatteryReadbackAvailability) -> &'static str {
    match availability {
        BatteryReadbackAvailability::Available => "available",
        BatteryReadbackAvailability::Unavailable => "unavailable",
        BatteryReadbackAvailability::Unsupported => "unsupported",
    }
}

fn fault_history_availability_name(
    availability: cutout_core::FaultHistoryAvailability,
) -> &'static str {
    match availability {
        cutout_core::FaultHistoryAvailability::Available => "available",
        cutout_core::FaultHistoryAvailability::Unavailable => "unavailable",
        cutout_core::FaultHistoryAvailability::Unsupported => "unsupported",
    }
}

struct CommandFieldWriter<'formatter, 'output> {
    output: &'formatter mut fmt::Formatter<'output>,
    prefix: &'static str,
    fields: FieldCount,
}

impl<'formatter, 'output> CommandFieldWriter<'formatter, 'output> {
    const fn new(output: &'formatter mut fmt::Formatter<'output>, prefix: &'static str) -> Self {
        Self {
            output,
            prefix,
            fields: FieldCount::empty(),
        }
    }

    fn write_measured<T: fmt::Display>(
        &mut self,
        name: &'static str,
        measured: Option<Measured<T>>,
    ) -> fmt::Result {
        if let Some(measured) = measured {
            self.write_field_name(name)?;
            write!(self.output, "{}", measured.value)?;
        }
        Ok(())
    }

    fn write_raw_field(&mut self, field: cutout_core::RawFieldValue) -> fmt::Result {
        if self.fields.is_empty() {
            write!(self.output, "{} ", self.prefix)?;
        } else {
            write!(self.output, " ")?;
        }
        self.fields = self.fields.increment();
        write!(self.output, "raw_{:04x}={}", field.id, field.value)
    }

    fn write_str_field(&mut self, name: &'static str, value: &'static str) -> fmt::Result {
        self.write_field_name(name)?;
        write!(self.output, "{value}")
    }

    fn write_field_name(&mut self, name: &'static str) -> fmt::Result {
        if self.fields.is_empty() {
            write!(self.output, "{} {name}=", self.prefix)?;
        } else {
            write!(self.output, " {name}=")?;
        }
        self.fields = self.fields.increment();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct FieldCount(usize);

impl FieldCount {
    const fn empty() -> Self {
        Self(0)
    }

    const fn is_empty(self) -> bool {
        self.0 == 0
    }

    const fn increment(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, thread};

    use btleplug::api::CharPropFlags;
    use clap::Parser;
    use cutout_btle::{
        BridgeIdentityConfidence, BridgeIdentityEvidence, BridgeIdentityEvidenceKind,
        BridgeIdentityResolution, CapturedBtlePacket, ConnectionSummary, ConnectionTarget,
        PeripheralObservation, RawNotificationRecord, ServiceSummary, SessionCaptureRecord,
    };
    use cutout_core::{
        DeviceEvent, GattChannel, MonotonicTimestamp, NotificationByteLen, ParserDiagnosticCount,
        ParserDroppedBytes, ParserFrameLen, PayloadBodyLen, PevcapHeader, PevcapRecord,
        ProtocolFamily, ProtocolSelector, SemanticEventCount, SessionInput, SignalStrength,
        TransportWriteLimit, VerificationStatus, VerifiedValue, WriteMode,
    };
    use uuid::Uuid;

    use super::*;
    use crate::cli::ScanArgs;

    const fn ms(value: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::new(value)
    }

    const fn wc(value: u64) -> WallClockUnixTimestamp {
        WallClockUnixTimestamp::new(value)
    }

    const fn rssi(value: i16) -> SignalStrength {
        SignalStrength::from_dbm(value)
    }

    const fn write_len(value: u16) -> TransportWriteLimit {
        TransportWriteLimit::from_bytes(value)
    }

    const fn dropped_bytes(value: u64) -> ParserDroppedBytes {
        ParserDroppedBytes::from_bytes(value)
    }

    const fn diag_count(value: u64) -> ParserDiagnosticCount {
        ParserDiagnosticCount::from_events(value)
    }

    fn battery_response(payload: BatteryPagePayload) -> ReadOnlyResponse {
        ReadOnlyResponse::Battery(BatteryReadback::available(payload))
    }

    fn available_battery_page(response: &ReadOnlyResponse) -> Option<BatteryPagePayload> {
        match response {
            ReadOnlyResponse::Battery(readback) => readback.page,
            _ => None,
        }
    }

    const fn frame_len(value: usize) -> ParserFrameLen {
        ParserFrameLen::from_bytes(value)
    }

    struct DropSignal(mpsc::Sender<()>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct RecordingSession;

    impl cutout_core::ProtocolSession for RecordingSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(link) => {
                    output.push(SessionOutput::Event(DeviceEvent::LinkUp(link)));
                }
                SessionInput::LinkDown => {
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown));
                }
                SessionInput::Notification {
                    channel,
                    bytes,
                    monotonic_ms,
                } => output.push(SessionOutput::NotificationIngest(
                    cutout_core::NotificationIngestOutcome::ignored_wrong_channel(
                        channel,
                        NotificationByteLen::from_bytes(bytes.len()),
                        monotonic_ms,
                    ),
                )),
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                }
                SessionInput::Command(_) => {}
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct OverflowingReplaySession;

    impl cutout_core::ProtocolSession for OverflowingReplaySession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            let SessionInput::Notification { .. } = input else {
                return;
            };

            output.extend(
                (0..=cutout_core::DEFAULT_REPLAY_OUTPUT_LIMIT.as_outputs()).map(|offset_ms| {
                    SessionOutput::Event(DeviceEvent::Tick {
                        monotonic_ms: ms(offset_ms as u64),
                    })
                }),
            );
        }
    }

    fn dashboard_args(demo: bool, device: Option<&str>) -> DashboardArgs {
        DashboardArgs {
            demo,
            pevcap: None,
            pevcap_format: PevcapFormat::Jsonl,
            device: device.map(ToOwned::to_owned),
            scan: ScanArgs { seconds: 5 },
        }
    }

    fn sample_pevcap_capture() -> PevcapCapture {
        let service = GattChannel::from_bytes([0xFE; 16]);
        let characteristic = GattChannel::from_bytes([0xE1; 16]);
        let header = PevcapHeader::new(
            wc(1_725_000_123_456),
            "darwin",
            Some(write_len(182)),
            &[service],
            &[],
            None,
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::BegodeGotway),
                model: Some(VerifiedValue {
                    value: "Begode Falcon".to_owned(),
                    verification: VerificationStatus::Inferred,
                }),
                firmware: None,
            }),
            "0.1.0",
            [0x42; 32],
            &["review"],
        )
        .expect("header should validate");

        PevcapCapture::new(
            header,
            vec![
                PevcapRecord::outbound_write(
                    ms(7),
                    characteristic,
                    WriteMode::WithoutResponse,
                    b"N".to_vec(),
                ),
                PevcapRecord::inbound_notification(
                    ms(9),
                    characteristic,
                    service,
                    b"NAME=Falcon".to_vec(),
                ),
            ],
        )
    }

    fn sample_falcon_live_a_replay_capture(annotations: &[&str]) -> PevcapCapture {
        sample_falcon_replay_capture_with_records(
            annotations,
            vec![PevcapRecord::inbound_notification(
                ms(42),
                BEGODE_DATA_CHANNEL,
                BEGODE_DATA_CHANNEL,
                hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a").to_vec(),
            )],
        )
    }

    fn sample_falcon_replay_capture_with_records(
        annotations: &[&str],
        records: Vec<PevcapRecord>,
    ) -> PevcapCapture {
        let header = PevcapHeader::new(
            wc(1_725_000_123_456),
            "darwin",
            Some(write_len(182)),
            &[BEGODE_DATA_CHANNEL],
            &[],
            None,
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::BegodeGotway),
                model: Some(VerifiedValue {
                    value: "Begode Falcon".to_owned(),
                    verification: VerificationStatus::Inferred,
                }),
                firmware: None,
            }),
            "0.1.0",
            [0x42; 32],
            annotations,
        )
        .expect("header should validate");

        PevcapCapture::new(header, records)
    }

    fn split_falcon_bms_summary_pevcap_records() -> Vec<PevcapRecord> {
        const BMS_SUMMARY: [u8; 24] =
            hex_literal::hex!("55aa2710000003b6ff9c0019001a0190000001035a5a5a5a");
        vec![
            PevcapRecord::inbound_notification(
                ms(41),
                BEGODE_DATA_CHANNEL,
                BEGODE_DATA_CHANNEL,
                BMS_SUMMARY[..20].to_vec(),
            ),
            PevcapRecord::inbound_notification(
                ms(42),
                BEGODE_DATA_CHANNEL,
                BEGODE_DATA_CHANNEL,
                BMS_SUMMARY[20..].to_vec(),
            ),
        ]
    }

    fn split_falcon_bms_summary_session_records() -> Vec<SessionCaptureRecord> {
        const BMS_SUMMARY: [u8; 24] =
            hex_literal::hex!("55aa2710000003b6ff9c0019001a0190000001035a5a5a5a");
        let service = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
        let characteristic = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
        vec![
            SessionCaptureRecord::Notification {
                monotonic_ms: MonotonicMs::new(41),
                characteristic,
                service,
                bytes: CapturedBtlePacket::from_raw_bytes(bytes::Bytes::copy_from_slice(
                    &BMS_SUMMARY[..20],
                )),
            },
            SessionCaptureRecord::Notification {
                monotonic_ms: MonotonicMs::new(42),
                characteristic,
                service,
                bytes: CapturedBtlePacket::from_raw_bytes(bytes::Bytes::copy_from_slice(
                    &BMS_SUMMARY[20..],
                )),
            },
        ]
    }

    fn falcon_connection_summary() -> ConnectionSummary {
        ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "falcon-uuid".to_owned(),
                address: None,
                name: Some("GotWay_002441".to_owned()),
                rssi: Some(rssi(-67)),
                advertised_services: vec![Uuid::from_u128(
                    0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
                )]
                .into(),
                manufacturer_data: Vec::new().into(),
            },
            services: vec![ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![cutout_btle::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }]
                .into(),
            }]
            .into(),
        }
    }

    fn sample_aero_replay_capture() -> PevcapCapture {
        let header = PevcapHeader::new(
            wc(1_725_000_123_456),
            "darwin",
            Some(write_len(23)),
            &[VETERAN_DATA_CHANNEL],
            &[],
            None,
            None,
            "0.1.0",
            [0x24; 32],
            &["aero replay"],
        )
        .expect("header should validate");
        PevcapCapture::new(
            header,
            vec![PevcapRecord::inbound_notification(
                ms(42),
                VETERAN_DATA_CHANNEL,
                VETERAN_DATA_CHANNEL,
                hex_literal::hex!(
                    "dc5a5c5f2a09000000170000ab6c001700000bea\
                     045c00000226021ca8f607801b1f000080c80000\
                     808080808080030689065706a20686067c06f700\
                     00000000000000000000000e0e0e0200000000a5\
                     11000053f401c50000000000bffffaf33f9782"
                )
                .to_vec(),
            )],
        )
    }

    fn sample_aero_reserved_replay_capture() -> PevcapCapture {
        let header = PevcapHeader::new(
            wc(1_725_000_123_456),
            "darwin",
            Some(write_len(23)),
            &[VETERAN_DATA_CHANNEL],
            &[],
            None,
            None,
            "0.1.0",
            [0x25; 32],
            &["aero selector 8 replay"],
        )
        .expect("header should validate");
        PevcapCapture::new(
            header,
            vec![PevcapRecord::inbound_notification(
                ms(42),
                VETERAN_DATA_CHANNEL,
                VETERAN_DATA_CHANNEL,
                hex_literal::hex!(
                    "dc5a5c4729f2000000170000ab6c001700000be9\
                     045a00000226021ca8f607801b25000080c80000\
                     808080808080080000803200364f371e00000100\
                     808028062e7964800080801540e23a"
                )
                .to_vec(),
            )],
        )
    }

    #[test]
    fn pevcap_converter_turns_jsonl_into_binary_container() {
        let capture = sample_pevcap_capture();
        let jsonl = capture.to_jsonl().expect("sample serializes");

        let binary =
            convert_pevcap_bytes(jsonl.as_bytes(), PevcapFormat::Jsonl, PevcapFormat::Binary)
                .expect("JSONL converts to binary");
        let decoded = PevcapCapture::from_binary(&binary).expect("binary decodes");

        assert_eq!(decoded, capture);
    }

    #[test]
    fn pevcap_converter_turns_binary_container_into_jsonl() {
        let capture = sample_pevcap_capture();
        let binary = capture.to_binary().expect("sample serializes");

        let jsonl = convert_pevcap_bytes(&binary, PevcapFormat::Binary, PevcapFormat::Jsonl)
            .expect("binary converts to JSONL");
        let decoded =
            PevcapCapture::from_jsonl(std::str::from_utf8(&jsonl).expect("JSONL is UTF-8"))
                .expect("JSONL decodes");

        assert_eq!(decoded, capture);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn cli_encodes_session_capture_to_pevcap_bytes() {
        let summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "cb-uuid".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: Some(rssi(-67)),
                advertised_services: vec![Uuid::from_u128(
                    0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
                )]
                .into(),
                manufacturer_data: Vec::new().into(),
            },
            services: vec![ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![cutout_btle::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }]
                .into(),
            }]
            .into(),
        };
        let capture = SessionCapture {
            records: vec![
                SessionCaptureRecord::Link {
                    monotonic_ms: MonotonicMs::new(0),
                    max_write_len: Some(cutout_btle::NegotiatedWriteLimit::from_bytes(23)),
                },
                SessionCaptureRecord::Write {
                    monotonic_ms: MonotonicMs::new(2),
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteMode::WithoutResponse,
                    bytes: CapturedBtlePacket::from_raw_bytes(bytes::Bytes::from_static(b"N")),
                    provenance: WriteProvenance::Stable,
                },
                SessionCaptureRecord::Notification {
                    monotonic_ms: MonotonicMs::new(3),
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    bytes: CapturedBtlePacket::from_raw_bytes(bytes::Bytes::from_static(
                        b"NAME=NF2557",
                    )),
                },
            ],
            report: SessionBridgeReport::default(),
        };

        let bytes = encode_session_capture_pevcap(
            &capture,
            &summary,
            PevcapFormat::Binary,
            wc(42),
            selected_aero_session_profile(),
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
                model: None,
                firmware: None,
            }),
            &["capture_label=charging", "capture_privacy=private"],
        )
        .expect("capture encodes");
        let decoded =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary PEVCAP decodes");

        assert_eq!(decoded.header.wall_clock_start_unix_ms, wc(42));
        assert_eq!(decoded.header.write_limit, Some(write_len(23)));
        assert_eq!(
            decoded.header.annotations.as_slice(),
            &[
                "cutout-cli capture".to_owned(),
                "capture_label=charging".to_owned(),
                "capture_privacy=private".to_owned(),
            ]
        );
        assert_eq!(
            decoded.header.resolver_evidence.as_slice(),
            &[
                "selected_session_key=nosfet-aero-read-only".to_owned(),
                "resolved_protocol_family=VeteranLeaperkimNosfet".to_owned(),
            ]
        );
        assert!(decoded.header.resolver_warnings.is_empty());
        assert_eq!(
            decoded.header.registry_hash,
            cutout_core::registry_entries_hash(&[&BEGODE_FALCON_REGISTRY_ENTRY])
        );
        assert_ne!(decoded.header.registry_hash, [0; 32]);
        assert_eq!(
            decoded
                .header
                .resolved_identity
                .as_ref()
                .and_then(|identity| identity.protocol_family),
            Some(ProtocolFamily::VeteranLeaperkimNosfet)
        );
        assert_eq!(decoded.records.len(), 3);
        assert_eq!(decoded.records[0].direction, PevcapDirection::LinkUp);
        assert_eq!(
            decoded.records[1].write_mode,
            Some(WriteMode::WithoutResponse)
        );
        assert_eq!(decoded.records[2].bytes.as_ref(), b"NAME=NF2557");
    }

    #[test]
    fn capture_pevcap_records_falcon_resolver_warning_when_voltage_evidence_is_missing() {
        let summary = falcon_connection_summary();
        let capture = SessionCapture {
            records: vec![
                SessionCaptureRecord::Link {
                    monotonic_ms: MonotonicMs::new(0),
                    max_write_len: Some(cutout_btle::NegotiatedWriteLimit::from_bytes(23)),
                },
                SessionCaptureRecord::Notification {
                    monotonic_ms: MonotonicMs::new(7),
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    bytes: CapturedBtlePacket::from_raw_bytes(bytes::Bytes::from_static(
                        b"GW1621003",
                    )),
                },
            ],
            report: SessionBridgeReport::default(),
        };

        let bytes = encode_session_capture_pevcap(
            &capture,
            &summary,
            PevcapFormat::Binary,
            wc(42),
            selected_falcon_session_profile(),
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::BegodeGotway),
                model: Some(VerifiedValue {
                    value: "Begode Falcon".to_owned(),
                    verification: VerificationStatus::Inferred,
                }),
                firmware: None,
            }),
            &[
                "capture_label=powered_on_stationary",
                "capture_privacy=private",
            ],
        )
        .expect("falcon capture encodes");
        let decoded =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary PEVCAP decodes");

        assert_eq!(
            decoded.header.resolver_evidence.as_slice(),
            &[
                "selected_session_key=begode-falcon-read-only".to_owned(),
                "resolved_protocol_family=BegodeGotway".to_owned(),
                "resolved_model=Begode Falcon".to_owned(),
            ]
        );
        assert_eq!(
            decoded.header.resolver_warnings.as_slice(),
            &[
                "missing_falcon_battery_voltage_evidence".to_owned(),
                "missing_falcon_battery_capacity_evidence".to_owned(),
                "missing_falcon_battery_layout_evidence".to_owned(),
                "falcon_battery_evidence_incomplete".to_owned(),
            ]
        );
    }

    #[test]
    fn capture_pevcap_records_falcon_resolver_warnings_include_capacity_and_layout_status() {
        let summary = falcon_connection_summary();
        let capture = SessionCapture {
            records: vec![],
            report: SessionBridgeReport::default(),
        };

        let bytes = encode_session_capture_pevcap(
            &capture,
            &summary,
            PevcapFormat::Binary,
            wc(42),
            selected_falcon_session_profile(),
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::BegodeGotway),
                model: Some(VerifiedValue {
                    value: "Begode Falcon".to_owned(),
                    verification: VerificationStatus::Inferred,
                }),
                firmware: None,
            }),
            &[
                "battery=84v",
                "nominal_capacity_mah=10000",
                "reported_wh=900",
            ],
        )
        .expect("falcon capture encodes");
        let decoded =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary PEVCAP decodes");

        assert!(
            decoded
                .header
                .resolver_warnings
                .contains(&"missing_falcon_battery_layout_evidence".to_owned())
        );
        assert!(
            decoded
                .header
                .resolver_warnings
                .contains(&"falcon_battery_evidence_incomplete".to_owned())
        );
    }

    #[test]
    fn capture_pevcap_uses_chunked_falcon_bms_voltage_evidence() {
        let summary = falcon_connection_summary();
        let capture = SessionCapture {
            records: split_falcon_bms_summary_session_records(),
            report: SessionBridgeReport::default(),
        };

        let bytes = encode_session_capture_pevcap(
            &capture,
            &summary,
            PevcapFormat::Binary,
            wc(42),
            selected_falcon_session_profile(),
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::BegodeGotway),
                model: Some(VerifiedValue {
                    value: "Begode Falcon".to_owned(),
                    verification: VerificationStatus::Inferred,
                }),
                firmware: None,
            }),
            &["capture_label=charging"],
        )
        .expect("falcon capture encodes");
        let decoded =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary PEVCAP decodes");

        assert!(
            decoded
                .header
                .resolver_evidence
                .contains(&"bms_voltage=95000".to_owned())
        );
        assert!(
            !decoded
                .header
                .resolver_warnings
                .contains(&"missing_falcon_battery_voltage_evidence".to_owned())
        );
        assert!(
            decoded
                .header
                .resolver_warnings
                .contains(&"missing_falcon_battery_capacity_evidence".to_owned())
        );
    }

    #[test]
    fn reconnect_capture_maps_probe_commands_for_first_link_policy() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture",
            "--name-contains",
            "NF2557",
            "--reconnect-attempts",
            "2",
            "--probe",
            "identity",
        ])
        .expect("parser accepts reconnect probe request");
        let Command::Capture(args) = cli.command else {
            panic!("expected capture command");
        };

        assert_eq!(
            read_probe_commands(args.target.probes()),
            vec![DeviceCommand::RequestIdentity]
        );
    }

    #[test]
    fn reconnect_summary_merge_preserves_later_gatt_evidence() {
        let ffe0 = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
        let ffe1 = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
        let ffe2 = Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb);
        let battery = Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb);
        let battery_level = Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb);
        let first = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "first-link".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: Some(rssi(-67)),
                advertised_services: vec![ffe0].into(),
                manufacturer_data: Vec::new().into(),
            },
            services: vec![ServiceSummary {
                uuid: ffe0,
                primary: true,
                characteristics: vec![cutout_btle::CharacteristicSummary {
                    uuid: ffe1,
                    service_uuid: ffe0,
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }]
                .into(),
            }]
            .into(),
        };
        let second = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "second-link".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: Some(rssi(-70)),
                advertised_services: vec![ffe0, battery].into(),
                manufacturer_data: Vec::new().into(),
            },
            services: vec![
                ServiceSummary {
                    uuid: ffe0,
                    primary: true,
                    characteristics: vec![cutout_btle::CharacteristicSummary {
                        uuid: ffe2,
                        service_uuid: ffe0,
                        properties: CharPropFlags::READ,
                    }]
                    .into(),
                },
                ServiceSummary {
                    uuid: battery,
                    primary: true,
                    characteristics: vec![cutout_btle::CharacteristicSummary {
                        uuid: battery_level,
                        service_uuid: battery,
                        properties: CharPropFlags::READ,
                    }]
                    .into(),
                },
            ]
            .into(),
        };

        let merged = merge_reconnect_summaries([&first, &second]).expect("summaries merge");

        assert_eq!(merged.observation.identifier, "first-link");
        assert_eq!(
            merged.observation.advertised_services.as_slice(),
            [ffe0, battery]
        );
        assert_eq!(merged.services.len(), 2);
        assert_eq!(
            merged
                .services
                .iter()
                .find(|service| service.uuid == ffe0)
                .expect("ffe0 service")
                .characteristics
                .len(),
            2
        );
        assert!(
            merged
                .services
                .iter()
                .any(|service| service.uuid == battery)
        );
    }

    #[test]
    fn cli_encodes_raw_notifications_to_pevcap_bytes() {
        let service = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
        let characteristic = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
        let summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "cb-uuid".to_owned(),
                address: None,
                name: Some("Unknown PEV".to_owned()),
                rssi: Some(rssi(-67)),
                advertised_services: vec![service].into(),
                manufacturer_data: Vec::new().into(),
            },
            services: vec![ServiceSummary {
                uuid: service,
                primary: true,
                characteristics: vec![cutout_btle::CharacteristicSummary {
                    uuid: characteristic,
                    service_uuid: service,
                    properties: CharPropFlags::NOTIFY,
                }]
                .into(),
            }]
            .into(),
        };
        let records = [RawNotificationRecord {
            monotonic_ms: MonotonicMs::new(7),
            characteristic,
            service,
            bytes: CapturedBtlePacket::from_raw_bytes(bytes::Bytes::from_static(&[
                0xde, 0xad, 0xbe, 0xef,
            ])),
        }];

        let bytes = encode_raw_capture_pevcap(
            &records,
            &summary,
            Some(185),
            PevcapFormat::Binary,
            wc(99),
            &[
                "capture_label=powered_on_stationary",
                "capture_privacy=private",
            ],
        )
        .expect("raw capture encodes");
        let decoded =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary PEVCAP decodes");

        assert_eq!(decoded.header.wall_clock_start_unix_ms, wc(99));
        assert_eq!(decoded.header.write_limit, Some(write_len(185)));
        assert_eq!(
            decoded.header.advertised_services.as_slice(),
            &[gatt_channel_from_uuid(service)]
        );
        assert_eq!(decoded.header.gatt_fingerprints.len(), 1);
        assert_eq!(decoded.header.resolved_identity, None);
        assert_eq!(
            decoded.header.annotations.as_slice(),
            &[
                "cutout-cli subscribe-raw".to_owned(),
                "capture_label=powered_on_stationary".to_owned(),
                "capture_privacy=private".to_owned(),
            ]
        );
        assert_eq!(decoded.replay_input_count(), 3);

        let mut host = HostSession::new(RecordingSession);
        let mut replay = Vec::with_capacity(3);
        decoded.replay_into_host(&mut host, &mut replay);

        assert!(matches!(
            replay[0],
            SessionOutput::Event(DeviceEvent::LinkUp(link))
                if link.monotonic_ms == ms(0) && link.max_write_len == Some(write_len(185))
        ));
        assert!(matches!(
            &replay[1],
            SessionOutput::NotificationIngest(
                cutout_core::NotificationIngestOutcome::Ignored {
                    evidence,
                    reason: cutout_core::IgnoredNotificationReason::WrongChannel,
                }
            ) if evidence.monotonic_ms == ms(7) && evidence.len == NotificationByteLen::from_bytes(4)
        ));
        assert!(matches!(
            replay[2],
            SessionOutput::Event(DeviceEvent::LinkDown)
        ));
    }

    #[test]
    fn pevcap_replay_report_renders_counts() {
        let report = PevcapReplayReport {
            replay_records: ReplayRecordCount::new(2),
            outputs: ReplayOutputCount::new(3),
            telemetry: ReplayTelemetryCount::new(1),
            read_only_responses: ReplayReadOnlyResponseCount::new(1),
            diagnostics: ReplayDiagnosticCount::new(1),
            arbitrary_chunk_plan_len: ReplayChunkPlanLen::new(3),
            chunk_one_byte_matches: true,
            chunk_arbitrary_matches: true,
            telemetry_snapshot: TelemetrySnapshot::default(),
            firmware: None,
            capacity: BegodeCapacitySelection::Missing,
            layout: BegodePackLayoutSelection::Missing,
            pack_evidence_consistency: None,
            diagnostic_snapshots: Vec::new(),
            diagnostic_errors: Vec::new(),
            read_only_response_events: Vec::new(),
            events: Vec::new(),
        };

        assert_eq!(
            render_pevcap_replay_report(&report).to_string(),
            "pevcap replay records=2 outputs=3 telemetry=1 read_only_responses=1 diagnostics=1 arbitrary_chunk_plan_len=3 chunk_one_byte_matches=true chunk_arbitrary_matches=true"
        );
    }

    #[test]
    fn pevcap_replay_reports_output_overflow_with_capture_context() {
        let capture = sample_pevcap_capture();
        let expected_records = capture.replay_input_count();
        let expected_chunk_plan_len = capture.arbitrary_notification_chunk_lengths().len();
        let error = replay_pevcap_with_session(&capture, OverflowingReplaySession)
            .expect_err("overflowing replay should fail");
        let message = format!("{error:#}");

        assert!(
            message.contains("PEVCAP replay chunk comparison failed"),
            "missing replay context: {message}"
        );
        assert!(
            message.contains(&format!("replay_records={expected_records}")),
            "missing capture record count: {message}"
        );
        assert!(
            message.contains(&format!(
                "arbitrary_chunk_plan_len={expected_chunk_plan_len}"
            )),
            "missing arbitrary chunk plan length: {message}"
        );
        assert!(
            message.contains("session output count"),
            "missing overflow source: {message}"
        );
    }

    #[test]
    fn diagnostic_snapshot_jsonl_uses_stable_snake_case_fields() {
        let line = render_diagnostic_snapshot_jsonl(
            JsonSequence::new(7),
            DiagnosticSnapshot {
                dropped_bytes: dropped_bytes(11),
                resyncs: diag_count(2),
                bad_checksums: diag_count(3),
                timeouts: diag_count(5),
                oversized_frames: diag_count(8),
                malformed_frames: diag_count(13),
                unmatched_replies: diag_count(21),
            },
        )
        .expect("diagnostic snapshot serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("diagnostic JSONL is JSON");
        assert_eq!(value["type"], "diagnostic_snapshot");
        assert_eq!(value["sequence"], 7);
        assert_eq!(value["dropped_bytes"], 11);
        assert!(
            value.get("dropped").is_none(),
            "byte counts must keep unit-bearing field names: {value}"
        );
        assert_eq!(value["resyncs"], 2);
        assert_eq!(value["bad_checksums"], 3);
        assert_eq!(value["timeouts"], 5);
        assert_eq!(value["oversized_frames"], 8);
        assert_eq!(value["malformed_frames"], 13);
        assert_eq!(value["unmatched_replies"], 21);
    }

    #[test]
    fn read_only_battery_jsonl_preserves_page_metadata_and_measured_values() {
        let response = battery_response(
            BatteryPagePayload::raw(
                cutout_core::BatteryPageMetadata::raw(
                    ProtocolSelector::new(8),
                    VerificationStatus::SourceVerified,
                ),
                cutout_core::BatteryInfo {
                    voltage: Some(voltage(80_000)),
                    current: Some(battery_current(-10_000)),
                    level_reported: None,
                    level_estimated: Some(level_estimated(61)),
                    temperature: Some(temperature(25_000)),
                    raw_state: Some(cutout_core::RawFieldValue::new(0x0008, 0x55aa)),
                },
            )
            .with_bms_pack_currents(cutout_core::BmsPackCurrents::reported(
                cutout_core::BatteryCurrent::from_milliamps(-1_230),
                cutout_core::BatteryCurrent::from_milliamps(450),
            )),
        );

        let line = render_read_only_response_jsonl(JsonSequence::new(2), response)
            .expect("read-only battery response serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("read-only response JSONL is JSON");
        assert_eq!(value["type"], "read_only_response");
        assert_eq!(value["sequence"], 2);
        assert_eq!(value["command_kind"], "request_battery_info");
        assert_eq!(value["response"], "battery");
        assert_eq!(value["availability"], "available");
        assert_eq!(value["page"]["selector"], 8);
        assert_eq!(value["page"]["side"], serde_json::Value::Null);
        assert_eq!(value["page"]["kind"], "raw");
        assert_eq!(value["page"]["verification"], "source_verified");
        assert_eq!(value["page"]["battery"]["voltage"]["value"], 80_000);
        assert_eq!(value["page"]["battery"]["voltage"]["source"], "reported");
        assert_eq!(value["page"]["battery"]["voltage"]["quality"], "known");
        assert_eq!(
            value["page"]["battery"]["voltage"]["verification"],
            "hardware_verified"
        );
        assert_eq!(value["page"]["battery"]["current"]["value"], -10_000);
        assert_eq!(
            value["page"]["battery"]["bms_pack_current_0"]["value"],
            -1_230
        );
        assert_eq!(value["page"]["battery"]["bms_pack_current_1"]["value"], 450);
        assert_eq!(
            value["page"]["battery"]["level_reported"],
            serde_json::Value::Null
        );
        assert_eq!(value["page"]["battery"]["level_estimated"]["value"], 61);
        assert_eq!(
            value["page"]["battery"]["level_estimated"]["source"],
            "estimated"
        );
        assert_eq!(value["page"]["battery"]["temperature"]["value"], 25_000);
        assert_eq!(value["page"]["battery"]["raw_state"]["id"], 8);
        assert_eq!(value["page"]["battery"]["raw_state"]["value"], 0x55aa);
        assert_eq!(value["page"]["temperatures"], serde_json::Value::Null);
    }

    #[test]
    fn read_only_battery_jsonl_preserves_temperature_page_values() {
        let temperatures = [
            Some(temperature(16_730)),
            Some(temperature(17_030)),
            Some(temperature(17_330)),
            Some(temperature(17_060)),
            Some(temperature(17_080)),
            Some(temperature(17_830)),
        ];
        let response = battery_response(BatteryPagePayload::temperature_values(
            cutout_core::BatteryPageMetadata::temperature(
                ProtocolSelector::new(3),
                VerificationStatus::HardwareVerified,
            ),
            cutout_core::BatteryInfo {
                temperature: Some(temperature(16_730)),
                ..cutout_core::BatteryInfo::default()
            },
            temperatures,
        ));

        let line = render_read_only_response_jsonl(JsonSequence::new(3), response)
            .expect("read-only temperature battery response serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("read-only response JSONL is JSON");
        assert_eq!(value["type"], "read_only_response");
        assert_eq!(value["sequence"], 3);
        assert_eq!(value["page"]["selector"], 3);
        assert_eq!(value["page"]["side"], "left");
        assert_eq!(value["page"]["kind"], "temperature");
        assert_eq!(value["page"]["verification"], "hardware_verified");
        assert_eq!(value["availability"], "available");
        assert_eq!(value["page"]["battery"]["temperature"]["value"], 16_730);
        assert_eq!(value["page"]["temperatures"][0]["value"], 16_730);
        assert_eq!(value["page"]["temperatures"][5]["value"], 17_830);
    }

    #[test]
    fn read_only_battery_jsonl_labels_selector_seven_as_right() {
        let temperatures = [
            Some(temperature(16_030)),
            Some(temperature(15_630)),
            Some(temperature(16_230)),
            Some(temperature(16_930)),
            Some(temperature(16_530)),
            Some(temperature(17_430)),
        ];
        let response = battery_response(BatteryPagePayload::temperature_values(
            cutout_core::BatteryPageMetadata::temperature(
                ProtocolSelector::new(7),
                VerificationStatus::HardwareVerified,
            ),
            cutout_core::BatteryInfo {
                temperature: Some(temperature(16_030)),
                ..cutout_core::BatteryInfo::default()
            },
            temperatures,
        ));

        let line = render_read_only_response_jsonl(JsonSequence::new(4), response)
            .expect("read-only temperature battery response serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("read-only response JSONL is JSON");
        assert_eq!(value["page"]["selector"], 7);
        assert_eq!(value["page"]["side"], "right");
        assert_eq!(value["page"]["kind"], "temperature");
        assert_eq!(value["page"]["verification"], "hardware_verified");
    }

    #[test]
    fn pevcap_replay_summary_collects_read_only_response_events() {
        let response = battery_response(BatteryPagePayload::raw(
            cutout_core::BatteryPageMetadata::raw(
                ProtocolSelector::new(8),
                VerificationStatus::SourceVerified,
            ),
            cutout_core::BatteryInfo::default(),
        ));
        let outputs = [SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
            response,
        ))];

        let report = summarize_pevcap_replay(
            ReplayRecordCount::new(1),
            ReplayChunkPlanLen::new(1),
            &outputs,
            ReplayChunkComparison {
                whole_semantic_events: SemanticEventCount::from_events(1),
                one_byte_semantic_events: SemanticEventCount::from_events(1),
                arbitrary_semantic_events: SemanticEventCount::from_events(1),
                one_byte_matches: true,
                arbitrary_matches: true,
            },
        );

        assert_eq!(report.read_only_response_events, vec![response]);
    }

    #[test]
    fn pevcap_replay_summary_collects_typed_notification_ingest_events() {
        let outcome = cutout_core::NotificationIngestOutcome::known_reserved(
            ProtocolFamily::VeteranLeaperkimNosfet,
            VETERAN_DATA_CHANNEL,
            NotificationByteLen::from_bytes(75),
            ms(42),
            cutout_core::ReservedPayloadEvidence {
                classifier: cutout_core::PayloadClassifier::selector(ProtocolSelector::new(8)),
                body_len: PayloadBodyLen::from_bytes(24),
                retained_payload: cutout_core::RetainedNotificationPayload::from_bytes(&[0x08]),
                verification: VerificationStatus::HardwareVerified,
            },
        );
        let outputs = [SessionOutput::NotificationIngest(outcome.clone())];

        let report = summarize_pevcap_replay(
            ReplayRecordCount::new(1),
            ReplayChunkPlanLen::new(1),
            &outputs,
            ReplayChunkComparison {
                whole_semantic_events: SemanticEventCount::from_events(0),
                one_byte_semantic_events: SemanticEventCount::from_events(0),
                arbitrary_semantic_events: SemanticEventCount::from_events(0),
                one_byte_matches: true,
                arbitrary_matches: true,
            },
        );

        assert_eq!(
            report.events.as_slice(),
            &[SessionBridgeEvent::NotificationIngest {
                monotonic_ms: MonotonicMs::new(42),
                outcome,
            }]
        );
    }

    #[test]
    fn pevcap_replay_chunk_modes_preserve_final_typed_ingest_outcomes() {
        let capture = sample_aero_reserved_replay_capture();
        let arbitrary_lengths = capture.arbitrary_notification_chunk_lengths();

        let whole = aero_pevcap_ingest_outcomes(&capture, PevcapReplayMode::Whole);
        let one_byte = aero_pevcap_ingest_outcomes(&capture, PevcapReplayMode::OneByte);
        let arbitrary =
            aero_pevcap_ingest_outcomes(&capture, PevcapReplayMode::Arbitrary(&arbitrary_lengths));

        assert!(one_byte[..one_byte.len() - 1].iter().all(|outcome| {
            matches!(
                outcome,
                cutout_core::NotificationIngestOutcome::BufferedFragment(_)
            )
        }));
        assert!(
            arbitrary[..arbitrary.len() - 1].iter().all(|outcome| {
                matches!(
                    outcome,
                    cutout_core::NotificationIngestOutcome::BufferedFragment(_)
                )
            }),
            "arbitrary replay may differ only by explicit buffered progress: {arbitrary:?}"
        );
        assert_eq!(whole.last(), one_byte.last());
        assert_eq!(whole.last(), arbitrary.last());
        assert!(matches!(
            whole.as_slice(),
            [cutout_core::NotificationIngestOutcome::KnownReserved { payload, .. }]
                if payload.classifier.selector_value() == Some(ProtocolSelector::new(8))
        ));
    }

    #[derive(Clone, Copy)]
    enum PevcapReplayMode<'a> {
        Whole,
        OneByte,
        Arbitrary(&'a [cutout_core::NotificationChunkLen]),
    }

    fn aero_pevcap_ingest_outcomes(
        capture: &PevcapCapture,
        mode: PevcapReplayMode<'_>,
    ) -> Vec<cutout_core::NotificationIngestOutcome> {
        let mut host = HostSession::new(
            selected_aero_session_profile()
                .session_registration()
                .expect("registered Aero session exists")
                .construct(),
        );
        let mut outputs = Vec::new();
        match mode {
            PevcapReplayMode::Whole => capture.replay_into_host(&mut host, &mut outputs),
            PevcapReplayMode::OneByte => {
                capture.replay_one_byte_notifications_into_host(&mut host, &mut outputs);
            }
            PevcapReplayMode::Arbitrary(lengths) => {
                capture.replay_notification_chunks_into_host(lengths, &mut host, &mut outputs);
            }
        }
        outputs
            .into_iter()
            .filter_map(|output| match output {
                SessionOutput::NotificationIngest(outcome) => Some(outcome),
                SessionOutput::Transport(_) | SessionOutput::Event(_) => None,
            })
            .collect()
    }

    #[test]
    fn live_session_diagnostics_jsonl_uses_aggregate_report_snapshot() {
        let report = SessionBridgeReport {
            protocol_writes: ProtocolWriteCount::from_events(1),
            writes: TransportWriteCount::from_events(3),
            diagnostics_snapshot: ParserDiagnostics {
                dropped_bytes: dropped_bytes(1),
                resyncs: diag_count(2),
                bad_checksums: diag_count(3),
                timeouts: diag_count(4),
                oversized_frames: diag_count(5),
                malformed_frames: diag_count(6),
                unmatched_replies: diag_count(7),
            },
            diagnostic_errors: vec![DiagnosticError::from_parser_error(
                cutout_core::ParserError::MalformedFrame,
            )],
            ..SessionBridgeReport::default()
        };

        let line = render_session_diagnostics_jsonl(&report)
            .expect("session diagnostics JSONL serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("session diagnostics JSONL is JSON");
        assert_eq!(value["type"], "diagnostic_snapshot");
        assert_eq!(value["sequence"], 0);
        assert_eq!(value["protocol_writes"], 1);
        assert_eq!(value["writes"], 3);
        assert_eq!(value["dropped_bytes"], 1);
        assert_eq!(value["resyncs"], 2);
        assert_eq!(value["bad_checksums"], 3);
        assert_eq!(value["timeouts"], 4);
        assert_eq!(value["oversized_frames"], 5);
        assert_eq!(value["malformed_frames"], 6);
        assert_eq!(value["unmatched_replies"], 7);

        let error_lines = render_diagnostic_errors_jsonl(&report.diagnostic_errors)
            .collect::<Result<Vec<_>, _>>()
            .expect("session diagnostic errors JSONL serializes");
        let error: serde_json::Value =
            serde_json::from_str(&error_lines[0]).expect("diagnostic error JSONL is JSON");
        assert_eq!(error["type"], "diagnostic_error");
        assert_eq!(error["kind"], "malformed_frame");
    }

    #[test]
    fn reconnect_attempt_diagnostics_jsonl_distinguishes_link_attempts() {
        let attempt = ReconnectAttemptReport {
            attempt: cutout_btle::ReconnectAttempt::new(2),
            summary: ConnectionSummary {
                observation: PeripheralObservation {
                    identifier: "NF2557".to_owned(),
                    address: None,
                    name: Some("NF2557".to_owned()),
                    rssi: Some(rssi(-71)),
                    advertised_services: Vec::new().into(),
                    manufacturer_data: Vec::new().into(),
                },
                services: Vec::new().into(),
            },
            report: SessionBridgeReport {
                protocol_writes: ProtocolWriteCount::from_events(2),
                writes: TransportWriteCount::from_events(3),
                subscribes: SubscribeCount::from_events(1),
                notifications: NotificationCount::from_events(8),
                disconnects: DisconnectCount::default(),
                diagnostics_snapshot: ParserDiagnostics {
                    dropped_bytes: dropped_bytes(5),
                    resyncs: diag_count(1),
                    bad_checksums: diag_count(0),
                    timeouts: diag_count(0),
                    oversized_frames: diag_count(0),
                    malformed_frames: diag_count(2),
                    unmatched_replies: diag_count(0),
                },
                ..SessionBridgeReport::default()
            },
        };

        let line = render_reconnect_attempt_diagnostics_jsonl(&attempt)
            .expect("attempt diagnostics JSONL serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("attempt diagnostics JSONL is JSON");
        assert_eq!(value["type"], "reconnect_attempt");
        assert_eq!(value["attempt"], 2);
        assert_eq!(value["identifier"], "NF2557");
        assert_eq!(value["protocol_writes"], 2);
        assert_eq!(value["writes"], 3);
        assert_eq!(value["subscribes"], 1);
        assert_eq!(value["notifications"], 8);
        assert_eq!(value["disconnects"], 0);
        assert_eq!(value["dropped_bytes"], 5);
        assert_eq!(value["resyncs"], 1);
        assert_eq!(value["malformed_frames"], 2);
    }

    #[test]
    fn diagnostic_error_jsonl_preserves_kind_and_fixed_unit_details() {
        let error = DiagnosticError::from_parser_error(cutout_core::ParserError::Timeout {
            elapsed_ms: ms(1_234),
            timeout_ms: ms(5_000),
        });

        let line = render_diagnostic_error_jsonl(JsonSequence::new(3), error)
            .expect("diagnostic error serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("diagnostic error JSONL is JSON");
        assert_eq!(value["type"], "diagnostic_error");
        assert_eq!(value["sequence"], 3);
        assert_eq!(value["kind"], "timeout");
        assert_eq!(value["claimed_len"], serde_json::Value::Null);
        assert_eq!(value["max_len"], serde_json::Value::Null);
        assert_eq!(value["elapsed_ms"], 1_234);
        assert_eq!(value["timeout_ms"], 5_000);
        assert!(
            value.get("elapsed").is_none(),
            "durations must keep unit-bearing field names: {value}"
        );
        assert!(
            value.get("timeout").is_none(),
            "durations must keep unit-bearing field names: {value}"
        );
    }

    #[test]
    fn pevcap_replay_summary_collects_diagnostic_error_events() {
        let error = DiagnosticError::from_parser_error(cutout_core::ParserError::OversizedFrame {
            claimed: frame_len(33),
            max: frame_len(24),
        });
        let outputs = [SessionOutput::Event(DeviceEvent::DiagnosticError(error))];

        let report = summarize_pevcap_replay(
            ReplayRecordCount::new(1),
            ReplayChunkPlanLen::new(1),
            &outputs,
            ReplayChunkComparison {
                whole_semantic_events: SemanticEventCount::from_events(1),
                one_byte_semantic_events: SemanticEventCount::from_events(1),
                arbitrary_semantic_events: SemanticEventCount::from_events(1),
                one_byte_matches: true,
                arbitrary_matches: true,
            },
        );

        assert_eq!(report.diagnostic_errors, vec![error]);
        assert!(report.diagnostic_snapshots.is_empty());
    }

    #[test]
    fn pevcap_replay_drives_selected_aero_session() {
        let capture = sample_aero_replay_capture();

        let report = replay_pevcap_capture(&capture, selected_aero_session_profile())
            .expect("Aero replay does not require Falcon battery evidence");

        assert_eq!(report.replay_records, ReplayRecordCount::new(2));
        assert!(report.arbitrary_chunk_plan_len.get() > 3);
        assert!(report.chunk_one_byte_matches);
        assert!(report.chunk_arbitrary_matches);
        assert!(report.telemetry.get() >= 1);
        assert!(report.read_only_responses.get() >= 1);
        assert_eq!(
            report
                .telemetry_snapshot
                .voltage
                .map(|voltage| voltage.value.get()),
            Some(107_610)
        );
        assert_eq!(
            report
                .firmware
                .and_then(|firmware| firmware.firmware_major.map(|major| major.value)),
            Some(43)
        );
    }

    #[test]
    fn pevcap_replay_builds_aero_dashboard_state() {
        let capture = sample_aero_replay_capture();

        let state = dashboard_state_from_aero_pevcap(&capture)
            .expect("Aero dashboard replay uses existing Aero session");

        assert_eq!(
            state.provenance.as_deref(),
            Some("pevcap replay platform=darwin annotations=aero replay")
        );
        let capture_provenance = state
            .capture_provenance
            .as_ref()
            .expect("dashboard replay tracks capture provenance");
        assert_eq!(capture_provenance.capture_label, None);
        assert_eq!(capture_provenance.capture_privacy, None);
        assert_eq!(capture_provenance.capture_evidence, None);
        assert_eq!(capture_provenance.capture_distribution, None);
        assert_eq!(capture_provenance.platform_id, "darwin");
        assert_eq!(capture_provenance.advertised_service_count, 1);
        assert_eq!(capture_provenance.gatt_fingerprint_count, 0);
        assert_eq!(capture_provenance.selected_session_key.as_deref(), None);
        assert_eq!(state.device.identifier, "darwin");
        assert_eq!(state.device.connection_state, "replayed");
        assert_eq!(
            state.counters.notifications,
            NotificationCount::from_events(1)
        );
        assert_eq!(
            state.counters.notification_bytes,
            NotificationPayloadTotal::from_bytes(99)
        );
        assert_eq!(
            state.counters.latest_notification_len,
            Some(NotificationByteLen::from_bytes(99))
        );
        assert_eq!(
            state
                .telemetry
                .latest_voltage
                .map(crate::dashboard::DisplayVoltage::get),
            Some(108)
        );
        assert!(state.read_only.firmware.is_some());
        assert!(
            state
                .read_only
                .settings
                .iter()
                .any(|setting| setting.verification == VerificationStatus::HardwareVerified)
        );
        assert!(
            state
                .read_only
                .bms_pages
                .iter()
                .any(|page| page.page().kind == BatteryPageKind::Temperature)
        );
        assert_eq!(
            state.read_only.unknown_raw_pages,
            crate::dashboard::RawReadOnlyPageCount::default()
        );
    }

    #[test]
    fn pevcap_replay_dashboard_renders_typed_ingest_outcomes() {
        let capture = sample_aero_reserved_replay_capture();

        let state = dashboard_state_from_aero_pevcap(&capture)
            .expect("Aero dashboard replay uses existing Aero session");

        assert!(
            state
                .logs
                .iter()
                .any(|line| line.message.contains("protocol known reserved")
                    && line.message.contains("selector=8")),
            "dashboard replay logs should include typed selector-8 reserved evidence: {:?}",
            state.logs
        );
        assert!(
            state
                .logs
                .iter()
                .all(|line| !line.message.contains("raw notification")),
            "dashboard replay logs should not recreate raw notification display paths: {:?}",
            state.logs
        );
    }

    #[test]
    fn pevcap_replay_dashboard_state_requires_equivalent_chunk_modes() {
        let capture = sample_aero_replay_capture();
        let report = replay_pevcap_capture(&capture, selected_aero_session_profile())
            .expect("Aero replay does not require Falcon battery evidence");

        assert!(report.chunk_one_byte_matches);
        assert!(report.chunk_arbitrary_matches);
        assert!(dashboard_state_from_aero_pevcap(&capture).is_ok());
    }

    #[test]
    fn pevcap_replay_auto_selects_falcon_from_resolved_identity() {
        let capture = sample_pevcap_capture();

        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects Falcon replay");

        assert_eq!(profile, selected_falcon_session_profile());
    }

    #[test]
    fn pevcap_replay_auto_selects_aero_from_resolved_identity() {
        let mut capture = sample_aero_replay_capture();
        capture.header.resolved_identity = Some(PevcapResolvedIdentity {
            protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
            model: Some(VerifiedValue {
                value: "NOSFET Aero".to_owned(),
                verification: VerificationStatus::Inferred,
            }),
            firmware: None,
        });

        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Aero identity selects Aero replay");

        assert_eq!(profile, selected_aero_session_profile());
    }

    #[test]
    fn pevcap_replay_auto_rejects_missing_identity() {
        let capture = sample_aero_replay_capture();

        let error = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect_err("missing identity should not guess a replay profile");

        assert!(
            error
                .to_string()
                .contains("requires resolved model identity metadata")
        );
    }

    #[test]
    fn pevcap_replay_auto_rejects_unknown_resolved_model() {
        let mut capture = sample_aero_replay_capture();
        capture.header.resolved_identity = Some(PevcapResolvedIdentity {
            protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
            model: Some(VerifiedValue {
                value: "NOSFET Unknown".to_owned(),
                verification: VerificationStatus::Inferred,
            }),
            firmware: None,
        });

        let error = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect_err("unknown model should not guess a replay profile");

        assert!(
            error
                .to_string()
                .contains("found no catalog entry for resolved model NOSFET Unknown")
        );
    }

    #[test]
    fn pevcap_replay_explicit_profile_overrides_missing_identity() {
        let capture = sample_aero_replay_capture();

        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Falcon)
            .expect("explicit profile does not require metadata");

        assert_eq!(profile, selected_falcon_session_profile());
    }

    #[test]
    fn pevcap_replay_uses_falcon_battery_annotation_for_voltage_scaling() {
        let capture = sample_falcon_live_a_replay_capture(&["battery=100.8v"]);

        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile).expect("battery evidence is present");

        assert_eq!(
            report
                .telemetry_snapshot
                .voltage
                .map(|voltage| voltage.value.get()),
            Some(90_075)
        );
    }

    #[test]
    fn pevcap_replay_rejects_missing_falcon_battery_evidence() {
        let capture = sample_falcon_live_a_replay_capture(&["capture_label=powered_on_stationary"]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");

        let error = replay_pevcap_capture(&capture, profile)
            .expect_err("missing Falcon battery evidence should not replay");

        assert!(
            error
                .to_string()
                .contains("requires explicit Falcon battery voltage evidence")
        );
    }

    #[test]
    fn pevcap_replay_does_not_treat_falcon_capacity_as_voltage_evidence() {
        let capture =
            sample_falcon_live_a_replay_capture(&["nominal_capacity_mah=10000", "reported_wh=672"]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");

        let error = replay_pevcap_capture(&capture, profile)
            .expect_err("capacity evidence alone should not select voltage profile");

        assert!(
            error
                .to_string()
                .contains("requires explicit Falcon battery voltage evidence")
        );
    }

    #[test]
    fn pevcap_replay_report_renders_explicit_falcon_capacity_evidence() {
        let capture = sample_falcon_live_a_replay_capture(&[
            "battery=84v",
            "nominal_capacity_mah=10000",
            "reported_wh=672",
        ]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile).expect("voltage evidence is present");

        assert!(
            render_pevcap_replay_report(&report)
                .to_string()
                .contains("capacity_nominal_mah=10000 capacity_reported_wh=672")
        );
    }

    #[test]
    fn pevcap_replay_report_renders_conflicting_falcon_capacity_evidence() {
        let capture = sample_falcon_live_a_replay_capture(&[
            "battery=84v",
            "nominal_capacity_mah=10000",
            "nominal_capacity_mah=9000",
        ]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile)
            .expect("capacity conflict does not block voltage replay");

        assert!(
            render_pevcap_replay_report(&report)
                .to_string()
                .contains("capacity_conflict=true")
        );
    }

    #[test]
    fn pevcap_replay_report_renders_explicit_falcon_pack_layout_evidence() {
        let capture = sample_falcon_live_a_replay_capture(&[
            "battery=84v",
            "cell_model=Samsung 50S",
            "series_cells=20",
            "parallel_count=1",
        ]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile).expect("voltage evidence is present");

        assert!(render_pevcap_replay_report(&report).to_string().contains(
            "layout_cell_model=Samsung 50S layout_series_cells=20 layout_parallel_count=1"
        ));
    }

    #[test]
    fn pevcap_replay_report_renders_conflicting_falcon_pack_layout_evidence() {
        let capture = sample_falcon_live_a_replay_capture(&[
            "battery=84v",
            "series_cells=20",
            "series_cells=24",
        ]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile)
            .expect("layout conflict does not block replay");

        assert!(
            render_pevcap_replay_report(&report)
                .to_string()
                .contains("layout_conflict=true")
        );
    }

    #[test]
    fn pevcap_replay_report_renders_inconsistent_falcon_pack_evidence() {
        let capture = sample_falcon_live_a_replay_capture(&[
            "battery=84v",
            "reported_wh=900",
            "cell_model=Samsung 50S",
            "series_cells=20",
            "parallel_count=2",
        ]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile).expect("voltage evidence is present");

        assert!(
            render_pevcap_replay_report(&report)
                .to_string()
                .contains("pack_evidence_inconsistent=true")
        );
    }

    #[test]
    fn pevcap_replay_report_renders_incomplete_falcon_pack_evidence() {
        let capture = sample_falcon_live_a_replay_capture(&[
            "battery=84v",
            "reported_wh=900",
            "cell_model=Samsung 50S",
            "series_cells=20",
        ]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile).expect("voltage evidence is present");

        assert!(
            render_pevcap_replay_report(&report)
                .to_string()
                .contains("pack_evidence_incomplete=true")
        );
    }

    #[test]
    fn pevcap_replay_does_not_treat_falcon_pack_layout_as_voltage_evidence() {
        let capture = sample_falcon_live_a_replay_capture(&[
            "cell_model=Samsung 50S",
            "series_cells=20",
            "parallel_count=1",
        ]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");

        let error = replay_pevcap_capture(&capture, profile)
            .expect_err("layout evidence alone should not select voltage profile");

        assert!(
            error
                .to_string()
                .contains("requires explicit Falcon battery voltage evidence")
        );
    }

    #[test]
    fn pevcap_replay_rejects_conflicting_falcon_battery_evidence() {
        let capture =
            sample_falcon_live_a_replay_capture(&["battery=84v", "app_voltage_class=100v"]);
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");

        let error = replay_pevcap_capture(&capture, profile)
            .expect_err("conflicting Falcon battery evidence should not replay");

        assert!(
            error
                .to_string()
                .contains("conflicting Falcon battery voltage evidence")
        );
    }

    #[test]
    fn pevcap_replay_uses_falcon_bms_voltage_evidence_for_scaling() {
        let capture = sample_falcon_replay_capture_with_records(
            &[],
            vec![
                PevcapRecord::inbound_notification(
                    ms(41),
                    BEGODE_DATA_CHANNEL,
                    BEGODE_DATA_CHANNEL,
                    hex_literal::hex!("55aa2710000003b6ff9c0019001a0190000001035a5a5a5a").to_vec(),
                ),
                PevcapRecord::inbound_notification(
                    ms(42),
                    BEGODE_DATA_CHANNEL,
                    BEGODE_DATA_CHANNEL,
                    hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a").to_vec(),
                ),
            ],
        );
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile)
            .expect("95V BMS evidence selects Falcon 100.8V profile");

        assert_eq!(
            report
                .telemetry_snapshot
                .voltage
                .map(|voltage| voltage.value.get()),
            Some(90_075)
        );
    }

    #[test]
    fn pevcap_replay_uses_chunked_falcon_bms_voltage_evidence() {
        let capture = sample_falcon_replay_capture_with_records(
            &[],
            split_falcon_bms_summary_pevcap_records(),
        );
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");
        let report = replay_pevcap_capture(&capture, profile)
            .expect("chunked BMS evidence selects Falcon voltage profile");

        assert_eq!(
            report
                .telemetry_snapshot
                .voltage
                .map(|voltage| voltage.value.get()),
            Some(95_000)
        );
    }

    #[test]
    fn pevcap_replay_rejects_ambiguous_falcon_bms_voltage_evidence() {
        let capture = sample_falcon_replay_capture_with_records(
            &[],
            vec![PevcapRecord::inbound_notification(
                ms(41),
                BEGODE_DATA_CHANNEL,
                BEGODE_DATA_CHANNEL,
                hex_literal::hex!("55aa271000000320ff9c0019001a0190000001035a5a5a5a").to_vec(),
            )],
        );
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("Falcon identity selects replay profile");

        let error = replay_pevcap_capture(&capture, profile)
            .expect_err("ambiguous BMS voltage should not select a Falcon profile");

        assert!(
            error
                .to_string()
                .contains("requires explicit Falcon battery voltage evidence")
        );
    }

    #[test]
    fn pevcap_replay_corpus_auto_selects_and_matches_chunk_modes() {
        for case in PEVCAP_REPLAY_CORPUS {
            let capture = PevcapCapture::decode(case.jsonl.as_bytes(), PevcapEncoding::Jsonl)
                .unwrap_or_else(|error| panic!("{} should decode: {error}", case.name));
            let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
                .unwrap_or_else(|error| panic!("{} should auto-select: {error}", case.name));
            let report = replay_pevcap_capture(&capture, profile)
                .unwrap_or_else(|error| panic!("{} should replay: {error}", case.name));

            assert_eq!(profile, case.profile, "{} profile", case.name);
            assert!(
                report.replay_records.get() >= 2,
                "{} replay records",
                case.name
            );
            assert!(
                report.chunk_one_byte_matches,
                "{} one-byte chunks",
                case.name
            );
            assert!(
                report.chunk_arbitrary_matches,
                "{} arbitrary chunks",
                case.name
            );
            assert!(
                report.arbitrary_chunk_plan_len >= case.minimum_chunk_plan_len,
                "{} chunk plan length",
                case.name
            );
        }
    }

    #[test]
    fn pevcap_replay_falcon_riding_capture_parses_all_events() {
        let capture = PevcapCapture::decode(
            include_str!("../fixtures/pevcap/falcon-riding-60s.jsonl").as_bytes(),
            PevcapEncoding::Jsonl,
        )
        .expect("falcon riding capture should decode");
        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect("falcon riding capture should auto-select");
        let report =
            replay_pevcap_capture(&capture, profile).expect("falcon riding capture should replay");

        assert_eq!(profile, selected_falcon_session_profile());
        assert_eq!(report.replay_records.get(), 1153);
        assert_eq!(report.outputs.get(), 2687);
        assert_eq!(report.telemetry.get(), 767);
        assert_eq!(report.read_only_responses.get(), 766);
        assert_eq!(report.diagnostics.get(), 0);
        assert!(report.diagnostic_errors.is_empty());
        assert!(report.chunk_one_byte_matches);
        assert!(report.chunk_arbitrary_matches);
    }

    #[test]
    fn pevcap_replay_lifted_wheel_exposes_typed_aero_bms_metadata_currents() {
        let capture = PevcapCapture::decode(
            include_str!(
                "../../cutout-protocols/fixtures/nosfet-aero/pevcap/nf2557-lifted-wheel-120s.pevcap.jsonl"
            )
            .as_bytes(),
            PevcapEncoding::Jsonl,
        )
        .expect("lifted-wheel Aero PEVCAP decodes");

        let report = replay_pevcap_capture(&capture, selected_aero_session_profile())
            .expect("lifted-wheel Aero PEVCAP replays");

        let mut observed = BTreeSet::new();
        for response in &report.read_only_response_events {
            if let Some(payload) = available_battery_page(response)
                && payload.page().kind == BatteryPageKind::Metadata
            {
                let currents = payload
                    .bms_pack_currents()
                    .expect("metadata page should carry typed BMS currents");
                assert_eq!(
                    payload.battery().current,
                    Some(Measured::reported(
                        cutout_core::BatteryCurrent::from_milliamps(
                            currents.current_0().as_milliamps()
                        )
                    ))
                );
                observed.insert((
                    payload.page().selector.get(),
                    currents.current_0().as_milliamps(),
                    currents.current_1().as_milliamps(),
                ));
            }
        }

        assert!(observed.contains(&(0, 10, 10)));
        assert!(observed.contains(&(0, 20, 10)));
        assert!(observed.contains(&(4, 10, 10)));
        assert!(observed.contains(&(4, 20, 10)));
    }

    #[test]
    fn pevcap_replay_dashboard_verification_capture_matches_live_aero_fields() {
        let capture = PevcapCapture::decode(
            include_str!(
                "../../cutout-protocols/fixtures/nosfet-aero/pevcap/nf2557-dashboard-verification-120s.pevcap.jsonl"
            )
            .as_bytes(),
            PevcapEncoding::Jsonl,
        )
        .expect("dashboard verification Aero PEVCAP decodes");

        let report = replay_pevcap_capture(&capture, selected_aero_session_profile())
            .expect("dashboard verification Aero PEVCAP replays");

        assert_eq!(report.telemetry, ReplayTelemetryCount::new(591));
        assert_eq!(
            report.read_only_responses,
            ReplayReadOnlyResponseCount::new(2_335)
        );
        assert_eq!(report.diagnostics, ReplayDiagnosticCount::default());
        assert!(report.chunk_one_byte_matches);
        assert!(report.chunk_arbitrary_matches);
        assert_eq!(
            report
                .telemetry_snapshot
                .speed
                .map(|speed| speed.value.get()),
            Some(0)
        );
        assert_eq!(
            report
                .telemetry_snapshot
                .voltage
                .map(|voltage| voltage.value.get()),
            Some(125_230)
        );
        assert_eq!(
            report
                .telemetry_snapshot
                .distance
                .map(|distance| distance.value.get()),
            Some(1_551_216_000)
        );
        assert_eq!(
            report
                .firmware
                .and_then(|firmware| firmware.firmware_major.map(|major| major.value)),
            Some(43)
        );

        let mut observed_pages = Vec::new();
        let mut observed_currents = BTreeSet::new();
        for response in &report.read_only_response_events {
            if let Some(payload) = available_battery_page(response) {
                observed_pages.push((payload.page().kind, payload.page().selector.get()));
                if payload.page().kind == BatteryPageKind::Metadata {
                    let currents = payload
                        .bms_pack_currents()
                        .expect("metadata page should carry typed BMS currents");
                    observed_currents.insert((
                        payload.page().selector.get(),
                        currents.current_0().as_milliamps(),
                        currents.current_1().as_milliamps(),
                    ));
                }
            }
        }

        assert!(observed_pages.contains(&(BatteryPageKind::Metadata, 0)));
        assert!(observed_pages.contains(&(BatteryPageKind::CellVoltage, 1)));
        assert!(observed_pages.contains(&(BatteryPageKind::CellVoltage, 2)));
        assert!(observed_pages.contains(&(BatteryPageKind::Temperature, 3)));
        assert!(observed_pages.contains(&(BatteryPageKind::Metadata, 4)));
        assert!(observed_pages.contains(&(BatteryPageKind::CellVoltage, 5)));
        assert!(observed_pages.contains(&(BatteryPageKind::CellVoltage, 6)));
        assert!(observed_pages.contains(&(BatteryPageKind::Temperature, 7)));
        assert!(observed_currents.contains(&(0, 20, 20)));
        assert!(observed_currents.contains(&(4, 20, 10)));
    }

    #[derive(Clone, Copy)]
    struct PevcapReplayCorpusCase {
        name: &'static str,
        jsonl: &'static str,
        profile: SelectedSessionProfile,
        minimum_chunk_plan_len: ReplayChunkPlanLen,
    }

    const PEVCAP_REPLAY_CORPUS: &[PevcapReplayCorpusCase] = &[
        PevcapReplayCorpusCase {
            name: "aero-veteran-live",
            jsonl: include_str!("../fixtures/pevcap/aero-veteran-live.jsonl"),
            profile: selected_aero_session_profile(),
            minimum_chunk_plan_len: ReplayChunkPlanLen::new(5),
        },
        PevcapReplayCorpusCase {
            name: "nf2557-dashboard-verification",
            jsonl: include_str!(
                "../../cutout-protocols/fixtures/nosfet-aero/pevcap/nf2557-dashboard-verification-120s.pevcap.jsonl"
            ),
            profile: selected_aero_session_profile(),
            minimum_chunk_plan_len: ReplayChunkPlanLen::new(6),
        },
        PevcapReplayCorpusCase {
            name: "falcon-begode-banner",
            jsonl: include_str!("../fixtures/pevcap/falcon-begode-banner.jsonl"),
            profile: selected_falcon_session_profile(),
            minimum_chunk_plan_len: ReplayChunkPlanLen::new(4),
        },
    ];

    #[test]
    fn telemetry_snapshot_renderer_includes_present_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(cutout_core::TelemetryDelta {
            speed: Some(speed(1_200)),
            voltage: Some(voltage(108_760)),
            battery_current: Some(battery_current(-1_700)),
            power: Some(power(-184_892)),
            controller_temperature: Some(temperature(33_270)),
            pwm: Some(duty_cycle_permille(-1_000)),
            distance: Some(distance(1_551_169_000)),
            pitch: Some(angle_mdeg(69_060)),
            battery_level_estimated: Some(level_estimated(47)),
            ..cutout_core::TelemetryDelta::empty(ms(42))
        });

        assert_eq!(
            render_telemetry_snapshot(&snapshot).map(|telemetry| telemetry.to_string()),
            Some(
                "telemetry speed=1200 voltage=108760 battery_current=-1700 power=-184892 controller_temperature=33270 pwm=-1000 distance=1551169000 pitch=69060 battery_level_estimated=47".to_owned()
            )
        );
    }

    #[test]
    fn telemetry_snapshot_renderer_omits_empty_snapshot() {
        assert!(render_telemetry_snapshot(&TelemetrySnapshot::default()).is_none());
    }

    #[test]
    fn firmware_renderer_includes_fixed_header_version_fields() {
        let firmware = FirmwareInfo {
            firmware_major: Some(Measured::reported(43)),
            firmware_minor: Some(Measured::reported(2)),
            firmware_patch: Some(Measured::reported(54)),
            build_id: Some(cutout_core::RawFieldValue::new(0x001c, 43_254)),
            ..FirmwareInfo::default()
        };

        assert_eq!(
            render_firmware_info(Some(firmware)).map(|firmware| firmware.to_string()),
            Some(
                "firmware firmware_major=43 firmware_minor=2 firmware_patch=54 raw_001c=43254"
                    .to_owned()
            )
        );
    }

    #[test]
    fn identity_renderer_includes_confidence_and_evidence() {
        let report = SessionBridgeReport {
            identity: Some(BridgeIdentityResolution {
                manufacturer: Some("Begode"),
                model: Some("Falcon"),
                confidence: BridgeIdentityConfidence::Model,
                evidence: BridgeIdentityEvidence::empty()
                    .with(BridgeIdentityEvidenceKind::AdvertisedNameHint)
                    .with(BridgeIdentityEvidenceKind::GattHint)
                    .with(BridgeIdentityEvidenceKind::PassiveFamilyMatch)
                    .with(BridgeIdentityEvidenceKind::BannerModelMatch),
            }),
            ..SessionBridgeReport::default()
        };

        assert_eq!(
            render_identity(&report).map(|identity| identity.to_string()),
            Some(
                "identity confidence=Model manufacturer=Begode model=Falcon advertised_name_hint=true gatt_hint=true passive_family=true banner_model=true".to_owned()
            )
        );
    }

    #[test]
    fn selected_session_profile_keeps_auto_on_existing_aero_path() {
        let auto = selected_session_profile(SessionProfile::Auto);
        let aero = selected_session_profile(SessionProfile::Aero);

        assert_eq!(auto, selected_aero_session_profile());
        assert_eq!(auto.session_key(), NOSFET_AERO_SESSION_KEY);
        assert_eq!(
            auto.session_registration()
                .expect("Aero registration exists")
                .data_channel,
            VETERAN_DATA_CHANNEL
        );
        assert_eq!(aero, selected_aero_session_profile());
        assert_eq!(aero.session_key(), NOSFET_AERO_SESSION_KEY);
    }

    #[test]
    fn selected_session_profile_for_summary_uses_advertised_name_hints() {
        let falcon_summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "GotWay_002441".to_owned(),
                address: None,
                name: Some("GotWay_002441".to_owned()),
                rssi: None,
                advertised_services: Vec::new().into(),
                manufacturer_data: Vec::new().into(),
            },
            services: Vec::new().into(),
        };
        let aero_summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "NF2557".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: None,
                advertised_services: Vec::new().into(),
                manufacturer_data: Vec::new().into(),
            },
            services: Vec::new().into(),
        };

        assert_eq!(
            selected_session_resolution_for_summary(SessionProfile::Auto, &falcon_summary)
                .expect("Falcon summary resolves")
                .selected_session,
            selected_falcon_session_profile()
        );
        assert_eq!(
            selected_session_resolution_for_summary(SessionProfile::Auto, &aero_summary)
                .expect("Aero summary resolves")
                .selected_session,
            selected_aero_session_profile()
        );
    }

    #[test]
    fn dashboard_session_profile_from_summary_uses_catalog_identity() {
        let falcon_summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "GotWay_002441".to_owned(),
                address: None,
                name: Some("GotWay_002441".to_owned()),
                rssi: None,
                advertised_services: Vec::new().into(),
                manufacturer_data: Vec::new().into(),
            },
            services: Vec::new().into(),
        };
        let aero_summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "NF2557".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: None,
                advertised_services: Vec::new().into(),
                manufacturer_data: Vec::new().into(),
            },
            services: Vec::new().into(),
        };

        assert_eq!(
            dashboard_session_profile_from_summary(&falcon_summary)
                .expect("Falcon summary resolves"),
            selected_falcon_session_profile()
        );
        assert_eq!(
            dashboard_session_profile_from_summary(&aero_summary).expect("Aero summary resolves"),
            selected_aero_session_profile()
        );
    }

    #[test]
    fn dashboard_session_profile_from_summary_rejects_unsupported_devices() {
        let summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "unknown".to_owned(),
                address: None,
                name: Some("unknown".to_owned()),
                rssi: None,
                advertised_services: Vec::new().into(),
                manufacturer_data: Vec::new().into(),
            },
            services: Vec::new().into(),
        };

        let error = dashboard_session_profile_from_summary(&summary)
            .expect_err("unsupported device should not silently fall back");
        assert!(
            error
                .to_string()
                .contains("dashboard cannot resolve a session profile")
        );
    }

    #[test]
    fn selected_session_profile_allows_explicit_falcon_verification_path() {
        let falcon = selected_session_profile(SessionProfile::Falcon);

        assert_eq!(falcon, selected_falcon_session_profile());
        assert_eq!(falcon.session_key(), BEGODE_FALCON_SESSION_KEY);
        assert_eq!(
            falcon
                .session_registration()
                .expect("Falcon registration exists")
                .data_channel,
            BEGODE_DATA_CHANNEL
        );
    }

    #[test]
    fn selected_session_registration_reports_missing_key() {
        let error = session_registration_by_key(SessionKey::new("missing-session"))
            .expect_err("missing session key should be rejected");

        assert!(
            error
                .to_string()
                .contains("selected session registration is missing: missing-session")
        );
    }

    #[test]
    fn read_probe_commands_preserve_explicit_order() {
        assert_eq!(
            read_probe_commands(&[
                ReadProbe::Identity,
                ReadProbe::Firmware,
                ReadProbe::Telemetry,
                ReadProbe::Battery,
                ReadProbe::Diagnostics,
            ]),
            vec![
                DeviceCommand::RequestIdentity,
                DeviceCommand::RequestFirmwareInfo,
                DeviceCommand::RequestTelemetry,
                DeviceCommand::RequestBatteryInfo,
                DeviceCommand::RequestDiagnostics,
            ]
        );
    }

    #[test]
    fn settings_renderer_includes_fixed_header_raw_fields() {
        let entry = |id, value| cutout_core::SettingsEntry {
            field: cutout_core::RawFieldValue::new(id, value),
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        };
        let settings = SettingsReadback::available([
            Some(entry(0x0014, 0)),
            Some(entry(0x0016, 0)),
            Some(entry(0x0018, 550)),
            Some(entry(0x001a, 540)),
        ]);
        let more_settings =
            SettingsReadback::available([Some(entry(0x001e, 1_920)), None, None, None]);
        let unsupported_settings = SettingsReadback::unsupported();
        let rendered = render_settings_readbacks(&[settings, more_settings, unsupported_settings])
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            rendered,
            vec![
                "settings raw_0014=0 raw_0016=0 raw_0018=550 raw_001a=540",
                "settings raw_001e=1920",
                "settings availability=unsupported",
            ]
        );
    }

    #[test]
    fn dashboard_live_target_requires_device_outside_demo_mode() {
        let error = dashboard_live_target(&dashboard_args(false, None))
            .expect_err("live dashboard requires an explicit device");

        assert_eq!(
            error.to_string(),
            "dashboard requires --demo or --device to start"
        );
    }

    #[test]
    fn dashboard_live_target_maps_device_to_name_filter() {
        let target = dashboard_live_target(&dashboard_args(false, Some("NF2557")))
            .expect("device becomes a live target");

        assert_eq!(
            target,
            ConnectionTarget {
                address: None,
                identifier: None,
                name_contains: Some("NF2557".to_owned()),
            }
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_starts_live_updates_before_terminal() {
        let (log_tx, log_rx) = mpsc::channel();

        let result = run_live_dashboard_with(
            DashboardState::empty(),
            log_tx,
            log_rx,
            move |tx| async move {
                tx.send(DashboardUpdate::Log {
                    level: "debug".to_owned(),
                    message: "live entered".to_owned(),
                })
                .expect("terminal receiver stays open");
            },
            move |_state, rx| {
                let _update = rx
                    .recv_timeout(Duration::from_secs(1))
                    .expect("terminal should receive first live update before reporting ready");
                Ok(())
            },
        );

        result.expect("dashboard runner exits after terminal exits");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_polls_updates_while_terminal_waits() {
        let (log_tx, log_rx) = mpsc::channel();
        let result = run_live_dashboard_with(
            DashboardState::empty(),
            log_tx,
            log_rx,
            |tx| async move {
                tx.send(DashboardUpdate::Log {
                    level: "debug".to_owned(),
                    message: "live update polled".to_owned(),
                })
                .expect("terminal receiver stays open");
            },
            |_state, rx| {
                let update = rx
                    .recv_timeout(Duration::from_secs(1))
                    .expect("live update future should be polled while terminal waits");
                assert_eq!(
                    update,
                    DashboardUpdate::Log {
                        level: "debug".to_owned(),
                        message: "live update polled".to_owned(),
                    }
                );
                Ok(())
            },
        );

        result.expect("dashboard runner exits after terminal exits");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_delivers_multiple_updates_while_terminal_waits() {
        let (log_tx, log_rx) = mpsc::channel();
        let result = run_live_dashboard_with(
            DashboardState::empty(),
            log_tx,
            log_rx,
            |tx| async move {
                for index in 0..3 {
                    tx.send(DashboardUpdate::Log {
                        level: "debug".to_owned(),
                        message: format!("live update {index}"),
                    })
                    .expect("terminal receiver stays open");
                    tokio::task::yield_now().await;
                }
            },
            |_state, rx| {
                let messages = (0..3)
                    .map(|_| {
                        rx.recv_timeout(Duration::from_secs(1))
                            .expect("live update should be delivered while terminal waits")
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    messages,
                    vec![
                        DashboardUpdate::Log {
                            level: "debug".to_owned(),
                            message: "live update 0".to_owned(),
                        },
                        DashboardUpdate::Log {
                            level: "debug".to_owned(),
                            message: "live update 1".to_owned(),
                        },
                        DashboardUpdate::Log {
                            level: "debug".to_owned(),
                            message: "live update 2".to_owned(),
                        },
                    ]
                );
                Ok(())
            },
        );

        result.expect("dashboard runner exits after terminal exits");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_constructs_registered_aero_session_before_terminal_exits() {
        let (constructed_tx, constructed_rx) = mpsc::channel();
        let (log_tx, log_rx) = mpsc::channel();

        let result = run_live_dashboard_with(
            DashboardState::empty(),
            log_tx,
            log_rx,
            move |_tx| async move {
                let session = selected_aero_session_profile()
                    .session_registration()
                    .expect("registered Aero session exists")
                    .construct();
                constructed_tx
                    .send(session)
                    .expect("test receiver waits for registered Aero session construction");
            },
            |_state, _rx| Ok(()),
        );

        result.expect("dashboard runner exits after terminal exits");
        assert!(matches!(
            constructed_rx.recv_timeout(Duration::from_secs(1)).expect(
                "registered Aero session construction should not block the live update runner"
            ),
            RegisteredReadOnlySession::NosfetAero(_)
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_returns_terminal_errors() {
        let (log_tx, log_rx) = mpsc::channel();
        let result = run_live_dashboard_with(
            DashboardState::empty(),
            log_tx,
            log_rx,
            |_tx| async {},
            |_state, _rx| anyhow::bail!("terminal failed"),
        );

        assert_eq!(
            result.expect_err("terminal errors propagate").to_string(),
            "terminal failed"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_aborts_live_updates_after_terminal_exit() {
        let (dropped_tx, dropped_rx) = mpsc::channel();
        let (log_tx, log_rx) = mpsc::channel();

        let result = run_live_dashboard_with(
            DashboardState::empty(),
            log_tx,
            log_rx,
            move |_tx| async move {
                let _signal = DropSignal(dropped_tx);
                std::future::pending::<()>().await;
            },
            |_state, _rx| Ok(()),
        );

        result.expect("dashboard runner exits after terminal exits");
        dropped_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("live update future should be aborted after terminal exit");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_polls_updates_while_terminal_blocks_independently() {
        let (polled_tx, polled_rx) = mpsc::channel();
        let (log_tx, log_rx) = mpsc::channel();

        let result = run_live_dashboard_with(
            DashboardState::empty(),
            log_tx,
            log_rx,
            move |_tx| async move {
                polled_tx
                    .send(())
                    .expect("test receiver waits for live update poll");
            },
            |_state, _rx| {
                thread::sleep(Duration::from_millis(100));
                Ok(())
            },
        );

        result.expect("dashboard runner exits after terminal exits");
        polled_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("live update future should be polled while terminal blocks");
    }
}
