mod beefy_keys;
mod error;

use clap::Parser;
pub use error::Error;

/// BEEFY Relayer CLI
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
	/// Node WebSocket endpoint (e.g. ws://localhost:9944)
	#[arg(short, long, default_value = "ws://localhost:9933")]
	node_url: String,

	/// File path of the beefy keys
	#[arg(short, long)]
	keys_path: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let cli = Cli::parse();

	// reading beefy keys from the given file path, and inserting to the chain
	if let Some(keys_path) = &cli.keys_path
		&& let Err(e) = beefy_keys::read_and_insert_to_chain(keys_path).await
	{
		println!("{e}");
	};

	Ok(())
}
