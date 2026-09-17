// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Ledger 9 -> 10 fork boundary for the toolkit's replay context.
//!
//! Unlike 8->9 there is no state translation: ledger 10 keeps every on-chain tag
//! (`ledger-state[v18]`, `zswap-local-state[v6]`, ...) and both generations share one
//! `midnight-storage` arena, so a v9 arena root *is* a v10 root - only the Rust type
//! changes. Every conversion below is therefore a re-typed key or a serialise/deserialise
//! round-trip through byte-identical encodings.

use std::collections::HashMap;

use tokio::sync::Mutex as MutexTokio;

type Db9 = crate::ledger_9::DefaultDB;
type Db10 = crate::ledger_10::DefaultDB;

use crate::ledger_9::{LedgerContext as LedgerContext9, SecretKeys as SecretKeys9};
use crate::ledger_10::{
	BlockContext as BlockContext10, DEFAULT_RESOLVER, DustWallet, HashOutput as HashOutput10,
	LedgerContext as LedgerContext10, LedgerState as LedgerState10, SecretKeys, ShieldedWallet,
	UnshieldedWallet, Wallet, WalletSeed, WalletState, default_storage,
};

pub fn old_to_new_sp<T1, T2>(
	mut t1: crate::ledger_9::Sp<T1, Db9>,
) -> Result<crate::ledger_10::Sp<T2, Db10>, std::io::Error>
where
	T1: crate::ledger_9::Storable<Db9> + crate::ledger_9::Tagged,
	T2: crate::ledger_10::Storable<Db10> + crate::ledger_10::Tagged,
{
	t1.persist();
	let old_root = t1.as_typed_key().key;
	// Both ArenaKey types are the same type (one shared `midnight-storage`).
	let new_arena_key: crate::ledger_10::ArenaKey = old_root;
	let new_root = crate::ledger_10::mn_ledger_storage::arena::TypedArenaKey::<
		T2,
		<Db9 as crate::ledger_10::DB>::Hasher,
	>::from(new_arena_key);
	default_storage::<Db10>().arena.get_lazy(&new_root)
}

pub fn old_to_new_ser<
	T1: crate::ledger_9::Serializable + crate::ledger_9::Tagged,
	T2: crate::ledger_10::Deserializable + crate::ledger_10::Tagged,
>(
	t1: &T1,
) -> Result<T2, std::io::Error> {
	let t_bytes = crate::ledger_9::serialize(t1)?;
	crate::ledger_10::deserialize(&mut &t_bytes[..])
}

pub fn old_to_new_ser_untagged<
	T1: crate::ledger_9::Serializable,
	T2: crate::ledger_10::Deserializable,
>(
	t1: &T1,
) -> Result<T2, std::io::Error> {
	let t_bytes = crate::ledger_9::serialize_untagged(t1)?;
	crate::ledger_10::deserialize_untagged(&mut &t_bytes[..])
}

pub fn block_context_9_to_10(ctx9: &crate::ledger_9::BlockContext) -> BlockContext10 {
	BlockContext10 {
		tblock: ctx9.tblock,
		tblock_err: ctx9.tblock_err,
		parent_block_hash: HashOutput10(ctx9.parent_block_hash.0),
		last_block_time: ctx9.last_block_time,
	}
}

pub fn fork_context_9_to_10(
	context9: LedgerContext9<Db9>,
) -> Result<LedgerContext10<Db10>, std::io::Error> {
	let ledger_state_9 = context9.ledger_state.lock().expect("failed to lock ledger state");
	// Same tag, same shape, same arena: the v9 root re-typed is the v10 state.
	let ledger_state: crate::ledger_10::Sp<LedgerState10<Db10>, Db10> =
		old_to_new_sp(ledger_state_9.clone())?;

	let mut wallets = HashMap::new();
	for (k, v) in context9.wallets.lock().expect("failed to lock wallets").iter() {
		let new_secret_keys: Result<Option<SecretKeys>, _> = v
			.shielded
			.secret_keys
			.as_ref()
			.map(|SecretKeys9 { coin_secret_key, encryption_secret_key }| {
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
				state: (*old_to_new_sp::<_, WalletState<Db10>>(crate::ledger_9::Sp::new(
					v.shielded.state.clone(),
				))?)
				.clone(),
				coin_public_key: old_to_new_ser(&v.shielded.coin_public_key)?,
				enc_public_key: old_to_new_ser(&v.shielded.enc_public_key)?,
				secret_keys: new_secret_keys?,
			},
			unshielded: (*old_to_new_sp::<_, UnshieldedWallet>(crate::ledger_9::Sp::new(
				v.unshielded.clone(),
			))?)
			.clone(),
			dust: (*old_to_new_sp::<_, DustWallet<Db10>>(crate::ledger_9::Sp::new(
				v.dust.clone(),
			))?)
			.clone(),
		};
		let new_key: WalletSeed = old_to_new_ser_untagged(&k)?;
		wallets.insert(new_key, new_wallet);
	}

	let latest_block_context = block_context_9_to_10(&context9.latest_block_context());

	Ok(LedgerContext10 {
		ledger_state: ledger_state.into(),
		latest_block_context: Some(latest_block_context).into(),
		wallets: wallets.into(),
		resolver: MutexTokio::new(&DEFAULT_RESOLVER),
	})
}
