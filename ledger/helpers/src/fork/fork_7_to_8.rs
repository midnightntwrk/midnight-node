use std::collections::HashMap;

use tokio::sync::Mutex as MutexTokio;

type Db7 = crate::ledger_7::DefaultDB;
type Db8 = crate::ledger_8::DefaultDB;

use crate::ledger_7::{LedgerContext as LedgerContext7, SecretKeys as SecretKeys7};
use crate::ledger_8::{
	BlockContext as BlockContext8, DEFAULT_RESOLVER, DustWallet, HashOutput as HashOutput8,
	LedgerContext as LedgerContext8, LedgerState as LedgerState8, SecretKeys, ShieldedWallet,
	Timestamp as Timestamp8, UnshieldedWallet, Wallet, WalletSeed, WalletState, default_storage,
};

pub fn old_to_new_sp<T1, T2>(
	mut t1: crate::ledger_7::Sp<T1, Db7>,
) -> Result<crate::ledger_8::Sp<T2, Db8>, std::io::Error>
where
	T1: crate::ledger_7::Storable<Db7>,
	T2: crate::ledger_8::Storable<Db8> + crate::ledger_8::Tagged,
{
	t1.persist();
	let old_root = t1.as_typed_key().key;
	// Both ArenaKey types are the same type (unified via midnight-storage-core patch).
	let new_arena_key: crate::ledger_8::ArenaKey = old_root;
	let new_root = crate::ledger_8::mn_ledger_storage::arena::TypedArenaKey::<
		T2,
		<Db8 as crate::ledger_8::DB>::Hasher,
	>::from(new_arena_key);
	default_storage::<Db8>().arena.get_lazy(&new_root)
}

/// Serialize with ledger-7 format, deserialize with ledger-8 format (tagged).
pub fn old_to_new_ser<
	T1: crate::ledger_7::Serializable + crate::ledger_7::Tagged,
	T2: crate::ledger_8::Deserializable + crate::ledger_8::Tagged,
>(
	t1: &T1,
) -> Result<T2, std::io::Error> {
	let t_bytes = crate::ledger_7::serialize(t1)?;
	crate::ledger_8::deserialize(&mut &t_bytes[..])
}

/// Serialize with ledger-7 format, deserialize with ledger-8 format (untagged).
pub fn old_to_new_ser_untagged<
	T1: crate::ledger_7::Serializable,
	T2: crate::ledger_8::Deserializable,
>(
	t1: &T1,
) -> Result<T2, std::io::Error> {
	let t_bytes = crate::ledger_7::serialize_untagged(t1)?;
	crate::ledger_8::deserialize_untagged(&mut &t_bytes[..])
}

/// Convert a ledger-7 BlockContext to a ledger-8 BlockContext.
///
/// Ledger-7's BlockContext lacks `last_block_time`, so we approximate it with `tblock`.
/// This is acceptable for fork transitions and toolkit usage where the exact previous
/// block time is not critical.
pub fn block_context_7_to_8(ctx7: &crate::ledger_7::BlockContext) -> BlockContext8 {
	BlockContext8 {
		tblock: Timestamp8::from_secs(ctx7.tblock.to_secs()),
		tblock_err: ctx7.tblock_err,
		parent_block_hash: HashOutput8(ctx7.parent_block_hash.0),
		last_block_time: Timestamp8::from_secs(ctx7.tblock.to_secs()),
	}
}

pub fn fork_context_7_to_8(
	context7: LedgerContext7<Db7>,
) -> Result<LedgerContext8<Db8>, std::io::Error> {
	let ledger_state_7 = context7.ledger_state.lock().expect("failed to lock ledger state");
	let ledger_state: crate::ledger_8::Sp<LedgerState8<Db8>, Db8> =
		old_to_new_sp(ledger_state_7.clone())?;

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
				state: (*old_to_new_sp::<_, WalletState<Db8>>(crate::ledger_7::Sp::new(
					v.shielded.state.clone(),
				))?)
				.clone(),
				coin_public_key: old_to_new_ser(&v.shielded.coin_public_key)?,
				enc_public_key: old_to_new_ser(&v.shielded.enc_public_key)?,
				secret_keys: new_secret_keys?,
			},
			unshielded: (*old_to_new_sp::<_, UnshieldedWallet>(crate::ledger_7::Sp::new(
				v.unshielded.clone(),
			))?)
			.clone(),
			dust: (*old_to_new_sp::<_, DustWallet<Db8>>(crate::ledger_7::Sp::new(v.dust.clone()))?)
				.clone(),
		};
		let new_key: WalletSeed = old_to_new_ser_untagged(&k)?;
		wallets.insert(new_key, new_wallet);
	}

	let latest_block_context = block_context_7_to_8(&context7.latest_block_context());

	Ok(LedgerContext8 {
		ledger_state: ledger_state.into(),
		latest_block_context: Some(latest_block_context).into(),
		wallets: wallets.into(),
		resolver: MutexTokio::new(&DEFAULT_RESOLVER),
	})
}
