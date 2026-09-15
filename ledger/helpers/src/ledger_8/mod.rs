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

//! Ledger generation 8: the frozen pre-hardfork ledger.
//!
//! Every module under `ledger_8/` binds the v8 ledger crates through the aliases
//! declared below. Its `ledger_9/` counterpart is a separate copy on purpose:
//! `diff -r ledger_8 ledger_9` shows exactly where the two generations diverge,
//! and an edit here cannot leak into v9.

use crate::ContractVerifyingKeyBytes;

pub use crate::CoinSelectionStrategy;
#[cfg(feature = "can-panic")]
pub use crate::extract_tx_with_context::extract_tx_with_context_ledger_8 as extract_tx_with_context;

/// Ledger generation implemented by this module.
pub const LEDGER_VERSION: u32 = 8;
/// The contract-maintenance-authority verifying-key type for this generation. Pre-ledger-9
/// ledgers have no dedicated maintenance-key enum, so it is the plain signature verifying key.
pub type MaintenanceVerifyingKey = SignatureVerifyingKey;
/// Workspace dependency name of the ledger crate backing this module.
pub const CRATE_NAME: &str = "mn-ledger-8";
pub use {
	base_crypto, coin_structure, ledger_storage_ledger_8 as ledger_storage, midnight_serialize,
	mn_ledger_8 as mn_ledger, onchain_runtime_ledger_8 as onchain_runtime, transient_crypto, zkir,
	zswap_ledger_8 as zswap,
};

use midnight_serialize::tagged_deserialize;

// Vendored test-utilities shim for v8.
pub mod test_utilities_local;

mod block_context;
pub use block_context::*;

// ECDSA is only supported from ledger 9; this module is stubs that panic.
mod ecdsa;
pub use ecdsa::{SigningKeyEcdsa, VerifyingKeyEcdsa};

pub use base_crypto::signatures::{
	Signature, Signature as TransactionSignature, SigningKey as TransactionSigningKey,
	VerifyingKey as SignatureVerifyingKey,
};

pub use crate::ledger_8::{
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

/// Builds a contract operation from a verifier key. `_ir_source` is accepted
/// for cross-version call-site compatibility but silently dropped: pre-ledger-9
/// contract operations have no on-chain IR slot.
pub fn contract_operation_new(
	vk: Option<ContractVerifyingKeyBytes>,
	_ir_source: Option<Vec<u8>>,
) -> Result<onchain_runtime::state::ContractOperation, std::io::Error> {
	let vk =
		vk.map(|b| tagged_deserialize(&mut b.0.as_slice()).expect("failed to read verifier key"));
	Ok(onchain_runtime::state::ContractOperation::new(vk))
}

/// Wraps a verifier key in the maintenance-update enum for this ledger generation.
/// Pre-ledger-9 ledgers expose only the `V3` (zk-stdlib v1) variant, which takes the
/// same `transient_crypto::proofs::VerifierKey` this module deserializes.
pub fn contract_operation_versioned_verifier_key(
	vk: Vec<u8>,
) -> Result<mn_ledger::structure::ContractOperationVersionedVerifierKey, std::io::Error> {
	let vk: transient_crypto::proofs::VerifierKey = tagged_deserialize(&mut &vk[..])?;
	Ok(mn_ledger::structure::ContractOperationVersionedVerifierKey::V3(vk))
}

/// The verifier-key slot version for this ledger generation, used when *removing*
/// a key (the entry point alone doesn't say which slot the key lives in).
/// Pre-ledger-9 ledgers only have the `V3` slot, so removals target it. Mirrors
/// `contract_operation_versioned_verifier_key` above.
pub fn contract_operation_version_of(
	_op: &onchain_runtime::state::ContractOperation,
) -> mn_ledger::structure::ContractOperationVersion {
	mn_ledger::structure::ContractOperationVersion::V3
}

pub fn signature_verifying_key(
	key: base_crypto::signatures::VerifyingKey,
) -> SignatureVerifyingKey {
	key
}

pub fn transaction_signing_key(key: &base_crypto::signatures::SigningKey) -> TransactionSigningKey {
	key.clone()
}

pub fn transaction_signature(
	signature: base_crypto::signatures::Signature,
) -> TransactionSignature {
	signature
}

pub fn maintenance_verifying_key(
	key: base_crypto::signatures::VerifyingKey,
) -> SignatureVerifyingKey {
	key
}

pub fn signature_verifying_key_ecdsa(_key: VerifyingKeyEcdsa) -> SignatureVerifyingKey {
	unimplemented!("ecdsa is only supported from ledger 9")
}

pub fn transaction_signing_key_ecdsa(_key: &SigningKeyEcdsa) -> TransactionSigningKey {
	unimplemented!("ecdsa is only supported from ledger 9")
}

pub fn transaction_signature_ecdsa(
	_signature: base_crypto::ecdsa::Signature,
) -> TransactionSignature {
	unimplemented!("ecdsa is only supported from ledger 9")
}

pub fn maintenance_verifying_key_ecdsa(_key: VerifyingKeyEcdsa) -> SignatureVerifyingKey {
	unimplemented!("ecdsa is only supported from ledger 9")
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
