#![allow(clippy::result_large_err)]

mod authorities;
mod beefy_keys;
mod cardano_encoding;
mod error;
mod helper;

mod justification;
mod relayer;

use clap::Parser;
use midnight_node_metadata::midnight_metadata_latest as mn_meta;

pub use error::Error;

pub type BeefyValidatorSet =
	sp_consensus_beefy::ValidatorSet<sp_consensus_beefy::ecdsa_crypto::Public>;
pub type BeefySignedCommitment =
	sp_consensus_beefy::SignedCommitment<BlockNumber, sp_consensus_beefy::ecdsa_crypto::Signature>;
pub type BeefyId = sp_consensus_beefy::ecdsa_crypto::AuthorityId;

pub type BlockNumber = u32;
pub type MmrProof = mmr_rpc::LeavesProof<sp_core::H256>;

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
	env_logger::init();

	let cli = Cli::parse();

	// reading beefy keys from the given file path, and inserting to the chain
	if let Some(keys_path) = &cli.keys_path
		&& let Err(e) = beefy_keys::read_and_insert_to_chain(keys_path).await
	{
		log::error!("{e}");
	};

	loop {
		log::info!("Starting relay...");

		match relayer::Relayer::new(&cli.node_url.clone()).await {
			Err(e) => log::error!("Failed to created relayer: {e}"),
			Ok(relayer) => relayer.run_relay_by_subscription().await?,
		}
	}
}
