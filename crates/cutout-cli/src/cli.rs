use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use cutout_btle::ConnectionTarget;
use cutout_core::{CaptureDistribution, CaptureEvidence, CapturePrivacy, CaptureSessionLabel};
use uuid::Uuid;

const DEFAULT_SCAN_SECONDS: u64 = 5;
const CLI_LONG_ABOUT: &str = "\
Cutout is a cautious Bluetooth Low Energy utility for inspecting nearby PEV
controllers and collecting read-only Aero/Veteran-family protocol evidence.

The CLI currently focuses on discovery, endpoint inspection, and fixture
capture. Commands may connect to hardware, but the protocol session used here
is read-only.";
const CLI_AFTER_LONG_HELP: &str = "\
Examples:
  cutout scan --seconds 10
  cutout connect --name-contains Aero
  cutout connect --address AA:BB:CC:DD:EE:FF --seconds 8
  cutout subscribe-raw --name-contains NF2557 --characteristic 0000ffe1-0000-1000-8000-00805f9b34fb
  cutout capture-aero --name-contains NF2557 --seconds 20
  cutout pevcap convert --input session.pevcap.jsonl --input-format jsonl --output session.pevcap --output-format binary
  cutout validation
  cutout dashboard --demo --device \"Aero NF2557\"

Target selection:
  --address matches the Bluetooth address reported by the platform.
  --name-contains matches a case-sensitive substring of the advertised name.
  When neither target filter is supplied, the first matching peripheral from
  the scan results is used.";
const SCAN_LONG_ABOUT: &str = "\
Scan for nearby Bluetooth Low Energy peripherals and print one observation per
line. Output includes the platform identifier or address, advertised name,
RSSI when available, advertised service UUIDs, manufacturer-data summaries,
and candidate family hints. Names and advertised services are hints only, not
final device identification.";
const CONNECT_LONG_ABOUT: &str = "\
Scan for a peripheral, connect to it, discover its GATT services, and print a
summary of the discovered service/characteristic tree.

If writable and notification-capable endpoints are discovered, Cutout also
runs a read-only probe session against the selected endpoints and prints bridge
counters. Profile auto currently keeps the existing Aero/Veteran path; use
--profile falcon with --probe identity --probe firmware for explicit Falcon
verification.";
const SUBSCRIBE_RAW_LONG_ABOUT: &str = "\
Scan for a peripheral, connect, discover GATT, subscribe to a notify/indicate
characteristic, and print raw notification chunks with timestamps.

This command is protocol-agnostic. Use --characteristic to select a specific
notify/indicate characteristic UUID; otherwise the first notify-capable
characteristic in the discovered GATT tree is used.";
const CAPTURE_AERO_LONG_ABOUT: &str = "\
Connect to an Aero/Veteran-family device and print capture records suitable for
fixture work. Records include link metadata, subscribe/write actions, inbound
notifications, provisional write bytes, and bridge counters. For explicit
Falcon verification, use --profile falcon with --probe identity --probe
firmware. Profile auto currently keeps the existing Aero/Veteran path.

Capture output may include device identifiers and raw notification payloads.
Review it before sharing logs publicly.";
const VALIDATION_LONG_ABOUT: &str = "\
Print a generated hardware validation matrix for the registry and capture
notes Cutout already knows about. The matrix shows device families, firmware
versions, capture IDs, tested fields, inferred fields, unverified fields,
controls, and acceptance status.";
const PEVCAP_LONG_ABOUT: &str = "\
Work with PEVCAP capture files without connecting to Bluetooth hardware.

PEVCAP JSONL is the review-friendly line-oriented representation. PEVCAP
binary is the compact framed container used for storage and replay tooling.";
const PEVCAP_CONVERT_LONG_ABOUT: &str = "\
Convert a PEVCAP capture between JSONL and binary container formats.

The command decodes the input through the canonical cutout-core PEVCAP model
before writing the output, so malformed magic, unsupported versions, truncated
records, and header bound violations fail before a converted file is written.";
const PEVCAP_REPLAY_LONG_ABOUT: &str = "\
Replay a PEVCAP capture into a selected read-only protocol session without
connecting to Bluetooth hardware.

By default the command auto-selects the replay profile from persisted PEVCAP
identity metadata. Use --profile to override that metadata for verification.
Use --diagnostics-jsonl to emit stable diagnostic snapshot records for replay
tooling.";
const DASHBOARD_LONG_ABOUT: &str = "\
Open a read-only Ratatui dashboard backed by the Termina terminal backend.
The dashboard is intended as a live inspection surface for discovery, device
selection, telemetry samples, and recent events while the profile model grows.
Use --demo for fixture-backed data. Without --demo, --device selects a live
Bluetooth device by advertised name substring and the dashboard opens only
after the device connects.

This command does not modify the device. It is a visualization and monitoring
surface for the data Cutout already knows how to collect.";

/// Parsed command-line arguments for the `cutout` binary.
#[derive(Debug, Parser)]
#[command(
    name = "cutout",
    about = "Inspect and capture read-only BTLE data from supported PEVs",
    long_about = CLI_LONG_ABOUT,
    after_long_help = CLI_AFTER_LONG_HELP
)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub(crate) enum Command {
    /// List nearby BLE peripherals.
    #[command(long_about = SCAN_LONG_ABOUT)]
    Scan(ScanArgs),

    /// Inspect GATT services on a selected peripheral.
    #[command(long_about = CONNECT_LONG_ABOUT)]
    Connect(TargetedScanArgs),

    /// Subscribe to a raw notify/indicate characteristic.
    #[command(long_about = SUBSCRIBE_RAW_LONG_ABOUT)]
    SubscribeRaw(RawSubscribeArgs),

    /// Capture read-only Aero/Veteran protocol evidence.
    #[command(long_about = CAPTURE_AERO_LONG_ABOUT)]
    CaptureAero(CaptureAeroArgs),

    /// Print the generated hardware validation matrix.
    #[command(long_about = VALIDATION_LONG_ABOUT)]
    Validation,

    /// Work with PEVCAP capture files.
    #[command(long_about = PEVCAP_LONG_ABOUT)]
    Pevcap(PevcapArgs),

    /// Open the interactive read-only dashboard.
    #[command(long_about = DASHBOARD_LONG_ABOUT)]
    Dashboard(DashboardArgs),
}

#[derive(Clone, Copy, Debug, Args, PartialEq, Eq)]
pub(crate) struct ScanArgs {
    /// Seconds to listen for advertisements before continuing.
    #[arg(long, value_name = "SECONDS", default_value_t = DEFAULT_SCAN_SECONDS)]
    pub(crate) seconds: u64,
}

impl ScanArgs {
    pub(crate) const fn seconds(self) -> u64 {
        self.seconds
    }
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct TargetedScanArgs {
    #[command(flatten)]
    pub(crate) target: TargetArgs,

    #[command(flatten)]
    pub(crate) scan: ScanArgs,

    /// Read-only protocol profile to run after connecting.
    #[arg(long, value_enum, default_value_t = SessionProfile::Auto)]
    pub(crate) profile: SessionProfile,

    /// Explicit read-only probe command to issue after subscribing.
    #[arg(long = "probe", value_enum)]
    pub(crate) probes: Vec<ReadProbe>,

    /// Emit aggregate session diagnostics as JSONL records.
    #[arg(long = "diagnostics-jsonl")]
    pub(crate) diagnostics_jsonl: bool,

    /// Emit read-only response DTOs as JSONL records.
    #[arg(long = "read-only-jsonl")]
    pub(crate) read_only_jsonl: bool,
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct CaptureAeroArgs {
    #[command(flatten)]
    pub(crate) target: TargetedScanArgs,

    /// Maximum connected-link attempts for reconnect capture.
    #[arg(long = "reconnect-attempts", value_name = "COUNT", default_value_t = 1)]
    pub(crate) reconnect_attempts: usize,

    /// Standard capture scenario label stored in PEVCAP metadata.
    #[arg(long = "capture-label", value_enum)]
    pub(crate) capture_label: Option<CaptureLabelArg>,

    /// Capture privacy marker stored in PEVCAP metadata.
    #[arg(long = "capture-privacy", value_enum)]
    pub(crate) capture_privacy: Option<CapturePrivacyArg>,

    /// Capture evidence marker stored in PEVCAP metadata.
    #[arg(long = "capture-evidence", value_enum)]
    pub(crate) capture_evidence: Option<CaptureEvidenceArg>,

    /// Capture redistribution marker stored in PEVCAP metadata.
    #[arg(long = "capture-distribution", value_enum)]
    pub(crate) capture_distribution: Option<CaptureDistributionArg>,

    /// Optional PEVCAP output path for the captured session.
    #[arg(long = "pevcap-output", value_name = "PATH")]
    pub(crate) pevcap_output: Option<PathBuf>,

    /// PEVCAP output format when --pevcap-output is supplied.
    #[arg(long = "pevcap-format", value_enum, default_value_t = PevcapFormat::Jsonl)]
    pub(crate) pevcap_format: PevcapFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub(crate) enum CaptureLabelArg {
    PoweredOnStationary,
    RollingForward,
    RollingBackward,
    LiftedWheel,
    Charging,
    HeadlightToggled,
    Horn,
    RideModeChange,
    AlarmChange,
    BmsScreen,
    DisconnectReconnect,
    PowerCycle,
}

impl From<CaptureLabelArg> for CaptureSessionLabel {
    fn from(value: CaptureLabelArg) -> Self {
        match value {
            CaptureLabelArg::PoweredOnStationary => Self::PoweredOnStationary,
            CaptureLabelArg::RollingForward => Self::RollingForward,
            CaptureLabelArg::RollingBackward => Self::RollingBackward,
            CaptureLabelArg::LiftedWheel => Self::LiftedWheel,
            CaptureLabelArg::Charging => Self::Charging,
            CaptureLabelArg::HeadlightToggled => Self::HeadlightToggled,
            CaptureLabelArg::Horn => Self::Horn,
            CaptureLabelArg::RideModeChange => Self::RideModeChange,
            CaptureLabelArg::AlarmChange => Self::AlarmChange,
            CaptureLabelArg::BmsScreen => Self::BmsScreen,
            CaptureLabelArg::DisconnectReconnect => Self::DisconnectReconnect,
            CaptureLabelArg::PowerCycle => Self::PowerCycle,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub(crate) enum CapturePrivacyArg {
    Private,
    Redacted,
}

impl From<CapturePrivacyArg> for CapturePrivacy {
    fn from(value: CapturePrivacyArg) -> Self {
        match value {
            CapturePrivacyArg::Private => Self::Private,
            CapturePrivacyArg::Redacted => Self::Redacted,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub(crate) enum CaptureEvidenceArg {
    HardwareTested,
    Inferred,
    Unverified,
}

impl From<CaptureEvidenceArg> for CaptureEvidence {
    fn from(value: CaptureEvidenceArg) -> Self {
        match value {
            CaptureEvidenceArg::HardwareTested => Self::HardwareTested,
            CaptureEvidenceArg::Inferred => Self::Inferred,
            CaptureEvidenceArg::Unverified => Self::Unverified,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub(crate) enum CaptureDistributionArg {
    Redistributable,
}

impl From<CaptureDistributionArg> for CaptureDistribution {
    fn from(value: CaptureDistributionArg) -> Self {
        match value {
            CaptureDistributionArg::Redistributable => Self::Redistributable,
        }
    }
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct RawSubscribeArgs {
    #[command(flatten)]
    pub(crate) target: TargetArgs,

    #[command(flatten)]
    pub(crate) scan: ScanArgs,

    /// Notify/indicate characteristic UUID to subscribe to.
    #[arg(long, value_name = "UUID")]
    pub(crate) characteristic: Option<Uuid>,

    /// Standard capture scenario label stored in PEVCAP metadata.
    #[arg(long = "capture-label", value_enum)]
    pub(crate) capture_label: Option<CaptureLabelArg>,

    /// Capture privacy marker stored in PEVCAP metadata.
    #[arg(long = "capture-privacy", value_enum)]
    pub(crate) capture_privacy: Option<CapturePrivacyArg>,

    /// Capture evidence marker stored in PEVCAP metadata.
    #[arg(long = "capture-evidence", value_enum)]
    pub(crate) capture_evidence: Option<CaptureEvidenceArg>,

    /// Capture redistribution marker stored in PEVCAP metadata.
    #[arg(long = "capture-distribution", value_enum)]
    pub(crate) capture_distribution: Option<CaptureDistributionArg>,

    /// Optional PEVCAP output path for the raw notification capture.
    #[arg(long = "pevcap-output", value_name = "PATH")]
    pub(crate) pevcap_output: Option<PathBuf>,

    /// PEVCAP output format when --pevcap-output is supplied.
    #[arg(long = "pevcap-format", value_enum, default_value_t = PevcapFormat::Jsonl)]
    pub(crate) pevcap_format: PevcapFormat,
}

impl RawSubscribeArgs {
    pub(crate) const fn seconds(&self) -> u64 {
        self.scan.seconds()
    }
}

impl TargetedScanArgs {
    pub(crate) fn into_target(self) -> ConnectionTarget {
        self.target.into()
    }

    pub(crate) const fn seconds(&self) -> u64 {
        self.scan.seconds()
    }

    pub(crate) const fn profile(&self) -> SessionProfile {
        self.profile
    }

    pub(crate) fn probes(&self) -> &[ReadProbe] {
        &self.probes
    }

    pub(crate) const fn diagnostics_jsonl(&self) -> bool {
        self.diagnostics_jsonl
    }

    pub(crate) const fn read_only_jsonl(&self) -> bool {
        self.read_only_jsonl
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum SessionProfile {
    #[default]
    Auto,
    Aero,
    Falcon,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ReadProbe {
    Identity,
    Firmware,
    Telemetry,
    Battery,
    Diagnostics,
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct PevcapArgs {
    #[command(subcommand)]
    pub(crate) command: PevcapCommand,
}

#[derive(Clone, Debug, Subcommand, PartialEq, Eq)]
pub(crate) enum PevcapCommand {
    /// Convert a capture between JSONL and binary PEVCAP formats.
    #[command(long_about = PEVCAP_CONVERT_LONG_ABOUT)]
    Convert(PevcapConvertArgs),

    /// Replay a capture through a selected read-only session.
    #[command(long_about = PEVCAP_REPLAY_LONG_ABOUT)]
    Replay(PevcapReplayArgs),
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct PevcapConvertArgs {
    /// Input capture path.
    #[arg(long, value_name = "PATH")]
    pub(crate) input: PathBuf,

    /// Input capture format.
    #[arg(long = "input-format", value_enum)]
    pub(crate) input_format: PevcapFormat,

    /// Output capture path.
    #[arg(long, value_name = "PATH")]
    pub(crate) output: PathBuf,

    /// Output capture format.
    #[arg(long = "output-format", value_enum)]
    pub(crate) output_format: PevcapFormat,
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct PevcapReplayArgs {
    /// Input capture path.
    #[arg(long, value_name = "PATH")]
    pub(crate) input: PathBuf,

    /// Input capture format.
    #[arg(long = "input-format", value_enum)]
    pub(crate) input_format: PevcapFormat,

    /// Read-only protocol profile used for replay.
    #[arg(long, value_enum, default_value_t = SessionProfile::Auto)]
    pub(crate) profile: SessionProfile,

    /// Emit diagnostic snapshots as JSONL records after the replay summary.
    #[arg(long = "diagnostics-jsonl")]
    pub(crate) diagnostics_jsonl: bool,

    /// Emit read-only response DTOs as JSONL records after the replay summary.
    #[arg(long = "read-only-jsonl")]
    pub(crate) read_only_jsonl: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum PevcapFormat {
    Jsonl,
    Binary,
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct DashboardArgs {
    /// Use checked-in fixture data instead of a live session.
    #[arg(long)]
    pub(crate) demo: bool,

    /// Live device name substring, or demo device name when used with --demo.
    #[arg(long = "device", value_name = "NAME")]
    pub(crate) device: Option<String>,

    #[command(flatten)]
    pub(crate) scan: ScanArgs,
}

impl DashboardArgs {
    pub(crate) const fn seconds(&self) -> u64 {
        self.scan.seconds()
    }
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct TargetArgs {
    /// Bluetooth address to select from scan results.
    #[arg(long, value_name = "ADDR")]
    pub(crate) address: Option<String>,

    /// Platform-specific peripheral identifier to select from scan results.
    #[arg(long = "id", value_name = "ID")]
    pub(crate) identifier: Option<String>,

    /// Case-sensitive substring that must appear in the advertised name.
    #[arg(long = "name-contains", value_name = "TEXT")]
    pub(crate) name_contains: Option<String>,
}

impl From<TargetArgs> for ConnectionTarget {
    fn from(target: TargetArgs) -> Self {
        Self {
            address: target.address,
            identifier: target.identifier,
            name_contains: target.name_contains,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::{CommandFactory, Parser, error::ErrorKind};
    use uuid::Uuid;

    use super::{
        CaptureAeroArgs, CaptureDistributionArg, CaptureEvidenceArg, CaptureLabelArg,
        CapturePrivacyArg, Cli, Command, DEFAULT_SCAN_SECONDS, DashboardArgs, PevcapArgs,
        PevcapCommand, PevcapConvertArgs, PevcapFormat, PevcapReplayArgs, RawSubscribeArgs,
        ReadProbe, ScanArgs, SessionProfile, TargetArgs, TargetedScanArgs,
    };

    fn assert_contains_all(haystack: &str, needles: &[&str]) {
        for needle in needles {
            assert!(
                haystack.contains(needle),
                "expected help text to contain {needle:?}:\n{haystack}"
            );
        }
    }

    fn long_help_for(subcommand: &str) -> String {
        let mut command = Cli::command();
        command
            .find_subcommand_mut(subcommand)
            .expect("subcommand exists")
            .render_long_help()
            .to_string()
    }

    fn short_help_for(subcommand: &str) -> String {
        let mut command = Cli::command();
        command
            .find_subcommand_mut(subcommand)
            .expect("subcommand exists")
            .render_help()
            .to_string()
    }

    #[test]
    fn parses_scan_command_with_default_duration() {
        let cli = Cli::try_parse_from(["cutout", "scan"]).expect("parser accepts scan");

        assert_eq!(
            cli.command,
            Command::Scan(ScanArgs {
                seconds: DEFAULT_SCAN_SECONDS
            })
        );
    }

    #[test]
    fn parses_scan_command_with_custom_duration() {
        let cli = Cli::try_parse_from(["cutout", "scan", "--seconds", "12"])
            .expect("parser accepts scan duration");

        assert_eq!(cli.command, Command::Scan(ScanArgs { seconds: 12 }));
    }

    #[test]
    fn parses_scan_command_with_zero_duration() {
        let cli = Cli::try_parse_from(["cutout", "scan", "--seconds", "0"])
            .expect("parser accepts zero duration");

        assert_eq!(cli.command, Command::Scan(ScanArgs { seconds: 0 }));
    }

    #[test]
    fn parses_connect_command_with_default_duration() {
        let cli = Cli::try_parse_from(["cutout", "connect", "--address", "AA:BB:CC:DD:EE:FF"])
            .expect("parser accepts connect with default duration");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                    identifier: None,
                    name_contains: None,
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
                profile: SessionProfile::Auto,
                probes: Vec::new(),
                diagnostics_jsonl: false,
                read_only_jsonl: false,
            })
        );
    }

    #[test]
    fn parses_connect_command_with_address_target() {
        let cli = Cli::try_parse_from([
            "cutout",
            "connect",
            "--address",
            "AA:BB:CC:DD:EE:FF",
            "--name-contains",
            "Aero",
            "--seconds",
            "8",
        ])
        .expect("parser accepts connect");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                    identifier: None,
                    name_contains: Some("Aero".to_owned()),
                },
                scan: ScanArgs { seconds: 8 },
                profile: SessionProfile::Auto,
                probes: Vec::new(),
                diagnostics_jsonl: false,
                read_only_jsonl: false,
            })
        );
    }

    #[test]
    fn parses_connect_command_with_diagnostics_jsonl() {
        let cli = Cli::try_parse_from([
            "cutout",
            "connect",
            "--name-contains",
            "NF2557",
            "--diagnostics-jsonl",
        ])
        .expect("parser accepts connect diagnostic JSONL flag");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: None,
                    identifier: None,
                    name_contains: Some("NF2557".to_owned()),
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
                profile: SessionProfile::Auto,
                probes: Vec::new(),
                diagnostics_jsonl: true,
                read_only_jsonl: false,
            })
        );
    }

    #[test]
    fn parses_connect_command_with_read_only_jsonl() {
        let cli = Cli::try_parse_from([
            "cutout",
            "connect",
            "--name-contains",
            "NF2557",
            "--read-only-jsonl",
        ])
        .expect("parser accepts connect read-only JSONL flag");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: None,
                    identifier: None,
                    name_contains: Some("NF2557".to_owned()),
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
                profile: SessionProfile::Auto,
                probes: Vec::new(),
                diagnostics_jsonl: false,
                read_only_jsonl: true,
            })
        );
    }

    #[test]
    fn parses_connect_command_with_scan_duration_before_target() {
        let cli = Cli::try_parse_from([
            "cutout",
            "connect",
            "--seconds",
            "11",
            "--name-contains",
            "Aero",
        ])
        .expect("parser accepts connect duration before target");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: None,
                    identifier: None,
                    name_contains: Some("Aero".to_owned()),
                },
                scan: ScanArgs { seconds: 11 },
                profile: SessionProfile::Auto,
                probes: Vec::new(),
                diagnostics_jsonl: false,
                read_only_jsonl: false,
            })
        );
    }

    #[test]
    fn parses_connect_command_with_name_target_only() {
        let cli = Cli::try_parse_from(["cutout", "connect", "--name-contains", "Aero"])
            .expect("parser accepts connect by name");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: None,
                    identifier: None,
                    name_contains: Some("Aero".to_owned()),
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
                profile: SessionProfile::Auto,
                probes: Vec::new(),
                diagnostics_jsonl: false,
                read_only_jsonl: false,
            })
        );
    }

    #[test]
    fn parses_connect_command_without_target_filters() {
        let cli =
            Cli::try_parse_from(["cutout", "connect"]).expect("parser accepts untargeted connect");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: None,
                    identifier: None,
                    name_contains: None,
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
                profile: SessionProfile::Auto,
                probes: Vec::new(),
                diagnostics_jsonl: false,
                read_only_jsonl: false,
            })
        );
    }

    #[test]
    fn parses_subscribe_raw_command_with_explicit_characteristic() {
        let cli = Cli::try_parse_from([
            "cutout",
            "subscribe-raw",
            "--name-contains",
            "NF2557",
            "--seconds",
            "7",
            "--characteristic",
            "0000ffe1-0000-1000-8000-00805f9b34fb",
        ])
        .expect("parser accepts raw subscription");

        assert_eq!(
            cli.command,
            Command::SubscribeRaw(RawSubscribeArgs {
                target: TargetArgs {
                    address: None,
                    identifier: None,
                    name_contains: Some("NF2557".to_owned()),
                },
                scan: ScanArgs { seconds: 7 },
                characteristic: Some(Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb)),
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: None,
                pevcap_format: PevcapFormat::Jsonl,
            })
        );
    }

    #[test]
    fn parses_subscribe_raw_command_with_pevcap_output() {
        let cli = Cli::try_parse_from([
            "cutout",
            "subscribe-raw",
            "--name-contains",
            "NF2557",
            "--pevcap-output",
            "raw.pevcap",
            "--pevcap-format",
            "binary",
        ])
        .expect("parser accepts raw PEVCAP output");

        assert_eq!(
            cli.command,
            Command::SubscribeRaw(RawSubscribeArgs {
                target: TargetArgs {
                    address: None,
                    identifier: None,
                    name_contains: Some("NF2557".to_owned()),
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
                characteristic: None,
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: Some(PathBuf::from("raw.pevcap")),
                pevcap_format: PevcapFormat::Binary,
            })
        );
    }

    #[test]
    fn parses_subscribe_raw_command_with_capture_annotations() {
        let cli = Cli::try_parse_from([
            "cutout",
            "subscribe-raw",
            "--name-contains",
            "Unknown PEV",
            "--capture-label",
            "powered-on-stationary",
            "--capture-privacy",
            "private",
            "--capture-evidence",
            "hardware-tested",
        ])
        .expect("parser accepts raw capture provenance annotations");

        let Command::SubscribeRaw(args) = cli.command else {
            panic!("expected subscribe-raw command");
        };

        assert_eq!(
            args.capture_label,
            Some(CaptureLabelArg::PoweredOnStationary)
        );
        assert_eq!(args.capture_privacy, Some(CapturePrivacyArg::Private));
        assert_eq!(
            args.capture_evidence,
            Some(CaptureEvidenceArg::HardwareTested)
        );
        assert_eq!(args.capture_distribution, None);
    }

    #[test]
    fn parses_capture_aero_command_with_name_target() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture-aero",
            "--name-contains",
            "NF2557",
            "--seconds",
            "3",
        ])
        .expect("parser accepts capture-aero");

        assert_eq!(
            cli.command,
            Command::CaptureAero(CaptureAeroArgs {
                target: TargetedScanArgs {
                    target: TargetArgs {
                        address: None,
                        identifier: None,
                        name_contains: Some("NF2557".to_owned()),
                    },
                    scan: ScanArgs { seconds: 3 },
                    profile: SessionProfile::Auto,
                    probes: Vec::new(),
                    diagnostics_jsonl: false,
                    read_only_jsonl: false,
                },
                reconnect_attempts: 1,
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: None,
                pevcap_format: PevcapFormat::Jsonl,
            })
        );
    }

    #[test]
    fn parses_capture_aero_command_with_reconnect_attempts() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture-aero",
            "--name-contains",
            "NF2557",
            "--reconnect-attempts",
            "3",
        ])
        .expect("parser accepts reconnect attempts");

        let Command::CaptureAero(args) = cli.command else {
            panic!("expected capture-aero command");
        };

        assert_eq!(args.reconnect_attempts, 3);
    }

    #[test]
    fn parses_capture_aero_command_with_address_target() {
        let cli = Cli::try_parse_from(["cutout", "capture-aero", "--address", "AA:BB:CC:DD:EE:FF"])
            .expect("parser accepts capture-aero by address");

        assert_eq!(
            cli.command,
            Command::CaptureAero(CaptureAeroArgs {
                target: TargetedScanArgs {
                    target: TargetArgs {
                        address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                        identifier: None,
                        name_contains: None,
                    },
                    scan: ScanArgs {
                        seconds: DEFAULT_SCAN_SECONDS
                    },
                    profile: SessionProfile::Auto,
                    probes: Vec::new(),
                    diagnostics_jsonl: false,
                    read_only_jsonl: false,
                },
                reconnect_attempts: 1,
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: None,
                pevcap_format: PevcapFormat::Jsonl,
            })
        );
    }

    #[test]
    fn parses_capture_aero_command_with_both_target_filters() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture-aero",
            "--address",
            "AA:BB:CC:DD:EE:FF",
            "--name-contains",
            "NF2557",
            "--seconds",
            "21",
        ])
        .expect("parser accepts capture-aero with both filters");

        assert_eq!(
            cli.command,
            Command::CaptureAero(CaptureAeroArgs {
                target: TargetedScanArgs {
                    target: TargetArgs {
                        address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                        identifier: None,
                        name_contains: Some("NF2557".to_owned()),
                    },
                    scan: ScanArgs { seconds: 21 },
                    profile: SessionProfile::Auto,
                    probes: Vec::new(),
                    diagnostics_jsonl: false,
                    read_only_jsonl: false,
                },
                reconnect_attempts: 1,
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: None,
                pevcap_format: PevcapFormat::Jsonl,
            })
        );
    }

    #[test]
    fn parses_capture_aero_command_with_falcon_profile() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture-aero",
            "--name-contains",
            "Falcon",
            "--profile",
            "falcon",
        ])
        .expect("parser accepts explicit Falcon profile");

        assert_eq!(
            cli.command,
            Command::CaptureAero(CaptureAeroArgs {
                target: TargetedScanArgs {
                    target: TargetArgs {
                        address: None,
                        identifier: None,
                        name_contains: Some("Falcon".to_owned()),
                    },
                    scan: ScanArgs {
                        seconds: DEFAULT_SCAN_SECONDS
                    },
                    profile: SessionProfile::Falcon,
                    probes: Vec::new(),
                    diagnostics_jsonl: false,
                    read_only_jsonl: false,
                },
                reconnect_attempts: 1,
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: None,
                pevcap_format: PevcapFormat::Jsonl,
            })
        );
    }

    #[test]
    fn parses_capture_aero_command_with_falcon_profile_and_read_probes() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture-aero",
            "--name-contains",
            "Falcon",
            "--profile",
            "falcon",
            "--probe",
            "identity",
            "--probe",
            "firmware",
        ])
        .expect("parser accepts explicit Falcon read probes");

        assert_eq!(
            cli.command,
            Command::CaptureAero(CaptureAeroArgs {
                target: TargetedScanArgs {
                    target: TargetArgs {
                        address: None,
                        identifier: None,
                        name_contains: Some("Falcon".to_owned()),
                    },
                    scan: ScanArgs {
                        seconds: DEFAULT_SCAN_SECONDS
                    },
                    profile: SessionProfile::Falcon,
                    probes: vec![ReadProbe::Identity, ReadProbe::Firmware],
                    diagnostics_jsonl: false,
                    read_only_jsonl: false,
                },
                reconnect_attempts: 1,
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: None,
                pevcap_format: PevcapFormat::Jsonl,
            })
        );
    }

    #[test]
    fn parses_capture_aero_command_with_pevcap_output() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture-aero",
            "--name-contains",
            "NF2557",
            "--pevcap-output",
            "session.pevcap",
            "--pevcap-format",
            "binary",
        ])
        .expect("parser accepts capture PEVCAP output");

        assert_eq!(
            cli.command,
            Command::CaptureAero(CaptureAeroArgs {
                target: TargetedScanArgs {
                    target: TargetArgs {
                        address: None,
                        identifier: None,
                        name_contains: Some("NF2557".to_owned()),
                    },
                    scan: ScanArgs {
                        seconds: DEFAULT_SCAN_SECONDS
                    },
                    profile: SessionProfile::Auto,
                    probes: Vec::new(),
                    diagnostics_jsonl: false,
                    read_only_jsonl: false,
                },
                reconnect_attempts: 1,
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: Some(PathBuf::from("session.pevcap")),
                pevcap_format: PevcapFormat::Binary,
            })
        );
    }

    #[test]
    fn parses_capture_aero_command_with_capture_annotations() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture-aero",
            "--name-contains",
            "NF2557",
            "--capture-label",
            "charging",
            "--capture-privacy",
            "private",
            "--capture-evidence",
            "hardware-tested",
            "--capture-distribution",
            "redistributable",
        ])
        .expect("parser accepts capture provenance annotations");

        let Command::CaptureAero(args) = cli.command else {
            panic!("expected capture-aero command");
        };

        assert_eq!(args.capture_label, Some(CaptureLabelArg::Charging));
        assert_eq!(args.capture_privacy, Some(CapturePrivacyArg::Private));
        assert_eq!(
            args.capture_evidence,
            Some(CaptureEvidenceArg::HardwareTested)
        );
        assert_eq!(
            args.capture_distribution,
            Some(CaptureDistributionArg::Redistributable)
        );
    }

    #[test]
    fn parses_capture_aero_command_with_diagnostics_jsonl() {
        let cli = Cli::try_parse_from([
            "cutout",
            "capture-aero",
            "--name-contains",
            "NF2557",
            "--diagnostics-jsonl",
        ])
        .expect("parser accepts capture diagnostic JSONL flag");

        assert_eq!(
            cli.command,
            Command::CaptureAero(CaptureAeroArgs {
                target: TargetedScanArgs {
                    target: TargetArgs {
                        address: None,
                        identifier: None,
                        name_contains: Some("NF2557".to_owned()),
                    },
                    scan: ScanArgs {
                        seconds: DEFAULT_SCAN_SECONDS
                    },
                    profile: SessionProfile::Auto,
                    probes: Vec::new(),
                    diagnostics_jsonl: true,
                    read_only_jsonl: false,
                },
                reconnect_attempts: 1,
                capture_label: None,
                capture_privacy: None,
                capture_evidence: None,
                capture_distribution: None,
                pevcap_output: None,
                pevcap_format: PevcapFormat::Jsonl,
            })
        );
    }

    #[test]
    fn parses_dashboard_command() {
        let cli = Cli::try_parse_from(["cutout", "dashboard"]).expect("parser accepts dashboard");

        assert_eq!(
            cli.command,
            Command::Dashboard(DashboardArgs {
                demo: false,
                device: None,
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS,
                },
            })
        );
    }

    #[test]
    fn parses_dashboard_command_with_demo_and_device() {
        let cli = Cli::try_parse_from(["cutout", "dashboard", "--demo", "--device", "Aero NF2557"])
            .expect("parser accepts dashboard options");

        assert_eq!(
            cli.command,
            Command::Dashboard(DashboardArgs {
                demo: true,
                device: Some("Aero NF2557".to_owned()),
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS,
                },
            })
        );
    }

    #[test]
    fn parses_dashboard_command_with_live_device_and_scan_duration() {
        let cli = Cli::try_parse_from([
            "cutout",
            "dashboard",
            "--device",
            "NF2557",
            "--seconds",
            "12",
        ])
        .expect("parser accepts dashboard live target options");

        assert_eq!(
            cli.command,
            Command::Dashboard(DashboardArgs {
                demo: false,
                device: Some("NF2557".to_owned()),
                scan: ScanArgs { seconds: 12 },
            })
        );
    }

    #[test]
    fn parses_validation_command() {
        let cli = Cli::try_parse_from(["cutout", "validation"]).expect("parser accepts validation");

        assert_eq!(cli.command, Command::Validation);
    }

    #[test]
    fn parses_pevcap_convert_command() {
        let cli = Cli::try_parse_from([
            "cutout",
            "pevcap",
            "convert",
            "--input",
            "session.pevcap.jsonl",
            "--input-format",
            "jsonl",
            "--output",
            "session.pevcap",
            "--output-format",
            "binary",
        ])
        .expect("parser accepts PEVCAP conversion");

        assert_eq!(
            cli.command,
            Command::Pevcap(PevcapArgs {
                command: PevcapCommand::Convert(PevcapConvertArgs {
                    input: PathBuf::from("session.pevcap.jsonl"),
                    input_format: PevcapFormat::Jsonl,
                    output: PathBuf::from("session.pevcap"),
                    output_format: PevcapFormat::Binary,
                })
            })
        );
    }

    #[test]
    fn parses_pevcap_replay_command() {
        let cli = Cli::try_parse_from([
            "cutout",
            "pevcap",
            "replay",
            "--input",
            "session.pevcap",
            "--input-format",
            "binary",
            "--profile",
            "falcon",
        ])
        .expect("parser accepts PEVCAP replay");

        assert_eq!(
            cli.command,
            Command::Pevcap(PevcapArgs {
                command: PevcapCommand::Replay(PevcapReplayArgs {
                    input: PathBuf::from("session.pevcap"),
                    input_format: PevcapFormat::Binary,
                    profile: SessionProfile::Falcon,
                    diagnostics_jsonl: false,
                    read_only_jsonl: false,
                })
            })
        );
    }

    #[test]
    fn parses_pevcap_replay_command_with_auto_profile() {
        let cli = Cli::try_parse_from([
            "cutout",
            "pevcap",
            "replay",
            "--input",
            "session.pevcap",
            "--input-format",
            "binary",
        ])
        .expect("parser accepts PEVCAP replay without explicit profile");

        assert_eq!(
            cli.command,
            Command::Pevcap(PevcapArgs {
                command: PevcapCommand::Replay(PevcapReplayArgs {
                    input: PathBuf::from("session.pevcap"),
                    input_format: PevcapFormat::Binary,
                    profile: SessionProfile::Auto,
                    diagnostics_jsonl: false,
                    read_only_jsonl: false,
                })
            })
        );
    }

    #[test]
    fn parses_pevcap_replay_command_with_diagnostics_jsonl() {
        let cli = Cli::try_parse_from([
            "cutout",
            "pevcap",
            "replay",
            "--input",
            "session.pevcap",
            "--input-format",
            "jsonl",
            "--diagnostics-jsonl",
        ])
        .expect("parser accepts diagnostic JSONL replay flag");

        assert_eq!(
            cli.command,
            Command::Pevcap(PevcapArgs {
                command: PevcapCommand::Replay(PevcapReplayArgs {
                    input: PathBuf::from("session.pevcap"),
                    input_format: PevcapFormat::Jsonl,
                    profile: SessionProfile::Auto,
                    diagnostics_jsonl: true,
                    read_only_jsonl: false,
                })
            })
        );
    }

    #[test]
    fn parses_pevcap_replay_command_with_read_only_jsonl() {
        let cli = Cli::try_parse_from([
            "cutout",
            "pevcap",
            "replay",
            "--input",
            "session.pevcap",
            "--input-format",
            "jsonl",
            "--read-only-jsonl",
        ])
        .expect("parser accepts read-only JSONL replay flag");

        assert_eq!(
            cli.command,
            Command::Pevcap(PevcapArgs {
                command: PevcapCommand::Replay(PevcapReplayArgs {
                    input: PathBuf::from("session.pevcap"),
                    input_format: PevcapFormat::Jsonl,
                    profile: SessionProfile::Auto,
                    diagnostics_jsonl: false,
                    read_only_jsonl: true,
                })
            })
        );
    }

    #[test]
    fn converts_target_args_to_connection_target() {
        let target: cutout_btle::ConnectionTarget = TargetArgs {
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            identifier: None,
            name_contains: Some("Aero".to_owned()),
        }
        .into();

        assert_eq!(target.address.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
        assert_eq!(target.identifier, None);
        assert_eq!(target.name_contains.as_deref(), Some("Aero"));
    }

    #[test]
    fn converts_identifier_target_args_to_connection_target() {
        let target: cutout_btle::ConnectionTarget = TargetArgs {
            address: None,
            identifier: Some("cb-uuid-1234".to_owned()),
            name_contains: None,
        }
        .into();

        assert_eq!(target.identifier.as_deref(), Some("cb-uuid-1234"));
    }

    #[test]
    fn parses_connect_command_with_identifier_target() {
        let cli = Cli::try_parse_from(["cutout", "connect", "--id", "cb-uuid-1234"])
            .expect("parser accepts connect by platform id");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: None,
                    identifier: Some("cb-uuid-1234".to_owned()),
                    name_contains: None,
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
                profile: SessionProfile::Auto,
                probes: Vec::new(),
                diagnostics_jsonl: false,
                read_only_jsonl: false,
            })
        );
    }

    #[test]
    fn converts_empty_target_args_to_default_connection_target() {
        let target: cutout_btle::ConnectionTarget = TargetArgs {
            address: None,
            identifier: None,
            name_contains: None,
        }
        .into();

        assert_eq!(target, cutout_btle::ConnectionTarget::default());
    }

    #[test]
    fn targeted_scan_args_preserve_seconds_before_conversion() {
        let args = TargetedScanArgs {
            target: TargetArgs {
                address: None,
                identifier: None,
                name_contains: Some("NF2557".to_owned()),
            },
            scan: ScanArgs { seconds: 30 },
            profile: SessionProfile::Auto,
            probes: Vec::new(),
            diagnostics_jsonl: false,
            read_only_jsonl: false,
        };

        assert_eq!(args.seconds(), 30);
        assert_eq!(args.profile(), SessionProfile::Auto);
        assert_eq!(args.probes(), &[]);
        assert_eq!(args.into_target().name_contains.as_deref(), Some("NF2557"));
    }

    #[test]
    fn rejects_missing_subcommand() {
        let error = Cli::try_parse_from(["cutout"]).expect_err("subcommand is required");

        assert_eq!(
            error.kind(),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn rejects_unknown_subcommand() {
        let error =
            Cli::try_parse_from(["cutout", "pair"]).expect_err("unknown subcommand is rejected");

        assert_eq!(error.kind(), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn rejects_negative_duration() {
        let error = Cli::try_parse_from(["cutout", "scan", "--seconds", "-1"])
            .expect_err("duration must be unsigned");

        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_non_numeric_duration() {
        let error = Cli::try_parse_from(["cutout", "scan", "--seconds", "soon"])
            .expect_err("duration must be numeric");

        assert_eq!(error.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn rejects_missing_duration_value() {
        let error = Cli::try_parse_from(["cutout", "scan", "--seconds"])
            .expect_err("duration needs a value");

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn rejects_unknown_scan_option() {
        let error = Cli::try_parse_from(["cutout", "scan", "--address", "AA:BB:CC:DD:EE:FF"])
            .expect_err("scan has no target filters");

        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_unknown_connect_option() {
        let error = Cli::try_parse_from(["cutout", "connect", "--device", "Aero"])
            .expect_err("connect rejects unknown target flag");

        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_unknown_capture_option() {
        let error = Cli::try_parse_from(["cutout", "capture-aero", "--write"])
            .expect_err("capture-aero rejects unsupported write flag");

        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn clap_metadata_builds() {
        Cli::command().debug_assert();
    }

    #[test]
    fn top_level_short_help_lists_command_surface() {
        let help = Cli::command().render_help().to_string();

        assert_contains_all(
            &help,
            &[
                "Inspect and capture read-only BTLE data",
                "Usage: cutout <COMMAND>",
                "scan",
                "connect",
                "capture-aero",
                "pevcap",
                "validation",
                "dashboard",
            ],
        );
    }

    #[test]
    fn top_level_long_help_describes_safety_scope_and_examples() {
        let help = Cli::command().render_long_help().to_string();

        assert_contains_all(
            &help,
            &[
                "read-only",
                "Commands may connect to hardware",
                "Examples:",
                "cutout capture-aero --name-contains NF2557 --seconds 20",
                "--name-contains matches a case-sensitive substring",
            ],
        );
    }

    #[test]
    fn top_level_long_help_documents_untargeted_selection() {
        let help = Cli::command().render_long_help().to_string();

        assert_contains_all(
            &help,
            &[
                "Target selection:",
                "When neither target filter is supplied",
                "first matching peripheral",
            ],
        );
    }

    #[test]
    fn top_level_long_help_examples_cover_each_subcommand() {
        let help = Cli::command().render_long_help().to_string();

        assert_contains_all(
            &help,
            &[
                "cutout scan --seconds 10",
                "cutout connect --name-contains Aero",
                "cutout connect --address AA:BB:CC:DD:EE:FF --seconds 8",
                "cutout subscribe-raw --name-contains NF2557",
                "cutout capture-aero --name-contains NF2557 --seconds 20",
                "cutout pevcap convert --input session.pevcap.jsonl",
                "cutout validation",
                "cutout dashboard",
            ],
        );
    }

    #[test]
    fn command_names_are_stable() {
        let command = Cli::command();
        let names = command
            .get_subcommands()
            .map(clap::Command::get_name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            [
                "scan",
                "connect",
                "subscribe-raw",
                "capture-aero",
                "validation",
                "pevcap",
                "dashboard"
            ]
        );
    }

    #[test]
    fn scan_short_help_is_concise_but_actionable() {
        let help = short_help_for("scan");

        assert_contains_all(
            &help,
            &[
                "List nearby BLE peripherals",
                "Usage: scan [OPTIONS]",
                "--seconds <SECONDS>",
            ],
        );
    }

    #[test]
    fn scan_long_help_describes_observation_output() {
        let help = long_help_for("scan");

        assert_contains_all(
            &help,
            &[
                "one observation per",
                "platform identifier or address",
                "advertised service UUIDs",
                "manufacturer-data summaries",
                "candidate family hints",
                "hints only",
                "RSSI",
                "--seconds <SECONDS>",
            ],
        );
    }

    #[test]
    fn scan_help_shows_default_duration() {
        let help = long_help_for("scan");

        assert_contains_all(
            &help,
            &["default: 5", "Seconds to listen for advertisements"],
        );
    }

    #[test]
    fn connect_short_help_lists_target_filters() {
        let help = short_help_for("connect");

        assert_contains_all(
            &help,
            &[
                "Inspect GATT services",
                "Usage: connect [OPTIONS]",
                "--address <ADDR>",
                "--name-contains <TEXT>",
                "--seconds <SECONDS>",
            ],
        );
    }

    #[test]
    fn connect_long_help_describes_discovery_and_session_probe() {
        let help = long_help_for("connect");

        assert_contains_all(
            &help,
            &[
                "discover its GATT services",
                "service/characteristic tree",
                "--profile falcon",
                "--probe identity",
                "--probe firmware",
                "auto currently keeps the existing Aero/Veteran path",
                "prints bridge",
                "counters",
                "--address <ADDR>",
                "--name-contains <TEXT>",
            ],
        );
    }

    #[test]
    fn connect_help_explains_address_and_name_filters() {
        let help = long_help_for("connect");

        assert_contains_all(
            &help,
            &[
                "Bluetooth address to select from scan results",
                "Case-sensitive substring",
                "advertised name",
            ],
        );
    }

    #[test]
    fn capture_aero_short_help_lists_capture_options() {
        let help = short_help_for("capture-aero");

        assert_contains_all(
            &help,
            &[
                "Capture read-only Aero/Veteran protocol evidence",
                "Usage: capture-aero [OPTIONS]",
                "--address <ADDR>",
                "--name-contains <TEXT>",
                "--seconds <SECONDS>",
            ],
        );
    }

    #[test]
    fn capture_aero_long_help_documents_explicit_falcon_verification_path() {
        let help = long_help_for("capture-aero");

        assert_contains_all(
            &help,
            &[
                "explicit",
                "Falcon verification",
                "--profile falcon",
                "--probe identity",
                "--probe",
                "firmware",
                "auto currently keeps the existing Aero/Veteran path",
            ],
        );
    }

    #[test]
    fn capture_aero_long_help_warns_about_raw_capture_contents() {
        let help = long_help_for("capture-aero");

        assert_contains_all(
            &help,
            &[
                "fixture work",
                "link metadata",
                "provisional write bytes",
                "raw notification payloads",
                "Review it before sharing logs publicly",
            ],
        );
    }

    #[test]
    fn capture_aero_help_emphasizes_read_only_scope() {
        let help = long_help_for("capture-aero");

        assert_contains_all(
            &help,
            &[
                "Aero/Veteran-family",
                "Capture output",
                "device identifiers",
            ],
        );
    }

    #[test]
    fn dashboard_short_help_names_the_terminal_surface() {
        let help = short_help_for("dashboard");

        assert_contains_all(
            &help,
            &[
                "Open the interactive read-only dashboard",
                "Usage: dashboard",
            ],
        );
    }

    #[test]
    fn validation_short_help_describes_the_matrix() {
        let help = short_help_for("validation");

        assert_contains_all(
            &help,
            &[
                "Print the generated hardware validation matrix",
                "Usage: validation",
            ],
        );
    }

    #[test]
    fn pevcap_long_help_describes_offline_capture_tooling() {
        let help = long_help_for("pevcap");

        assert_contains_all(
            &help,
            &[
                "capture files",
                "without connecting to Bluetooth hardware",
                "JSONL",
                "binary",
            ],
        );
    }

    #[test]
    fn dashboard_long_help_describes_termina_scope() {
        let help = long_help_for("dashboard");

        assert_contains_all(
            &help,
            &[
                "Ratatui dashboard",
                "Termina terminal backend",
                "dashboard opens only",
                "visualization and monitoring",
                "does not modify the device",
            ],
        );
    }

    #[test]
    fn validation_long_help_describes_generated_matrix() {
        let help = long_help_for("validation");

        assert_contains_all(
            &help,
            &[
                "generated hardware validation matrix",
                "capture IDs",
                "tested fields",
                "unverified fields",
            ],
        );
    }
}
