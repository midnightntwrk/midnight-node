// set for the recursive plutus data call of ProofNodes, see authorities.rs
#![recursion_limit = "128"]
#![allow(clippy::result_large_err)]

mod authorities;
mod beefy;
mod cardano_encoding;
mod error;
mod helpers;
mod mmr;
mod relayer;
mod types;

use clap::Parser;
use error::Error;
use std::{fs::File, io::BufReader};
use subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params};

pub use midnight_node_metadata::midnight_metadata_latest as mn_meta;

/// Used for inserting keys to the keystore
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BeefyKeyInfo {
	/// Secret seed, for inserting beefy key
	suri: String,

	/// The public key of the secret seed (in ECDSA)
	pub_key: String,

	/// The node url, where to insert the keys
	node_url: String,
}

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

	if let Some(keys_path) = &cli.keys_path
		&& let Err(e) = insert_keys_to_chain(keys_path).await
	{
		println!("{e}");
	};

	loop {
		println!("Starting relay...");

		match relayer::Relayer::new(&cli.node_url.clone()).await {
			Err(e) => println!("Failed to created relayer: {e}"),
			Ok(relayer) => relayer.run_relay_by_subscription().await?,
		}
	}
}

async fn insert_keys_to_chain(key_file_path: &str) -> Result<(), Error> {
	// reading keys from file
	let beefy_key_infos = keys_from_file(key_file_path)?;

	// insert each key to keystore of each respective urls
	for key_info in beefy_key_infos {
		key_info.insert_key().await;
	}

	Ok(())
}

impl BeefyKeyInfo {
	async fn insert_key(self) {
		match RpcClient::from_url(&self.node_url).await {
			Ok(rpc) => self._insert_key(rpc).await,
			Err(e) => println!("Warning: Failed to Connect to {}: {e:#?}", self.node_url),
		}
	}

	async fn _insert_key(self, rpc: RpcClient) {
		let params = rpc_params!["beef".to_string(), self.suri, self.pub_key.clone()];

		if let Err(e) = rpc.request::<()>("author_insertKey", params).await {
			println!("Warning: Failed to insert key({}): {e:?}", self.pub_key);
			return;
		}

		println!("Added beefy key: {} to {}", self.pub_key, self.node_url);
	}
}

fn keys_from_file(key_file: &str) -> Result<Vec<BeefyKeyInfo>, Error> {
	let file = File::open(key_file).map_err(|e| {
		println!("WARNING: {e:#?}");
		Error::InvalidKeysFile(key_file.to_string())
	})?;

	let reader = BufReader::new(file);

	// Read the JSON contents of the file as an instance of `User`.
	serde_json::from_reader(reader).map_err(|e| {
		println!("WARNING: {e:#?}");
		Error::SerdeDecode(key_file.to_string())
	})
}
