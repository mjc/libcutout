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
#[command(
    name = "cutout-server",
    about = "Cutout music, charging and website server"
)]
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
        /// Serve music, charging and website routes on separate canonical hosts.
        #[arg(long, requires = "trusted_proxy")]
        music: bool,
        #[arg(long, default_value = "music.cutout.lol", value_parser = parse_host)]
        music_host: crate::vhosts::PublicHost,
        #[arg(long, default_value = "rides.cutout.lol", value_parser = parse_host)]
        rides_host: crate::vhosts::PublicHost,
        #[arg(long, default_value = "cutout.lol", value_parser = parse_host)]
        site_host: crate::vhosts::PublicHost,
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
    /// Inspect and operate the private alpha signup outbox from the server host.
    AlphaSignup {
        #[command(subcommand)]
        command: AlphaSignupCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AlphaSignupCommand {
    /// List sync and removal status; matched removal requests include the applicant email.
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
    validate_listener(&arguments.command)?;
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
            music,
            music_host,
            rides_host,
            site_host,
        } => {
            let mode = match (music, trusted_proxy) {
                (true, Some(proxy)) => ServiceMode::Combined(
                    crate::vhosts::PublicVhosts::new(
                        music_host,
                        rides_host,
                        site_host,
                        crate::TrustedProxy(proxy),
                    )
                    .map_err(|_| Error::Invalid)?,
                ),
                (true, None) => return Err(Error::Invalid),
                (false, proxy) => ServiceMode::Directory(proxy),
            };
            serve(listen, mode, store).await
        }
        Command::Identity { moderator } => {
            let role = if moderator {
                Role::Moderator
            } else {
                Role::Contributor
            };
            let identity = store.identity(Registration::Operator { role }).await?;
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

enum ServiceMode {
    Directory(Option<IpAddr>),
    Combined(crate::vhosts::PublicVhosts),
}

fn parse_host(value: &str) -> Result<crate::vhosts::PublicHost, crate::vhosts::HostConfigError> {
    crate::vhosts::PublicHost::parse(value)
}

fn validate_listener(command: &Command) -> Result<(), Error> {
    if let Command::Serve {
        music: true,
        listen,
        ..
    } = command
    {
        if !listen.ip().is_loopback() {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}

async fn serve(listen: SocketAddr, mode: ServiceMode, store: Store) -> Result<(), Error> {
    let lific_worker = crate::signup_sync::LificSync::from_env();
    let signup_state = match (
        crate::signup::SignupState::enabled_from_env(),
        lific_worker.is_some(),
    ) {
        (Some(state), true) => state,
        _ => crate::signup::SignupState::from_env(),
    };
    let router = build_router(store.clone(), mode, signup_state).await?;
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|_| Error::Unavailable)?;
    let address = listener.local_addr().map_err(|_| Error::Unavailable)?;
    tracing::info!(%address, "cutout server listening");
    let worker =
        lific_worker.map(|lific| tokio::spawn(crate::signup_sync::run_worker(store, lific)));
    let result = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await
    .map_err(|_| Error::Unavailable);
    if let Some(worker) = worker {
        worker.abort();
        let _ = worker.await;
    }
    result
}

async fn build_router(
    store: Store,
    mode: ServiceMode,
    signup: crate::signup::SignupState,
) -> Result<axum::Router, Error> {
    match mode {
        ServiceMode::Directory(proxy) => Ok(build_router_with_signup(store, proxy, signup)),
        ServiceMode::Combined(policy) => {
            let music = cutout_soundcloud_auth_broker::MusicBroker::from_environment()
                .await
                .map_err(|error| {
                    tracing::error!(%error, "music broker initialization failed");
                    Error::Unavailable
                })?;
            Ok(crate::vhosts::router(
                policy,
                music.router(),
                crate::http::charging_router(store.clone()),
                crate::http::site_router(store, signup),
            ))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{Arguments, validate_listener};
    use clap::Parser;

    #[test]
    fn combined_service_requires_an_explicit_trusted_proxy() {
        let result = Arguments::try_parse_from([
            "cutout-server",
            "--database",
            "unused.sqlite",
            "serve",
            "--music",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn combined_service_rejects_a_public_listener() {
        let arguments = Arguments::try_parse_from([
            "cutout-server",
            "--database",
            "unused.sqlite",
            "serve",
            "--music",
            "--trusted-proxy",
            "127.0.0.1",
            "--listen",
            "0.0.0.0:8788",
        ])
        .expect("valid command syntax");
        assert!(validate_listener(&arguments.command).is_err());
    }

    #[test]
    fn combined_service_rejects_noncanonical_host_configuration() {
        let result = Arguments::try_parse_from([
            "cutout-server",
            "--database",
            "unused.sqlite",
            "serve",
            "--music",
            "--trusted-proxy",
            "127.0.0.1",
            "--music-host",
            "Music.cutout.lol",
        ]);
        assert!(result.is_err());
    }
}
