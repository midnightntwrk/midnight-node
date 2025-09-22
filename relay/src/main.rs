mod beefy;
mod cardano_encoding;
mod relayer;

use tokio::time::{sleep, Duration};
use clap::Parser;

pub use midnight_node_metadata::midnight_metadata as mn_meta;
/// BEEFY Relayer CLI
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
	/// Node WebSocket endpoint (e.g. ws://localhost:9944)
	#[arg(short, long, default_value = "ws://localhost:9944")]
	node_url: String,

	/// File path of the beefy keys
	#[arg(short, long)]
	keys_path: Option<String>,
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

	let beefy_relayer = match cli.keys_path {
		None => relayer::Relayer::new(&cli.node_url.clone()).await,
		Some(keys_path) => relayer::Relayer::new_with_keys_file(&cli.node_url, keys_path).await,
	};

	loop {
		println!("Starting relay...");
		
		beefy_relayer.run_relay().await;
	}

	println!("the end");
	Ok(())
}