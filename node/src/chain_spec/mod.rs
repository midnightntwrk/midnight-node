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

use crate::cfg::{addresses::Addresses, chain_spec_cfg::ChainSpecCfg};
use midnight_node_res::native_token_observation_consts::{
	TEST_CNIGHT_ASSET_NAME, TEST_CNIGHT_CURRENCY_POLICY_ID, TEST_CNIGHT_REGISTRATIONS_ADDRESS,
};
use midnight_node_res::networks::MidnightNetwork;

use midnight_node_runtime::NativeTokenObservationConfig;
use midnight_node_runtime::{
	AccountId, BalancesConfig, Block, CrossChainPublic, MidnightCall, MidnightConfig,
	NativeTokenManagementConfig, RuntimeCall, RuntimeGenesisConfig,
	SessionCommitteeManagementConfig, SessionConfig, SidechainConfig, Signature, SudoConfig,
	UncheckedExtrinsic, WASM_BINARY, opaque::SessionKeys,
};

use sc_chain_spec::{ChainSpecExtension, GenericChainSpec};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{Encode, H256, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};
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
			ChainSpecInitError::Missing(msg) => write!(f, "ChainSpec Missing error: {}", msg),
			ChainSpecInitError::ParseError(msg) => write!(f, "ChainSpec Parse error: {}", msg),
			ChainSpecInitError::Serialization(msg) => {
				write!(f, "ChainSpec Serialization error: {}", msg)
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
	let addresses = Addresses::load(path).map_err(|e| {
		ChainSpecInitError::ParseError(format!("{} while trying to load {}", e, path))
	})?;

	let err = |var: &str| {
		ChainSpecInitError::ParseError(format!("Failed to parse {} from addresses_json", var))
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

pub fn spec_properties(genesis_tx: &[u8]) -> serde_json::map::Map<String, serde_json::Value> {
	let extrinsic = UncheckedExtrinsic::new_unsigned(RuntimeCall::Midnight(
		MidnightCall::send_mn_transaction {
			midnight_tx: hex::encode(genesis_tx).as_bytes().to_vec(),
		},
	));

	serde_json::json!({
		"genesis_tx": hex::encode(extrinsic.encode()),
	})
	.as_object()
	.expect("Map given; qed")
	.clone()
}

pub fn block_from_hash(block_hash: &str) -> H256 {
	H256::from_slice(&hex::decode(block_hash.replace("0x", "")).unwrap()[..])
}

pub fn chain_config<T: MidnightNetwork>(
	cfg: &ChainSpecCfg,
	genesis: T,
) -> Result<ChainSpec, ChainSpecInitError> {
	let chain_spec_builder = ChainSpec::builder(runtime_wasm(), Default::default())
		.with_name(genesis.name())
		.with_id(genesis.id())
		.with_chain_type(genesis.chain_type())
		.with_properties(spec_properties(genesis.genesis_tx()))
		.with_genesis_config(genesis_config(cfg, genesis)?);

	Ok(chain_spec_builder.build())
}

fn genesis_config<T: MidnightNetwork>(
	cfg: &ChainSpecCfg,
	genesis: T,
) -> Result<serde_json::Value, ChainSpecInitError> {
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

	let config = RuntimeGenesisConfig {
		system: Default::default(),
		balances: BalancesConfig {
			balances: genesis
				.endowed_accounts()
				.into_iter()
				.map(|a| (a.pubkey.into(), a.balance))
				.collect(),
		},
		aura: Default::default(),
		grandpa: Default::default(),
		sudo: SudoConfig { key: genesis.root_key().map(|k| k.into()) },
		midnight: MidnightConfig {
			_config: Default::default(),
			network_id: genesis.network_id() as u8,
			genesis_state_key: midnight_node_ledger::get_root(
				genesis.genesis_state(),
				genesis.network_id(),
			),
		},
		session: SessionConfig {
			initial_validators: authority_keys
				.iter()
				.cloned()
				.map(|keys| (keys.cross_chain.into(), keys.session))
				.collect::<Vec<_>>(),
		},
		sidechain: SidechainConfig {
			// We need to default genesis_utxo if missing so that
			// midnight-node key insert doesn't blow up when called from
			// midnight-node wizards generate-keys
			genesis_utxo: cfg.genesis_utxo.unwrap_or_default(),
			slots_per_epoch: sidechain_slots::SlotsPerEpoch(1200),
			..Default::default()
		},
		session_committee_management: SessionCommitteeManagementConfig {
			initial_authorities: authority_keys
				.iter()
				.cloned()
				.map(|keys| (keys.cross_chain, keys.session))
				.collect::<Vec<_>>(),
			// We need to default main_chain_scripts if missing so that
			// midnight-node key insert doesn't blow up when called from
			// midnight-node wizards generate-keys
			main_chain_scripts: read_mainchain_scripts_from_addresses_json(
				&cfg.addresses_json.clone().unwrap(),
			)
			.unwrap_or_default(),
		},
		tx_pause: Default::default(),
		pallet_session: Default::default(),
		native_token_management: NativeTokenManagementConfig { ..Default::default() },
		native_token_observation: NativeTokenObservationConfig {
			registration_address: TEST_CNIGHT_REGISTRATIONS_ADDRESS.into(),
			token_policy_id: TEST_CNIGHT_CURRENCY_POLICY_ID.into(),
			token_asset_name: TEST_CNIGHT_ASSET_NAME.into(),
			..Default::default()
		},
	};

	Ok(serde_json::to_value(config).expect("Genesis config must be serialized correctly"))
}
