use crate::WalletSeed;
use crate::cli_parsers::{self as cli};
use clap::Args;

#[derive(Args, Clone)]
pub struct ShowSeedArgs {
	/// Wallet seed
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	seed: WalletSeed,
}

pub fn execute(args: ShowSeedArgs) -> String {
	hex::encode(args.seed.as_bytes())
}
