use shadowfs_core::platform::cli::run_cli;

#[tokio::main]
async fn main() {
    run_cli().await;
}