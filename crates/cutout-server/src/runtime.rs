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
    about = "Cutout music, charging, ride, and website server"
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
        /// Accepted for the existing unit. The `Host` header selects the handler.
        #[arg(long, hide = true)]
        music: bool,
        #[arg(long, default_value = "music.cutout.lol", value_parser = parse_host)]
        music_host: crate::vhosts::PublicHost,
        #[arg(long, default_value = "rides.cutout.lol", value_parser = parse_host)]
        rides_host: crate::vhosts::PublicHost,
        #[arg(long, default_value = "cutout.lol", value_parser = parse_host)]
        site_host: crate::vhosts::PublicHost,
        /// Ride database for this process. Defaults to `ride.sqlite` beside the charging database.
        #[arg(long, env = "CUTOUT_RIDE_DATABASE")]
        ride_database: Option<PathBuf>,
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
    let charging_database = arguments.database.clone();
    let store = open_store(&arguments)?;
    dispatch_command(arguments.command, store, charging_database).await
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

async fn dispatch_command(
    command: Command,
    store: Store,
    charging_database: PathBuf,
) -> Result<(), Error> {
    match command {
        Command::Serve {
            listen,
            trusted_proxy,
            music: _,
            music_host,
            rides_host,
            site_host,
            ride_database,
        } => {
            let Some(proxy) = trusted_proxy else {
                return Err(Error::Invalid);
            };
            let vhosts = crate::vhosts::PublicVhosts::new(
                music_host,
                rides_host,
                site_host,
                crate::TrustedProxy(proxy),
            )
            .map_err(|_| Error::Invalid)?;
            let ride_database =
                ride_database.unwrap_or_else(|| charging_database.with_file_name("ride.sqlite"));
            serve(listen, vhosts, store, ride_database).await
        }
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

fn parse_host(value: &str) -> Result<crate::vhosts::PublicHost, crate::vhosts::HostConfigError> {
    crate::vhosts::PublicHost::parse(value)
}

fn validate_listener(command: &Command) -> Result<(), Error> {
    if let Command::Serve {
        listen,
        trusted_proxy,
        ..
    } = command
        && (trusted_proxy.is_none() || !listen.ip().is_loopback())
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

async fn open_ride_service(path: PathBuf) -> Result<libcutout_persistence::RideDatabase, Error> {
    tokio::task::spawn_blocking(
        move || -> Result<libcutout_persistence::RideDatabase, libcutout_persistence::StorageError> {
            let database = libcutout_persistence::RideDatabase::open(&path)?;
            database.capabilities()?;
            Ok(database)
        },
    )
    .await
    .map_err(|_| Error::Unavailable)?
    .map_err(|error| {
        tracing::error!(%error, "ride service failed to open");
        Error::Unavailable
    })
}

async fn shutdown_ride_service(database: libcutout_persistence::RideDatabase) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || database.shutdown())
        .await
        .map_err(|_| Error::Unavailable)?
        .map_err(|error| {
            tracing::error!(%error, "ride service shutdown failed");
            Error::Unavailable
        })
}

async fn serve(
    listen: SocketAddr,
    vhosts: crate::vhosts::PublicVhosts,
    store: Store,
    ride_database: PathBuf,
) -> Result<(), Error> {
    let ride_service = open_ride_service(ride_database).await?;
    let result = serve_http(listen, vhosts, store, ride_service.clone()).await;
    shutdown_ride_service(ride_service).await?;
    result
}

async fn serve_http(
    listen: SocketAddr,
    vhosts: crate::vhosts::PublicVhosts,
    store: Store,
    ride: libcutout_persistence::RideDatabase,
) -> Result<(), Error> {
    let lific_worker = crate::signup_sync::LificSync::from_env();
    let signup = signup_state(lific_worker.is_some());
    let router = build_router(store.clone(), vhosts, signup, ride).await?;
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|_| Error::Unavailable)?;
    let address = listener.local_addr().map_err(|_| Error::Unavailable)?;
    store.cleanup_capture_staging().await?;
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

fn signup_state(worker_configured: bool) -> crate::signup::SignupState {
    match (
        crate::signup::SignupState::enabled_from_env(),
        worker_configured,
    ) {
        (Some(state), true) => state,
        _ => crate::signup::SignupState::from_env(),
    }
}

async fn build_router(
    store: Store,
    vhosts: crate::vhosts::PublicVhosts,
    signup: crate::signup::SignupState,
    ride: libcutout_persistence::RideDatabase,
) -> Result<axum::Router, Error> {
    let music = cutout_soundcloud_auth_broker::MusicBroker::from_environment()
        .await
        .map_err(|error| {
            tracing::error!(%error, "music broker initialization failed");
            Error::Unavailable
        })?;
    Ok(crate::vhosts::router(
        vhosts,
        music.router(),
        crate::http::charging_router(store.clone(), ride),
        crate::http::site_router(store, signup),
    ))
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
    fn server_requires_an_explicit_trusted_proxy() {
        let arguments =
            Arguments::try_parse_from(["cutout-server", "--database", "unused.sqlite", "serve"])
                .expect("valid command syntax");
        assert!(validate_listener(&arguments.command).is_err());
    }

    #[test]
    fn server_rejects_a_public_listener() {
        let arguments = Arguments::try_parse_from([
            "cutout-server",
            "--database",
            "unused.sqlite",
            "serve",
            "--trusted-proxy",
            "127.0.0.1",
            "--listen",
            "0.0.0.0:8788",
        ])
        .expect("valid command syntax");
        assert!(validate_listener(&arguments.command).is_err());
    }

    #[test]
    fn server_rejects_noncanonical_host_configuration() {
        let result = Arguments::try_parse_from([
            "cutout-server",
            "--database",
            "unused.sqlite",
            "serve",
            "--trusted-proxy",
            "127.0.0.1",
            "--music-host",
            "Music.cutout.lol",
        ]);
        assert!(result.is_err());
    }
}
