use crate::{
    Error, Store,
    model::id,
    store::{Registration, Role},
};
use clap::{Parser, Subcommand};
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

#[derive(Debug, Parser)]
#[command(name = "cutout-server", about = "Charging-spot directory")]
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
        /// Trust X-Real-IP only from this peer; the proxy must overwrite that header.
        #[arg(long, env = "CHARGING_TRUSTED_PROXY")]
        trusted_proxy: Option<IpAddr>,
    },
    /// Provision an operator or contributor credential; stdout contains the secret.
    Identity {
        #[arg(long)]
        moderator: bool,
        /// Allow private capture review; charging moderation alone does not grant this.
        #[arg(long)]
        private_capture_reviewer: bool,
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
    /// Inspect and operate the private alpha signup outbox from the server host.
    AlphaSignup {
        #[command(subcommand)]
        command: AlphaSignupCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AlphaSignupCommand {
    /// List the bounded sync queue without printing applicant fields.
    Status,
    /// Retry a failed sync for one request.
    Retry { request_id: String },
    /// Remove one request from Lific and the local database.
    Withdraw { request_id: String },
}

/// Runs the service/operator command selected by CLI arguments.
///
/// # Errors
/// Returns a classified error for database, command, output or listener failures.
pub async fn run_from_arguments() -> Result<(), Error> {
    let arguments = Arguments::parse();
    let store = open_store(&arguments)?;
    dispatch_command(arguments.command, store).await
}

fn open_store(arguments: &Arguments) -> Result<Store, Error> {
    match &arguments.command {
        Command::Serve { .. } | Command::Identity { .. } => Store::open(&arguments.database),
        Command::Revoke { .. }
        | Command::Backup { .. }
        | Command::Check
        | Command::AlphaSignup { .. } => Store::open_existing(&arguments.database),
    }
}

async fn dispatch_command(command: Command, store: Store) -> Result<(), Error> {
    match command {
        Command::Serve {
            listen,
            trusted_proxy,
        } => serve(listen, trusted_proxy, store).await,
        Command::Identity {
            moderator,
            private_capture_reviewer,
        } => {
            let role = if moderator {
                Role::Moderator
            } else {
                Role::Contributor
            };
            let identity = if private_capture_reviewer {
                store.operator_capture_reviewer(role).await?
            } else {
                store.identity(Registration::Operator { role }).await?
            };
            serde_json::to_writer(std::io::stdout().lock(), &identity)
                .map_err(|_| Error::Unavailable)
        }
        Command::Revoke { identity } => store.revoke(id(&identity)?).await,
        Command::Backup { output } => store.backup(output).await,
        Command::Check => store.check().await,
        Command::AlphaSignup { command } => match command {
            AlphaSignupCommand::Status => {
                let status = serde_json::json!({
                    "sync": store.signup_sync_status().await?,
                    "removal_requests": store.signup_removal_status().await?,
                });
                serde_json::to_writer(std::io::stdout().lock(), &status)
                    .map_err(|_| Error::Unavailable)
            }
            AlphaSignupCommand::Retry { request_id } => {
                store.retry_signup_sync_local(id(&request_id)?).await
            }
            AlphaSignupCommand::Withdraw { request_id } => {
                store
                    .request_signup_withdrawal_local(id(&request_id)?)
                    .await
            }
        },
    }
}

async fn serve(
    listen: SocketAddr,
    trusted_proxy: Option<IpAddr>,
    store: Store,
) -> Result<(), Error> {
    let lific_worker = crate::signup_sync::LificSync::from_env();
    let signup_state = match (
        crate::signup::SignupState::enabled_from_env(),
        lific_worker.is_some(),
    ) {
        (Some(state), true) => state,
        _ => crate::signup::SignupState::from_env(),
    };
    if let Some(lific) = lific_worker {
        tokio::spawn(crate::signup_sync::run_worker(store.clone(), lific));
    }
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|_| Error::Unavailable)?;
    let address = listener.local_addr().map_err(|_| Error::Unavailable)?;
    store.cleanup_capture_staging().await?;
    tracing::info!(%address, "charging directory listening");
    let router = build_router_with_signup(store, trusted_proxy, signup_state);
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await
    .map_err(|_| Error::Unavailable)
}

fn build_router_with_signup(
    store: Store,
    trusted_proxy: Option<IpAddr>,
    signup: crate::signup::SignupState,
) -> axum::Router {
    match trusted_proxy {
        Some(proxy) => crate::http::router_with_signup_state(store, signup)
            .layer(axum::Extension(crate::TrustedProxy(proxy))),
        None => crate::http::router_with_signup_state(store, signup),
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
