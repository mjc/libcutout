#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    if let Err(error) = cutout_server::run_from_arguments().await {
        tracing::error!(%error, "charging directory command failed");
        std::process::exit(1);
    }
}
