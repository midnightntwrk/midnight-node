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

/// A ledger-8 wallet for a seed whose NIGHT identity is ECDSA: real shielded and dust
/// sub-wallets (both derive from the root seed alone, so they are scheme-independent) plus a
/// watch-only unshielded sub-wallet holding the seed's ECDSA address and no key material.
///
/// The address is computed with the ledger-9 types because ledger 8's `coin-structure` has no
/// `From<ecdsa::VerifyingKey> for UserAddress`; both generations' `UserAddress` is the same
/// 32-byte hash, so the bytes carry over directly.
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
	/// ECDSA identities are only representable from ledger 9 (see `ledger_8::ecdsa`). On an
	/// earlier generation such a seed gets a **watch-only** wallet at its ECDSA NIGHT address:
	/// no signing key of any kind is derived for it, and in particular not the seed's Schnorr
	/// one — a distinct identity, at a distinct derivation path, that the caller did not ask for.
	/// The unshielded sub-wallet is never read while replaying blocks (shielded replay uses
	/// `shielded`, dust replay uses `dust`), so the seed still accumulates its pre-fork shielded
	/// history; [`crate::ledger_9::context::LedgerContext::install_unshielded_keys`] installs the real
	/// ECDSA key material once the 8->9 fork is crossed.
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

	let block_context = block_context_from_raw_8(block);

	ctx.update_from_block(
		&transactions,
		&block_context,
		block.state_root.as_ref(),
		block.state.as_ref(),
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

	let block_context = block_context_from_raw_9(block);

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

	/// An `ecdsa:` seed must never cause the seed's *Schnorr* signing key to be derived as a
	/// pre-fork stand-in: that is a different identity, at a different derivation path
	/// (`.../0/0` vs `.../4/0`), which the caller did not ask for. On ledger 8 the seed gets a
	/// watch-only wallet at its ECDSA address — no key material at all.
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

	/// The shielded sub-wallet is real regardless: it derives from the root seed alone, so the
	/// watch-only wallet still replays the seed's pre-fork shielded history.
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
