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

use base_crypto::{signatures::Signature, time::Timestamp};
use ledger_storage::DefaultDB;
use midnight_node_res::networks::MidnightNetwork;
use midnight_node_res::networks::QanetNetwork;
use midnight_serialize::{NetworkId, Serializable};
use mn_ledger::{
	semantics::TransactionContext,
	structure::{LedgerState, ProofMarker, Transaction},
};
use pretty_assertions::assert_eq;
use sha2::{Digest, Sha256};
use transient_crypto::commitment::PedersenRandomness;

use midnight_node_res::{
	deserialize_mn,
	networks::{DevnetNetwork, Testnet02Network, UndeployedNetwork},
};

type Tx = Transaction<Signature, ProofMarker, PedersenRandomness, DefaultDB>;

fn hash(data: &[u8]) -> Vec<u8> {
	let mut hasher = Sha256::new();
	hasher.update(data);
	hasher.finalize().to_vec()
}

fn check_integrity<T: std::io::Read>(
	genesis_tx: T,
	expected_state: &[u8],
	expected_network_id: NetworkId,
) {
	let tx_context = TransactionContext::default();
	let tx: Tx = deserialize_mn(genesis_tx, expected_network_id)
		.expect("failed to deserialize transaction from input");
	let (mut ledger_state, result) = tx_context.ref_state.apply(&tx, &tx_context);
	ledger_state = ledger_state.post_block_update(Timestamp::from_secs(0));

	assert!(
		matches!(result, mn_ledger::semantics::TransactionResult::Success),
		"failed to apply genesis transaction {:?}",
		result
	);

	let size = Serializable::serialized_size(&ledger_state);
	let mut calculated_state_bytes = Vec::with_capacity(size);
	midnight_serialize::serialize(&ledger_state, &mut calculated_state_bytes, expected_network_id)
		.expect("failed to serialize ledger state");

	let expected_state: LedgerState<DefaultDB> =
		deserialize_mn(expected_state, expected_network_id).unwrap();

	// We can't compare binary representations as we can't serialize to old formats.
	// decode(encode(state)) != encode(decode(old_state))
	assert_eq!(ledger_state, expected_state);
}

fn read_genesis_tx(chain_spec_filename: &str) -> Vec<u8> {
	let chain_spec_file =
		std::fs::File::open(chain_spec_filename).expect("failed to open devnet chainspec");
	let chain_spec: serde_json::Value =
		serde_json::from_reader(chain_spec_file).expect("failed to parser chainspec as json");

	let genesis_tx_value = ["properties", "genesis_tx"]
		.iter()
		.fold(&chain_spec, |value, key| value.get(key).unwrap());

	let genesis_tx_str = genesis_tx_value.as_str().unwrap();

	hex::decode(hex::decode(&genesis_tx_str[22..]).expect("failed to decode genesis tx"))
		.expect("failed to decode genesis tx")
}

// Checks that the genesis in the chainstate files matches the genesis.mn files
#[test]
fn check_chainstate_genesis_integrity_devnet() {
	let genesis_tx = read_genesis_tx("devnet/chain-spec.json");
	assert_eq!(hash(&genesis_tx), hash(DevnetNetwork.genesis_tx()));
}

#[test]
fn check_chainstate_genesis_integrity_qanet() {
	let genesis_tx = read_genesis_tx("qanet/chain-spec.json");
	assert_eq!(hash(&genesis_tx), hash(QanetNetwork.genesis_tx()));
}

#[test]
fn check_chainstate_genesis_integrity_testnet_02() {
	let genesis_tx = read_genesis_tx("testnet-02/chain-spec.json");

	assert_eq!(hash(&genesis_tx), hash(Testnet02Network.genesis_tx()));
}

#[test]
fn check_genesis_file_integrity_undeployed() {
	let genesis_state = UndeployedNetwork.genesis_state();
	let genesis_tx = UndeployedNetwork.genesis_tx();
	let network_id = NetworkId::Undeployed;

	check_integrity(genesis_tx, &genesis_state, network_id);
}

#[test]
fn check_genesis_file_integrity_devnet() {
	let genesis_state = DevnetNetwork.genesis_state();
	let genesis_tx = DevnetNetwork.genesis_tx();
	let network_id = NetworkId::DevNet;

	check_integrity(genesis_tx, &genesis_state, network_id);
}

#[test]
fn check_genesis_file_integrity_testnet_02() {
	let genesis_state = Testnet02Network.genesis_state();
	let genesis_tx = Testnet02Network.genesis_tx();
	let network_id = NetworkId::TestNet;

	check_integrity(genesis_tx, &genesis_state, network_id);
}
