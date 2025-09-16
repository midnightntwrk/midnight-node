mod relayer;
pub mod cardano_encoding;

use tokio::time::{sleep, Duration};
use clap::Parser;

/// BEEFY Relayer CLI
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Node WebSocket endpoint (e.g. ws://localhost:9944)
    #[arg(short, long, default_value = "ws://localhost:9944")]
    node_url: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let beefy_relayer = relayer::Relayer::new(&cli.node_url.clone()).await;
    loop {
        println!("Starting relay...");
        beefy_relayer.run_relay().await;
    }
    Ok(())
}