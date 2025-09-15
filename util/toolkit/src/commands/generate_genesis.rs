use clap::Args;
use midnight_node_toolkit::{
	cli_parsers::{self as cli},
	network_as_str,
};
use std::path::{Path, PathBuf};

use midnight_node_ledger_helpers::{NetworkId, Serializable, Tagged, WalletSeed, serialize};
use midnight_node_toolkit::genesis_generator::{FundingArgs, GENESIS_NONCE_SEED, GenesisGenerator};

#[derive(Args)]
pub struct GenerateGenesisArgs {
	/// Seed for genesis block generation
	#[arg(
        short,
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
        default_value = GENESIS_NONCE_SEED,
    )]
	nonce_seed: [u8; 32],
	// Target Network
	#[arg(long, value_parser = cli::network_id_decode)]
	network: NetworkId,
	// Proof Server Host
	#[arg(long, short)]
	proof_server: Option<String>,
	// Output suffix (defaults to match network)
	#[arg(long)]
	suffix: Option<String>,
	/// File containing the wallet seeds to fund
	#[arg(long)]
	seeds_file: PathBuf,
	/// Arguments for funding wallets
	#[command(flatten)]
	funding: FundingArgs,
	/// Output directory
	#[arg(long, short = 'o', default_value = "out")]
	out_dir: String,
}

pub async fn execute(
	args: GenerateGenesisArgs,
) -> Result<GenesisGenerator, Box<dyn std::error::Error + Send + Sync>> {
	let dir = Path::new(&args.out_dir);
	std::fs::create_dir_all(&dir)?;

	let network = args.network;
	let network_string = network_as_str(network);

	let suffix = if let Some(ref suffix) = args.suffix { suffix } else { network_string };
	println!("generating genesis for network {network_string} ({suffix})...");

	// Parse the seeds file
	let seeds_str = std::fs::read_to_string(args.seeds_file)?;
	let seeds_json: serde_json::Value = serde_json::from_str(&seeds_str)?;
	let seeds: Result<Vec<WalletSeed>, Box<dyn std::error::Error + Send + Sync>> = seeds_json
		.as_object()
		.unwrap()
		.iter()
		.map(|(_k, v)| {
			let wallet_seed_str = v.as_str().ok_or("seeds file object value was not a string")?;
			let wallet_seed_bytes: [u8; 32] = cli::hex_str_decode(wallet_seed_str)?;
			Ok(WalletSeed::from(wallet_seed_bytes))
		})
		.collect();

	let genesis =
		GenesisGenerator::new(args.nonce_seed, network, args.proof_server, args.funding, &seeds?)
			.await?;

	let genesis_state_path = dir.join(format!("genesis_state_{suffix}.mn"));
	serialize_and_write(&genesis.state, &genesis_state_path)?;

	let genesis_tx_path = dir.join(format!("genesis_block_{suffix}.mn"));
	serialize_and_write(&genesis.txs, &genesis_tx_path)?;

	println!("Number of genesis txs: {}", genesis.txs.len());

	Ok(genesis)
}

fn serialize_and_write<T: Serializable + Tagged>(
	value: &T,
	file_path: &PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let serialized_value = serialize(value)?;
	std::fs::write(file_path, serialized_value)?;

	println!("Written to {}", file_path.display());

	Ok(())
}

#[cfg(test)]
mod test {
	use super::serialize_and_write;
	use crate::{Cli, DefaultDB, LedgerState, NetworkId, run_command};
	use clap::Parser;
	use std::{
		env::temp_dir,
		fs::{self, remove_file},
	};

	#[test]
	fn test_serialize_and_write() {
		let state = LedgerState::<DefaultDB>::new(NetworkId::Undeployed);

		let path = temp_dir().join("state.mn");

		assert!(serialize_and_write(&state, &path).is_ok());
		assert!(path.exists());
		remove_file(&path).expect("It should be removed");
	}

	fn check_file(path: &str) {
		let file_exist = fs::exists(path).expect("file exist failed");
		assert!(file_exist);
		remove_file(path).expect("file failed to remove");
	}

	#[tokio::test]
	async fn test_generate_genesis() {
		let path = temp_dir().join("undeployed-seeds.json");
		let mut seed_map = std::collections::HashMap::new();
		seed_map.insert(
			"wallet-seed-0",
			"0000000000000000000000000000000000000000000000000000000000000001",
		);
		seed_map.insert(
			"wallet-seed-1",
			"0000000000000000000000000000000000000000000000000000000000000002",
		);
		seed_map.insert(
			"wallet-seed-2",
			"0000000000000000000000000000000000000000000000000000000000000003",
		);
		seed_map.insert(
			"wallet-seed-3",
			"0000000000000000000000000000000000000000000000000000000000000004",
		);

		let mut dest = std::fs::OpenOptions::new()
			.create(true)
			.write(true)
			.open(&path)
			.expect("failed to open seed file as writer");
		serde_json::to_writer(&mut dest, &seed_map).expect("failed to write seed file");

		let args = vec![
			"midnight-node-toolkit",
			"generate-genesis",
			"--network",
			"undeployed",
			"--seeds-file",
			path.to_str().unwrap(),
		];

		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		let path = "out/genesis_state_undeployed.mn";
		check_file(path);

		let path = "out/genesis_block_undeployed.mn";
		check_file(path);
	}
}
