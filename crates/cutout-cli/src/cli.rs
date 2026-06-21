use clap::{Args, Parser, Subcommand};
use cutout_btle::ConnectionTarget;

const DEFAULT_SCAN_SECONDS: u64 = 5;

/// Parsed command-line arguments for the `cutout` binary.
#[derive(Debug, Parser)]
#[command(name = "cutout", about = "Cutout Aero connection CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub(crate) enum Command {
    /// Scan for nearby BLE peripherals.
    Scan(ScanArgs),

    /// Connect to a selected peripheral and print discovered services.
    Connect(TargetedScanArgs),

    /// Connect to an Aero and print capture records for fixture work.
    CaptureAero(TargetedScanArgs),
}

#[derive(Clone, Copy, Debug, Args, PartialEq, Eq)]
pub(crate) struct ScanArgs {
    /// Scan duration in seconds.
    #[arg(long, default_value_t = DEFAULT_SCAN_SECONDS)]
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
}

impl TargetedScanArgs {
    pub(crate) fn into_target(self) -> ConnectionTarget {
        self.target.into()
    }

    pub(crate) const fn seconds(&self) -> u64 {
        self.scan.seconds()
    }
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub(crate) struct TargetArgs {
    /// Match the peripheral by address.
    #[arg(long)]
    pub(crate) address: Option<String>,

    /// Match the peripheral name by substring.
    #[arg(long = "name-contains")]
    pub(crate) name_contains: Option<String>,
}

impl From<TargetArgs> for ConnectionTarget {
    fn from(target: TargetArgs) -> Self {
        Self {
            address: target.address,
            name_contains: target.name_contains,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser, error::ErrorKind};

    use super::{Cli, Command, DEFAULT_SCAN_SECONDS, ScanArgs, TargetArgs, TargetedScanArgs};

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
    fn parses_connect_command_with_default_duration() {
        let cli = Cli::try_parse_from(["cutout", "connect", "--address", "AA:BB:CC:DD:EE:FF"])
            .expect("parser accepts connect with default duration");

        assert_eq!(
            cli.command,
            Command::Connect(TargetedScanArgs {
                target: TargetArgs {
                    address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                    name_contains: None,
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
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
                    name_contains: Some("Aero".to_owned()),
                },
                scan: ScanArgs { seconds: 8 },
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
                    name_contains: Some("Aero".to_owned()),
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
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
                    name_contains: None,
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
            })
        );
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
            Command::CaptureAero(TargetedScanArgs {
                target: TargetArgs {
                    address: None,
                    name_contains: Some("NF2557".to_owned()),
                },
                scan: ScanArgs { seconds: 3 },
            })
        );
    }

    #[test]
    fn parses_capture_aero_command_with_address_target() {
        let cli = Cli::try_parse_from(["cutout", "capture-aero", "--address", "AA:BB:CC:DD:EE:FF"])
            .expect("parser accepts capture-aero by address");

        assert_eq!(
            cli.command,
            Command::CaptureAero(TargetedScanArgs {
                target: TargetArgs {
                    address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                    name_contains: None,
                },
                scan: ScanArgs {
                    seconds: DEFAULT_SCAN_SECONDS
                },
            })
        );
    }

    #[test]
    fn converts_target_args_to_connection_target() {
        let target: cutout_btle::ConnectionTarget = TargetArgs {
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name_contains: Some("Aero".to_owned()),
        }
        .into();

        assert_eq!(target.address.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
        assert_eq!(target.name_contains.as_deref(), Some("Aero"));
    }

    #[test]
    fn targeted_scan_args_preserve_seconds_before_conversion() {
        let args = TargetedScanArgs {
            target: TargetArgs {
                address: None,
                name_contains: Some("NF2557".to_owned()),
            },
            scan: ScanArgs { seconds: 30 },
        };

        assert_eq!(args.seconds(), 30);
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
    fn clap_metadata_builds() {
        Cli::command().debug_assert();
    }
}
