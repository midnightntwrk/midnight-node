use std::collections::HashMap;

use super::ledger_helpers_local::{
	self, DefaultDB, UnshieldedWallet, WalletSeed, serialize_untagged,
};
use super::serde_convert::{qualified_dust_output_to_ser, utxo_to_ser};
use crate::commands::show_wallet::WalletInfoJson;
use crate::serde_def::{QualifiedInfoSer, UtxoSer};
use hex::ToHex;

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

pub enum ShowWalletResult {
	Debug(String, Vec<UtxoSer>),
	Json(WalletInfoJson),
}
