use std::{
    fs,
    future::Future,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use cutout_btle::{
    BtleError, BtleplugReconnectHost, ConnectedPeripheral, ConnectionTarget, PevcapSessionMetadata,
    RawNotificationRecord, ReconnectAttemptReport, SessionBridgeReport, SessionCapture,
    SessionEndpoints, SessionPeripheral, capture_raw_notifications,
    capture_reconnecting_session_with_commands, capture_session_with_commands,
    connect_and_discover, drive_session, drive_session_with_commands, read_battery_level,
    scan_peripherals,
};
use cutout_core::{
    BatteryInfo, BatteryPageKind, BatteryPagePayload, CaptureDistribution, CaptureEvidence,
    CapturePrivacy, CaptureSessionLabel, DeviceCommand, DeviceEvent, DiagnosticError,
    DiagnosticErrorKind, DiagnosticSnapshot, FirmwareInfo, GattChannel, HostSession, Measured,
    ParserDiagnostics, PevcapCapture, PevcapDirection, PevcapEncoding, PevcapHeader, PevcapRecord,
    PevcapResolvedIdentity, ProtocolFamily, ReadOnlyResponse, ReplayChunkComparison, SessionOutput,
    SettingsReadback, TelemetrySnapshot, ValueQuality, ValueSource, VerificationStatus,
    VerifiedValue,
};
use cutout_protocols::{
    BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BegodeBmsSummary, BegodeCapacityEvidence,
    BegodeCapacitySelection, BegodeFalconModel, BegodeFrame, BegodeNotificationDecoder,
    BegodePackEvidenceConsistency, BegodePackLayoutEvidence, BegodePackLayoutSelection,
    BegodeVoltageEvidence, BegodeVoltageProfileSelection, NosfetAeroModel, ReadOnlySession,
    VETERAN_DATA_CHANNEL, select_begode_pack_capacity_from_annotations,
    select_begode_pack_layout_from_annotations, select_begode_pack_voltage_profile,
    select_begode_pack_voltage_profile_from_annotations, validate_begode_pack_evidence,
};
use tracing::{debug, info};

use crate::cli::{
    CaptureArgs, Cli, Command, DashboardArgs, PevcapArgs, PevcapCommand, PevcapConvertArgs,
    PevcapFormat, RawSubscribeArgs, ReadProbe, SessionProfile, TargetedScanArgs,
};
use crate::dashboard::{
    DashboardState, DashboardUpdate, run_dashboard, run_dashboard_with_updates,
};
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
    println!(
        "converted {} ({:?}) -> {} ({:?})",
        args.input.display(),
        args.input_format,
        args.output.display(),
        args.output_format
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
    println!("{}", render_pevcap_replay_report(&report));
    if args.read_only_jsonl {
        for line in render_read_only_responses_jsonl(&report.read_only_response_events)? {
            println!("{line}");
        }
    }
    if args.diagnostics_jsonl {
        for line in render_diagnostic_snapshots_jsonl(&report.diagnostic_snapshots)? {
            println!("{line}");
        }
        for line in render_diagnostic_errors_jsonl(&report.diagnostic_errors)? {
            println!("{line}");
        }
    }
    if let Some(telemetry) = render_telemetry_snapshot(&report.telemetry_snapshot) {
        println!("{telemetry}");
    }
    if let Some(firmware) = render_firmware_info(report.firmware) {
        println!("{firmware}");
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PevcapReplayReport {
    replay_records: usize,
    outputs: usize,
    telemetry: usize,
    read_only_responses: usize,
    diagnostics: usize,
    arbitrary_chunk_plan_len: usize,
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
}

fn replay_pevcap_capture(
    capture: &PevcapCapture,
    profile: SelectedSessionProfile,
) -> Result<PevcapReplayReport> {
    let mut report = match profile {
        SelectedSessionProfile::Aero => replay_pevcap_with_session(
            capture,
            ReadOnlySession::<NosfetAeroModel, false>::default(),
        ),
        SelectedSessionProfile::Falcon => {
            replay_pevcap_with_session(capture, falcon_replay_session(capture)?)
        }
    };
    report.capacity =
        select_begode_pack_capacity_from_annotations(capture.header.annotations.iter());
    report.layout = select_begode_pack_layout_from_annotations(capture.header.annotations.iter());
    if profile == SelectedSessionProfile::Falcon {
        report.pack_evidence_consistency = Some(validate_begode_pack_evidence(
            select_falcon_replay_voltage_profile(capture),
            report.capacity,
            report.layout,
        ));
    }
    Ok(report)
}

fn falcon_replay_session(
    capture: &PevcapCapture,
) -> Result<ReadOnlySession<BegodeFalconModel, true>> {
    match select_falcon_replay_voltage_profile(capture) {
        BegodeVoltageProfileSelection::Selected(profile) => {
            Ok(ReadOnlySession::<BegodeFalconModel, true>::with_decoder(
                BegodeNotificationDecoder::with_pack_voltage_profile(profile),
            ))
        }
        BegodeVoltageProfileSelection::Missing => {
            bail!("Falcon PEVCAP replay requires explicit Falcon battery voltage evidence")
        }
        BegodeVoltageProfileSelection::Conflicting => {
            bail!("Falcon PEVCAP replay has conflicting Falcon battery voltage evidence")
        }
    }
}

fn select_falcon_replay_voltage_profile(capture: &PevcapCapture) -> BegodeVoltageProfileSelection {
    match select_begode_pack_voltage_profile_from_annotations(capture.header.annotations.iter()) {
        BegodeVoltageProfileSelection::Conflicting => BegodeVoltageProfileSelection::Conflicting,
        BegodeVoltageProfileSelection::Selected(profile) => select_begode_pack_voltage_profile(
            core::iter::once(profile_evidence(profile))
                .chain(falcon_replay_bms_voltage_evidence(capture))
                .collect::<Vec<_>>()
                .as_slice(),
        ),
        BegodeVoltageProfileSelection::Missing => select_begode_pack_voltage_profile(
            &falcon_replay_bms_voltage_evidence(capture).collect::<Vec<_>>(),
        ),
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

fn falcon_replay_bms_voltage_evidence(
    capture: &PevcapCapture,
) -> impl Iterator<Item = BegodeVoltageEvidence> + '_ {
    capture.records.iter().filter_map(|record| {
        if record.direction != PevcapDirection::Inbound
            || record.characteristic != BEGODE_DATA_CHANNEL
        {
            return None;
        }
        let frame = BegodeFrame::try_from_slice(record.bytes.as_slice()).ok()?;
        let summary = BegodeBmsSummary::decode(&frame).ok()?;
        u32::try_from(summary.pack_voltage_mv)
            .ok()
            .map(BegodeVoltageEvidence::ObservedPackVoltageMv)
    })
}

fn replay_pevcap_with_session<S>(capture: &PevcapCapture, session: S) -> PevcapReplayReport
where
    S: Clone + cutout_core::ProtocolSession,
{
    let records = capture.replay_records();
    let comparison_session = session.clone();
    let mut host = HostSession::new(session);
    let outputs = cutout_core::replay_capture(&mut host, &records);
    let arbitrary_chunks = cutout_core::replay_arbitrary_chunk_lengths(&records);
    let comparison = cutout_core::compare_replay_capture_chunks(
        || comparison_session.clone(),
        &records,
        &arbitrary_chunks,
    );
    summarize_pevcap_replay(records.len(), arbitrary_chunks.len(), &outputs, comparison)
}

fn summarize_pevcap_replay(
    replay_records: usize,
    arbitrary_chunk_plan_len: usize,
    outputs: &[SessionOutput],
    chunk_comparison: ReplayChunkComparison,
) -> PevcapReplayReport {
    let mut report = PevcapReplayReport {
        replay_records,
        outputs: outputs.len(),
        telemetry: 0,
        read_only_responses: 0,
        diagnostics: 0,
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
    };

    for output in outputs {
        let SessionOutput::Event(event) = output else {
            continue;
        };
        match event {
            DeviceEvent::Telemetry(delta) => {
                report.telemetry += 1;
                report.telemetry_snapshot.apply_delta(*delta);
            }
            DeviceEvent::ReadOnlyResponse(response) => {
                report.read_only_responses += 1;
                report.read_only_response_events.push(*response);
                if let ReadOnlyResponse::Firmware(firmware) = response {
                    report.firmware = Some(*firmware);
                }
            }
            DeviceEvent::Diagnostics(diagnostics) => {
                report.diagnostics += 1;
                report
                    .diagnostic_snapshots
                    .push(DiagnosticSnapshot::from_parser_diagnostics(*diagnostics));
            }
            DeviceEvent::DiagnosticError(error) => {
                report.diagnostic_errors.push(*error);
            }
            DeviceEvent::LinkUp(_)
            | DeviceEvent::LinkDown
            | DeviceEvent::NotificationReceived { .. }
            | DeviceEvent::Tick { .. }
            | DeviceEvent::ControlRefusal(_) => {}
        }
    }

    report
}

fn dashboard_state_from_aero_pevcap(capture: &PevcapCapture) -> Result<DashboardState> {
    let report = replay_pevcap_capture(capture, SelectedSessionProfile::Aero)?;
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
        protocol_writes: 0,
        writes: 0,
        subscribes: 0,
        notifications: replay_notification_count(capture),
        notification_bytes: replay_notification_bytes(capture),
        latest_notification_len: latest_replay_notification_len(capture),
        telemetry: report.telemetry,
        telemetry_snapshot: report.telemetry_snapshot,
        read_only_responses: report.read_only_responses,
        read_only_response_events: report.read_only_response_events,
        firmware: report.firmware,
        settings: Vec::new(),
        diagnostics: report.diagnostics,
        diagnostics_snapshot: ParserDiagnostics::default(),
        diagnostic_errors: report.diagnostic_errors,
        identity: None,
        events: Vec::new(),
        disconnects: 0,
    });
    Ok(state)
}

fn pevcap_dashboard_provenance(capture: &PevcapCapture) -> String {
    if capture.header.annotations.is_empty() {
        return format!("pevcap replay platform={}", capture.header.platform_id);
    }

    format!(
        "pevcap replay platform={} annotations={}",
        capture.header.platform_id,
        capture
            .header
            .annotations
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn replay_notification_count(capture: &PevcapCapture) -> usize {
    capture
        .records
        .iter()
        .filter(|record| record.direction == PevcapDirection::Inbound)
        .count()
}

fn replay_notification_bytes(capture: &PevcapCapture) -> usize {
    capture
        .records
        .iter()
        .filter(|record| record.direction == PevcapDirection::Inbound)
        .map(|record| record.bytes.len())
        .sum()
}

fn latest_replay_notification_len(capture: &PevcapCapture) -> Option<usize> {
    capture
        .records
        .iter()
        .rev()
        .find(|record| record.direction == PevcapDirection::Inbound)
        .map(|record| record.bytes.len())
}

fn render_diagnostic_snapshots_jsonl(
    snapshots: &[DiagnosticSnapshot],
) -> Result<Vec<String>, serde_json::Error> {
    snapshots
        .iter()
        .enumerate()
        .map(|(sequence, snapshot)| render_diagnostic_snapshot_jsonl(sequence, *snapshot))
        .collect()
}

fn render_diagnostic_errors_jsonl(
    errors: &[DiagnosticError],
) -> Result<Vec<String>, serde_json::Error> {
    errors
        .iter()
        .enumerate()
        .map(|(sequence, error)| render_diagnostic_error_jsonl(sequence, *error))
        .collect()
}

fn render_diagnostic_snapshot_jsonl(
    sequence: usize,
    snapshot: DiagnosticSnapshot,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&serde_json::json!({
        "type": "diagnostic_snapshot",
        "sequence": sequence,
        "dropped_bytes": snapshot.dropped_bytes,
        "resyncs": snapshot.resyncs,
        "bad_checksums": snapshot.bad_checksums,
        "timeouts": snapshot.timeouts,
        "oversized_frames": snapshot.oversized_frames,
        "malformed_frames": snapshot.malformed_frames,
        "unmatched_replies": snapshot.unmatched_replies,
    }))
}

fn render_diagnostic_error_jsonl(
    sequence: usize,
    error: DiagnosticError,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&serde_json::json!({
        "type": "diagnostic_error",
        "sequence": sequence,
        "kind": diagnostic_error_kind_name(error.kind),
        "claimed_len": error.claimed_len,
        "max_len": error.max_len,
        "elapsed_ms": error.elapsed_ms,
        "timeout_ms": error.timeout_ms,
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

fn render_pevcap_replay_report(report: &PevcapReplayReport) -> String {
    let mut rendered = format!(
        "pevcap replay records={} outputs={} telemetry={} read_only_responses={} diagnostics={} arbitrary_chunk_plan_len={} chunk_one_byte_matches={} chunk_arbitrary_matches={}",
        report.replay_records,
        report.outputs,
        report.telemetry,
        report.read_only_responses,
        report.diagnostics,
        report.arbitrary_chunk_plan_len,
        report.chunk_one_byte_matches,
        report.chunk_arbitrary_matches
    );
    append_capacity_evidence(&mut rendered, report.capacity);
    append_layout_evidence(&mut rendered, report.layout);
    append_pack_evidence_consistency(&mut rendered, report.pack_evidence_consistency);
    rendered
}

fn append_pack_evidence_consistency(
    rendered: &mut String,
    consistency: Option<BegodePackEvidenceConsistency>,
) {
    match consistency {
        None | Some(BegodePackEvidenceConsistency::Consistent) => {}
        Some(BegodePackEvidenceConsistency::Incomplete) => {
            rendered.push_str(" pack_evidence_incomplete=true");
        }
        Some(BegodePackEvidenceConsistency::Inconsistent) => {
            rendered.push_str(" pack_evidence_inconsistent=true");
        }
    }
}

fn append_capacity_evidence(rendered: &mut String, capacity: BegodeCapacitySelection) {
    match capacity {
        BegodeCapacitySelection::Missing => {}
        BegodeCapacitySelection::Conflicting => rendered.push_str(" capacity_conflict=true"),
        BegodeCapacitySelection::Selected(evidence) => {
            append_selected_capacity_evidence(rendered, evidence);
        }
    }
}

fn append_selected_capacity_evidence(rendered: &mut String, evidence: BegodeCapacityEvidence) {
    if let Some(nominal_capacity_mah) = evidence.nominal_capacity_mah {
        rendered.push_str(" capacity_nominal_mah=");
        rendered.push_str(&nominal_capacity_mah.to_string());
    }
    if let Some(reported_wh) = evidence.reported_wh {
        rendered.push_str(" capacity_reported_wh=");
        rendered.push_str(&reported_wh.to_string());
    }
}

fn append_layout_evidence(rendered: &mut String, layout: BegodePackLayoutSelection) {
    match layout {
        BegodePackLayoutSelection::Missing => {}
        BegodePackLayoutSelection::Conflicting => rendered.push_str(" layout_conflict=true"),
        BegodePackLayoutSelection::Selected(evidence) => {
            append_selected_layout_evidence(rendered, evidence);
        }
    }
}

fn append_selected_layout_evidence(rendered: &mut String, evidence: BegodePackLayoutEvidence) {
    if let Some(cell_model) = evidence.cell_model {
        rendered.push_str(" layout_cell_model=");
        rendered.push_str(cell_model.label());
    }
    if let Some(series_cells) = evidence.series_cells {
        rendered.push_str(" layout_series_cells=");
        rendered.push_str(&series_cells.to_string());
    }
    if let Some(parallel_count) = evidence.parallel_count {
        rendered.push_str(" layout_parallel_count=");
        rendered.push_str(&parallel_count.to_string());
    }
}

async fn dashboard(args: DashboardArgs) -> Result<()> {
    if args.demo {
        return run_dashboard(DashboardState::demo(args.device.as_deref()));
    }

    if let Some(path) = args.pevcap.as_ref() {
        let input = fs::read(path)?;
        let capture = PevcapCapture::decode(&input, pevcap_encoding(args.pevcap_format))?;
        return run_dashboard(dashboard_state_from_aero_pevcap(&capture)?);
    }

    let target = dashboard_live_target(&args)?;
    info!(
        device = target.name_contains.as_deref().unwrap_or("<none>"),
        seconds = args.seconds(),
        "scanning for dashboard device"
    );
    let connection = connect_and_discover(&target, Duration::from_secs(args.seconds())).await?;
    info!(
        observation = %connection.summary.observation,
        "connected dashboard device"
    );
    let mut state = DashboardState::live_connected(&target, &connection.summary);
    match read_battery_level(&connection.peripheral, &connection.summary).await? {
        Some(percent) => {
            info!(percent, "read dashboard battery level");
            state.apply_battery_percent(percent);
        }
        None => {
            info!("dashboard battery level unavailable from standard BLE characteristic");
        }
    }
    run_live_dashboard(state, connection)
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

fn run_live_dashboard(state: DashboardState, connection: ConnectedPeripheral) -> Result<()> {
    info!(
        observation = %connection.summary.observation,
        "starting dashboard live runner"
    );
    run_live_dashboard_with(
        state,
        move |tx| run_dashboard_live_updates(connection, tx),
        |state, rx| run_dashboard_with_updates(state, &rx),
    )
}

fn run_live_dashboard_with<Start, Fut, Run>(
    state: DashboardState,
    start_live_updates: Start,
    run_terminal: Run,
) -> Result<()>
where
    Start: FnOnce(mpsc::Sender<DashboardUpdate>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
    Run: FnOnce(DashboardState, mpsc::Receiver<DashboardUpdate>) -> Result<()> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
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
            let mut live_updates = tokio::spawn(start_live_updates(tx));
            let _ = started_tx.send(Ok(()));
            info!("dashboard live update task spawned in dedicated runtime");
            tokio::select! {
                _ = &mut live_updates => {
                    info!("dashboard live update task finished");
                }
                _ = shutdown_rx => {
                    info!("dashboard live update task aborting after shutdown");
                    live_updates.abort();
                    let _ = live_updates.await;
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

    info!("dashboard live update constructing Aero read-only session");
    let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
    info!("dashboard live update constructed Aero read-only session");
    let mut iteration = 0_u64;
    debug!("dashboard live update checking battery refresh capability");
    let refresh_battery = connection.summary.battery_level_characteristic().is_some();
    info!(
        refresh_battery,
        "dashboard live update battery refresh policy"
    );

    loop {
        if !run_dashboard_live_iteration(&connection, &tx, &mut session, iteration, refresh_battery)
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
    session: &mut ReadOnlySession<NosfetAeroModel, false>,
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
        notify = ?endpoints
            .notify
            .map(|characteristic| characteristic.uuid.to_string()),
        window_ms = DASHBOARD_LIVE_WINDOW.as_millis(),
        "dashboard drive_session starting"
    );
    info!(iteration, "dashboard awaiting drive_session");
    match drive_session(
        &connection.peripheral,
        session,
        VETERAN_DATA_CHANNEL,
        &connection.summary,
        endpoints,
        DASHBOARD_LIVE_WINDOW,
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
        notifications = report.notifications,
        read_only_responses = report.read_only_responses,
        telemetry = report.telemetry,
        "dashboard drive_session returned"
    );
    debug!(
        iteration,
        subscribes = report.subscribes,
        notifications = report.notifications,
        notification_bytes = report.notification_bytes,
        telemetry = report.telemetry,
        read_only_responses = report.read_only_responses,
        diagnostics = report.diagnostics,
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
            info!(iteration, percent, "dashboard battery refresh succeeded");
            tx.send(DashboardUpdate::BatteryPercent(percent)).is_ok()
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
    for observation in scan_peripherals(Duration::from_secs(seconds)).await? {
        println!("{observation}");
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
    let connection = connect_and_discover(&target.into(), Duration::from_secs(seconds)).await?;
    println!("{}", connection.summary);
    let Some(characteristic) = connection.summary.select_notify_characteristic(requested) else {
        if let Some(uuid) = requested {
            bail!("no notify/indicate characteristic matched {uuid}");
        }
        bail!("no notify/indicate characteristics discovered");
    };
    println!(
        "raw subscribe characteristic={} service={} window={}s",
        characteristic.uuid, characteristic.service_uuid, seconds
    );
    let records = capture_raw_notifications(
        &connection.peripheral,
        characteristic,
        Duration::from_secs(seconds),
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
            println!("wrote raw pevcap {} ({format:?})", path.display());
        }
        None => {
            for record in records {
                println!(
                    "raw-notification t_ms={} characteristic={} service={} bytes={}",
                    record.monotonic_ms,
                    record.characteristic,
                    record.service,
                    encode_hex(&record.bytes)
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
    let profile = selected_session_profile(args.profile());
    let commands = read_probe_commands(args.probes());
    let diagnostics_jsonl = args.diagnostics_jsonl();
    let read_only_jsonl = args.read_only_jsonl();
    let connection =
        connect_and_discover(&args.into_target(), Duration::from_secs(seconds)).await?;

    println!("{}", connection.summary);
    if let Some(endpoints) = connection.summary.select_session_endpoints() {
        print_session_endpoints(endpoints);
        mode.run(
            &connection,
            endpoints,
            profile,
            SessionRunOptions {
                commands: &commands,
                window: Duration::from_secs(seconds),
                diagnostics_jsonl,
                read_only_jsonl,
            },
        )
        .await?;
    }

    Ok(())
}

async fn capture(args: CaptureArgs) -> Result<()> {
    let annotations = capture_annotations(&args);
    if args.reconnect_attempts > 1 {
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
    let profile = selected_session_profile(args.target.profile());
    let commands = read_probe_commands(args.target.probes());
    let diagnostics_jsonl = args.target.diagnostics_jsonl();
    let read_only_jsonl = args.target.read_only_jsonl();
    let mut host =
        BtleplugReconnectHost::new(args.target.into_target(), Duration::from_secs(seconds));
    let reconnecting_capture = match profile {
        SelectedSessionProfile::Aero => {
            capture_reconnecting_session_with_commands(
                &mut host,
                &mut ReadOnlySession::<NosfetAeroModel, false>::default(),
                VETERAN_DATA_CHANNEL,
                Duration::from_secs(seconds),
                args.reconnect_attempts,
                false,
                &commands,
            )
            .await?
        }
        SelectedSessionProfile::Falcon => {
            capture_reconnecting_session_with_commands(
                &mut host,
                &mut ReadOnlySession::<BegodeFalconModel, true>::default(),
                BEGODE_DATA_CHANNEL,
                Duration::from_secs(seconds),
                args.reconnect_attempts,
                false,
                &commands,
            )
            .await?
        }
    };
    let summary = merge_reconnect_summaries(
        reconnecting_capture
            .attempts
            .iter()
            .map(|attempt| &attempt.summary),
    )
    .ok_or(BtleError::NoPeripheralMatched)?;
    write_or_print_capture(
        reconnecting_capture.capture,
        &summary,
        &output,
        profile,
        diagnostics_jsonl,
        read_only_jsonl,
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

#[derive(Clone, Copy, Debug)]
struct SessionRunOptions<'a> {
    commands: &'a [DeviceCommand],
    window: Duration,
    diagnostics_jsonl: bool,
    read_only_jsonl: bool,
}

impl SessionMode {
    async fn run(
        self,
        connection: &ConnectedPeripheral,
        endpoints: SessionEndpoints<'_>,
        profile: SelectedSessionProfile,
        options: SessionRunOptions<'_>,
    ) -> Result<()> {
        match profile {
            SelectedSessionProfile::Aero => {
                let binding =
                    SessionBinding::new(VETERAN_DATA_CHANNEL, SelectedSessionProfile::Aero);
                self.run_with_session(
                    connection,
                    endpoints,
                    ReadOnlySession::<NosfetAeroModel, false>::default(),
                    binding,
                    options,
                )
                .await
            }
            SelectedSessionProfile::Falcon => {
                let binding =
                    SessionBinding::new(BEGODE_DATA_CHANNEL, SelectedSessionProfile::Falcon);
                self.run_with_session(
                    connection,
                    endpoints,
                    ReadOnlySession::<BegodeFalconModel, true>::default(),
                    binding,
                    options,
                )
                .await
            }
        }
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
enum SelectedSessionProfile {
    Aero,
    Falcon,
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
        SessionProfile::Auto | SessionProfile::Aero => SelectedSessionProfile::Aero,
        SessionProfile::Falcon => SelectedSessionProfile::Falcon,
    }
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
    match capture
        .header
        .resolved_identity
        .as_ref()
        .and_then(|identity| identity.protocol_family)
    {
        Some(ProtocolFamily::VeteranLeaperkimNosfet) => Ok(SelectedSessionProfile::Aero),
        Some(ProtocolFamily::BegodeGotway) => Ok(SelectedSessionProfile::Falcon),
        None | Some(ProtocolFamily::Vesc) => {
            bail!("PEVCAP replay --profile auto requires resolved Aero or Falcon identity metadata")
        }
    }
}

fn pevcap_identity_for_profile(profile: SelectedSessionProfile) -> PevcapResolvedIdentity {
    match profile {
        SelectedSessionProfile::Aero => PevcapResolvedIdentity {
            protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
            model: Some(VerifiedValue {
                value: "NOSFET Aero".to_owned(),
                verification: VerificationStatus::Inferred,
            }),
            firmware: None,
        },
        SelectedSessionProfile::Falcon => PevcapResolvedIdentity {
            protocol_family: Some(ProtocolFamily::BegodeGotway),
            model: Some(VerifiedValue {
                value: "Begode Falcon".to_owned(),
                verification: VerificationStatus::Inferred,
            }),
            firmware: None,
        },
    }
}

fn print_session_endpoints(endpoints: SessionEndpoints<'_>) {
    println!(
        "session write={} notify={}",
        endpoints.write.uuid,
        endpoints
            .notify
            .map_or_else(|| "<none>".to_owned(), |notify| notify.uuid.to_string())
    );
}

fn print_capture(
    capture: SessionCapture,
    diagnostics_jsonl: bool,
    read_only_jsonl: bool,
) -> Result<()> {
    for record in capture.records {
        println!("{record}");
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
                annotation_refs.as_slice(),
            )?;
            fs::write(path, bytes)?;
            println!("wrote pevcap {} ({format:?})", path.display());
            print_session_report(&report);
            print_session_read_only_jsonl(&report, read_only_jsonl)?;
            print_session_diagnostics_jsonl(&report, diagnostics_jsonl)?;
            Ok(())
        }
    }
}

fn encode_session_capture_pevcap(
    capture: &SessionCapture,
    summary: &cutout_btle::ConnectionSummary,
    format: PevcapFormat,
    wall_clock_start_unix_ms: u64,
    profile: SelectedSessionProfile,
    annotations: &[&str],
) -> Result<Vec<u8>> {
    let mut capture_annotations = Vec::with_capacity(annotations.len() + 1);
    capture_annotations.push("cutout-cli capture");
    capture_annotations.extend_from_slice(annotations);
    let pevcap = capture.to_pevcap(
        summary,
        PevcapSessionMetadata {
            wall_clock_start_unix_ms,
            platform_id: std::env::consts::OS,
            library_version: env!("CARGO_PKG_VERSION"),
            registry_hash: cutout_core::registry_entries_hash(&[&BEGODE_FALCON_REGISTRY_ENTRY]),
            resolved_identity: Some(pevcap_identity_for_profile(profile)),
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
    wall_clock_start_unix_ms: u64,
    annotations: &[&str],
) -> Result<Vec<u8>> {
    let advertised_services = summary
        .observation
        .advertised_services
        .iter()
        .copied()
        .map(gatt_channel_from_uuid)
        .collect::<Vec<_>>();
    let gatt_fingerprints = summary.gatt_fingerprints();
    let mut pevcap_records = Vec::with_capacity(records.len().saturating_add(2));
    pevcap_records.push(PevcapRecord::link_up(0, write_limit));
    pevcap_records.extend(records.iter().map(|record| {
        PevcapRecord::inbound_notification(
            record.monotonic_ms,
            gatt_channel_from_uuid(record.characteristic),
            gatt_channel_from_uuid(record.service),
            record.bytes.clone(),
        )
    }));
    pevcap_records.push(PevcapRecord::link_down(
        records.last().map_or(0, |record| record.monotonic_ms),
    ));
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
        env!("CARGO_PKG_VERSION"),
        cutout_core::registry_entries_hash(&[&BEGODE_FALCON_REGISTRY_ENTRY]),
        capture_annotations.as_slice(),
    )?;
    Ok(PevcapCapture::new(header, pevcap_records).encode(pevcap_encoding(format))?)
}

fn gatt_channel_from_uuid(uuid: uuid::Uuid) -> GattChannel {
    GattChannel::from_bytes(*uuid.as_bytes())
}

fn capture_wall_clock_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn print_session_report(report: &SessionBridgeReport) {
    println!(
        "session protocol_writes={} writes={} subscribes={} notifications={} telemetry={} read_only_responses={} diagnostics={} disconnects={}",
        report.protocol_writes,
        report.writes,
        report.subscribes,
        report.notifications,
        report.telemetry,
        report.read_only_responses,
        report.diagnostics,
        report.disconnects
    );
    if let Some(telemetry) = render_telemetry_snapshot(&report.telemetry_snapshot) {
        println!("{telemetry}");
    }
    if let Some(firmware) = render_firmware_info(report.firmware) {
        println!("{firmware}");
    }
    if let Some(identity) = render_identity(report) {
        println!("{identity}");
    }
    for settings in render_settings_readbacks(&report.settings) {
        println!("{settings}");
    }
}

fn print_session_diagnostics_jsonl(
    report: &SessionBridgeReport,
    enabled: bool,
) -> Result<(), serde_json::Error> {
    if enabled {
        println!("{}", render_session_diagnostics_jsonl(report)?);
        for line in render_diagnostic_errors_jsonl(&report.diagnostic_errors)? {
            println!("{line}");
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
            println!("{}", render_reconnect_attempt_diagnostics_jsonl(attempt)?);
        }
    }
    Ok(())
}

fn print_session_read_only_jsonl(
    report: &SessionBridgeReport,
    enabled: bool,
) -> Result<(), serde_json::Error> {
    if enabled {
        for line in render_read_only_responses_jsonl(&report.read_only_response_events)? {
            println!("{line}");
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
        "protocol_writes": report.protocol_writes,
        "writes": report.writes,
        "dropped_bytes": diagnostics.dropped_bytes,
        "resyncs": diagnostics.resyncs,
        "bad_checksums": diagnostics.bad_checksums,
        "timeouts": diagnostics.timeouts,
        "oversized_frames": diagnostics.oversized_frames,
        "malformed_frames": diagnostics.malformed_frames,
        "unmatched_replies": diagnostics.unmatched_replies,
    }))
}

fn render_reconnect_attempt_diagnostics_jsonl(
    attempt: &ReconnectAttemptReport,
) -> Result<String, serde_json::Error> {
    let diagnostics =
        DiagnosticSnapshot::from_parser_diagnostics(attempt.report.diagnostics_snapshot);
    serde_json::to_string(&serde_json::json!({
        "type": "reconnect_attempt",
        "attempt": attempt.attempt,
        "identifier": attempt.summary.observation.identifier,
        "name": attempt.summary.observation.name,
        "rssi": attempt.summary.observation.rssi,
        "protocol_writes": attempt.report.protocol_writes,
        "writes": attempt.report.writes,
        "subscribes": attempt.report.subscribes,
        "notifications": attempt.report.notifications,
        "telemetry": attempt.report.telemetry,
        "read_only_responses": attempt.report.read_only_responses,
        "diagnostics": attempt.report.diagnostics,
        "disconnects": attempt.report.disconnects,
        "dropped_bytes": diagnostics.dropped_bytes,
        "resyncs": diagnostics.resyncs,
        "bad_checksums": diagnostics.bad_checksums,
        "timeouts": diagnostics.timeouts,
        "oversized_frames": diagnostics.oversized_frames,
        "malformed_frames": diagnostics.malformed_frames,
        "unmatched_replies": diagnostics.unmatched_replies,
    }))
}

fn render_read_only_responses_jsonl(
    responses: &[ReadOnlyResponse],
) -> Result<Vec<String>, serde_json::Error> {
    responses
        .iter()
        .enumerate()
        .map(|(sequence, response)| render_read_only_response_jsonl(sequence, *response))
        .collect()
}

fn render_read_only_response_jsonl(
    sequence: usize,
    response: ReadOnlyResponse,
) -> Result<String, serde_json::Error> {
    match response {
        ReadOnlyResponse::Battery(payload) => render_battery_response_jsonl(sequence, payload),
        ReadOnlyResponse::Firmware(firmware) => serde_json::to_string(&serde_json::json!({
            "type": "read_only_response",
            "sequence": sequence,
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
            "sequence": sequence,
            "command_kind": command_kind_name(response.command_kind()),
            "response": "settings",
            "entries": settings.entries.into_iter().flatten().map(settings_entry_json).collect::<Vec<_>>(),
        })),
        ReadOnlyResponse::Diagnostics(diagnostics) => serde_json::to_string(&serde_json::json!({
            "type": "read_only_response",
            "sequence": sequence,
            "command_kind": command_kind_name(response.command_kind()),
            "response": "diagnostics",
            "details": diagnostics.details.into_iter().flatten().map(diagnostic_detail_json).collect::<Vec<_>>(),
        })),
        ReadOnlyResponse::RawTelemetry(raw) => serde_json::to_string(&serde_json::json!({
            "type": "read_only_response",
            "sequence": sequence,
            "command_kind": command_kind_name(response.command_kind()),
            "response": "raw_telemetry",
            "fields": raw.fields.into_iter().flatten().map(|field| raw_field_json(Some(field))).collect::<Vec<_>>(),
        })),
    }
}

fn render_battery_response_jsonl(
    sequence: usize,
    payload: BatteryPagePayload,
) -> Result<String, serde_json::Error> {
    let page = payload.page();
    serde_json::to_string(&serde_json::json!({
        "type": "read_only_response",
        "sequence": sequence,
        "command_kind": command_kind_name(ReadOnlyResponse::Battery(payload).command_kind()),
        "response": "battery",
        "page": {
            "selector": page.selector,
            "kind": battery_page_kind_name(page.kind),
            "verification": verification_status_name(page.verification),
        },
        "battery": battery_info_json(payload.battery()),
    }))
}

fn battery_info_json(battery: BatteryInfo) -> serde_json::Value {
    serde_json::json!({
        "voltage_mv": measured_i32_json(battery.voltage_mv),
        "current_ma": measured_i32_json(battery.current_ma),
        "percent_reported": measured_u8_json(battery.percent_reported),
        "percent_estimated": measured_u8_json(battery.percent_estimated),
        "temperature_mc": measured_i32_json(battery.temperature_mc),
        "raw_state": raw_field_json(battery.raw_state),
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

fn diagnostic_detail_json(detail: cutout_core::DiagnosticDetail) -> serde_json::Value {
    serde_json::json!({
        "field": raw_field_json(Some(detail.field)),
        "severity": diagnostic_severity_name(detail.severity),
        "quality": value_quality_name(detail.quality),
        "verification": verification_status_name(detail.verification),
    })
}

const fn command_kind_name(kind: cutout_core::CommandKind) -> &'static str {
    match kind {
        cutout_core::CommandKind::RequestIdentity => "request_identity",
        cutout_core::CommandKind::RequestTelemetry => "request_telemetry",
        cutout_core::CommandKind::RequestFirmwareInfo => "request_firmware_info",
        cutout_core::CommandKind::RequestBatteryInfo => "request_battery_info",
        cutout_core::CommandKind::RequestDiagnostics => "request_diagnostics",
        cutout_core::CommandKind::RequestSettings => "request_settings",
        cutout_core::CommandKind::SetLights => "set_lights",
        cutout_core::CommandKind::SoundHorn => "sound_horn",
        cutout_core::CommandKind::SetRawMotorCurrent => "set_raw_motor_current",
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

fn render_identity(report: &SessionBridgeReport) -> Option<String> {
    let identity = report.identity.as_ref()?;
    Some(format!(
        "identity confidence={:?} manufacturer={} model={} advertised_name_hint={} gatt_hint={} passive_family={} banner_model={}",
        identity.confidence,
        identity.manufacturer.unwrap_or("<unknown>"),
        identity.model.unwrap_or("<unknown>"),
        identity.evidence.has_advertised_name_hint(),
        identity.evidence.has_gatt_hint(),
        identity.evidence.has_passive_family_match(),
        identity.evidence.has_banner_model_match()
    ))
}

fn render_telemetry_snapshot(snapshot: &TelemetrySnapshot) -> Option<String> {
    let mut fields = Vec::new();
    push_measured_i32(&mut fields, "speed_mm_s", snapshot.speed_mm_s);
    push_measured_i32(&mut fields, "voltage_mv", snapshot.voltage_mv);
    push_measured_i32(
        &mut fields,
        "battery_current_ma",
        snapshot.battery_current_ma,
    );
    push_measured_i32(&mut fields, "motor_current_ma", snapshot.motor_current_ma);
    push_measured_i64(&mut fields, "power_mw", snapshot.power_mw);
    push_measured_i32(
        &mut fields,
        "controller_temperature_mc",
        snapshot.controller_temperature_mc,
    );
    push_measured_i32(
        &mut fields,
        "motor_temperature_mc",
        snapshot.motor_temperature_mc,
    );
    push_measured_i32(
        &mut fields,
        "battery_temperature_mc",
        snapshot.battery_temperature_mc,
    );
    push_measured_i16(&mut fields, "pwm_permille", snapshot.pwm_permille);
    push_measured_u64(&mut fields, "distance_mm", snapshot.distance_mm);
    push_measured_i32(&mut fields, "pitch_mdeg", snapshot.pitch_mdeg);
    push_measured_i32(&mut fields, "roll_mdeg", snapshot.roll_mdeg);
    push_measured_u8(
        &mut fields,
        "battery_percent_reported",
        snapshot.battery_percent_reported,
    );
    push_measured_u8(
        &mut fields,
        "battery_percent_estimated",
        snapshot.battery_percent_estimated,
    );

    (!fields.is_empty()).then(|| format!("telemetry {}", fields.join(" ")))
}

fn push_measured_i16(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<i16>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn push_measured_i32(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<i32>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn push_measured_i64(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<i64>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn push_measured_u8(fields: &mut Vec<String>, name: &'static str, measured: Option<Measured<u8>>) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn push_measured_u64(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<u64>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn render_firmware_info(firmware: Option<FirmwareInfo>) -> Option<String> {
    let firmware = firmware?;
    let mut fields = Vec::new();
    push_measured_u16(&mut fields, "firmware_major", firmware.firmware_major);
    push_measured_u16(&mut fields, "firmware_minor", firmware.firmware_minor);
    push_measured_u16(&mut fields, "firmware_patch", firmware.firmware_patch);
    if let Some(build_id) = firmware.build_id {
        fields.push(format!("raw_{:04x}={}", build_id.id, build_id.value));
    }

    (!fields.is_empty()).then(|| format!("firmware {}", fields.join(" ")))
}

fn render_settings_readbacks(settings: &[SettingsReadback]) -> Vec<String> {
    settings
        .iter()
        .filter_map(|settings| {
            let mut fields = Vec::new();
            for entry in settings.entries.into_iter().flatten() {
                fields.push(format!("raw_{:04x}={}", entry.field.id, entry.field.value));
            }

            (!fields.is_empty()).then(|| format!("settings {}", fields.join(" ")))
        })
        .collect()
}

fn push_measured_u16(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<u16>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use btleplug::api::{CharPropFlags, WriteType};
    use clap::Parser;
    use cutout_btle::{
        BridgeIdentityResolution, ConnectionSummary, ConnectionTarget, PeripheralObservation,
        RawNotificationRecord, ServiceSummary, SessionCaptureRecord,
    };
    use cutout_core::{
        CaptureRecord, GattChannel, LinkInfo, PevcapHeader, PevcapRecord, ProtocolFamily,
        VerificationStatus, VerifiedValue, WriteMode,
    };
    use cutout_protocols::{
        BEGODE_FALCON_REGISTRY_ENTRY, BegodeBanner, DeviceFamily, IdentityConfidence,
        ProtocolFamilyClassification, StagedIdentityInput, identify_model,
    };
    use uuid::Uuid;

    use super::*;
    use crate::cli::ScanArgs;

    struct DropSignal(mpsc::Sender<()>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            let _ = self.0.send(());
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
            1_725_000_123_456,
            "darwin",
            Some(182),
            &[service],
            &[],
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
                    7,
                    characteristic,
                    WriteMode::WithoutResponse,
                    b"N".to_vec(),
                ),
                PevcapRecord::inbound_notification(
                    9,
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
                42,
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
            1_725_000_123_456,
            "darwin",
            Some(182),
            &[BEGODE_DATA_CHANNEL],
            &[],
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

    fn sample_aero_replay_capture() -> PevcapCapture {
        let header = PevcapHeader::new(
            1_725_000_123_456,
            "darwin",
            Some(23),
            &[VETERAN_DATA_CHANNEL],
            &[],
            None,
            "0.1.0",
            [0x24; 32],
            &["aero replay"],
        )
        .expect("header should validate");
        PevcapCapture::new(
            header,
            vec![PevcapRecord::inbound_notification(
                42,
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
    fn cli_encodes_session_capture_to_pevcap_bytes() {
        let summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "cb-uuid".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: Some(-67),
                advertised_services: vec![Uuid::from_u128(
                    0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
                )],
                manufacturer_data: Vec::new(),
            },
            services: vec![ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![cutout_btle::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }],
            }],
        };
        let capture = SessionCapture {
            records: vec![
                SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(23),
                },
                SessionCaptureRecord::Write {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteType::WithoutResponse,
                    bytes: b"N".to_vec(),
                    provisional: false,
                },
                SessionCaptureRecord::Notification {
                    monotonic_ms: 3,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    bytes: b"NAME=NF2557".to_vec(),
                },
            ],
            report: SessionBridgeReport::default(),
        };

        let bytes = encode_session_capture_pevcap(
            &capture,
            &summary,
            PevcapFormat::Binary,
            42,
            SelectedSessionProfile::Aero,
            &["capture_label=charging", "capture_privacy=private"],
        )
        .expect("capture encodes");
        let decoded =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary PEVCAP decodes");

        assert_eq!(decoded.header.wall_clock_start_unix_ms, 42);
        assert_eq!(decoded.header.write_limit, Some(23));
        assert_eq!(
            decoded.header.annotations.as_slice(),
            &[
                "cutout-cli capture".to_owned(),
                "capture_label=charging".to_owned(),
                "capture_privacy=private".to_owned(),
            ]
        );
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
        assert_eq!(decoded.records[2].bytes, b"NAME=NF2557");
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
                rssi: Some(-67),
                advertised_services: vec![ffe0],
                manufacturer_data: Vec::new(),
            },
            services: vec![ServiceSummary {
                uuid: ffe0,
                primary: true,
                characteristics: vec![cutout_btle::CharacteristicSummary {
                    uuid: ffe1,
                    service_uuid: ffe0,
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }],
            }],
        };
        let second = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "second-link".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: Some(-70),
                advertised_services: vec![ffe0, battery],
                manufacturer_data: Vec::new(),
            },
            services: vec![
                ServiceSummary {
                    uuid: ffe0,
                    primary: true,
                    characteristics: vec![cutout_btle::CharacteristicSummary {
                        uuid: ffe2,
                        service_uuid: ffe0,
                        properties: CharPropFlags::READ,
                    }],
                },
                ServiceSummary {
                    uuid: battery,
                    primary: true,
                    characteristics: vec![cutout_btle::CharacteristicSummary {
                        uuid: battery_level,
                        service_uuid: battery,
                        properties: CharPropFlags::READ,
                    }],
                },
            ],
        };

        let merged = merge_reconnect_summaries([&first, &second]).expect("summaries merge");

        assert_eq!(merged.observation.identifier, "first-link");
        assert_eq!(merged.observation.advertised_services, vec![ffe0, battery]);
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
                rssi: Some(-67),
                advertised_services: vec![service],
                manufacturer_data: Vec::new(),
            },
            services: vec![ServiceSummary {
                uuid: service,
                primary: true,
                characteristics: vec![cutout_btle::CharacteristicSummary {
                    uuid: characteristic,
                    service_uuid: service,
                    properties: CharPropFlags::NOTIFY,
                }],
            }],
        };
        let records = [RawNotificationRecord {
            monotonic_ms: 7,
            characteristic,
            service,
            bytes: vec![0xde, 0xad, 0xbe, 0xef],
        }];

        let bytes = encode_raw_capture_pevcap(
            &records,
            &summary,
            Some(185),
            PevcapFormat::Binary,
            99,
            &[
                "capture_label=powered_on_stationary",
                "capture_privacy=private",
            ],
        )
        .expect("raw capture encodes");
        let decoded =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary PEVCAP decodes");

        assert_eq!(decoded.header.wall_clock_start_unix_ms, 99);
        assert_eq!(decoded.header.write_limit, Some(185));
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
        let replay = decoded.replay_records();
        assert_eq!(replay.len(), 3);
        assert!(matches!(
            replay[0],
            CaptureRecord::LinkUp(LinkInfo {
                monotonic_ms: 0,
                max_write_len: Some(185),
            })
        ));
        assert!(matches!(
            &replay[1],
            CaptureRecord::Notification {
                monotonic_ms: 7,
                bytes,
                ..
            } if bytes.as_slice() == [0xde, 0xad, 0xbe, 0xef]
        ));
        assert_eq!(replay[2], CaptureRecord::LinkDown);
    }

    #[test]
    fn pevcap_replay_report_renders_counts() {
        let report = PevcapReplayReport {
            replay_records: 2,
            outputs: 3,
            telemetry: 1,
            read_only_responses: 1,
            diagnostics: 1,
            arbitrary_chunk_plan_len: 3,
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
        };

        assert_eq!(
            render_pevcap_replay_report(&report),
            "pevcap replay records=2 outputs=3 telemetry=1 read_only_responses=1 diagnostics=1 arbitrary_chunk_plan_len=3 chunk_one_byte_matches=true chunk_arbitrary_matches=true"
        );
    }

    #[test]
    fn diagnostic_snapshot_jsonl_uses_stable_snake_case_fields() {
        let line = render_diagnostic_snapshot_jsonl(
            7,
            DiagnosticSnapshot {
                dropped_bytes: 11,
                resyncs: 2,
                bad_checksums: 3,
                timeouts: 5,
                oversized_frames: 8,
                malformed_frames: 13,
                unmatched_replies: 21,
            },
        )
        .expect("diagnostic snapshot serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("diagnostic JSONL is JSON");
        assert_eq!(value["type"], "diagnostic_snapshot");
        assert_eq!(value["sequence"], 7);
        assert_eq!(value["dropped_bytes"], 11);
        assert_eq!(value["resyncs"], 2);
        assert_eq!(value["bad_checksums"], 3);
        assert_eq!(value["timeouts"], 5);
        assert_eq!(value["oversized_frames"], 8);
        assert_eq!(value["malformed_frames"], 13);
        assert_eq!(value["unmatched_replies"], 21);
    }

    #[test]
    fn read_only_battery_jsonl_preserves_page_metadata_and_measured_values() {
        let response = ReadOnlyResponse::Battery(BatteryPagePayload::raw(
            cutout_core::BatteryPageMetadata::raw(8, VerificationStatus::SourceVerified),
            BatteryInfo {
                voltage_mv: Some(Measured::reported(80_000)),
                current_ma: Some(Measured::reported(-10_000)),
                percent_reported: None,
                percent_estimated: Some(Measured::estimated(61)),
                temperature_mc: Some(Measured::reported(25_000)),
                raw_state: Some(cutout_core::RawFieldValue::new(0x0008, 0x55aa)),
            },
        ));

        let line = render_read_only_response_jsonl(2, response)
            .expect("read-only battery response serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("read-only response JSONL is JSON");
        assert_eq!(value["type"], "read_only_response");
        assert_eq!(value["sequence"], 2);
        assert_eq!(value["command_kind"], "request_battery_info");
        assert_eq!(value["response"], "battery");
        assert_eq!(value["page"]["selector"], 8);
        assert_eq!(value["page"]["kind"], "raw");
        assert_eq!(value["page"]["verification"], "source_verified");
        assert_eq!(value["battery"]["voltage_mv"]["value"], 80_000);
        assert_eq!(value["battery"]["voltage_mv"]["source"], "reported");
        assert_eq!(value["battery"]["voltage_mv"]["quality"], "known");
        assert_eq!(
            value["battery"]["voltage_mv"]["verification"],
            "hardware_verified"
        );
        assert_eq!(value["battery"]["current_ma"]["value"], -10_000);
        assert_eq!(
            value["battery"]["percent_reported"],
            serde_json::Value::Null
        );
        assert_eq!(value["battery"]["percent_estimated"]["value"], 61);
        assert_eq!(value["battery"]["percent_estimated"]["source"], "estimated");
        assert_eq!(value["battery"]["temperature_mc"]["value"], 25_000);
        assert_eq!(value["battery"]["raw_state"]["id"], 8);
        assert_eq!(value["battery"]["raw_state"]["value"], 0x55aa);
    }

    #[test]
    fn pevcap_replay_summary_collects_read_only_response_events() {
        let response = ReadOnlyResponse::Battery(BatteryPagePayload::raw(
            cutout_core::BatteryPageMetadata::raw(8, VerificationStatus::SourceVerified),
            BatteryInfo::default(),
        ));
        let outputs = [SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
            response,
        ))];

        let report = summarize_pevcap_replay(
            1,
            1,
            &outputs,
            ReplayChunkComparison {
                whole_semantic_events: 1,
                one_byte_semantic_events: 1,
                arbitrary_semantic_events: 1,
                one_byte_matches: true,
                arbitrary_matches: true,
            },
        );

        assert_eq!(report.read_only_response_events, vec![response]);
    }

    #[test]
    fn live_session_diagnostics_jsonl_uses_aggregate_report_snapshot() {
        let report = SessionBridgeReport {
            protocol_writes: 1,
            writes: 3,
            diagnostics_snapshot: ParserDiagnostics {
                dropped_bytes: 1,
                resyncs: 2,
                bad_checksums: 3,
                timeouts: 4,
                oversized_frames: 5,
                malformed_frames: 6,
                unmatched_replies: 7,
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
            .expect("session diagnostic errors JSONL serializes");
        let error: serde_json::Value =
            serde_json::from_str(&error_lines[0]).expect("diagnostic error JSONL is JSON");
        assert_eq!(error["type"], "diagnostic_error");
        assert_eq!(error["kind"], "malformed_frame");
    }

    #[test]
    fn reconnect_attempt_diagnostics_jsonl_distinguishes_link_attempts() {
        let attempt = ReconnectAttemptReport {
            attempt: 2,
            summary: ConnectionSummary {
                observation: PeripheralObservation {
                    identifier: "NF2557".to_owned(),
                    address: None,
                    name: Some("NF2557".to_owned()),
                    rssi: Some(-71),
                    advertised_services: Vec::new(),
                    manufacturer_data: Vec::new(),
                },
                services: Vec::new(),
            },
            report: SessionBridgeReport {
                protocol_writes: 2,
                writes: 3,
                subscribes: 1,
                notifications: 8,
                disconnects: 0,
                diagnostics_snapshot: ParserDiagnostics {
                    dropped_bytes: 5,
                    resyncs: 1,
                    bad_checksums: 0,
                    timeouts: 0,
                    oversized_frames: 0,
                    malformed_frames: 2,
                    unmatched_replies: 0,
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
            elapsed_ms: 1_234,
            timeout_ms: 5_000,
        });

        let line = render_diagnostic_error_jsonl(3, error).expect("diagnostic error serializes");

        let value: serde_json::Value =
            serde_json::from_str(&line).expect("diagnostic error JSONL is JSON");
        assert_eq!(value["type"], "diagnostic_error");
        assert_eq!(value["sequence"], 3);
        assert_eq!(value["kind"], "timeout");
        assert_eq!(value["claimed_len"], serde_json::Value::Null);
        assert_eq!(value["max_len"], serde_json::Value::Null);
        assert_eq!(value["elapsed_ms"], 1_234);
        assert_eq!(value["timeout_ms"], 5_000);
    }

    #[test]
    fn pevcap_replay_summary_collects_diagnostic_error_events() {
        let error = DiagnosticError::from_parser_error(cutout_core::ParserError::OversizedFrame {
            claimed: 33,
            max: 24,
        });
        let outputs = [SessionOutput::Event(DeviceEvent::DiagnosticError(error))];

        let report = summarize_pevcap_replay(
            1,
            1,
            &outputs,
            ReplayChunkComparison {
                whole_semantic_events: 1,
                one_byte_semantic_events: 1,
                arbitrary_semantic_events: 1,
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

        let report = replay_pevcap_capture(&capture, SelectedSessionProfile::Aero)
            .expect("Aero replay does not require Falcon battery evidence");

        assert_eq!(report.replay_records, 2);
        assert!(report.arbitrary_chunk_plan_len > 3);
        assert!(report.chunk_one_byte_matches);
        assert!(report.chunk_arbitrary_matches);
        assert!(report.telemetry >= 1);
        assert!(report.read_only_responses >= 1);
        assert_eq!(
            report
                .telemetry_snapshot
                .voltage_mv
                .map(|voltage| voltage.value),
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
        assert_eq!(state.device.identifier, "darwin");
        assert_eq!(state.device.connection_state, "replayed");
        assert_eq!(state.counters.notifications, 1);
        assert_eq!(state.counters.notification_bytes, 99);
        assert_eq!(state.counters.latest_notification_len, Some(99));
        assert_eq!(state.telemetry.latest_voltage_v, Some(108));
        assert!(state.read_only.firmware.is_some());
        assert!(
            state
                .read_only
                .settings
                .iter()
                .any(|setting| setting.contains("hardware_verified"))
        );
        assert!(
            state
                .read_only
                .bms_pages
                .iter()
                .any(|page| page.contains("selector=3 kind=raw"))
        );
        assert_eq!(state.read_only.unknown_raw_pages, 1);
    }

    #[test]
    fn pevcap_replay_dashboard_state_requires_equivalent_chunk_modes() {
        let capture = sample_aero_replay_capture();
        let report = replay_pevcap_capture(&capture, SelectedSessionProfile::Aero)
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

        assert_eq!(profile, SelectedSessionProfile::Falcon);
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

        assert_eq!(profile, SelectedSessionProfile::Aero);
    }

    #[test]
    fn pevcap_replay_auto_rejects_missing_identity() {
        let capture = sample_aero_replay_capture();

        let error = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
            .expect_err("missing identity should not guess a replay profile");

        assert!(
            error
                .to_string()
                .contains("requires resolved Aero or Falcon")
        );
    }

    #[test]
    fn pevcap_replay_explicit_profile_overrides_missing_identity() {
        let capture = sample_aero_replay_capture();

        let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Falcon)
            .expect("explicit profile does not require metadata");

        assert_eq!(profile, SelectedSessionProfile::Falcon);
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
                .voltage_mv
                .map(|voltage| voltage.value),
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

        assert!(render_pevcap_replay_report(&report).contains("capacity_conflict=true"));
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

        assert!(render_pevcap_replay_report(&report).contains(
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

        assert!(render_pevcap_replay_report(&report).contains("layout_conflict=true"));
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

        assert!(render_pevcap_replay_report(&report).contains("pack_evidence_inconsistent=true"));
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

        assert!(render_pevcap_replay_report(&report).contains("pack_evidence_incomplete=true"));
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
                    41,
                    BEGODE_DATA_CHANNEL,
                    BEGODE_DATA_CHANNEL,
                    hex_literal::hex!("55aa2710000003b6ff9c0019001a0190000001035a5a5a5a").to_vec(),
                ),
                PevcapRecord::inbound_notification(
                    42,
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
                .voltage_mv
                .map(|voltage| voltage.value),
            Some(90_075)
        );
    }

    #[test]
    fn pevcap_replay_rejects_ambiguous_falcon_bms_voltage_evidence() {
        let capture = sample_falcon_replay_capture_with_records(
            &[],
            vec![PevcapRecord::inbound_notification(
                41,
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
            assert!(report.replay_records >= 2, "{} replay records", case.name);
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

    #[derive(Clone, Copy)]
    struct PevcapReplayCorpusCase {
        name: &'static str,
        jsonl: &'static str,
        profile: SelectedSessionProfile,
        minimum_chunk_plan_len: usize,
    }

    const PEVCAP_REPLAY_CORPUS: &[PevcapReplayCorpusCase] = &[
        PevcapReplayCorpusCase {
            name: "aero-veteran-live",
            jsonl: include_str!("../fixtures/pevcap/aero-veteran-live.jsonl"),
            profile: SelectedSessionProfile::Aero,
            minimum_chunk_plan_len: 5,
        },
        PevcapReplayCorpusCase {
            name: "falcon-begode-banner",
            jsonl: include_str!("../fixtures/pevcap/falcon-begode-banner.jsonl"),
            profile: SelectedSessionProfile::Falcon,
            minimum_chunk_plan_len: 4,
        },
    ];

    #[test]
    fn telemetry_snapshot_renderer_includes_present_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(cutout_core::TelemetryDelta {
            speed_mm_s: Some(Measured::reported(1_200)),
            voltage_mv: Some(Measured::reported(108_760)),
            motor_current_ma: Some(Measured::reported(-1_700)),
            power_mw: Some(Measured::calculated(-184_892)),
            controller_temperature_mc: Some(Measured::reported(33_270)),
            pwm_permille: Some(Measured::reported(-1_000)),
            distance_mm: Some(Measured::reported(1_551_169_000)),
            pitch_mdeg: Some(Measured::reported(69_060)),
            battery_percent_estimated: Some(Measured::estimated(47)),
            ..cutout_core::TelemetryDelta::empty(42)
        });

        assert_eq!(
            render_telemetry_snapshot(&snapshot).as_deref(),
            Some(
                "telemetry speed_mm_s=1200 voltage_mv=108760 motor_current_ma=-1700 power_mw=-184892 controller_temperature_mc=33270 pwm_permille=-1000 distance_mm=1551169000 pitch_mdeg=69060 battery_percent_estimated=47"
            )
        );
    }

    #[test]
    fn telemetry_snapshot_renderer_omits_empty_snapshot() {
        assert_eq!(
            render_telemetry_snapshot(&TelemetrySnapshot::default()),
            None
        );
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
            render_firmware_info(Some(firmware)).as_deref(),
            Some("firmware firmware_major=43 firmware_minor=2 firmware_patch=54 raw_001c=43254")
        );
    }

    #[test]
    fn identity_renderer_includes_confidence_and_evidence() {
        let resolution = identify_model(
            StagedIdentityInput {
                advertised_name: Some("Begode_Falcon"),
                gatt: BEGODE_FALCON_REGISTRY_ENTRY.gatt,
                stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
                banner: Some(BegodeBanner::ModelName("Falcon")),
            },
            &[&BEGODE_FALCON_REGISTRY_ENTRY],
        );
        let report = SessionBridgeReport {
            identity: Some(BridgeIdentityResolution {
                manufacturer: Some("Begode"),
                model: Some("Falcon"),
                confidence: IdentityConfidence::Model,
                evidence: resolution.evidence,
            }),
            ..SessionBridgeReport::default()
        };

        assert_eq!(
            render_identity(&report).as_deref(),
            Some(
                "identity confidence=Model manufacturer=Begode model=Falcon advertised_name_hint=true gatt_hint=true passive_family=true banner_model=true"
            )
        );
    }

    #[test]
    fn selected_session_profile_keeps_auto_on_existing_aero_path() {
        assert_eq!(
            selected_session_profile(SessionProfile::Auto),
            SelectedSessionProfile::Aero
        );
        assert_eq!(
            selected_session_profile(SessionProfile::Aero),
            SelectedSessionProfile::Aero
        );
    }

    #[test]
    fn selected_session_profile_allows_explicit_falcon_verification_path() {
        assert_eq!(
            selected_session_profile(SessionProfile::Falcon),
            SelectedSessionProfile::Falcon
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
        let settings = SettingsReadback {
            entries: [
                Some(entry(0x0014, 0)),
                Some(entry(0x0016, 0)),
                Some(entry(0x0018, 550)),
                Some(entry(0x001a, 540)),
            ],
        };
        let more_settings = SettingsReadback {
            entries: [Some(entry(0x001e, 1_920)), None, None, None],
        };

        assert_eq!(
            render_settings_readbacks(&[settings, more_settings]),
            vec![
                "settings raw_0014=0 raw_0016=0 raw_0018=550 raw_001a=540",
                "settings raw_001e=1920",
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
        let (order_tx, order_rx) = mpsc::channel();

        let result = run_live_dashboard_with(
            DashboardState::empty(),
            {
                let order_tx = order_tx.clone();
                move |tx| async move {
                    tx.send(DashboardUpdate::Log {
                        level: "debug".to_owned(),
                        message: "live entered".to_owned(),
                    })
                    .expect("terminal receiver stays open");
                    order_tx
                        .send("live")
                        .expect("terminal should not close ordering receiver");
                }
            },
            move |_state, rx| {
                let _update = rx
                    .recv_timeout(Duration::from_secs(1))
                    .expect("terminal should receive first live update before reporting ready");
                order_tx
                    .send("terminal")
                    .expect("test waits for terminal ordering event");
                Ok(())
            },
        );

        result.expect("dashboard runner exits after terminal exits");
        let observed = [
            order_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("live ordering event should be sent"),
            order_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("terminal ordering event should be sent"),
        ];
        assert_eq!(observed, ["live", "terminal"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_polls_updates_while_terminal_waits() {
        let result = run_live_dashboard_with(
            DashboardState::empty(),
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
        let result = run_live_dashboard_with(
            DashboardState::empty(),
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
    async fn live_dashboard_runner_constructs_aero_session_before_terminal_exits() {
        let (constructed_tx, constructed_rx) = mpsc::channel();

        let result = run_live_dashboard_with(
            DashboardState::empty(),
            move |_tx| async move {
                let _session = ReadOnlySession::<NosfetAeroModel, false>::default();
                constructed_tx
                    .send(())
                    .expect("test receiver waits for Aero session construction");
            },
            |_state, _rx| Ok(()),
        );

        result.expect("dashboard runner exits after terminal exits");
        constructed_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Aero session construction should not block the live update runner");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_dashboard_runner_returns_terminal_errors() {
        let result = run_live_dashboard_with(
            DashboardState::empty(),
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

        let result = run_live_dashboard_with(
            DashboardState::empty(),
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

        let result = run_live_dashboard_with(
            DashboardState::empty(),
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
