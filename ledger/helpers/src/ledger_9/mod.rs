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

//! Ledger generation 9: the post-hardfork ledger.
//!
//! Every module under `ledger_9/` binds the v9 ledger crates through the aliases
//! declared below. Its `ledger_8/` counterpart is a separate copy on purpose:
//! `diff -r ledger_8 ledger_9` shows exactly where the two generations diverge,
//! and an edit here cannot leak into v8.

use crate::ContractVerifyingKeyBytes;

pub use crate::CoinSelectionStrategy;
#[cfg(feature = "can-panic")]
pub use crate::extract_tx_with_context::extract_tx_with_context_ledger_9 as extract_tx_with_context;

/// Ledger generation implemented by this module.
pub const LEDGER_VERSION: u32 = 9;
/// The contract-maintenance-authority verifying-key type for this generation: from ledger 9 the
/// ledger carries a dedicated Schnorr/ECDSA maintenance-key enum.
pub type MaintenanceVerifyingKey = ContractMaintenanceVerifyingKey;
/// Workspace dependency name of the ledger crate backing this module.
pub const CRATE_NAME: &str = "mn-ledger-9";
pub use {
	base_crypto, coin_structure_ledger_9 as coin_structure,
	ledger_storage_ledger_8 as ledger_storage, midnight_serialize, mn_ledger_9 as mn_ledger,
	onchain_runtime_ledger_9 as onchain_runtime, transient_crypto_ledger_9 as transient_crypto,
	zkir, zswap_ledger_9 as zswap,
};

use midnight_serialize::{peek_tag, tagged_deserialize};

// The v9 ledger ships its own test utilities; v8 vendors a shim of the same name.
pub mod test_utilities_local;

mod block_context;
pub use block_context::*;

// ECDSA is natively supported from ledger 9.
mod ecdsa;
pub use ecdsa::{SigningKeyEcdsa, VerifyingKeyEcdsa};

// Ledger-9-only ECDSA wallet tests (no v8 counterpart); see the module docs.
// `can-panic`-gated, as the whole `wallet` module (hence `UnshieldedWallet`) is.
#[cfg(all(test, feature = "can-panic"))]
mod ecdsa_wallet_tests;

pub use mn_ledger::structure::{
	Signature, Signature as TransactionSignature, SignatureVerifyingKey,
	SigningKey as TransactionSigningKey,
};
pub use onchain_runtime::state::ContractMaintenanceVerifyingKey;

pub use crate::ledger_9::{
	base_crypto::{
		cost_model::{
			CostDuration, FeePrices, FixedPoint, NormalizedCost, RunningCost, SyntheticCost,
		},
		data_provider::{FetchMode, MidnightDataProvider, OutputMode},
		fab::AlignedValue,
		hash::{HashOutput, PERSISTENT_HASH_BYTES, persistent_commit, persistent_hash},
		rng::SplittableRng,
		signatures::{
			SigningKey, SigningKey as SigningKeySchnorr, VerifyingKey,
			VerifyingKey as VerifyingKeySchnorr,
		},
		time::{Duration, Timestamp},
	},
	coin_structure::{
		coin::{
			Info as CoinInfo, NIGHT, Nonce, Nullifier, PublicAddress, PublicKey as CoinPublicKey,
			QualifiedInfo, ShieldedTokenType, TokenType, UnshieldedTokenType, UserAddress,
		},
		contract::ContractAddress,
		transfer::Recipient,
	},
	ledger_storage::{
		self as mn_ledger_storage, DefaultDB, Storable,
		arena::{ArenaKey, Sp},
		db::DB,
		storable::Loader,
		storage,
		storage::{Array, HashMap as HashMapStorage, HashSet, default_storage},
	},
	midnight_serialize::{self as mn_ledger_serialize, Deserializable, Serializable, Tagged},
	mn_ledger::{
		construct::{ContractCallPrototype, PreTranscript, partition_transcripts},
		dust::{
			DUST_EXPECTED_FILES, DustActions, DustGenerationInfo, DustLocalState, DustNullifier,
			DustOutput, DustParameters, DustPublicKey, DustRegistration, DustResolver,
			DustSecretKey, DustSpend, DustSpendError as MnLedgerDustSpendError, InitialNonce,
			QualifiedDustOutput,
		},
		error::{
			BlockLimitExceeded, EventReplayError, FeeCalculationError, MalformedTransaction,
			PartitionFailure, SystemTransactionError, TransactionInvalid, TransactionProvingError,
		},
		events::Event,
		prove::Resolver,
		semantics::{TransactionContext, TransactionResult},
		structure::{
			BindingKind, CNightGeneratesDustActionType, CNightGeneratesDustEvent, ClaimKind,
			ClaimRewardsTransaction, ContractAction, ContractDeploy, ContractOperationVersion,
			ContractOperationVersionedVerifierKey, FEE_TOKEN, INITIAL_PARAMETERS, Intent,
			IntentHash, LedgerParameters, LedgerState, MAX_SUPPLY, MaintenanceUpdate,
			OutputInstructionUnshielded, PedersenDowngradeable, ProofKind, ProofMarker,
			ProofPreimageMarker, SignatureKind, SingleUpdate, StandardTransaction,
			SystemTransaction, Transaction, TransactionCostModel, TransactionHash, UnshieldedOffer,
			Utxo, UtxoOutput, UtxoSpend, VerifiedTransaction,
		},
		verify::WellFormedStrictness,
	},
	onchain_runtime::{
		HistoricMerkleTree_check_root, HistoricMerkleTree_insert,
		context::{
			BlockContext, ClaimedUnshieldedSpendsKey, Effects as ContractEffects, QueryContext,
		},
		cost_model::CostModel,
		error::TranscriptRejected,
		ops::{Key, Op, key},
		result_mode::{ResultModeGather, ResultModeVerify},
		state::{
			ChargedState, ContractMaintenanceAuthority, ContractOperation, ContractState,
			EntryPointBuf, StateValue, stval,
		},
		transcript::Transcript,
	},
	test_utilities_local::{PUBLIC_PARAMS, Pk, ProofServerProvider, test_resolver},
	transient_crypto::{
		commitment::{Pedersen, PedersenRandomness, PureGeneratorPedersen},
		curve::Fr,
		encryption::PublicKey as EncryptionPublicKey,
		fab::ValueReprAlignedValue,
		merkle_tree::{MerklePath, MerkleTree, leaf_hash},
		proofs::{
			KeyLocation, ParamsProver, ParamsProverProvider, ProofPreimage, ProverKey,
			ProvingKeyMaterial, Resolver as ResolverTrait, VerifierKey,
		},
	},
	zkir::{IrSource, LocalProvingProvider},
	zswap::{
		Delta, Input, Offer, Output, Transient, ZSWAP_EXPECTED_FILES,
		error::OfferCreationFailed,
		keys::{SecretKeys, Seed},
		ledger::State as ZswapChainState,
		local::State as WalletState,
		prove::ZswapResolver,
	},
};

pub use rand::{
	Rng, SeedableRng,
	rngs::{OsRng, StdRng},
};

// Module declarations with can-panic feature
#[cfg(feature = "can-panic")]
pub mod block_data;
#[cfg(feature = "can-panic")]
pub mod context;
#[cfg(feature = "can-panic")]
pub mod contract;
#[cfg(feature = "can-panic")]
mod input;
#[cfg(feature = "can-panic")]
mod intent;
#[cfg(feature = "can-panic")]
mod network_id;
#[cfg(feature = "can-panic")]
mod offer;
#[cfg(feature = "can-panic")]
mod output;
#[cfg(feature = "can-panic")]
pub mod transaction;
#[cfg(feature = "can-panic")]
mod transient;
#[cfg(feature = "can-panic")]
mod unshielded_offer;
#[cfg(feature = "can-panic")]
mod utxo_output;
#[cfg(feature = "can-panic")]
mod utxo_spend;
#[cfg(feature = "can-panic")]
pub mod wallet;

// Module declarations without can-panic feature
mod proving;
pub mod types;

// Re-exports with can-panic feature
#[cfg(feature = "can-panic")]
pub use {
	context::*, contract::*, input::*, intent::*, network_id::*, offer::*, output::*, proving::*,
	transaction::*, transient::*, unshielded_offer::*, utxo_output::*, utxo_spend::*, wallet::*,
};

// Re-exports without can-panic feature
pub use types::*;

/// Builds a contract operation from a verifier key plus, from ledger 9 on,
/// the circuit's zkir. `ir_source` is stored on-chain alongside the verifier
/// key so the deployed contract's circuits can later be re-proven/upgraded
/// from chain state alone; it counts toward `max_contract_metadata_size`.
pub fn contract_operation_new(
	vk: Option<ContractVerifyingKeyBytes>,
	ir_source: Option<Vec<u8>>,
) -> Result<onchain_runtime::state::ContractOperation, std::io::Error> {
	let ir =
		ir_source.map(|bytes| ledger_storage::arena::Sp::new(onchain_runtime::state::IrBuf(bytes)));
	let mut op = onchain_runtime::state::ContractOperation::new(None, ir);

	if let Some(vk) = vk {
		let tag = peek_tag(&mut std::io::Cursor::new(&vk.0))?;
		match tag.as_str() {
			"verifier-key[v6]" => op.v2 = Some(tagged_deserialize(&mut &vk.0[..])?),
			"verifier-key[v7]" => op.v3 = Some(tagged_deserialize(&mut &vk.0[..])?),
			_ => panic!("unknown verifier key tag: '{tag}'"),
		}
	}

	Ok(op)
}

/// Wraps a verifier key in the maintenance-update enum for this ledger generation.
/// Ledger 9 accepts either a legacy 2.x (`v6`) key, stored in the `V3` slot via the
/// crate-level (non-ledger-9-aliased) `transient_crypto` — the same 2.x
/// `midnight-transient-crypto` build `op.v2` uses in `contract_operation_new` above —
/// or a 3.x/zk-stdlib-v2 (`v7`) key, stored in the `V4` slot. The tag on the key file
/// itself says which, mirroring the dispatch in `contract_operation_new`.
pub fn contract_operation_versioned_verifier_key(
	vk: Vec<u8>,
) -> Result<mn_ledger::structure::ContractOperationVersionedVerifierKey, std::io::Error> {
	let tag = peek_tag(&mut std::io::Cursor::new(&vk))?;
	match tag.as_str() {
		"verifier-key[v6]" => {
			let vk: ::transient_crypto::proofs::VerifierKey = tagged_deserialize(&mut &vk[..])?;
			Ok(mn_ledger::structure::ContractOperationVersionedVerifierKey::V3(vk))
		},
		"verifier-key[v7]" => {
			let vk: transient_crypto::proofs::VerifierKey = tagged_deserialize(&mut &vk[..])?;
			Ok(mn_ledger::structure::ContractOperationVersionedVerifierKey::V4(vk))
		},
		_ => panic!("unknown verifier key tag: '{tag}'"),
	}
}

/// The verifier-key slot version an *existing* contract operation's key actually lives
/// in (the entry point alone doesn't say which slot). Ledger 9 keys can land in either
/// `V3` (legacy 2.x/v6) or `V4` (3.x/v7, preferred if somehow both are set) depending on
/// what compiled the circuit; removals must target whichever slot is populated, or they
/// fail with `VerifierKeyNotFound`.
pub fn contract_operation_version_of(
	op: &onchain_runtime::state::ContractOperation,
) -> mn_ledger::structure::ContractOperationVersion {
	if op.v3.is_some() {
		mn_ledger::structure::ContractOperationVersion::V4
	} else {
		mn_ledger::structure::ContractOperationVersion::V3
	}
}

pub fn signature_verifying_key(
	key: base_crypto::signatures::VerifyingKey,
) -> SignatureVerifyingKey {
	SignatureVerifyingKey::Schnorr(key)
}

pub fn transaction_signing_key(key: &base_crypto::signatures::SigningKey) -> TransactionSigningKey {
	TransactionSigningKey::Schnorr(key.clone())
}

pub fn transaction_signature(
	signature: base_crypto::signatures::Signature,
) -> TransactionSignature {
	TransactionSignature::Schnorr(signature)
}

pub fn maintenance_verifying_key(
	key: base_crypto::signatures::VerifyingKey,
) -> ContractMaintenanceVerifyingKey {
	ContractMaintenanceVerifyingKey::Schnorr(key)
}

pub fn signature_verifying_key_ecdsa(
	key: base_crypto::ecdsa::VerifyingKey,
) -> SignatureVerifyingKey {
	SignatureVerifyingKey::ECDSA(key)
}

pub fn transaction_signing_key_ecdsa(
	key: &base_crypto::ecdsa::SigningKey,
) -> TransactionSigningKey {
	TransactionSigningKey::ECDSA(key.clone())
}

pub fn transaction_signature_ecdsa(
	signature: base_crypto::ecdsa::Signature,
) -> TransactionSignature {
	TransactionSignature::ECDSA(signature)
}

pub fn maintenance_verifying_key_ecdsa(
	key: base_crypto::ecdsa::VerifyingKey,
) -> ContractMaintenanceVerifyingKey {
	ContractMaintenanceVerifyingKey::ECDSA(key)
}

/// Compatibility trait: L8 `apply` returns `WalletState<D>`, L9 returns `Result<WalletState<D>, _>`.
pub trait IntoWalletState<D: DB + Clone> {
	fn into_wallet_state(self) -> WalletState<D>;
}
impl<D: DB + Clone> IntoWalletState<D> for WalletState<D> {
	fn into_wallet_state(self) -> WalletState<D> {
		self
	}
}
impl<D: DB + Clone, E: std::fmt::Debug> IntoWalletState<D> for Result<WalletState<D>, E> {
	fn into_wallet_state(self) -> WalletState<D> {
		self.expect("wallet state apply failed")
	}
}

/// Raw zkir bytes for circuit `name` (the `zkir/{name}.bzkir` the resolver
/// loads as `ProvingKeyMaterial::ir_source`). Ledger 9+ stores these on-chain
/// in the contract operation so deployed circuits can be re-proven/upgraded
/// from chain state alone; pre-9 `contract_operation_new` ignores them.
pub async fn ir_source(resolver: &Resolver, name: &'static str) -> Option<Vec<u8>> {
	let material = resolver
		.resolve_key(KeyLocation(std::borrow::Cow::Borrowed(name)))
		.await
		.ok()??;
	Some(material.ir_source)
}

/// Resolves a circuit's verifier key by name.
pub async fn verifier_key(
	resolver: &Resolver,
	name: &'static str,
) -> Option<ContractVerifyingKeyBytes> {
	let material = resolver
		.resolve_key(KeyLocation(std::borrow::Cow::Borrowed(name)))
		.await
		.ok()??;
	Some(ContractVerifyingKeyBytes(material.verifier_key))
}

/// Serializes a mn_ledger::serialize-able type into bytes
pub fn serialize_untagged<T: Serializable>(value: &T) -> Result<Vec<u8>, std::io::Error> {
	let size = Serializable::serialized_size(value);
	let mut bytes = Vec::with_capacity(size);
	T::serialize(value, &mut bytes)?;
	Ok(bytes)
}

/// Deserializes a mn_ledger::serialize-able type from bytes
pub fn deserialize_untagged<T: Deserializable>(
	mut bytes: impl std::io::Read,
) -> Result<T, std::io::Error> {
	let val: T = T::deserialize(&mut bytes, 0)?;
	Ok(val)
}

/// Serializes a mn_ledger::serialize-able type into bytes
pub fn serialize<T: Serializable + Tagged>(value: &T) -> Result<Vec<u8>, std::io::Error> {
	let size = mn_ledger_serialize::tagged_serialized_size(value);
	let mut bytes = Vec::with_capacity(size);
	mn_ledger_serialize::tagged_serialize(value, &mut bytes)?;
	Ok(bytes)
}

/// Deserializes a mn_ledger::serialize-able type from bytes
pub fn deserialize<T: Deserializable + Tagged, H: std::io::Read>(
	bytes: H,
) -> Result<T, std::io::Error> {
	let val: T = mn_ledger_serialize::tagged_deserialize(bytes)?;
	Ok(val)
}

/// Computes the overall block fullness as the maximum across all cost dimensions.
///
/// This value is used by the ledger's fee adjustment algorithm to update prices
/// based on block utilization. The overall fullness represents the most congested
/// dimension of the block.
///
/// TODO: Confirm that "max of all dimensions" is the correct semantic for overall
//  fullness. This was inferred from ledger API usage patterns but not explicitly
//  documented.
pub fn compute_overall_fullness(normalized: &NormalizedCost) -> FixedPoint {
	FixedPoint::max(
		FixedPoint::max(
			FixedPoint::max(normalized.read_time, normalized.compute_time),
			normalized.block_usage,
		),
		FixedPoint::max(normalized.bytes_written, normalized.bytes_churned),
	)
}

/// Clamps cost to limits and normalizes, logging an error if clamping was needed.
///
/// `SyntheticCost::normalize()` returns `None` when any dimension exceeds its limit.
/// This function clamps to limits first, ensuring normalization always succeeds and
/// overfull blocks are reported as full (100%) rather than failing.
///
/// Blocks should never exceed limits (validation should prevent this), but if they somehow do,
/// it seems more pragmatic to clamp costs, log error, but not fail.
pub fn clamp_and_normalize(
	cost: &SyntheticCost,
	limits: &SyntheticCost,
	context: &str,
) -> NormalizedCost {
	let clamped = SyntheticCost {
		read_time: cost.read_time.min(limits.read_time),
		compute_time: cost.compute_time.min(limits.compute_time),
		block_usage: cost.block_usage.min(limits.block_usage),
		bytes_written: cost.bytes_written.min(limits.bytes_written),
		bytes_churned: cost.bytes_churned.min(limits.bytes_churned),
	};

	if clamped != *cost {
		log::error!(
			"Fatal: Ledger block limit exceeded (Substrate-Ledger weight mismatch?) in {}, \
			clamping to limits. Original: {:?}, limits: {:?}",
			context,
			cost,
			limits
		);
	}

	clamped
		.normalize(*limits)
		.expect("clamped cost should always normalize successfully")
}

#[cfg(feature = "can-panic")]
pub fn token_type_decode(input: &str) -> TokenType {
	let bytes = hex::decode(input).expect("Token value should be an hex");

	let tt_bytes: [u8; 32] = bytes.try_into().expect("Token size should be 32 bytes");

	TokenType::Shielded(ShieldedTokenType(HashOutput(tt_bytes)))
}

#[cfg(test)]
mod tests {
	use super::*;

	const ONE: FixedPoint = FixedPoint::ONE;

	#[test]
	fn cost_under_limits_normalizes_correctly() {
		let cost = make_cost(50, 100, 200, 300, 400);
		let limits = make_cost(100, 200, 400, 600, 800);
		let half = FixedPoint::from_u64_div(1, 2);

		let normalized = clamp_and_normalize(&cost, &limits, "test");

		assert_eq!(normalized, make_normalized(half, half, half, half, half));
	}

	#[test]
	fn cost_over_the_limits_clamps_correct_dimensions() {
		let cost = make_cost(150, 100, 401, 300, 400);
		let limits = make_cost(100, 200, 400, 600, 800);
		let half = FixedPoint::from_u64_div(1, 2);

		let normalized = clamp_and_normalize(&cost, &limits, "test");

		assert_eq!(normalized, make_normalized(ONE, half, ONE, half, half));
	}

	fn make_cost(read: u64, compute: u64, block: u64, written: u64, churned: u64) -> SyntheticCost {
		SyntheticCost {
			read_time: CostDuration::from_picoseconds(read),
			compute_time: CostDuration::from_picoseconds(compute),
			block_usage: block,
			bytes_written: written,
			bytes_churned: churned,
		}
	}

	fn make_normalized(
		read: FixedPoint,
		compute: FixedPoint,
		block: FixedPoint,
		written: FixedPoint,
		churned: FixedPoint,
	) -> NormalizedCost {
		NormalizedCost {
			read_time: read,
			compute_time: compute,
			block_usage: block,
			bytes_written: written,
			bytes_churned: churned,
		}
	}
}
