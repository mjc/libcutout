use std::time::Duration;

use clap::{Parser, Subcommand};
use cutout_btle::{ConnectionTarget, connect_and_discover, scan_peripherals};

#[derive(Debug, Parser)]
#[command(name = "cutout", about = "Cutout Aero connection CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
enum Command {
    /// Scan for nearby BLE peripherals.
    Scan {
        /// Scan duration in seconds.
        #[arg(long, default_value_t = 5)]
        seconds: u64,
    },

    /// Connect to a selected peripheral and print discovered services.
    Connect {
        /// Match the peripheral by address.
        #[arg(long)]
        address: Option<String>,

        /// Match the peripheral name by substring.
        #[arg(long = "name-contains")]
        name_contains: Option<String>,

        /// Scan duration in seconds before attempting connect.
        #[arg(long, default_value_t = 5)]
        seconds: u64,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), cutout_btle::BtleError> {
    match cli.command {
        Command::Scan { seconds } => {
            for observation in scan_peripherals(Duration::from_secs(seconds)).await? {
                println!("{observation}");
            }
            Ok(())
        }
        Command::Connect {
            address,
            name_contains,
            seconds,
        } => {
            let target = ConnectionTarget {
                address,
                name_contains,
            };
            let summary = connect_and_discover(&target, Duration::from_secs(seconds)).await?;
            println!("{summary}");
            if let Some(endpoints) = summary.select_session_endpoints() {
                println!(
                    "session write={} notify={}",
                    endpoints.write.uuid,
                    endpoints
                        .notify
                        .map_or_else(|| "<none>".to_owned(), |notify| notify.uuid.to_string(),)
                );
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn parses_scan_command_with_default_duration() {
        let cli = Cli::try_parse_from(["cutout", "scan"]).expect("parser accepts scan");

        assert_eq!(cli.command, Command::Scan { seconds: 5 });
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
            Command::Connect {
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name_contains: Some("Aero".to_owned()),
                seconds: 8,
            }
        );
    }
}
