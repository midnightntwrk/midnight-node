use std::collections::HashMap;

use super::serde_convert::{qualified_dust_output_to_ser, utxo_to_ser};
use crate::commands::show_wallet::WalletInfoJson;
use crate::serde_def::{QualifiedInfoSer, UtxoSer};
use hex::ToHex;
use ledger_helpers_local::{DefaultDB, UnshieldedWallet, WalletSeed, serialize_untagged};
use midnight_ledger_unsafe_helpers::ledger_9 as ledger_helpers_local;

pub fn show_wallet_from_seed(
	context: &ledger_helpers_local::context::LedgerContext<DefaultDB>,
	seed: WalletSeed,
	debug: bool,
) -> ShowWalletResult {
	context.with_ledger_state(|ledger_state| {
		context.with_wallet_from_seed(seed, |wallet| {
			if debug {
				let utxos = wallet.unshielded_utxos(ledger_state);
				let utxo_sers: Vec<UtxoSer> = utxos.into_iter().map(utxo_to_ser).collect();
				let debug_str = format!("{wallet:#?}");
				ShowWalletResult::Debug(debug_str, utxo_sers)
			} else {
				let utxos =
					wallet.unshielded_utxos(ledger_state).into_iter().map(utxo_to_ser).collect();
				let coins = wallet
					.shielded
					.state
					.coins
					.iter()
					.map(|(k, v)| {
						(
							serialize_untagged(&k).unwrap().encode_hex(),
							QualifiedInfoSer {
								nonce: serialize_untagged(&v.nonce).unwrap().encode_hex(),
								token_type: serialize_untagged(&v.type_).unwrap().encode_hex(),
								value: v.value,
								mt_index: v.mt_index,
							},
						)
					})
					.collect();
				let dust_utxos = wallet
					.dust
					.dust_local_state
					.as_ref()
					.map_or(vec![], |s| s.utxos().map(qualified_dust_output_to_ser).collect());
				let user_address = wallet.unshielded.user_address;
				let claimable_block_rewards =
					ledger_state.unclaimed_block_rewards.get(&user_address).copied().unwrap_or(0);
				let claimable_bridge_transfers =
					ledger_state.bridge_receiving.get(&user_address).copied().unwrap_or(0);
				ShowWalletResult::Json(WalletInfoJson {
					coins,
					dust_utxos,
					utxos,
					claimable_block_rewards,
					claimable_bridge_transfers,
				})
			}
		})
	})
}

pub fn show_wallet_from_address(
	context: &ledger_helpers_local::context::LedgerContext<DefaultDB>,
	address: ledger_helpers_local::WalletAddress,
) -> ShowWalletResult {
	let utxos = context.utxos(address.clone()).into_iter().map(utxo_to_ser).collect();
	// The claimable maps are public ledger state keyed by the unshielded `UserAddress`, so they can
	// be read from an address alone (no secret needed). A non-unshielded address yields zeroes.
	let (claimable_block_rewards, claimable_bridge_transfers) =
		match UnshieldedWallet::try_from(&address) {
			Ok(unshielded) => context.with_ledger_state(|ledger_state| {
				let addr = unshielded.user_address;
				(
					ledger_state.unclaimed_block_rewards.get(&addr).copied().unwrap_or(0),
					ledger_state.bridge_receiving.get(&addr).copied().unwrap_or(0),
				)
			}),
			Err(_) => (0, 0),
		};
	ShowWalletResult::Json(WalletInfoJson {
		coins: HashMap::new(),
		utxos,
		dust_utxos: Vec::new(),
		claimable_block_rewards,
		claimable_bridge_transfers,
	})
}

/// Indexer-backed wallet reconstruction for this ledger generation.
///
/// The GraphQL client itself is version-independent — it deals in hex blobs — but the blobs it
/// returns are this chain's native ledger encodings, so they must be decoded with this
/// generation's codecs. `show_wallet::execute` picks the copy matching the chain's reported
/// protocol version.
#[cfg(feature = "indexer-client")]
pub async fn show_wallet_from_indexer(
	indexer_url: &str,
	network: &str,
	seed: WalletSeed,
	debug: bool,
) -> Result<ShowWalletResult, Box<dyn std::error::Error + Send + Sync>> {
	use ledger_helpers_local::{BuilderContext, IndexerContext};

	let ctx = IndexerContext::<DefaultDB>::new(indexer_url, network)?;
	ctx.init_wallets(std::slice::from_ref(&seed)).await?;

	let (coins, dust_utxos, debug_str) = ctx.with_wallet_from_seed(seed.clone(), |wallet| {
		let coins = wallet
			.shielded
			.state
			.coins
			.iter()
			.map(|(k, v)| {
				(
					serialize_untagged(&k).unwrap().encode_hex(),
					QualifiedInfoSer {
						nonce: serialize_untagged(&v.nonce).unwrap().encode_hex(),
						token_type: serialize_untagged(&v.type_).unwrap().encode_hex(),
						value: v.value,
						mt_index: v.mt_index,
					},
				)
			})
			.collect::<HashMap<String, QualifiedInfoSer>>();
		let dust_utxos = wallet
			.dust
			.dust_local_state
			.as_ref()
			.map_or(vec![], |s| s.utxos().map(qualified_dust_output_to_ser).collect());
		let debug_str = debug.then(|| format!("{wallet:#?}"));
		(coins, dust_utxos, debug_str)
	});

	let utxos: Vec<UtxoSer> = ctx
		.unshielded_utxos(seed)
		.await
		.into_iter()
		.map(|(utxo, _ctime)| utxo_to_ser(utxo))
		.collect();

	Ok(match debug_str {
		Some(debug_str) => ShowWalletResult::Debug(debug_str, utxos),
		// The indexer reconstructs shielded/unshielded/dust wallet state but not the node's full
		// `LedgerState`, so the ledger-level claimable maps (block rewards / bridge transfers) are
		// unavailable on this path and reported as zero.
		None => ShowWalletResult::Json(WalletInfoJson {
			coins,
			utxos,
			dust_utxos,
			claimable_block_rewards: 0,
			claimable_bridge_transfers: 0,
		}),
	})
}

pub enum ShowWalletResult {
	Debug(String, Vec<UtxoSer>),
	Json(WalletInfoJson),
}
