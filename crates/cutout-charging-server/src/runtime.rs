use crate::{Error, Store, model::id};
use clap::{Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "cutout-charging-server", about = "Charging-spot directory")]
struct Arguments {
    #[arg(long, env = "CHARGING_DATABASE")]
    database: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the HTTP service. Put the public HTTPS proxy in front of this listener.
    Serve {
        #[arg(long, env = "CHARGING_LISTEN", default_value = "127.0.0.1:8788")]
        listen: SocketAddr,
    },
    /// Provision an operator or contributor credential; stdout contains the secret.
    Identity {
        #[arg(long)]
        moderator: bool,
    },
    /// Revoke a credential and clear its votes.
    Revoke {
        #[arg(long)]
        identity: String,
    },
    /// Make a consistent backup without replacing an existing output file.
    Backup {
        #[arg(long)]
        output: PathBuf,
    },
    /// Check that the database is readable and passes SQLite's quick check.
    Check,
}

/// Runs the service/operator command selected by CLI arguments.
///
/// # Errors
/// Returns a classified error for database, command, output or listener failures.
pub async fn run_from_arguments() -> Result<(), Error> {
    let arguments = Arguments::parse();
    let store = Store::open(&arguments.database)?;
    match arguments.command {
        Command::Serve { listen } => {
            let listener = tokio::net::TcpListener::bind(listen)
                .await
                .map_err(|_| Error::Unavailable)?;
            tracing::info!(address=%listener.local_addr().map_err(|_|Error::Unavailable)?,"charging directory listening");
            axum::serve(listener, crate::router(store))
                .with_graceful_shutdown(shutdown())
                .await
                .map_err(|_| Error::Unavailable)
        }
        Command::Identity { moderator } => {
            let identity = store.identity(moderator, false).await?;
            serde_json::to_writer(std::io::stdout().lock(), &identity)
                .map_err(|_| Error::Unavailable)
        }
        Command::Revoke { identity } => store.revoke(id(&identity)?).await,
        Command::Backup { output } => store.backup(output).await,
        Command::Check => store.check().await,
    }
}

async fn shutdown() {
    let interrupt = tokio::signal::ctrl_c();
    #[cfg(unix)]
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut terminate) => {
            tokio::select! {_=interrupt=>{},_=terminate.recv()=>{}}
        }
        Err(_) => {
            let _ = interrupt.await;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = interrupt.await;
    }
}
