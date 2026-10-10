#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();
    if cutout_soundcloud_auth_broker::run_from_environment()
        .await
        .is_err()
    {
        tracing::error!("SoundCloud token broker failed to start or stopped unexpectedly");
        std::process::exit(1);
    }
}
