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

pub use crate::fork::fork_8_to_9::fork_context_8_to_9;
use midnight_node_ledger_helpers::fork::raw_block_data::{
	LedgerVersion, RawBlockData, RawTransaction,
};

type Db8 = crate::ledger_8::DefaultDB;
type Db9 = crate::ledger_9::DefaultDB;

pub enum ForkAwareLedgerContext {
	Ledger8(crate::ledger_8::context::LedgerContext<Db8>),
	Ledger9(crate::ledger_9::context::LedgerContext<Db9>),
}

/// Ledger-8 wallet for an `ecdsa:` seed: real shielded and dust sub-wallets, watch-only unshielded
/// one at the ECDSA address. Ledger 8 cannot derive that address, so it is computed with the
/// ledger-9 types; `UserAddress` is the same 32-byte hash in both.
pub fn watch_only_ecdsa_wallet_8(
	seed: &crate::ledger_9::WalletSeed,
	seed_8: crate::ledger_8::WalletSeed,
	ledger_state: &crate::ledger_8::LedgerState<Db8>,
) -> crate::ledger_8::Wallet<Db8> {
	let address = crate::ledger_9::UnshieldedWallet::new(
		seed.clone(),
		crate::ledger_9::UnshieldedSignatureScheme::Ecdsa,
	)
	.user_address;
	let address_8 = crate::ledger_8::UserAddress(crate::ledger_8::HashOutput(address.0.0));

	crate::ledger_8::Wallet {
		root_seed: Some(seed_8.clone()),
		shielded: crate::ledger_8::ShieldedWallet::default(seed_8.clone()),
		unshielded: crate::ledger_8::UnshieldedWallet::from(address_8),
		dust: crate::ledger_8::DustWallet::default(seed_8, Some(&ledger_state.parameters)),
	}
}

impl ForkAwareLedgerContext {
	/// Create a new context at the given ledger version.
	pub fn new(version: LedgerVersion, network_id: impl Into<String>) -> Self {
		let network_id = network_id.into();
		match version {
			LedgerVersion::Ledger8 => {
				Self::Ledger8(crate::ledger_8::context::LedgerContext::new(network_id))
			},
			LedgerVersion::Ledger9 => {
				Self::Ledger9(crate::ledger_9::context::LedgerContext::new(network_id))
			},
		}
	}

	/// Create a new context with wallet seeds at the given ledger version.
	pub fn new_from_wallet_seeds(
		version: LedgerVersion,
		network_id: impl Into<String>,
		seeds: &[crate::ledger_9::WalletSeed],
	) -> Self {
		let network_id = network_id.into();
		match version {
			LedgerVersion::Ledger8 => {
				// Convert ledger_9 WalletSeeds to ledger_8 WalletSeeds
				let seeds_8: Vec<crate::ledger_8::WalletSeed> = seeds
					.iter()
					.map(|s| {
						crate::ledger_8::WalletSeed::try_from(s.as_bytes())
							.expect("ledger seed format should be backwards compatible")
					})
					.collect();
				Self::Ledger8(crate::ledger_8::context::LedgerContext::new_from_wallet_seeds(
					network_id, &seeds_8,
				))
			},
			LedgerVersion::Ledger9 => Self::Ledger9(
				crate::ledger_9::context::LedgerContext::new_from_wallet_seeds(network_id, seeds),
			),
		}
	}

	/// Like [`Self::new_from_wallet_seeds`] but with a per-seed unshielded signature scheme.
	///
	/// Ledger 8 cannot hold an ECDSA key, so there such seeds get a watch-only wallet (see
	/// [`watch_only_ecdsa_wallet_8`]) and their keys are installed after the 8->9 fork.
	pub fn new_from_wallet_seeds_with_schemes(
		version: LedgerVersion,
		network_id: impl Into<String>,
		seeds: &[(crate::ledger_9::WalletSeed, crate::ledger_9::UnshieldedSignatureScheme)],
	) -> Self {
		let network_id = network_id.into();
		match version {
			LedgerVersion::Ledger9 => Self::Ledger9(
				crate::ledger_9::context::LedgerContext::new_from_wallet_seeds_with_schemes(
					network_id, seeds,
				),
			),
			LedgerVersion::Ledger8 => {
				use crate::ledger_9::UnshieldedSignatureScheme as Scheme;
				let (schnorr, ecdsa): (Vec<_>, Vec<_>) =
					seeds.iter().partition(|(_, scheme)| *scheme == Scheme::Schnorr);

				let plain: Vec<crate::ledger_9::WalletSeed> =
					schnorr.iter().map(|(s, _)| s.clone()).collect();
				let Self::Ledger8(ctx) = Self::new_from_wallet_seeds(version, network_id, &plain)
				else {
					unreachable!("Ledger8 version builds a Ledger8 context")
				};

				if !ecdsa.is_empty() {
					let ledger_state =
						ctx.ledger_state.lock().expect("failed to lock ledger state").clone();
					let mut wallets = ctx.wallets.lock().expect("failed to lock wallets");
					for (seed, _) in ecdsa {
						let seed_8 = crate::ledger_8::WalletSeed::try_from(seed.as_bytes())
							.expect("ledger seed format should be backwards compatible");
						wallets.insert(
							seed_8.clone(),
							watch_only_ecdsa_wallet_8(seed, seed_8, &ledger_state),
						);
					}
				}
				Self::Ledger8(ctx)
			},
		}
	}

	/// Get the current ledger version.
	pub fn version(&self) -> LedgerVersion {
		match self {
			Self::Ledger8(_) => LedgerVersion::Ledger8,
			Self::Ledger9(_) => LedgerVersion::Ledger9,
		}
	}

	/// Dispatch on the ledger version, passing the inner context to the
	/// appropriate closure.
	pub fn dispatch<T>(
		self,
		f8: impl FnOnce(crate::ledger_8::context::LedgerContext<Db8>) -> T,
		f9: impl FnOnce(crate::ledger_9::context::LedgerContext<Db9>) -> T,
	) -> T {
		match self {
			Self::Ledger8(ctx) => f8(ctx),
			Self::Ledger9(ctx) => f9(ctx),
		}
	}

	/// Extract the inner Ledger8 context, consuming self.
	///
	/// Returns `None` if the context is not Ledger8.
	pub fn into_ledger8(self) -> Option<crate::ledger_8::context::LedgerContext<Db8>> {
		match self {
			Self::Ledger9(_) => None,
			Self::Ledger8(ctx) => Some(ctx),
		}
	}

	// Extract the inner Ledger9 context, consuming self.
	///
	/// Returns `None` if the context is still before Ledger9.
	pub fn into_ledger9(self) -> Option<crate::ledger_9::context::LedgerContext<Db9>> {
		match self {
			Self::Ledger9(ctx) => Some(ctx),
			Self::Ledger8(_) => None,
		}
	}
}

pub fn block_context_from_raw_8(block: &RawBlockData) -> crate::ledger_8::BlockContext {
	crate::ledger_8::make_block_context(
		crate::ledger_8::Timestamp::from_secs(block.tblock_secs),
		crate::ledger_8::HashOutput(block.parent_block_hash),
		crate::ledger_8::Timestamp::from_secs(block.last_block_time_secs),
	)
}

pub fn block_context_from_raw_9(block: &RawBlockData) -> crate::ledger_9::BlockContext {
	crate::ledger_9::make_block_context(
		crate::ledger_9::Timestamp::from_secs(block.tblock_secs),
		crate::ledger_9::HashOutput(block.parent_block_hash),
		crate::ledger_9::Timestamp::from_secs(block.last_block_time_secs),
	)
}

/// Whether the runtime at `spec_version` verifies a block's first ledger transaction against
/// the corrected `tblock` (`parent_block_time + 12s`) rather than the block's own timestamp
/// (<https://github.com/midnightntwrk/midnight-node/issues/1924>).
///
/// On-chain the gate is which host function the runtime imports — v1 of
/// `Ledger8Bridge::apply_transaction` corrects, v2 does not — but that set is closed: v2
/// shipped with #2002 (backported as #2031) and the correction is never coming back, so the
/// range is hard-coded rather than read out of the runtime blob at every height.
///
/// Anything outside it does not correct, including locally built blocks (`spec_version` 0)
/// and unknown future runtimes. A wrong range fails the replay's state-root check at the
/// offending block rather than going quietly astray.
#[allow(clippy::zero_prefixed_literal)]
pub fn skews_tblock(spec_version: u32) -> bool {
	// Mirrors the lower bound of `LedgerVersion::from_spec_version`.
	const FIRST_LEDGER_8: u32 = 000_022_000;
	// The spec bump that shipped v2.
	const FIRST_UNCORRECTED: u32 = 001_000_002;

	(FIRST_LEDGER_8..FIRST_UNCORRECTED).contains(&spec_version)
}

/// Deserialize raw transactions and apply to a Ledger8 context, returning dust events.
pub fn apply_block_8(
	ctx: &crate::ledger_8::context::LedgerContext<Db8>,
	block: &RawBlockData,
) -> Vec<crate::ledger_8::Event<Db8>> {
	use crate::ledger_8::{
		SerdeTransaction, SystemTransaction, midnight_serialize::tagged_deserialize,
	};

	type MnTx8 = crate::ledger_8::Transaction<
		crate::ledger_8::Signature,
		crate::ledger_8::ProofMarker,
		crate::ledger_8::PureGeneratorPedersen,
		Db8,
	>;
	type SerdeTx8 = SerdeTransaction<crate::ledger_8::Signature, crate::ledger_8::ProofMarker, Db8>;

	let t = std::time::Instant::now();
	let mut transactions: Vec<SerdeTx8> = Vec::new();
	for raw_tx in &block.transactions {
		match raw_tx {
			RawTransaction::Midnight(bytes) => {
				let tx: MnTx8 = tagged_deserialize(&mut bytes.as_slice())
					.expect("failed to deserialize ledger 8 midnight transaction");
				transactions.push(SerdeTx8::Midnight(tx));
			},
			RawTransaction::System(bytes) => {
				let tx: SystemTransaction = tagged_deserialize(&mut bytes.as_slice())
					.expect("failed to deserialize ledger 8 system transaction");
				transactions.push(SerdeTx8::System(tx));
			},
		}
	}
	crate::replay_stats::record(&crate::replay_stats::TX_DESERIALIZE_NS, t);

	let block_context = block_context_from_raw_8(block);

	ctx.update_from_block(
		&transactions,
		&block_context,
		block.state_root.as_ref(),
		block.state.as_ref(),
		skews_tblock(block.spec_version),
	)
	.expect("failed to update ledger 8 context from block")
}

/// Deserialize raw transactions and apply to a Ledger9 context, returning dust events.
pub fn apply_block_9(
	ctx: &crate::ledger_9::context::LedgerContext<Db9>,
	block: &RawBlockData,
) -> Vec<crate::ledger_9::Event<Db9>> {
	use crate::ledger_9::{
		SerdeTransaction, SystemTransaction, midnight_serialize::tagged_deserialize,
	};

	type MnTx9 = crate::ledger_9::Transaction<
		crate::ledger_9::Signature,
		crate::ledger_9::ProofMarker,
		crate::ledger_9::PureGeneratorPedersen,
		Db9,
	>;
	type SerdeTx9 = SerdeTransaction<crate::ledger_9::Signature, crate::ledger_9::ProofMarker, Db9>;

	let t = std::time::Instant::now();
	let mut transactions: Vec<SerdeTx9> = Vec::new();
	for raw_tx in &block.transactions {
		match raw_tx {
			RawTransaction::Midnight(bytes) => {
				let tx: MnTx9 = tagged_deserialize(&mut bytes.as_slice())
					.expect("failed to deserialize ledger 9 midnight transaction");
				transactions.push(SerdeTx9::Midnight(tx));
			},
			RawTransaction::System(bytes) => {
				let tx: SystemTransaction = tagged_deserialize(&mut bytes.as_slice())
					.expect("failed to deserialize ledger 9 system transaction");
				transactions.push(SerdeTx9::System(tx));
			},
		}
	}
	crate::replay_stats::record(&crate::replay_stats::TX_DESERIALIZE_NS, t);

	let block_context = block_context_from_raw_9(block);

	// No `skew_tblock` counterpart here: `Ledger9Bridge`'s host functions never skew, because
	// every ledger-9 block was produced after the upgrade that closed the loophole.
	ctx.update_from_block(
		&transactions,
		&block_context,
		block.state_root.as_ref(),
		block.state.as_ref(),
	)
	.expect("failed to update ledger 9 context from block")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ledger_9::{UnshieldedSignatureScheme, UnshieldedWallet, WalletSeed};

	fn seed() -> WalletSeed {
		WalletSeed::Short([0x42; 16])
	}

	/// The runtimes that ran on-chain while `apply_transaction` v1 was the only version.
	#[test]
	#[allow(clippy::zero_prefixed_literal)]
	fn pre_upgrade_ledger_8_runtimes_skew() {
		assert!(skews_tblock(000_022_000));
		assert!(skews_tblock(001_000_000));
	}

	/// 1.0.3 and devnet's 1.0.300 are both built from a tree that already has v2, despite
	/// 1.0.3 reading like the older of the two.
	#[test]
	#[allow(clippy::zero_prefixed_literal)]
	fn post_upgrade_runtimes_do_not_skew() {
		assert!(!skews_tblock(001_000_003));
		assert!(!skews_tblock(001_000_300));
		assert!(!skews_tblock(002_000_000));
		assert!(!skews_tblock(004_000_000));
	}

	/// A block built locally rather than fetched from a chain is never historical.
	#[test]
	fn unknown_and_synthetic_runtimes_do_not_skew() {
		assert!(!skews_tblock(u32::MAX));
		let block = RawBlockData::new_from_timestamp(0, LedgerVersion::Ledger8, Vec::new());
		assert!(!skews_tblock(block.spec_version));
	}

	/// No key is derived pre-fork, and the watched address is the ECDSA one, not the Schnorr one.
	#[test]
	fn ecdsa_seed_is_watch_only_before_the_fork() {
		let ctx = ForkAwareLedgerContext::new_from_wallet_seeds_with_schemes(
			LedgerVersion::Ledger8,
			"undeployed",
			&[(seed(), UnshieldedSignatureScheme::Ecdsa)],
		);
		let ForkAwareLedgerContext::Ledger8(ctx) = ctx else {
			panic!("expected a ledger-8 context")
		};

		let seed_8 = crate::ledger_8::WalletSeed::try_from(seed().as_bytes()).unwrap();
		let wallets = ctx.wallets.lock().unwrap();
		let wallet = wallets.get(&seed_8).expect("the ECDSA seed still gets a wallet");

		assert!(
			wallet.unshielded.maintenance_verifying_key().is_none(),
			"no key material may be derived for an ECDSA identity on ledger 8",
		);

		let ecdsa = UnshieldedWallet::new(seed(), UnshieldedSignatureScheme::Ecdsa).user_address;
		let schnorr =
			UnshieldedWallet::new(seed(), UnshieldedSignatureScheme::Schnorr).user_address;
		assert_ne!(ecdsa.0.0, schnorr.0.0, "the two identities must be distinct");
		assert_eq!(wallet.unshielded.user_address.0.0, ecdsa.0.0, "must watch the ECDSA address");
	}

	/// Shielded and dust identities do not depend on the scheme, so pre-fork history still replays.
	#[test]
	fn ecdsa_seed_keeps_a_usable_shielded_subwallet_before_the_fork() {
		let ctx = ForkAwareLedgerContext::new_from_wallet_seeds_with_schemes(
			LedgerVersion::Ledger8,
			"undeployed",
			&[(seed(), UnshieldedSignatureScheme::Ecdsa)],
		);
		let ForkAwareLedgerContext::Ledger8(ctx) = ctx else {
			panic!("expected a ledger-8 context")
		};

		let seed_8 = crate::ledger_8::WalletSeed::try_from(seed().as_bytes()).unwrap();
		let wallets = ctx.wallets.lock().unwrap();
		let wallet = wallets.get(&seed_8).unwrap();

		let expected = crate::ledger_8::ShieldedWallet::<Db8>::default(seed_8);
		assert_eq!(
			wallet.shielded.coin_public_key, expected.coin_public_key,
			"shielded identity must be the seed's own, so pre-fork offers are picked up",
		);
		let expected_dust = crate::ledger_8::DustWallet::<Db8>::default(
			crate::ledger_8::WalletSeed::try_from(seed().as_bytes()).unwrap(),
			None,
		);
		assert_eq!(
			wallet.dust.public_key, expected_dust.public_key,
			"dust identity must be the seed's own, so dust events replay",
		);
	}
}
