use crate::WalletSeed;
use clap::Args;
use midnight_node_toolkit::cli_parsers::{self as cli};

#[derive(Args, Clone)]
pub struct ShowSeedArgs {
	/// Wallet seed
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	seed: WalletSeed,
}

pub fn execute(args: ShowSeedArgs) -> String {
	hex::encode(args.seed.as_bytes())
}
