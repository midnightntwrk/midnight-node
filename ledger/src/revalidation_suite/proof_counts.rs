// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! P8e: a strict hit verifies no proof, a miss verifies each of the tx's proofs once, and a warm
//! call verifies no more than the same call against an empty cache. Which proofs a revalidation
//! hit re-verifies is left to the ledger's own tests: predicting it here would re-implement
//! `RevalidationReference`.

use super::helpers::{PureGeneratorPedersen, Transaction, deserialize};
use super::ledger::{
	TransactionSignature, TxValidationCacheOutcome, ledger_storage_local::DefaultDB,
	mn_ledger_local::structure::ContractAction,
};
use super::{
	Class,
	model::ModelCall,
	proofs::{Counts, UnverifiedProofs},
	tree::Tree,
};
use proptest::prelude::*;

/// What a warm call that reached the cache met, for the reach test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Case {
	StrictHit,
	/// A cold call that passed on its first attempt, checked against the tx's proofs.
	Miss,
	/// A hit that verified fewer proofs than the cold call.
	Skipped,
	/// A hit that verified a contract proof again.
	ContractReverified,
	/// A hit that verified a dust spend again.
	DustReverified,
}

/// Every proof in tree tx `tx`, as [`UnverifiedProofs`] counts them.
fn proofs_in(tree: &Tree, tx: usize) -> Counts {
	let tx: Transaction<TransactionSignature, UnverifiedProofs, PureGeneratorPedersen, DefaultDB> =
		deserialize(&tree.txs[tx].bytes[..]).unwrap();
	let Transaction::Standard(stx) = tx else { panic!("tree txs are standard") };
	let mut counts = Counts::default();
	for (_, intent) in stx.intents.iter().map(|e| (*e).clone()) {
		counts.contract += intent
			.actions
			.iter()
			.filter(|a| matches!(&**a, ContractAction::Call(_)))
			.count();
		counts.dust += intent.dust_actions.as_ref().map_or(0, |dust| dust.spends.len());
	}
	counts.zswap = stx.guaranteed_coins.iter().count() + stx.fallible_coins.iter().count();
	counts
}

/// P8e for one call. `warm` and `cold` are what [`UnverifiedProofs`] counted during it against the
/// warm cache and an empty one; `attempts` are the P8a model's, `None` for a call that never
/// reached the cache.
pub fn check(
	tree: &Tree,
	tx: usize,
	call: &ModelCall,
	attempts: Option<&[TxValidationCacheOutcome]>,
	warm: Counts,
	cold: Counts,
	count: &mut impl FnMut(Class),
) -> Result<(), TestCaseError> {
	prop_assert!(
		warm.contract <= cold.contract && warm.dust <= cold.dust && warm.zswap <= cold.zswap,
		"P8e: the warm call verified {:?}, more than the cold call's {:?}",
		warm,
		cold
	);
	let Some(attempts) = attempts else {
		prop_assert_eq!(warm, Counts::default(), "P8e: verified before reaching the cache");
		return Ok(());
	};

	// A retried cold call verifies its failed first attempt's proofs too.
	if call.attempts.len() == 1 && call.passes_well_formed {
		prop_assert_eq!(cold, proofs_in(tree, tx), "P8e: a miss verifies each proof once");
		count(Class::ProofCount(Case::Miss));
	}

	use TxValidationCacheOutcome::*;
	if attempts.iter().all(|a| *a == StrictCacheHit) {
		prop_assert_eq!(warm, Counts::default(), "P8e: a strict hit verifies nothing");
		count(Class::ProofCount(Case::StrictHit));
	} else if attempts.iter().all(|a| *a != CacheMiss) {
		if warm != cold {
			count(Class::ProofCount(Case::Skipped));
		}
		if warm.contract > 0 {
			count(Class::ProofCount(Case::ContractReverified));
		}
		if warm.dust > 0 {
			count(Class::ProofCount(Case::DustReverified));
		}
	}
	Ok(())
}
