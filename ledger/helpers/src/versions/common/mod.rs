// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

#[cfg(feature = "std")]
pub use super::{
	base_crypto::{
		cost_model::{CostDuration, FeePrices, FixedPoint, RunningCost, SyntheticCost},
		data_provider::{FetchMode, MidnightDataProvider, OutputMode},
		fab::AlignedValue,
		hash::{HashOutput, PERSISTENT_HASH_BYTES, persistent_commit, persistent_hash},
		rng::SplittableRng,
		signatures::{Signature, SigningKey, VerifyingKey},
		time::{Duration, Timestamp},
	},
	coin_structure::{
		coin::{
			Info as CoinInfo, NIGHT, Nonce, PublicAddress, PublicKey as CoinPublicKey,
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
			ContractOperationVersionedVerifierKey, FEE_TOKEN, Intent, IntentHash, LedgerParameters,
			LedgerState, MaintenanceUpdate, OutputInstructionUnshielded, PedersenDowngradeable,
			ProofKind, ProofMarker, ProofPreimageMarker, SignatureKind, SingleUpdate,
			StandardTransaction, SystemTransaction, Transaction, TransactionCostModel,
			TransactionHash, UnshieldedOffer, Utxo, UtxoOutput, UtxoSpend, VerifiedTransaction,
		},
		test_utilities::{PUBLIC_PARAMS, Pk, ProofServerProvider, test_resolver, verifier_key},
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
		local::State as WalletState,
		prove::ZswapResolver,
	},
};

pub use rand::{
	Rng, SeedableRng,
	rngs::{OsRng, StdRng},
};

pub mod context;
pub mod contract;
mod input;
mod intent;
mod offer;
mod output;
mod proving;
pub mod transaction;
mod transient;
pub mod types;
mod unshielded_offer;
mod utxo_output;
mod utxo_spend;
pub mod wallet;

pub use context::*;
pub use contract::*;
pub use input::*;
pub use intent::*;
pub use offer::*;
pub use output::*;
pub use proving::*;
pub use transaction::*;
pub use transient::*;
pub use types::*;
pub use unshielded_offer::*;
pub use utxo_output::*;
pub use utxo_spend::*;
pub use wallet::*;

/// Serializes a mn_ledger::serialize-able type into bytes
pub fn serialize_untagged<T: Serializable>(value: &T) -> Result<Vec<u8>, std::io::Error> {
	let size = Serializable::serialized_size(value);
	let mut bytes = Vec::with_capacity(size);
	T::serialize(value, &mut bytes)?;
	Ok(bytes)
}

/// Deserializes a mn_ledger::serialize-able type from bytes
pub fn deserialize_untagged<T: Deserializable + Tagged>(
	mut bytes: impl std::io::Read,
) -> Result<T, std::io::Error> {
	let val: T = T::deserialize(&mut bytes, 0)?;
	Ok(val)
}

/// Serializes a mn_ledger::serialize-able type into bytes
#[cfg(feature = "std")]
pub fn serialize<T: Serializable + Tagged>(value: &T) -> Result<Vec<u8>, std::io::Error> {
	let size = mn_ledger_serialize::tagged_serialized_size(value);
	let mut bytes = Vec::with_capacity(size);
	mn_ledger_serialize::tagged_serialize(value, &mut bytes)?;
	Ok(bytes)
}

/// Deserializes a mn_ledger::serialize-able type from bytes
#[cfg(feature = "std")]
pub fn deserialize<T: Deserializable + Tagged, H: std::io::Read>(
	bytes: H,
) -> Result<T, std::io::Error> {
	let val: T = mn_ledger_serialize::tagged_deserialize(bytes)?;
	Ok(val)
}

#[cfg(all(feature = "std", feature = "can-panic"))]
pub fn token_type_decode(input: &str) -> TokenType {
	let bytes = hex::decode(input).expect("Token value should be an hex");

	let tt_bytes: [u8; 32] = bytes.try_into().expect("Token size should be 32 bytes");

	TokenType::Shielded(ShieldedTokenType(HashOutput(tt_bytes)))
}

#[cfg(all(feature = "std", feature = "can-panic"))]
pub fn extract_info_from_tx_with_context(bytes: &[u8]) -> (Vec<u8>, BlockContext) {
	let tx_with_context: TransactionWithContext<Signature, ProofMarker, DefaultDB> =
		deserialize(bytes)
			.unwrap_or_else(|err| panic!("Can't deserialize `TransactionWithContext: {err}"));
	let SerdeTransaction::Midnight(tx) = tx_with_context.tx else {
		panic!("expected test to run against midnight transaction");
	};
	let block_context = tx_with_context.block_context;
	let serialized_tx =
		serialize(&tx).unwrap_or_else(|err| panic!("Can't serialize `Transaction`: {err}"));

	(serialized_tx, block_context)
}
