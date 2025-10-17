use std::collections::HashMap;

use crate::{
	DB, DefaultDB, HRP_CREDENTIAL_SHIELDED, LedgerContext, ProofType, SignatureType, Source,
	TxGenerator, Utxo, Wallet, WalletAddress, WalletSeed,
};
use clap::Args;
use hex::ToHex;
use midnight_node_ledger_helpers::serialize_untagged;
use midnight_node_toolkit::{
	cli_parsers::{self as cli},
	serde_def::{QualifiedDustOutputSer, QualifiedInfoSer, UtxoSer},
};

#[derive(Debug)]
pub struct WalletInfo<D: DB + Clone> {
	pub wallet: Wallet<D>,
	pub utxos: Vec<Utxo>,
}

#[derive(Debug, serde::Serialize)]
pub struct WalletInfoJson {
	pub coins: HashMap<String, QualifiedInfoSer>,
	pub utxos: Vec<UtxoSer>,
	pub dust_utxos: Vec<QualifiedDustOutputSer>,
}

#[derive(Debug)]
pub enum ShowWalletResult<D: DB + Clone> {
	Debug(WalletInfo<D>),
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
) -> Result<ShowWalletResult<DefaultDB>, Box<dyn std::error::Error + Send + Sync>> {
	let src = TxGenerator::<SignatureType, ProofType>::source(args.source, args.dry_run).await?;

	if args.dry_run {
		if let Some(seed) = args.seed {
			println!("Dry-run: fetching wallet for seed {:?}", seed);
		} else {
			println!("Dry-run: fetching wallet for address {:?}", args.address.unwrap());
		}
		return Ok(ShowWalletResult::DryRun(()));
	}

	let source_blocks = src.get_txs().await?;
	let network_id = source_blocks.network().to_string();

	if let Some(seed) = args.seed {
		let context = LedgerContext::new_from_wallet_seeds(network_id, &[seed]);

		for block in source_blocks.blocks {
			context.update_from_block(block.transactions, block.context, block.state_root.clone());
		}

		Ok(context.with_ledger_state(|ledger_state| {
			context.with_wallet_from_seed(seed, |wallet| {
				if args.debug {
					let utxos = wallet.unshielded_utxos(ledger_state);
					ShowWalletResult::Debug(WalletInfo { wallet: wallet.clone(), utxos })
				} else {
					let utxos = wallet
						.unshielded_utxos(ledger_state)
						.into_iter()
						.map(|u| u.into())
						.collect();
					let coins = wallet
						.shielded
						.state
						.coins
						.iter()
						.map(|(k, v)| (serialize_untagged(&k).unwrap().encode_hex(), (*v).into()))
						.collect();
					let dust_utxos = wallet
						.dust
						.dust_local_state
						.as_ref()
						.map_or(vec![], |s| s.utxos().map(|s| s.into()).collect());
					ShowWalletResult::Json(WalletInfoJson { coins, dust_utxos, utxos })
				}
			})
		}))
	} else {
		let address = args.address.expect("parsing error; address not given");
		if address.human_readable_part().contains(HRP_CREDENTIAL_SHIELDED) {
			return Err("unavailable information - secret key needed".into());
		}

		let context = LedgerContext::new(network_id);
		for block in source_blocks.blocks {
			context.update_from_block(block.transactions, block.context, block.state_root.clone());
		}

		let utxos = context.utxos(address).into_iter().map(|u| u.into()).collect();
		Ok(ShowWalletResult::Json(WalletInfoJson {
			coins: Default::default(),
			utxos,
			dust_utxos: Default::default(),
		}))
	}
}

#[cfg(test)]
mod tests {
	//use std::str::FromStr;

	use super::*;
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
	) -> Result<ShowWalletResult<DefaultDB>, Box<dyn std::error::Error + Send + Sync>> {
		let args = ShowWalletArgs {
			source: Source { src_url: None, fetch_concurrency: 20, src_files: Some(src_files) },
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
	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000004", "genesis/genesis_block_undeployed.mn") =>
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
	) -> Result<ShowWalletResult<DefaultDB>, Box<dyn std::error::Error + Send + Sync>> {
		let seed = WalletSeed::try_from_hex_str(seed).unwrap();
		let args = ShowWalletArgs {
			source: Source { src_url: None, fetch_concurrency: 20, src_files: Some(src_files) },
			seed: Some(seed),
			address: None,
			debug: false,
			dry_run: false,
		};

		super::execute(args).await
	}
}
