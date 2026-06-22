use std::{
    fs,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use cutout_btle::{
    BtleError, ConnectedPeripheral, ConnectionTarget, PevcapSessionMetadata, RawNotificationRecord,
    SessionBridgeReport, SessionCapture, SessionEndpoints, SessionPeripheral,
    capture_raw_notifications, capture_session_with_commands, connect_and_discover, drive_session,
    drive_session_with_commands, read_battery_level, scan_peripherals,
};
use cutout_core::{
    DeviceCommand, DeviceEvent, FirmwareInfo, GattChannel, HostSession, Measured, PevcapCapture,
    PevcapEncoding, PevcapHeader, PevcapRecord, PevcapResolvedIdentity, ProtocolFamily,
    ReplayChunkComparison, SessionOutput, SettingsReadback, TelemetrySnapshot, VerificationStatus,
    VerifiedValue,
};
use cutout_protocols::{
    BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BegodeFalconModel, NosfetAeroModel,
    ReadOnlySession, VETERAN_DATA_CHANNEL,
};
use tracing::info;

use crate::cli::{
    CaptureAeroArgs, Cli, Command, DashboardArgs, PevcapArgs, PevcapCommand, PevcapConvertArgs,
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
        Command::CaptureAero(args) => capture_aero(args).await?,
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
    );
    println!("{}", render_pevcap_replay_report(&report));
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
}

fn replay_pevcap_capture(
    capture: &PevcapCapture,
    profile: SelectedSessionProfile,
) -> PevcapReplayReport {
    match profile {
        SelectedSessionProfile::Aero => replay_pevcap_with_session(
            capture,
            ReadOnlySession::<NosfetAeroModel, false>::default(),
        ),
        SelectedSessionProfile::Falcon => replay_pevcap_with_session(
            capture,
            ReadOnlySession::<BegodeFalconModel, true>::default(),
        ),
    }
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
                if let cutout_core::ReadOnlyResponse::Firmware(firmware) = response {
                    report.firmware = Some(*firmware);
                }
            }
            DeviceEvent::Diagnostics(_) => {
                report.diagnostics += 1;
            }
            DeviceEvent::LinkUp(_)
            | DeviceEvent::LinkDown
            | DeviceEvent::NotificationReceived { .. }
            | DeviceEvent::Tick { .. } => {}
        }
    }

    report
}

fn render_pevcap_replay_report(report: &PevcapReplayReport) -> String {
    format!(
        "pevcap replay records={} outputs={} telemetry={} read_only_responses={} diagnostics={} arbitrary_chunk_plan_len={} chunk_one_byte_matches={} chunk_arbitrary_matches={}",
        report.replay_records,
        report.outputs,
        report.telemetry,
        report.read_only_responses,
        report.diagnostics,
        report.arbitrary_chunk_plan_len,
        report.chunk_one_byte_matches,
        report.chunk_arbitrary_matches
    )
}

async fn dashboard(args: DashboardArgs) -> Result<()> {
    if args.demo {
        return run_dashboard(DashboardState::demo(args.device.as_deref()));
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
    let updates = spawn_dashboard_live_updates(connection);
    run_dashboard_with_updates(state, &updates)
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

fn spawn_dashboard_live_updates(
    connection: ConnectedPeripheral,
) -> mpsc::Receiver<DashboardUpdate> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = tx.send(DashboardUpdate::Log {
                    level: "error".to_owned(),
                    message: format!("dashboard update runtime failed: {error}"),
                });
                return;
            }
        };

        runtime.block_on(run_dashboard_live_updates(connection, tx));
    });
    rx
}

async fn run_dashboard_live_updates(
    connection: ConnectedPeripheral,
    tx: mpsc::Sender<DashboardUpdate>,
) {
    if connection.summary.select_session_endpoints().is_none() {
        let _ = tx.send(DashboardUpdate::Log {
            level: "warn".to_owned(),
            message: "dashboard session endpoints unavailable".to_owned(),
        });
        return;
    }

    let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
    let mut iteration = 0_u64;

    loop {
        if iteration % DASHBOARD_BATTERY_REFRESH_EVERY == 0 {
            match read_battery_level(&connection.peripheral, &connection.summary).await {
                Ok(Some(percent)) => {
                    if tx.send(DashboardUpdate::BatteryPercent(percent)).is_err() {
                        return;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    if tx
                        .send(DashboardUpdate::Log {
                            level: "warn".to_owned(),
                            message: format!("dashboard battery refresh failed: {error}"),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }

        let Some(endpoints) = connection.summary.select_session_endpoints() else {
            return;
        };
        match drive_session(
            &connection.peripheral,
            &mut session,
            VETERAN_DATA_CHANNEL,
            &connection.summary,
            endpoints,
            DASHBOARD_LIVE_WINDOW,
        )
        .await
        {
            Ok(report) => {
                if tx
                    .send(DashboardUpdate::SessionReport(Box::new(report)))
                    .is_err()
                {
                    return;
                }
            }
            Err(error) => {
                if tx
                    .send(DashboardUpdate::Log {
                        level: "warn".to_owned(),
                        message: format!("dashboard session update failed, retrying: {error}"),
                    })
                    .is_err()
                {
                    return;
                }
                tokio::time::sleep(DASHBOARD_LIVE_WINDOW).await;
            }
        }

        iteration = iteration.wrapping_add(1);
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
            let bytes = encode_raw_capture_pevcap(
                &records,
                &connection.summary,
                Some(connection.peripheral.mtu()),
                format,
                capture_wall_clock_unix_ms(),
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
    let connection =
        connect_and_discover(&args.into_target(), Duration::from_secs(seconds)).await?;

    println!("{}", connection.summary);
    if let Some(endpoints) = connection.summary.select_session_endpoints() {
        print_session_endpoints(endpoints);
        mode.run(
            &connection,
            endpoints,
            profile,
            &commands,
            Duration::from_secs(seconds),
        )
        .await?;
    }

    Ok(())
}

async fn capture_aero(args: CaptureAeroArgs) -> Result<()> {
    let output = args
        .pevcap_output
        .map_or(CaptureOutput::Text, |path| CaptureOutput::Pevcap {
            path,
            format: args.pevcap_format,
        });
    connect(args.target, SessionMode::Capture { output }).await
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
    },
}

impl SessionMode {
    async fn run(
        self,
        connection: &ConnectedPeripheral,
        endpoints: SessionEndpoints<'_>,
        profile: SelectedSessionProfile,
        commands: &[DeviceCommand],
        window: Duration,
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
                    commands,
                    window,
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
                    commands,
                    window,
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
        commands: &[DeviceCommand],
        window: Duration,
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
                    window,
                    commands,
                )
                .await?;
                print_session_report(&report);
            }
            Self::Capture { output } => {
                let capture = capture_session_with_commands(
                    &connection.peripheral,
                    &mut session,
                    binding.channel,
                    &connection.summary,
                    endpoints,
                    window,
                    commands,
                )
                .await?;
                write_or_print_capture(capture, &connection.summary, &output, binding.profile)?;
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

fn print_capture(capture: SessionCapture) {
    for record in capture.records {
        println!("{record}");
    }
    print_session_report(&capture.report);
}

fn write_or_print_capture(
    capture: SessionCapture,
    summary: &cutout_btle::ConnectionSummary,
    output: &CaptureOutput,
    profile: SelectedSessionProfile,
) -> Result<()> {
    match output {
        CaptureOutput::Text => {
            print_capture(capture);
            Ok(())
        }
        CaptureOutput::Pevcap { path, format } => {
            let report = capture.report.clone();
            let bytes = encode_session_capture_pevcap(
                &capture,
                summary,
                *format,
                capture_wall_clock_unix_ms(),
                profile,
            )?;
            fs::write(path, bytes)?;
            println!("wrote pevcap {} ({format:?})", path.display());
            print_session_report(&report);
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
) -> Result<Vec<u8>> {
    let pevcap = capture.to_pevcap(
        summary,
        PevcapSessionMetadata {
            wall_clock_start_unix_ms,
            platform_id: std::env::consts::OS,
            library_version: env!("CARGO_PKG_VERSION"),
            registry_hash: cutout_core::registry_entries_hash(&[&BEGODE_FALCON_REGISTRY_ENTRY]),
            resolved_identity: Some(pevcap_identity_for_profile(profile)),
            annotations: &["cutout-cli capture-aero"],
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
) -> Result<Vec<u8>> {
    let advertised_services = summary
        .observation
        .advertised_services
        .iter()
        .copied()
        .map(gatt_channel_from_uuid)
        .collect::<Vec<_>>();
    let gatt_fingerprints = summary.gatt_fingerprints();
    let pevcap_records = records
        .iter()
        .map(|record| {
            PevcapRecord::inbound_notification(
                record.monotonic_ms,
                gatt_channel_from_uuid(record.characteristic),
                gatt_channel_from_uuid(record.service),
                record.bytes.clone(),
            )
        })
        .collect();
    let header = PevcapHeader::new(
        wall_clock_start_unix_ms,
        std::env::consts::OS,
        write_limit,
        &advertised_services,
        &gatt_fingerprints,
        None,
        env!("CARGO_PKG_VERSION"),
        cutout_core::registry_entries_hash(&[&BEGODE_FALCON_REGISTRY_ENTRY]),
        &["cutout-cli subscribe-raw"],
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
        "session writes={} subscribes={} notifications={} telemetry={} read_only_responses={} diagnostics={} disconnects={}",
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
    use btleplug::api::{CharPropFlags, WriteType};
    use cutout_btle::{
        BridgeIdentityResolution, ConnectionSummary, ConnectionTarget, PeripheralObservation,
        RawNotificationRecord, ServiceSummary, SessionCaptureRecord,
    };
    use cutout_core::{
        GattChannel, PevcapHeader, PevcapRecord, ProtocolFamily, VerificationStatus, VerifiedValue,
        WriteMode,
    };
    use cutout_protocols::{
        BEGODE_FALCON_REGISTRY_ENTRY, BegodeBanner, DeviceFamily, IdentityConfidence,
        ProtocolFamilyClassification, StagedIdentityInput, identify_model,
    };
    use uuid::Uuid;

    use super::*;
    use crate::cli::ScanArgs;

    fn dashboard_args(demo: bool, device: Option<&str>) -> DashboardArgs {
        DashboardArgs {
            demo,
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
        )
        .expect("capture encodes");
        let decoded =
            PevcapCapture::decode(&bytes, PevcapEncoding::Binary).expect("binary PEVCAP decodes");

        assert_eq!(decoded.header.wall_clock_start_unix_ms, 42);
        assert_eq!(decoded.header.write_limit, Some(23));
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
        assert_eq!(decoded.records.len(), 2);
        assert_eq!(
            decoded.records[0].write_mode,
            Some(WriteMode::WithoutResponse)
        );
        assert_eq!(decoded.records[1].bytes, b"NAME=NF2557");
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

        let bytes =
            encode_raw_capture_pevcap(&records, &summary, Some(185), PevcapFormat::Binary, 99)
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
            &["cutout-cli subscribe-raw".to_owned()]
        );
        assert_eq!(decoded.records.len(), 1);
        assert_eq!(decoded.records[0].bytes, [0xde, 0xad, 0xbe, 0xef]);
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
        };

        assert_eq!(
            render_pevcap_replay_report(&report),
            "pevcap replay records=2 outputs=3 telemetry=1 read_only_responses=1 diagnostics=1 arbitrary_chunk_plan_len=3 chunk_one_byte_matches=true chunk_arbitrary_matches=true"
        );
    }

    #[test]
    fn pevcap_replay_drives_selected_aero_session() {
        let capture = sample_aero_replay_capture();

        let report = replay_pevcap_capture(&capture, SelectedSessionProfile::Aero);

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
    fn pevcap_replay_corpus_auto_selects_and_matches_chunk_modes() {
        for case in PEVCAP_REPLAY_CORPUS {
            let capture = PevcapCapture::decode(case.jsonl.as_bytes(), PevcapEncoding::Jsonl)
                .unwrap_or_else(|error| panic!("{} should decode: {error}", case.name));
            let profile = selected_pevcap_replay_profile(&capture, SessionProfile::Auto)
                .unwrap_or_else(|error| panic!("{} should auto-select: {error}", case.name));
            let report = replay_pevcap_capture(&capture, profile);

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
            source: cutout_core::ValueSource::Reported,
            quality: cutout_core::ValueQuality::Known,
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
}
