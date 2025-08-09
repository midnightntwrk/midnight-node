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
	data_provider::{FetchMode, MidnightDataProvider, OutputMode},
	fab::AlignedValue,
	hash::{HashOutput, PERSISTENT_HASH_BYTES},
	rng::SplittableRng,
	signatures::{Signature, SigningKey, VerifyingKey},
	time::Timestamp,
};
pub use coin_structure::{
	coin::{
		Info as CoinInfo, NIGHT, Nonce, PublicKey as CoinPublicKey, QualifiedInfo,
		ShieldedTokenType, TokenType, UnshieldedTokenType, UserAddress,
	},
	contract::ContractAddress,
};
pub use midnight_serialize::{
	self as mn_ledger_serialize, Deserializable, NetworkId, Serializable, Version, Versioned,
};
pub use onchain_runtime::{
	HistoricMerkleTree_check_root, HistoricMerkleTree_insert,
	context::{BlockContext, QueryContext},
	cost_model::DUMMY_COST_MODEL,
	error::TranscriptRejected,
	ops::{Key, Op, key},
	result_mode::{ResultModeGather, ResultModeVerify},
	state::{
		ContractMaintenanceAuthority, ContractOperation, ContractState, EntryPointBuf, StateValue,
		stval,
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
	commitment::{Pedersen, PedersenRandomness},
	encryption::PublicKey as EncryptionPublicKey,
	fab::ValueReprAlignedValue,
	merkle_tree::{MerklePath, MerkleTree, leaf_hash},
	proofs::{
		IrSource, KeyLocation, ParamsProver, ParamsProverProvider, ProofPreimage, ProverKey,
		Resolver as ResolverTrait, VerifierKey,
	},
};

pub use mn_ledger::{
	construct::ContractCallPrototype,
	error::{MalformedTransaction, PartitionFailure, TransactionInvalid, TransactionProvingError},
	prove::Resolver,
	semantics::{TransactionContext, TransactionResult},
	structure::{
		ClaimMintTransaction, ContractAction, ContractDeploy, ContractOperationVersion,
		ContractOperationVersionedVerifierKey, DUMMY_TRANSACTION_COST_MODEL, FEE_TOKEN, Intent,
		IntentHash, LedgerState, MaintenanceUpdate, ProofKind, ProofMarker, ProofPreimageMarker,
		ProvingData, SignatureKind, SingleUpdate, StandardTransaction, Transaction,
		TransactionHash, UnshieldedOffer, Utxo, UtxoOutput, UtxoSpend,
	},
	test_utilities::{PUBLIC_PARAMS, Pk, serialize_request_body, test_resolver, verifier_key},
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

/// Serializes a mn_ledger::serialize-able type into bytes
pub fn serialize<T: Serializable>(
	value: &T,
	network_id: NetworkId,
) -> Result<Vec<u8>, std::io::Error> {
	let size = Serializable::serialized_size(value);
	let mut bytes = Vec::with_capacity(size);
	midnight_serialize::serialize(value, &mut bytes, network_id)?;
	Ok(bytes)
}

/// Deserializes a mn_ledger::serialize-able type from bytes
pub fn deserialize<T: Deserializable, H: std::io::Read>(
	bytes: H,
	network_id: NetworkId,
) -> Result<T, std::io::Error> {
	let val: T = midnight_serialize::deserialize(bytes, network_id)?;
	Ok(val)
}

pub fn token_type_decode(input: &str) -> TokenType {
	let bytes = hex::decode(input).expect("Token value should be an hex");

	let tt_bytes: [u8; 32] = bytes.try_into().expect("Token size should be 32 bytes");

	TokenType::Shielded(ShieldedTokenType(HashOutput(tt_bytes)))
}

pub fn extract_info_from_tx_with_context(bytes: &[u8]) -> (Vec<u8>, BlockContext) {
	let network_id = NetworkId::Undeployed;

	let tx_with_context: TransactionWithContext<Signature, ProofMarker, DefaultDB> =
		deserialize(bytes, network_id)
			.unwrap_or_else(|err| panic!("Can't deserialize `TransactionWithContext: {err}"));
	let tx = tx_with_context.tx;
	let block_context = tx_with_context.block_context;
	let serialized_tx = serialize(&tx, network_id)
		.unwrap_or_else(|err| panic!("Can't serialize `Transaction`: {err}"));

	(serialized_tx, block_context)
}
