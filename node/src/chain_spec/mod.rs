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

use crate::cfg::addresses::Addresses;
use midnight_node_ledger_helpers::mn_ledger_serialize::tagged_deserialize;
use midnight_node_res::networks::MidnightNetwork;
use serde_valid::Validate as _;

use midnight_node_ledger_helpers::{
	BlockContext, DefaultDB, ProofMarker, Signature as LedgerSignature, TransactionWithContext,
	serialize,
};

use midnight_node_runtime::{
	AccountId, Block, CouncilConfig, CouncilMembershipConfig, CrossChainPublic, MidnightCall,
	MidnightConfig, MidnightSystemCall, RuntimeCall, RuntimeGenesisConfig,
	SessionCommitteeManagementConfig, SessionConfig, SidechainConfig, Signature, SudoConfig,
	TechnicalCommitteeConfig, TechnicalCommitteeMembershipConfig, UncheckedExtrinsic, WASM_BINARY,
	opaque::SessionKeys,
};
use midnight_node_runtime::{
	BeefyConfig, CNightObservationCall, CNightObservationConfig, TimestampCall,
};

use midnight_primitives_cnight_observation::ObservedUtxos;
use sc_chain_spec::{ChainSpecExtension, GenericChainSpec};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{Encode, H256, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, One, Verify};
use sp_session_validator_management::MainChainScripts;
use std::fmt;

pub enum ChainSpecInitError {
	Missing(String),
	ParseError(String),
	Serialization(String),
}

impl fmt::Display for ChainSpecInitError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			ChainSpecInitError::Missing(msg) => write!(f, "ChainSpec Missing error: {msg}"),
			ChainSpecInitError::ParseError(msg) => write!(f, "ChainSpec Parse error: {msg}"),
			ChainSpecInitError::Serialization(msg) => {
				write!(f, "ChainSpec Serialization error: {msg}")
			},
		}
	}
}

#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
	/// Block numbers with known hashes.
	pub fork_blocks: sc_client_api::ForkBlocks<Block>,
	/// Known bad block hashes.
	pub bad_blocks: sc_client_api::BadBlocks<Block>,
}

pub type ChainSpec = GenericChainSpec<Extensions>;

#[derive(Clone, Debug, PartialEq, sp_runtime::Serialize)]
pub struct AuthorityKeys {
	pub session: SessionKeys,
	pub cross_chain: CrossChainPublic,
}

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(seed, None)
		.expect("static values are valid; qed")
		.public()
}

type AccountPublic = <Signature as Verify>::Signer;

pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

pub fn authority_keys_from_seed(s: &str) -> AuthorityKeys {
	AuthorityKeys {
		session: SessionKeys {
			aura: get_from_seed::<AuraId>(s),
			grandpa: get_from_seed::<GrandpaId>(s),
		},
		cross_chain: get_from_seed::<CrossChainPublic>(s),
	}
}

pub fn runtime_wasm() -> &'static [u8] {
	WASM_BINARY.expect("Runtime wasm not available")
}

pub fn read_mainchain_scripts_from_addresses_json(
	path: &str,
) -> Result<MainChainScripts, ChainSpecInitError> {
	let addresses = Addresses::load(path)
		.map_err(|e| ChainSpecInitError::ParseError(format!("{e} while trying to load {path}")))?;

	let err = |var: &str| {
		ChainSpecInitError::ParseError(format!("Failed to parse {var} from addresses_json"))
	};

	Ok(MainChainScripts {
		committee_candidate_address: addresses
			.addresses
			.committee_candidate_validator
			.parse()
			.map_err(|_| err("committee_candidate_validator"))?,
		d_parameter_policy_id: addresses
			.policy_ids
			.d_parameter
			.parse()
			.map_err(|_| err("d_parameter"))?,
		permissioned_candidates_policy_id: addresses
			.policy_ids
			.permissioned_candidates
			.parse()
			.map_err(|_| err("permissioned_candidates"))?,
	})
}

pub fn get_chainspec_extrinsics(
	genesis_block: &[u8],
	observed_utxos_cnight: &ObservedUtxos,
) -> Vec<String> {
	let txs: Vec<TransactionWithContext<LedgerSignature, ProofMarker, DefaultDB>> =
		tagged_deserialize(&mut &genesis_block[..]).expect("Failed deserializing genesis block");

	let mut extrinsics: Vec<String> = Vec::with_capacity(txs.len());

	let mut block_context: Option<BlockContext> = None;

	for tx in txs {
		match tx.tx {
			midnight_node_ledger_helpers::SerdeTransaction::Midnight(transaction) => {
				let serialized_tx =
					serialize(&transaction).expect("failed to serialize transaction");
				let extrinsic = UncheckedExtrinsic::new_bare(RuntimeCall::Midnight(
					MidnightCall::send_mn_transaction { midnight_tx: serialized_tx },
				));
				extrinsics.push(hex::encode(extrinsic.encode()));
			},
			midnight_node_ledger_helpers::SerdeTransaction::System(system_transaction) => {
				let midnight_system_tx =
					serialize(&system_transaction).expect("failed to serialize system transaction");
				let extrinsic = UncheckedExtrinsic::new_bare(RuntimeCall::MidnightSystem(
					MidnightSystemCall::send_mn_system_transaction { midnight_system_tx },
				));
				extrinsics.push(hex::encode(extrinsic.encode()));
			},
		}
		if let Some(ref block_context) = block_context {
			if block_context.tblock != tx.block_context.tblock {
				panic!("Transactions in genesis block contain differing block contexts");
			}
		} else {
			block_context = Some(tx.block_context);
		}
	}

	// Add Timestamp Set extrinsic
	let timestamp_extrinsic =
		UncheckedExtrinsic::new_bare(RuntimeCall::Timestamp(TimestampCall::set {
			now: block_context.expect("missing block context").tblock.to_secs() * 1000,
		}));
	extrinsics.push(hex::encode(timestamp_extrinsic.encode()));

	// Add CNight extrinsic
	if !observed_utxos_cnight.utxos.is_empty() {
		let cnight_extrinsic = UncheckedExtrinsic::new_bare(RuntimeCall::CNightObservation(
			CNightObservationCall::process_tokens {
				utxos: observed_utxos_cnight.utxos.clone(),
				next_cardano_position: observed_utxos_cnight.end,
			},
		));
		extrinsics.push(hex::encode(cnight_extrinsic.encode()));
	}

	extrinsics
}

pub fn get_chainspec_properties(
	genesis_block: &[u8],
	genesis_state: &[u8],
	observed_utxos_cnight: &ObservedUtxos,
) -> serde_json::map::Map<String, serde_json::Value> {
	serde_json::json!({
		"genesis_extrinsics": get_chainspec_extrinsics(genesis_block, observed_utxos_cnight),
		"genesis_state": hex::encode(genesis_state),
	})
	.as_object()
	.expect("Map given; qed")
	.clone()
}

pub fn block_from_hash(block_hash: &str) -> H256 {
	H256::from_slice(&hex::decode(block_hash.replace("0x", "")).unwrap()[..])
}

pub fn chain_config<T: MidnightNetwork>(genesis: T) -> Result<ChainSpec, ChainSpecInitError> {
	let chain_spec_builder = ChainSpec::builder(runtime_wasm(), Default::default())
		.with_name(genesis.name())
		.with_id(genesis.id())
		.with_chain_type(genesis.chain_type())
		.with_properties(get_chainspec_properties(
			genesis.genesis_block(),
			genesis.genesis_state(),
			&genesis.cnight_genesis().observed_utxos,
		))
		.with_genesis_config(genesis_config(genesis)?);

	Ok(chain_spec_builder.build())
}

fn genesis_config<T: MidnightNetwork>(genesis: T) -> Result<serde_json::Value, ChainSpecInitError> {
	let authority_keys = genesis
		.initial_authorities()
		.into_iter()
		.map(|keys| AuthorityKeys {
			session: SessionKeys {
				aura: keys.aura_pubkey.into(),
				grandpa: keys.grandpa_pubkey.into(),
			},
			cross_chain: keys.crosschain_pubkey.into(),
		})
		.collect::<Vec<_>>();

	let cnight_genesis = genesis.cnight_genesis();
	cnight_genesis.validate().map_err(|e| {
		ChainSpecInitError::ParseError(format!("failed to validate cnight genesis config: {e}"))
	})?;

	let config = RuntimeGenesisConfig {
		system: Default::default(),
		aura: Default::default(),
		beefy: BeefyConfig {
			authorities: genesis
				.initial_authorities()
				.iter()
				.map(|v| v.beefy_pubkey.into())
				.collect(),
			genesis_block: Some(One::one()),
		},
		governed_map: pallet_governed_map::GenesisConfig {
			main_chain_scripts: genesis.main_chain_scripts().into(),
			_marker: std::marker::PhantomData,
		},
		grandpa: Default::default(),
		sudo: SudoConfig { key: genesis.root_key().map(|k| k.into()) },
		midnight: MidnightConfig {
			_config: Default::default(),
			network_id: genesis.id().to_string(),
			genesis_state_key: midnight_node_ledger::get_root(genesis.genesis_state()),
		},
		session: SessionConfig {
			initial_validators: authority_keys
				.iter()
				.cloned()
				.map(|keys| (keys.cross_chain.into(), keys.session))
				.collect::<Vec<_>>(),
		},
		sidechain: SidechainConfig {
			genesis_utxo: std::str::FromStr::from_str(genesis.genesis_utxo())
				.expect("failed to convert genesis_utxo"),
			slots_per_epoch: sidechain_slots::SlotsPerEpoch(300),
			..Default::default()
		},
		session_committee_management: SessionCommitteeManagementConfig {
			initial_authorities: authority_keys
				.iter()
				.cloned()
				.map(|keys| (keys.cross_chain, keys.session).into())
				.collect::<Vec<_>>(),
			main_chain_scripts: genesis.main_chain_scripts().into(),
		},
		tx_pause: Default::default(),
		pallet_session: Default::default(),
		c_night_observation: CNightObservationConfig {
			config: cnight_genesis,
			_marker: Default::default(),
		},
		council: CouncilConfig { ..Default::default() },
		council_membership: CouncilMembershipConfig {
			members: genesis
				.council()
				.members
				.iter()
				.cloned()
				.map(|key| key.into())
				.collect::<Vec<AccountId>>()
				.try_into()
				.expect("Too many members to initialize 'council_membership'"),
			..Default::default()
		},
		technical_committee: TechnicalCommitteeConfig { ..Default::default() },
		technical_committee_membership: TechnicalCommitteeMembershipConfig {
			members: genesis
				.technical_committee()
				.members
				.iter()
				.cloned()
				.map(|key| key.into())
				.collect::<Vec<AccountId>>()
				.try_into()
				.expect("Too many members to initialize 'technical_committee_membership'"),
			..Default::default()
		},
	};

	Ok(serde_json::to_value(config).expect("Genesis config must be serialized correctly"))
}
