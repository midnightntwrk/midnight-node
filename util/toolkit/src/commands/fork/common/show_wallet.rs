use std::collections::HashMap;

use super::ledger_helpers_local::{self, DefaultDB, WalletSeed, serialize_untagged};
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
				ShowWalletResult::Json(WalletInfoJson { coins, dust_utxos, utxos })
			}
		})
	})
}

pub fn show_wallet_from_address(
	context: &ledger_helpers_local::context::LedgerContext<DefaultDB>,
	address: ledger_helpers_local::WalletAddress,
) -> ShowWalletResult {
	let utxos = context.utxos(address).into_iter().map(utxo_to_ser).collect();
	ShowWalletResult::Json(WalletInfoJson { coins: HashMap::new(), utxos, dust_utxos: Vec::new() })
}

pub enum ShowWalletResult {
	Debug(String, Vec<UtxoSer>),
	Json(WalletInfoJson),
}
