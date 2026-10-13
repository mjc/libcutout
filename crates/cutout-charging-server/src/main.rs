#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    if cutout_charging_server::run_from_arguments().await.is_err() {
        tracing::error!("charging directory command failed");
        std::process::exit(1);
    }
}
