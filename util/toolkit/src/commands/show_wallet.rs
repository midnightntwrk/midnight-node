use std::collections::HashMap;

use crate::source::Source;
use crate::tx_generator::builder::build_fork_aware_context_cached;
use crate::tx_generator::source::create_file_wallet_cache;
use crate::{HRP_CREDENTIAL_SHIELDED, TxGenerator, WalletAddress, WalletSeed};
use crate::{
	cli_parsers::{self as cli},
	serde_def::{QualifiedDustOutputSer, QualifiedInfoSer, UtxoSer},
};
use clap::Args;

#[derive(Debug, serde::Serialize)]
pub struct WalletInfoJson {
	pub coins: HashMap<String, QualifiedInfoSer>,
	pub utxos: Vec<UtxoSer>,
	pub dust_utxos: Vec<QualifiedDustOutputSer>,
}

#[derive(Debug)]
pub enum ShowWalletResult {
	Debug(String, Vec<UtxoSer>),
	Json(WalletInfoJson),
	DryRun(()),
}

#[derive(Args)]
#[group(id = "wallet_id", required = true, multiple = false)]
pub struct ShowWalletArgs {
	#[command(flatten)]
	source: Source,
	/// The seed of the wallet to show wallet state for, including private state
	#[arg(long, value_parser = cli::wallet_seed_decode, group = "wallet_id")]
	seed: Option<WalletSeed>,
	/// The address of the wallet to show wallet state for, does not include private state
	#[arg(long, value_parser = cli::wallet_address, group = "wallet_id")]
	address: Option<WalletAddress>,
	/// Output the full wallet state using a debug print
	#[arg(long)]
	debug: bool,
	/// Dry-run - don't fetch wallet state, just print out settings
	#[arg(long)]
	dry_run: bool,
}

pub async fn execute(
	args: ShowWalletArgs,
) -> Result<ShowWalletResult, Box<dyn std::error::Error + Send + Sync>> {
	let ledger_state_db = args.source.ledger_state_db.clone();
	let fetch_cache = args.source.fetch_cache.clone();
	let src = TxGenerator::source(args.source, args.dry_run).await?;

	if args.dry_run {
		if let Some(seed) = args.seed {
			log::info!("Dry-run: fetching wallet for seed {:?}", seed);
		} else {
			log::info!("Dry-run: fetching wallet for address {:?}", args.address.unwrap());
		}
		return Ok(ShowWalletResult::DryRun(()));
	}

	let source_blocks = src.get_txs().await?;
	let wallet_cache = create_file_wallet_cache(&ledger_state_db, &fetch_cache);

	if let Some(seed) = args.seed {
		let fork_ctx =
			build_fork_aware_context_cached(&[seed], &source_blocks, wallet_cache.as_deref()).await;

		Ok(fork_ctx.dispatch(
			|ctx| {
				let seed_v7 =
					crate::tx_generator::builder::builders::ledger_7::type_convert::convert_wallet_seed(seed);
				let result = crate::commands::fork::ledger_7::show_wallet::show_wallet_from_seed(
					&ctx, seed_v7, args.debug,
				);
				fork_wallet_result_v7(result)
			},
			|ctx| {
				let result = crate::commands::fork::ledger_8::show_wallet::show_wallet_from_seed(
					&ctx, seed, args.debug,
				);
				fork_wallet_result_v8(result)
			},
		))
	} else {
		let address = args.address.expect("parsing error; address not given");
		if address.human_readable_part().contains(HRP_CREDENTIAL_SHIELDED) {
			return Err("unavailable information - secret key needed".into());
		}

		let fork_ctx =
			build_fork_aware_context_cached(&[], &source_blocks, wallet_cache.as_deref()).await;

		let address_clone = address.clone();
		Ok(fork_ctx.dispatch(
			|ctx| {
				let addr_v7 =
					crate::tx_generator::builder::builders::ledger_7::type_convert::convert_wallet_address(
						&address_clone,
					);
				let result = crate::commands::fork::ledger_7::show_wallet::show_wallet_from_address(
					&ctx, addr_v7,
				);
				fork_wallet_result_v7(result)
			},
			|ctx| {
				let result = crate::commands::fork::ledger_8::show_wallet::show_wallet_from_address(
					&ctx, address,
				);
				fork_wallet_result_v8(result)
			},
		))
	}
}

fn fork_wallet_result_v8(
	result: crate::commands::fork::ledger_8::show_wallet::ShowWalletResult,
) -> ShowWalletResult {
	use crate::commands::fork::ledger_8::show_wallet::ShowWalletResult as R;
	match result {
		R::Debug(s, u) => ShowWalletResult::Debug(s, u),
		R::Json(j) => ShowWalletResult::Json(j),
	}
}

fn fork_wallet_result_v7(
	result: crate::commands::fork::ledger_7::show_wallet::ShowWalletResult,
) -> ShowWalletResult {
	use crate::commands::fork::ledger_7::show_wallet::ShowWalletResult as R;
	match result {
		R::Debug(s, u) => ShowWalletResult::Debug(s, u),
		R::Json(j) => ShowWalletResult::Json(j),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tx_generator::source::FetchCacheConfig;
	use test_case::test_case;

	macro_rules! test_fixture {
		($addr:literal, $src:literal) => {
			($addr, vec![concat!(env!("CARGO_MANIFEST_DIR"), "/test-data/", $src).to_string()])
		};
	}

	#[test_case(test_fixture!("mn_addr_undeployed1h3ssm5ru2t6eqy4g3she78zlxn96e36ms6pq996aduvmateh9p9sk96u7s", "genesis/genesis_block_undeployed.mn") =>
		matches Ok(ShowWalletResult::Json(WalletInfoJson{ utxos, ..}))
			if !utxos.is_empty();
		"funded-unshielded-address-0"
	)]
	#[test_case(test_fixture!("mn_addr_undeployed1em04acpr67j9jr4ffvgjmmvux40497ddmvpgpw2ezmpa2rj0tlaqhgqswk", "genesis/genesis_block_undeployed.mn") =>
		matches Ok(ShowWalletResult::Json(WalletInfoJson{ utxos, ..}))
			if utxos.is_empty();
		"unfunded-unshielded-address"
	)]
	#[test_case(test_fixture!("mn_shield-addr_undeployed12p0cn6f9dtlw74r44pg8mwwjwkr74nuekt4xx560764703qeeuvqxqqgft8uzya2rud445nach4lk74s7upjwydl8s0nejeg6hh5vck0vueqyws5", "genesis/genesis_block_undeployed.mn") =>
		matches Err(error)
			if error.to_string() == "unavailable information - secret key needed";
		"illegal-shielded-address"
	)]
	#[tokio::test]
	async fn test_from_address(
		(addr, src_files): (&str, Vec<String>),
	) -> Result<ShowWalletResult, Box<dyn std::error::Error + Send + Sync>> {
		let args = ShowWalletArgs {
			source: Source {
				src_url: None,
				fetch_concurrency: 20,
				fetch_compute_concurrency: None,
				src_files: Some(src_files),
				dust_warp: false,
				ignore_block_context: false,
				fetch_only_cached: false,
				fetch_cache: FetchCacheConfig::InMemory,
				ledger_state_db: String::new(),
			},
			seed: None,
			address: Some(cli::wallet_address(addr).unwrap()),
			debug: false,
			dry_run: false,
		};

		super::execute(args).await
	}

	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000001", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::Json(WalletInfoJson {utxos, coins, dust_utxos}))
			if !utxos.is_empty() && !coins.is_empty() && !dust_utxos.is_empty();
		"funded-unshielded-seed-1"
	)]
	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000002", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::Json(WalletInfoJson {utxos, coins, dust_utxos}))
			if !utxos.is_empty() && !coins.is_empty() && !dust_utxos.is_empty();
		"funded-unshielded-seed-2"
	)]
	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000003", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::Json(WalletInfoJson {utxos, coins, dust_utxos}))
			if !utxos.is_empty() && !coins.is_empty() && !dust_utxos.is_empty();
		"funded-unshielded-seed-3"
	)]
	#[test_case(test_fixture!("a51c86de32d0791f7cffc3bdff1abd9bb54987f0ed5effc30c936dddbb9afd9d530c8db445e4f2d3ea42a321b260e022aadf05987c9a67ec7b6b6ca1d0593ec9", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::Json(WalletInfoJson {utxos, coins, dust_utxos}))
			if !utxos.is_empty() && !coins.is_empty() && !dust_utxos.is_empty();
		"funded-unshielded-seed-4"
	)]
	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000005", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::Json(WalletInfoJson {utxos, coins, dust_utxos}))
			if utxos.is_empty() && coins.is_empty() && dust_utxos.is_empty();
		"unfunded-unshielded-seed"
	)]
	#[tokio::test]
	async fn test_from_seed(
		(seed, src_files): (&str, Vec<String>),
	) -> Result<ShowWalletResult, Box<dyn std::error::Error + Send + Sync>> {
		let seed = WalletSeed::try_from_hex_str(seed).unwrap();
		let args = ShowWalletArgs {
			source: Source {
				src_url: None,
				fetch_concurrency: 20,
				fetch_compute_concurrency: None,
				src_files: Some(src_files),
				dust_warp: true,
				ignore_block_context: false,
				fetch_only_cached: false,
				fetch_cache: FetchCacheConfig::InMemory,
				ledger_state_db: String::new(),
			},
			seed: Some(seed),
			address: None,
			debug: false,
			dry_run: false,
		};

		super::execute(args).await
	}
}
