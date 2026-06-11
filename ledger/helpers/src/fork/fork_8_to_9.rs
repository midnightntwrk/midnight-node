use std::collections::HashMap;

use tokio::sync::Mutex as MutexTokio;

type Db8 = crate::ledger_8::DefaultDB;
type Db9 = crate::ledger_9::DefaultDB;

use crate::ledger_8::{LedgerContext as LedgerContext8, SecretKeys as SecretKeys8};
use crate::ledger_9::{
	BlockContext as BlockContext9, DEFAULT_RESOLVER, DustWallet, HashOutput as HashOutput9,
	LedgerContext as LedgerContext9, LedgerState as LedgerState8, SecretKeys, ShieldedWallet,
	UnshieldedWallet, Wallet, WalletSeed, WalletState, default_storage,
};

pub fn old_to_new_sp<T1, T2>(
	mut t1: crate::ledger_8::Sp<T1, Db8>,
) -> Result<crate::ledger_9::Sp<T2, Db9>, std::io::Error>
where
	T1: crate::ledger_8::Storable<Db8> + crate::ledger_8::Tagged,
	T2: crate::ledger_9::Storable<Db9> + crate::ledger_9::Tagged,
{
	t1.persist();
	let old_root = t1.as_typed_key().key;
	// Both ArenaKey types are the same type (unified via midnight-storage-core patch).
	let new_arena_key: crate::ledger_9::ArenaKey = old_root;
	let new_root = crate::ledger_9::mn_ledger_storage::arena::TypedArenaKey::<
		T2,
		<Db8 as crate::ledger_9::DB>::Hasher,
	>::from(new_arena_key);
	default_storage::<Db9>().arena.get_lazy(&new_root)
}

/// Serialize with ledger-8 format, deserialize with ledger-9 format (tagged).
pub fn old_to_new_ser<
	T1: crate::ledger_8::Serializable + crate::ledger_8::Tagged,
	T2: crate::ledger_9::Deserializable + crate::ledger_9::Tagged,
>(
	t1: &T1,
) -> Result<T2, std::io::Error> {
	let t_bytes = crate::ledger_8::serialize(t1)?;
	crate::ledger_9::deserialize(&mut &t_bytes[..])
}

/// Serialize with ledger-8 format, deserialize with ledger-9 format (untagged).
pub fn old_to_new_ser_untagged<
	T1: crate::ledger_8::Serializable,
	T2: crate::ledger_9::Deserializable,
>(
	t1: &T1,
) -> Result<T2, std::io::Error> {
	let t_bytes = crate::ledger_8::serialize_untagged(t1)?;
	crate::ledger_9::deserialize_untagged(&mut &t_bytes[..])
}

/// Convert a ledger-8 BlockContext to a ledger-9 BlockContext.
///
pub fn block_context_8_to_9(ctx8: &crate::ledger_8::BlockContext) -> BlockContext9 {
	BlockContext9 {
		tblock: ctx8.tblock,
		tblock_err: ctx8.tblock_err,
		parent_block_hash: HashOutput9(ctx8.parent_block_hash.0),
		last_block_time: ctx8.last_block_time,
	}
}

pub fn fork_context_8_to_9(
	context8: LedgerContext8<Db8>,
) -> Result<LedgerContext9<Db9>, std::io::Error> {
	let ledger_state_8 = context8.ledger_state.lock().expect("failed to lock ledger state");
	let ledger_state: crate::ledger_9::Sp<LedgerState8<Db8>, Db8> =
		old_to_new_sp(ledger_state_8.clone())?;

	let mut wallets = HashMap::new();
	for (k, v) in context8.wallets.lock().expect("failed to lock wallets").iter() {
		let new_secret_keys: Result<Option<SecretKeys>, _> = v
			.shielded
			.secret_keys
			.as_ref()
			.map(|SecretKeys8 { coin_secret_key, encryption_secret_key }| {
				Ok::<_, std::io::Error>(SecretKeys {
					coin_secret_key: old_to_new_ser(coin_secret_key)?,
					encryption_secret_key: old_to_new_ser(encryption_secret_key)?,
				})
			})
			.transpose();
		let new_wallet = Wallet {
			root_seed: v.root_seed.as_ref().map(|s| {
				WalletSeed::try_from(s.as_bytes())
					.expect("wallet seed different length between versions")
			}),
			shielded: ShieldedWallet {
				state: (*old_to_new_sp::<_, WalletState<Db8>>(crate::ledger_8::Sp::new(
					v.shielded.state.clone(),
				))?)
				.clone(),
				coin_public_key: old_to_new_ser(&v.shielded.coin_public_key)?,
				enc_public_key: old_to_new_ser(&v.shielded.enc_public_key)?,
				secret_keys: new_secret_keys?,
			},
			unshielded: (*old_to_new_sp::<_, UnshieldedWallet>(crate::ledger_8::Sp::new(
				v.unshielded.clone(),
			))?)
			.clone(),
			dust: (*old_to_new_sp::<_, DustWallet<Db8>>(crate::ledger_8::Sp::new(v.dust.clone()))?)
				.clone(),
		};
		let new_key: WalletSeed = old_to_new_ser_untagged(&k)?;
		wallets.insert(new_key, new_wallet);
	}

	let latest_block_context = block_context_8_to_9(&context8.latest_block_context());

	Ok(LedgerContext9 {
		ledger_state: ledger_state.into(),
		latest_block_context: Some(latest_block_context).into(),
		wallets: wallets.into(),
		resolver: MutexTokio::new(&DEFAULT_RESOLVER),
	})
}
