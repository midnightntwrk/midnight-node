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

pub use base_crypto::{
	cost_model::{CostDuration, FeePrices, FixedPoint, RunningCost, SyntheticCost},
	data_provider::{FetchMode, MidnightDataProvider, OutputMode},
	fab::AlignedValue,
	hash::{HashOutput, PERSISTENT_HASH_BYTES, persistent_commit, persistent_hash},
	rng::SplittableRng,
	signatures::{Signature, SigningKey, VerifyingKey},
	time::Timestamp,
};
pub use coin_structure::{
	coin::{
		Info as CoinInfo, NIGHT, Nonce, PublicAddress, PublicKey as CoinPublicKey, QualifiedInfo,
		ShieldedTokenType, TokenType, UnshieldedTokenType, UserAddress,
	},
	contract::ContractAddress,
	transfer::Recipient,
};
pub use midnight_serialize::{self as mn_ledger_serialize, Deserializable, Serializable, Tagged};
pub use onchain_runtime::{
	HistoricMerkleTree_check_root, HistoricMerkleTree_insert,
	context::{BlockContext, ClaimedUnshieldedSpendsKey, Effects as ContractEffects, QueryContext},
	cost_model::CostModel,
	error::TranscriptRejected,
	ops::{Key, Op, key},
	result_mode::{ResultModeGather, ResultModeVerify},
	state::{
		ChargedState, ContractMaintenanceAuthority, ContractOperation, ContractState,
		EntryPointBuf, StateValue, stval,
	},
	transcript::Transcript,
};

pub use ledger_storage::{
	DefaultDB,
	arena::Sp,
	db::DB,
	storage,
	storage::{Array, HashMap as HashMapStorage, default_storage},
};

pub use transient_crypto::{
	commitment::{Pedersen, PedersenRandomness, PureGeneratorPedersen},
	curve::Fr,
	encryption::PublicKey as EncryptionPublicKey,
	fab::ValueReprAlignedValue,
	merkle_tree::{MerklePath, MerkleTree, leaf_hash},
	proofs::{
		KeyLocation, ParamsProver, ParamsProverProvider, ProofPreimage, ProverKey,
		Resolver as ResolverTrait, VerifierKey,
	},
};
pub use zkir::{IrSource, LocalProvingProvider};

pub use mn_ledger::{
	construct::{ContractCallPrototype, PreTranscript, partition_transcripts},
	dust::{
		DUST_EXPECTED_FILES, DustActions, DustPublicKey, DustRegistration, DustResolver,
		DustSecretKey, InitialNonce,
	},
	error::{
		BlockLimitExceeded, FeeCalculationError, MalformedTransaction, PartitionFailure,
		SystemTransactionError, TransactionInvalid, TransactionProvingError,
	},
	prove::Resolver,
	semantics::{TransactionContext, TransactionResult},
	structure::{
		CNightGeneratesDustActionType, CNightGeneratesDustEvent, ClaimKind,
		ClaimRewardsTransaction, ContractAction, ContractDeploy, ContractOperationVersion,
		ContractOperationVersionedVerifierKey, FEE_TOKEN, Intent, IntentHash, LedgerParameters,
		LedgerState, MaintenanceUpdate, OutputInstructionUnshielded, ProofKind, ProofMarker,
		ProofPreimageMarker, SignatureKind, SingleUpdate, StandardTransaction, SystemTransaction,
		Transaction, TransactionCostModel, TransactionHash, UnshieldedOffer, Utxo, UtxoOutput,
		UtxoSpend, VerifiedTransaction,
	},
	test_utilities::{PUBLIC_PARAMS, Pk, test_resolver, verifier_key},
	verify::WellFormedStrictness,
};
pub use rand::{
	Rng, SeedableRng,
	rngs::{OsRng, StdRng},
};
pub use rand_chacha::ChaCha20Rng;
pub use zswap::{
	Delta, Input, Offer, Output, Transient, ZSWAP_EXPECTED_FILES,
	error::OfferCreationFailed,
	keys::{SecretKeys, Seed},
	local::State as WalletState,
	prove::ZswapResolver,
};

mod context;
mod contract;
mod input;
mod intent;
mod offer;
mod output;
mod proving;
mod transaction;
mod transient;
mod types;
mod unshielded_offer;
mod utxo_output;
mod utxo_spend;
mod wallet;

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

#[repr(u8)]
#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
//#[cfg_attr(feature = "proptest", derive(Arbitrary))]
pub enum NetworkId {
	Undeployed = 0,
	DevNet = 1,
	TestNet = 2,
	MainNet = 3,
}

impl From<NetworkId> for String {
	fn from(value: NetworkId) -> Self {
		match value {
			NetworkId::Undeployed => "undeployed",
			NetworkId::DevNet => "devnet",
			NetworkId::TestNet => "testnet",
			NetworkId::MainNet => "mainnet",
		}
		.to_string()
	}
}

impl Serializable for NetworkId {
	fn serialize(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
		Serializable::serialize(&(*self as u8), writer)
	}

	fn serialized_size(&self) -> usize {
		Serializable::serialized_size(&(*self as u8))
	}
}

impl Deserializable for NetworkId {
	fn deserialize(reader: &mut impl std::io::Read, recursion_depth: u32) -> std::io::Result<Self> {
		let discriminant = u8::deserialize(reader, recursion_depth)?;
		Ok(match discriminant {
			0 => Self::Undeployed,
			1 => Self::DevNet,
			2 => Self::TestNet,
			3 => Self::MainNet,
			_ => {
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					format!("unknown network id: {discriminant}"),
				));
			},
		})
	}
}

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
pub fn serialize<T: Serializable + Tagged>(value: &T) -> Result<Vec<u8>, std::io::Error> {
	let size = midnight_serialize::tagged_serialized_size(value);
	let mut bytes = Vec::with_capacity(size);
	midnight_serialize::tagged_serialize(value, &mut bytes)?;
	Ok(bytes)
}

/// Deserializes a mn_ledger::serialize-able type from bytes
pub fn deserialize<T: Deserializable + Tagged, H: std::io::Read>(
	bytes: H,
) -> Result<T, std::io::Error> {
	let val: T = midnight_serialize::tagged_deserialize(bytes)?;
	Ok(val)
}

pub fn token_type_decode(input: &str) -> TokenType {
	let bytes = hex::decode(input).expect("Token value should be an hex");

	let tt_bytes: [u8; 32] = bytes.try_into().expect("Token size should be 32 bytes");

	TokenType::Shielded(ShieldedTokenType(HashOutput(tt_bytes)))
}

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
