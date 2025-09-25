use crate::{
	DB, DefaultDB, HRP_CREDENTIAL_SHIELDED, LedgerContext, ProofType, SignatureType, Source,
	TxGenerator, Utxo, Wallet, WalletAddress, WalletSeed,
};
use clap::Args;
use midnight_node_toolkit::cli_parsers::{self as cli};

#[derive(Debug)]
pub struct WalletInfo<D: DB + Clone> {
	pub wallet: Wallet<D>,
	pub utxos: Vec<Utxo>,
}

#[derive(Debug)]
pub enum ShowWalletResult<D: DB + Clone> {
	FromSeed(WalletInfo<D>),
	FromAddress(Vec<Utxo>),
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
}

pub async fn execute(
	args: ShowWalletArgs,
) -> Result<ShowWalletResult<DefaultDB>, Box<dyn std::error::Error + Send + Sync>> {
	let src = TxGenerator::<SignatureType, ProofType>::source(args.source).await?;

	let source_blocks = src.get_txs().await?;
	let network_id = source_blocks.network().to_string();

	match args.seed {
		Some(seed) => {
			let context = LedgerContext::new_from_wallet_seeds(network_id, &[seed]);

			for block in source_blocks.blocks {
				context.update_from_block(block.transactions, block.context, );
			}

			Ok(context.with_ledger_state(|ledger_state| {
				context.with_wallet_from_seed(seed, |wallet| {
					let utxos = wallet.unshielded_utxos(ledger_state);
					ShowWalletResult::FromSeed(WalletInfo { wallet: wallet.clone(), utxos })
				})
			}))
		},
		None => match args.address {
			Some(address) => {
				if address.human_readable_part().contains(HRP_CREDENTIAL_SHIELDED) {
					return Err("unavailable information - secret key needed".into());
				}

				let context = LedgerContext::new(network_id);
				for block in source_blocks.blocks {
					context.update_from_block(block.transactions, block.context, );
				}

				let utxos = context.utxos(address);
				Ok(ShowWalletResult::FromAddress(utxos))
			},
			None => unreachable!(),
		},
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

	/*
	#[test_case(test_fixture!("mn_addr_undeployed13h0e3c2m7rcfem6wvjljnyjmxy5rkg9kkwcldzt73ya5pv7c4p8skzgqwj", "genesis/genesis_tx_undeployed.mn") =>
		matches Ok(ShowWalletResult::FromAddress(utxos))
			if !utxos.is_empty();
		"funded-unshielded-address-0"
	)]
	#[test_case(test_fixture!("mn_addr_undeployed1h3ssm5ru2t6eqy4g3she78zlxn96e36ms6pq996aduvmateh9p9sk96u7s", "genesis/genesis_tx_undeployed.mn") =>
		matches Ok(ShowWalletResult::FromAddress(utxos))
			if !utxos.is_empty();
		"funded-unshielded-address-1"
	)]
	#[test_case(test_fixture!("mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r", "genesis/genesis_tx_undeployed.mn") =>
		matches Ok(ShowWalletResult::FromAddress(utxos))
			if !utxos.is_empty();
		"funded-unshielded-address-2"
	)]
	#[test_case(test_fixture!("mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm", "genesis/genesis_tx_undeployed.mn") =>
		matches Ok(ShowWalletResult::FromAddress(utxos))
			if !utxos.is_empty();
		"funded-unshielded-address-3"
	)]
	#[test_case(test_fixture!("mn_addr_undeployed1em04acpr67j9jr4ffvgjmmvux40497ddmvpgpw2ezmpa2rj0tlaqhgqswk", "genesis/genesis_tx_undeployed.mn") =>
		matches Ok(ShowWalletResult::FromAddress(utxos))
			if utxos.is_empty();
		"unfunded-unshielded-address"
	)]
	#[test_case(test_fixture!("mn_shield-addr_undeployed12p0cn6f9dtlw74r44pg8mwwjwkr74nuekt4xx560764703qeeuvqxqqgft8uzya2rud445nach4lk74s7upjwydl8s0nejeg6hh5vck0vueqyws5", "genesis/genesis_tx_undeployed.mn") =>
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
			address: Some(WalletAddress::from_str(addr).unwrap()),
		};

		super::execute(args).await
	}
	*/

	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000001", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::FromSeed(WalletInfo {utxos, ..}))
			if !utxos.is_empty();
		"funded-unshielded-seed-1"
	)]
	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000002", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::FromSeed(WalletInfo {utxos, ..}))
			if !utxos.is_empty();
		"funded-unshielded-seed-2"
	)]
	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000003", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::FromSeed(WalletInfo {utxos, ..}))
			if !utxos.is_empty();
		"funded-unshielded-seed-3"
	)]
	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000004", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::FromSeed(WalletInfo {utxos, ..}))
			if !utxos.is_empty();
		"funded-unshielded-seed-4"
	)]
	#[test_case(test_fixture!("0000000000000000000000000000000000000000000000000000000000000005", "genesis/genesis_block_undeployed.mn") =>
	matches Ok(ShowWalletResult::FromSeed(WalletInfo {utxos, ..}))
			if utxos.is_empty();
		"unfunded-unshielded-seed"
	)]
	#[tokio::test]
	async fn test_from_seed(
		(seed, src_files): (&str, Vec<String>),
	) -> Result<ShowWalletResult<DefaultDB>, Box<dyn std::error::Error + Send + Sync>> {
		let seed = WalletSeed::from(seed);
		let args = ShowWalletArgs {
			source: Source { src_url: None, fetch_concurrency: 20, src_files: Some(src_files) },
			seed: Some(seed),
			address: None,
		};

		super::execute(args).await
	}
}
