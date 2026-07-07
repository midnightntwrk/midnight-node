use crate::cli_parsers::{self as cli};
use clap::Args;

#[derive(Args, Clone)]
pub struct ShowSeedArgs {
	/// Wallet seed (`--seed` = Schnorr, `--seed-ecdsa` = ECDSA). NOTE: the output is the raw seed
	/// bytes, which are scheme-independent — both flags print the same hex. `--seed-ecdsa` is
	/// accepted only for CLI parity with the other seed commands; it is a no-op here.
	#[command(flatten)]
	seed: cli::SeedArg,
}

pub fn execute(args: ShowSeedArgs) -> String {
	// The scheme is intentionally discarded: `show-seed` prints the raw seed bytes, which are the
	// same regardless of the unshielded signature scheme.
	let (seed, _scheme) = args.seed.resolve();
	hex::encode(seed.as_bytes())
}
