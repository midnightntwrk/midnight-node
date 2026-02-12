use std::collections::HashMap;

use ledger_storage::{
	Storable,
	arena::{Sp, TypedArenaKey},
};
use tokio::sync::Mutex as MutexTokio;

use midnight_serialize::{Deserializable, Serializable, Tagged};

use crate::ledger_7::{
	DB, LedgerContext as LedgerContext7, SecretKeys as SecretKeys7, deserialize,
	deserialize_untagged, serialize, serialize_untagged,
};
use crate::ledger_8::{
	BlockContext as BlockContext8, DEFAULT_RESOLVER, DustWallet, HashOutput as HashOutput8,
	LedgerContext as LedgerContext8, LedgerState as LedgerState8, SecretKeys, ShieldedWallet,
	Timestamp as Timestamp8, UnshieldedWallet, Wallet, WalletSeed, WalletState, default_storage,
};

pub fn old_to_new_sp<D: DB, T1: Storable<D>, T2: Storable<D>>(
	t1: Sp<T1, D>,
) -> Result<Sp<T2, D>, std::io::Error> {
	let old_root = t1.as_typed_key().key;
	let new_root = TypedArenaKey::<T2, <D as DB>::Hasher>::from(old_root);
	default_storage::<D>().arena.get(&new_root)
}

pub fn old_to_new_ser<T1: Serializable + Tagged, T2: Deserializable + Tagged>(
	t1: &T1,
) -> Result<T2, std::io::Error> {
	let t_bytes = serialize(t1)?;
	deserialize(&mut &t_bytes[..])
}

pub fn old_to_new_ser_untagged<T1: Serializable, T2: Deserializable>(
	t1: &T1,
) -> Result<T2, std::io::Error> {
	let t_bytes = serialize_untagged(t1)?;
	deserialize_untagged(&mut &t_bytes[..])
}

pub fn fork_context_7_to_8<D: DB + Clone>(
	context7: LedgerContext7<D>,
) -> Result<LedgerContext8<D>, std::io::Error> {
	let ledger_state_7 = context7.ledger_state.lock().expect("failed to lock ledger state");
	let ledger_state: Sp<LedgerState8<D>, D> = old_to_new_sp(ledger_state_7.clone())?;

	let mut wallets = HashMap::new();
	for (k, v) in context7.wallets.lock().expect("failed to lock wallets").iter() {
		let new_secret_keys: Result<Option<SecretKeys>, _> = v
			.shielded
			.secret_keys
			.as_ref()
			.map(|SecretKeys7 { coin_secret_key, encryption_secret_key }| {
				Ok::<_, std::io::Error>(SecretKeys {
					coin_secret_key: old_to_new_ser(coin_secret_key)?,
					encryption_secret_key: old_to_new_ser(encryption_secret_key)?,
				})
			})
			.transpose();
		let new_wallet = Wallet {
			root_seed: v.root_seed.map(|s| {
				WalletSeed::try_from(s.as_bytes())
					.expect("wallet seed different length between versions")
			}),
			shielded: ShieldedWallet {
				state: (*old_to_new_sp::<D, _, WalletState<D>>(Sp::new(v.shielded.state.clone()))?)
					.clone(),
				coin_public_key: old_to_new_ser(&v.shielded.coin_public_key)?,
				enc_public_key: old_to_new_ser(&v.shielded.enc_public_key)?,
				secret_keys: new_secret_keys?,
			},
			unshielded: (*old_to_new_sp::<D, _, UnshieldedWallet>(Sp::new(v.unshielded.clone()))?)
				.clone(),
			dust: (*old_to_new_sp::<D, _, DustWallet<D>>(Sp::new(v.dust.clone()))?).clone(),
		};
		let new_key: WalletSeed = old_to_new_ser_untagged(&k)?;
		wallets.insert(new_key, new_wallet);
	}

	let latest_block_context = context7.latest_block_context();
	let latest_block_context = BlockContext8 {
		tblock: Timestamp8::from_secs(latest_block_context.tblock.to_secs()),
		tblock_err: latest_block_context.tblock_err,
		parent_block_hash: HashOutput8(latest_block_context.parent_block_hash.0),
		last_block_time: Timestamp8::from_secs(latest_block_context.tblock.to_secs()), // Re-use tblock from current block
	};

	Ok(LedgerContext8 {
		ledger_state: ledger_state.into(),
		latest_block_context: Some(latest_block_context).into(),
		wallets: wallets.into(),
		resolver: MutexTokio::new(&DEFAULT_RESOLVER),
	})
}
