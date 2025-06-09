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

use std::collections::HashMap;

use super::{
	base_crypto_local, ledger_storage_local, midnight_serialize_local, mn_ledger_local,
	transient_crypto_local,
};

use base_crypto_local::time::Timestamp;
use ledger_storage_local::db::DB;
use midnight_serialize_local::{Deserializable, Serializable, Version, Versioned};
use transient_crypto_local::commitment::PedersenRandomness;

use mn_ledger_local::{
	error::MalformedTransaction,
	structure::{
		ClaimMintTransaction, ContractAction, LedgerState, ProofKind, ProofMarker, SignatureKind,
		StandardTransaction, Transaction as Tx, Utxo, UtxoOutput, UtxoSpend,
	},
};
use std::borrow::Borrow;

use super::{
	ContractAddress, DeserializableError, LOG_TARGET, Ledger, LedgerParameters, SerializableError,
	TransactionIdentifier, TransactionInvalid,
	types::{DeserializationError, LedgerApiError, SerializationError, TransactionError},
};
use crate::{
	common::types::{BlockContext, Hash, SegmentId, UtxoInfo},
	types::PERSISTENT_HASH_BYTES,
};

pub type InnerTx<S, D> = Tx<S, ProofMarker, PedersenRandomness, D>;

#[derive(Clone, Debug, Serializable)]
pub(crate) struct Transaction<S: SignatureKind<D>, D: DB>(pub InnerTx<S, D>);

impl<S: SignatureKind<D>, D: DB> Versioned for Transaction<S, D> {
	const VERSION: Option<Version> = None;
	const NETWORK_SPECIFIC: bool = <InnerTx<S, D> as Versioned>::NETWORK_SPECIFIC;
}

impl<S: SignatureKind<D>, D: DB> Deserializable for Transaction<S, D> {
	fn versioned_deserialize<R: std::io::Read>(
		reader: &mut R,
		_version: Option<&Version>,
		recursion_level: u32,
	) -> Result<Self, std::io::Error> {
		let tx = InnerTx::deserialize(reader, recursion_level)?;
		Ok(Transaction(tx))
	}
}

impl<S: SignatureKind<D>, D: DB> DeserializableError for Transaction<S, D> {
	fn error() -> DeserializationError {
		DeserializationError::Transaction
	}
}

impl<S: SignatureKind<D>, D: DB> Borrow<InnerTx<S, D>> for Transaction<S, D> {
	fn borrow(&self) -> &InnerTx<S, D> {
		&self.0
	}
}

impl<S: SignatureKind<D>, D: DB> SerializableError for InnerTx<S, D> {
	fn error() -> SerializationError {
		SerializationError::UnknownType
	}
}

pub enum ContractActionExt<P: ProofKind<D>, D: DB> {
	ContractAction(ContractAction<P, D>),
	ClaimMint { value: u128, coin_type: Hash },
}

struct UtxoOutputInfo {
	output: UtxoOutput,
	intent_hash: Hash,
	output_no: u32,
}

impl From<UtxoOutputInfo> for UtxoInfo {
	fn from(info: UtxoOutputInfo) -> Self {
		Self {
			address: info.output.owner.0.0,
			token_type: info.output.type_.0.0,
			intent_hash: info.intent_hash,
			value: info.output.value,
			output_no: info.output_no,
		}
	}
}

impl From<UtxoSpend> for UtxoInfo {
	fn from(spend: UtxoSpend) -> Self {
		let utxo = Utxo::from(spend.clone());

		Self {
			address: utxo.owner.0.0,
			token_type: utxo.type_.0.0,
			intent_hash: utxo.intent_hash.0.0,
			value: utxo.value,
			output_no: utxo.output_no,
		}
	}
}

#[derive(Default, Debug)]
pub struct UnshieldedUtxos {
	pub outputs: HashMap<SegmentId, Vec<UtxoInfo>>,
	pub inputs: HashMap<SegmentId, Vec<UtxoInfo>>,
}

impl UnshieldedUtxos {
	pub fn remove_failed_segments<D: DB>(
		&mut self,
		segments: &HashMap<SegmentId, Result<(), TransactionInvalid<D>>>,
	) {
		segments.iter().for_each(|(segment_id, maybe_tx_invalid)| {
			// Remove the failing segments from `outputs` and `inputs`
			if maybe_tx_invalid.is_err() {
				self.outputs.remove(segment_id);
				self.inputs.remove(segment_id);
			}
		});
	}

	pub fn inputs(&self) -> Vec<UtxoInfo> {
		self.inputs.values().flat_map(|utxos| utxos.iter()).cloned().collect()
	}

	pub fn outputs(&self) -> Vec<UtxoInfo> {
		self.outputs.values().flat_map(|utxos| utxos.iter()).cloned().collect()
	}
}

impl<S: SignatureKind<D>, D: DB> Transaction<S, D> {
	#[cfg(not(feature = "runtime-benchmarks"))]
	pub(crate) fn validate(
		&self,
		ledger: &Ledger<D>,
		block_context: &BlockContext,
	) -> Result<(), LedgerApiError> {
		self.0
			.well_formed(
				<Ledger<D> as Borrow<LedgerState<D>>>::borrow(ledger),
				mn_ledger_local::verify::WellFormedStrictness::default(),
				Timestamp::from_secs(block_context.tblock),
			)
			.map_err(|e| {
				log::error!(target: LOG_TARGET, "Error validating Transaction: {:?}", e);
				LedgerApiError::Transaction(TransactionError::Malformed(e.into()))
			})?;

		log::info!(
			target: LOG_TARGET,
			"✅ Validated Midnight transaction {:?}",
			hex::encode(self.hash())
		);

		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn validate(
		&self,
		_ledger: &Ledger<D>,
		_block_context: &BlockContext,
	) -> Result<(), LedgerApiError> {
		Ok(())
	}

	pub(crate) fn hash(&self) -> Hash {
		self.0.transaction_hash().0.0
	}

	pub(crate) fn identifiers(&self) -> impl Iterator<Item = TransactionIdentifier> + '_ {
		self.0.identifiers()
	}

	pub(crate) fn calls_and_deploys(&self) -> impl Iterator<Item = Operation> + '_ {
		let actions = match &self.0 {
			Tx::Standard(tx) => tx
				.actions()
				.map(|(_segment, call)| ContractActionExt::ContractAction(call.clone()))
				.collect(),
			Tx::ClaimMint(ClaimMintTransaction { value, type_, .. }) => {
				vec![ContractActionExt::ClaimMint { value: *value, coin_type: type_.0.0 }]
			},
		};

		actions.into_iter().map(|cd| match cd {
			ContractActionExt::ContractAction(ContractAction::Call(call_data)) => Operation::Call {
				address: call_data.address,
				entry_point: call_data.entry_point.to_vec(),
			},
			ContractActionExt::ContractAction(ContractAction::Deploy(deploy_data)) => {
				Operation::Deploy { address: deploy_data.address() }
			},
			ContractActionExt::ContractAction(ContractAction::Maintain(maintain_data)) => {
				Operation::Maintain { address: maintain_data.address }
			},
			ContractActionExt::ClaimMint { value, coin_type } => {
				Operation::ClaimMint { value, coin_type }
			},
		})
	}

	pub(crate) fn has_guaranteed_coins(&self) -> bool {
		match &self.0 {
			Tx::Standard(StandardTransaction { guaranteed_coins, .. }) => {
				guaranteed_coins.is_some()
			},
			_ => false,
		}
	}

	pub(crate) fn has_fallible_coins(&self) -> bool {
		match &self.0 {
			Tx::Standard(StandardTransaction { fallible_coins, .. }) => {
				fallible_coins.iter().count() > 0
			},
			_ => false,
		}
	}

	#[allow(dead_code)]
	pub(crate) fn fee(&self, params: &LedgerParameters) -> Result<u128, LedgerApiError> {
		self.0.fees(params).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error getting the transaction fee: {:?}", e);
			LedgerApiError::Transaction(TransactionError::Malformed(
				MalformedTransaction::<D>::from(e).into(),
			))
		})
	}

	pub(crate) fn unshielded_utxos(&self) -> UnshieldedUtxos {
		let mut outputs: HashMap<u16, Vec<UtxoInfo>> = HashMap::new();
		let mut inputs: HashMap<u16, Vec<UtxoInfo>> = HashMap::new();

		let utxos = match &self.0 {
			Tx::Standard(tx) => {
				for segment_id in tx.segments() {
					// Guaranteed phase
					if segment_id == 0 {
						for intent in tx.intents.values() {
							let parent = intent.erase_proofs().erase_signatures();
							let intent_hash = parent.intent_hash(segment_id).0.0;

							let utxo_outputs = intent.guaranteed_outputs();
							let outputs_info =
								Self::utxos_info_from_output(utxo_outputs, intent_hash);

							let utxo_inputs = intent.guaranteed_inputs();
							let inputs_info = Self::utxos_info_from_inputs(utxo_inputs);

							outputs.insert(segment_id, outputs_info);
							inputs.insert(segment_id, inputs_info);
						}
					// Fallible phase
					} else if let Some(intent) = tx.intents.get(&segment_id) {
						let parent = intent.erase_proofs().erase_signatures();
						let intent_hash = parent.intent_hash(segment_id).0.0;

						let utxo_outputs = intent.fallible_outputs();
						let outputs_info = Self::utxos_info_from_output(utxo_outputs, intent_hash);

						let utxo_inputs = intent.fallible_inputs();
						let inputs_info = Self::utxos_info_from_inputs(utxo_inputs);

						outputs.insert(segment_id, outputs_info);
						inputs.insert(segment_id, inputs_info);
					}
				}
				Some(UnshieldedUtxos { outputs, inputs })
			},
			_ => None,
		};

		utxos.unwrap_or_default()
	}

	pub(crate) fn utxos_info_from_output(
		outputs: Vec<UtxoOutput>,
		intent_hash: [u8; PERSISTENT_HASH_BYTES],
	) -> Vec<UtxoInfo> {
		outputs
			.into_iter()
			.enumerate()
			.map(|(output_no, output)| {
				let utxo_output_info =
					UtxoOutputInfo { output, intent_hash, output_no: output_no as u32 };
				UtxoInfo::from(utxo_output_info)
			})
			.collect()
	}

	pub(crate) fn utxos_info_from_inputs(inputs: Vec<UtxoSpend>) -> Vec<UtxoInfo> {
		inputs.into_iter().map(|utxo_spend| utxo_spend.into()).collect()
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
	Call { address: ContractAddress, entry_point: Vec<u8> },
	Deploy { address: ContractAddress },
	Maintain { address: ContractAddress },
	ClaimMint { value: u128, coin_type: Hash },
}

// grcov-excl-start
#[cfg(test)]
mod tests {
	use super::super::super::super::CRATE_NAME;
	use super::super::super::api;
	use super::*;
	use base_crypto_local::signatures::Signature;
	use ledger_storage_local::DefaultDB;
	use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
	use midnight_serialize_local::NetworkId;

	const NETWORK_ID: NetworkId = NetworkId::Undeployed;
	const DEPLOY: &[u8] = midnight_node_res::undeployed::transactions::DEPLOY_TX;
	const MALFORMED: &[u8] = include_bytes!("../../../../test-data/malformed_tx.json");

	fn prepare_ledger(api: &api::Api) -> Ledger<DefaultDB> {
		let genesis = UndeployedNetwork.genesis_state();

		let ledger = api.deserialize::<Ledger<DefaultDB>>(genesis);
		assert!(ledger.is_ok(), "Can't deserialize ledger from genesis: {}", ledger.unwrap_err());
		ledger.unwrap()
	}

	fn prepare_transaction(api: &api::Api, bytes: &[u8]) -> api::Transaction<Signature, DefaultDB> {
		let tx =
			api.deserialize::<Transaction<Signature, DefaultDB>>(hex::encode(bytes).as_bytes());
		assert!(tx.is_ok(), "Can't deserialize transaction: {}", tx.unwrap_err());
		tx.unwrap()
	}

	#[test]
	#[ignore = "TODO UNSHIELDED"]
	fn should_validate_transaction() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let api = api::new(NETWORK_ID);
		let tx = prepare_transaction(&api, DEPLOY);
		let result = tx.validate(&Ledger::default(), &BlockContext::default());
		assert!(result.is_ok(), "Transaction is invalid: {}", result.unwrap_err());
	}

	#[test]
	#[should_panic]
	fn should_fail_to_deserialize_transaction() {
		let api = api::new(NETWORK_ID);
		let bytes = "Invalid Tx".as_bytes();
		prepare_transaction(&api, bytes);
	}

	#[test]
	#[should_panic]
	fn should_not_validate_malformed_transactin() {
		let api = api::new(NETWORK_ID);
		prepare_transaction(&api, MALFORMED);
	}

	#[test]
	#[ignore = "TODO UNSHIELDED"]
	fn should_extract_identifiers() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let api = api::new(NETWORK_ID);
		let tx = prepare_transaction(&api, DEPLOY);
		let result: Vec<String> = tx
			.identifiers()
			.map(|id| hex::encode(api.serialize(&id).expect("Serialization should work")))
			.collect();
		let set: std::collections::BTreeSet<&String> = result.iter().collect();

		assert_eq!(result.len(), 3);
		assert_eq!(set.len(), 3, "identifiers are not unique");
	}

	#[test]
	#[ignore = "TODO UNSHIELDED"]
	fn should_get_parameters() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let api = api::new(NETWORK_ID);
		let ledger = prepare_ledger(&api);
		let parameters = ledger.get_parameters();
		let tx = prepare_transaction(&api, DEPLOY);

		let fee = tx.fee(&parameters);

		assert!(fee.unwrap() > 0);
	}
}
// grcov-excl-stop
