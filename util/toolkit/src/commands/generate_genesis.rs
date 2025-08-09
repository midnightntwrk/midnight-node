use clap::Args;
use midnight_node_toolkit::cli_parsers::{self as cli};
use std::path::{Path, PathBuf};

use midnight_node_ledger_helpers::{NetworkId, Serializable, serialize};
use midnight_node_toolkit::genesis_generator::{
	FundingArgsShielded, FundingArgsUnshielded, GENESIS_NONCE_SEED, GenesisGenerator,
};

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
	/// Aguments for funding Shielded wallets
	#[command(flatten)]
	shielded: FundingArgsShielded,
	/// Aguments for funding Unshielded wallets
	#[command(flatten)]
	unshielded: FundingArgsUnshielded,
	/// Output directory
	#[arg(long, short = 'o', default_value = "out")]
	out_dir: String,
}

pub async fn execute(
	args: GenerateGenesisArgs,
) -> Result<GenesisGenerator, Box<dyn std::error::Error>> {
	let dir = Path::new(&args.out_dir);
	std::fs::create_dir_all(&dir)?;

	let network = args.network;
	let network_string = network_as_str(network);

	let suffix = if let Some(ref suffix) = args.suffix { suffix } else { network_string };
	println!("generating genesis for network {network_string} ({suffix})...");

	let genesis = GenesisGenerator::new(
		args.nonce_seed,
		network,
		args.proof_server,
		args.shielded,
		args.unshielded,
	)
	.await
	.expect("panic");

	let genesis_state_path = dir.join(format!("genesis_state_{suffix}.mn"));
	serialize_and_write(&genesis.state, network, &genesis_state_path)?;

	let genesis_tx_path = dir.join(format!("genesis_tx_{suffix}.mn"));
	serialize_and_write(&genesis.tx, network, &genesis_tx_path)?;

	Ok(genesis)
}

fn serialize_and_write<T: Serializable>(
	value: &T,
	network: NetworkId,
	file_path: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
	let serialized_value = serialize(value, network)?;
	std::fs::write(file_path, serialized_value)?;

	println!("Written to {}", file_path.display());

	Ok(())
}

fn network_as_str(id: NetworkId) -> &'static str {
	match id {
		NetworkId::MainNet => "mainnet",
		NetworkId::DevNet => "devnet",
		NetworkId::TestNet => "testnet",
		NetworkId::Undeployed => "undeployed",
		_ => panic!("unknown network id: {id:?}"),
	}
}

#[cfg(test)]
mod test {
	use super::{network_as_str, serialize_and_write};
	use crate::{Cli, DefaultDB, LedgerState, NetworkId, run_command};
	use clap::Parser;
	use std::{
		env::temp_dir,
		fs::{self, remove_file},
	};

	#[test]
	fn test_network_as_str() {
		assert_eq!("mainnet", network_as_str(NetworkId::MainNet));
		assert_eq!("devnet", network_as_str(NetworkId::DevNet));
		assert_eq!("undeployed", network_as_str(NetworkId::Undeployed));
	}

	#[test]
	fn test_serialize_and_write() {
		let state = LedgerState::<DefaultDB>::new();
		let network = NetworkId::TestNet;

		let path = temp_dir().join("state.mn");

		assert!(serialize_and_write(&state, network, &path).is_ok());
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
		let args = vec!["midnight-node-toolkit", "generate-genesis", "--network", "undeployed"];

		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		let path = "out/genesis_state_undeployed.mn";
		check_file(path);

		let path = "out/genesis_tx_undeployed.mn";
		check_file(path);
	}
}
