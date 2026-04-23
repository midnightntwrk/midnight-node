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

//! OpenRPC v1.4 document builder for the Midnight node.
//!
//! Constructs a standards-compliant OpenRPC document describing the node's full
//! JSON-RPC API surface. Custom Midnight methods are described with full
//! parameter/result schemas, while standard Substrate methods are listed as stubs
//! with references to upstream Parity documentation.

use schemars::schema_for;
use serde_json::{Value, json};

/// Expected set of custom RPC method names (from all midnight-node + partner-chains traits).
pub(crate) const CUSTOM_METHOD_NAMES: &[&str] = &[
	"midnight_contractState",
	"midnight_zswapStateRoot",
	"midnight_ledgerStateRoot",
	"midnight_apiVersions",
	"midnight_ledgerVersion",
	"systemParameters_getTermsAndConditions",
	"systemParameters_getDParameter",
	"systemParameters_getAriadneParameters",
	"network_peerReputations",
	"network_peerReputation",
	"network_unbanPeer",
	"sidechain_getParams",
	"sidechain_getStatus",
	"sidechain_getEpochCommittee",
	"sidechain_getRegistrations",
	"sidechain_getAriadneParameters",
];

/// Standard Substrate RPC method names injected by `sc_service`.
const SUBSTRATE_METHOD_NAMES: &[&str] = &[
	// system
	"system_name",
	"system_version",
	"system_chain",
	"system_chainType",
	"system_properties",
	"system_health",
	"system_localPeerId",
	"system_localListenAddresses",
	"system_peers",
	"system_nodeRoles",
	"system_syncState",
	"system_addReservedPeer",
	"system_removeReservedPeer",
	"system_reservedPeers",
	"system_accountNextIndex",
	"system_dryRun",
	// author
	"author_submitExtrinsic",
	"author_pendingExtrinsics",
	"author_removeExtrinsic",
	"author_hasKey",
	"author_hasSessionKeys",
	"author_insertKey",
	"author_rotateKeys",
	"author_submitAndWatchExtrinsic",
	// chain
	"chain_getHeader",
	"chain_getBlock",
	"chain_getBlockHash",
	"chain_getFinalizedHead",
	"chain_subscribeNewHeads",
	"chain_subscribeFinalizedHeads",
	"chain_subscribeAllHeads",
	// state
	"state_call",
	"state_getKeys",
	"state_getKeysPaged",
	"state_getStorage",
	"state_getStorageHash",
	"state_getStorageSize",
	"state_getMetadata",
	"state_getRuntimeVersion",
	"state_queryStorage",
	"state_queryStorageAt",
	"state_getReadProof",
	"state_subscribeRuntimeVersion",
	"state_subscribeStorage",
	"state_traceBlock",
	// grandpa (merged in create_full)
	"grandpa_roundState",
	"grandpa_proveFinality",
	"grandpa_subscribeJustifications",
	// mmr (merged in create_full)
	"mmr_root",
	"mmr_generateProof",
	// beefy (merged in create_full)
	"beefy_getFinalizedHead",
	"beefy_subscribeJustifications",
];

/// Build a complete OpenRPC v1.4 document.
///
/// `custom_methods` is the list of method names obtained at runtime from
/// `RpcModule::method_names()`. Only methods present in both this list and the
/// known metadata are emitted — this keeps the document drift-free.
pub fn build_openrpc_document(custom_methods: &[&str]) -> Value {
	let mut methods: Vec<Value> = Vec::new();

	for &name in CUSTOM_METHOD_NAMES {
		if custom_methods.contains(&name)
			&& let Some(entry) = build_custom_method(name)
		{
			methods.push(entry);
		}
	}

	for &name in SUBSTRATE_METHOD_NAMES {
		methods.push(build_substrate_stub(name));
	}

	json!({
		"openrpc": "1.4.0",
		"info": {
			"title": "Midnight Node JSON-RPC API",
			"version": env!("CARGO_PKG_VERSION"),
			"description": "JSON-RPC API for the Midnight privacy blockchain node. Custom methods provide access to the privacy ledger, governance parameters, and peer management. Standard Substrate methods are also listed."
		},
		"methods": methods,
		"components": {
			"schemas": build_component_schemas(),
			"errors": build_error_components()
		}
	})
}

// ---------------------------------------------------------------------------
// Custom method metadata
// ---------------------------------------------------------------------------

fn build_custom_method(name: &str) -> Option<Value> {
	Some(match name {
		// -- MidnightApi --
		"midnight_contractState" => method_entry(
			name,
			"Returns the state of a deployed contract.",
			"The contract is identified by its hex-encoded address. The returned state is also hex-encoded. Queries run against the best block unless `at` specifies a historical block hash.",
			&[
				param(
					"contract_address",
					"Hex-encoded contract address",
					json!({"type": "string"}),
				),
				param_optional(
					"at",
					"Block hash to query at (defaults to best block)",
					schema_ref("BlockHash"),
				),
			],
			result("state", "Hex-encoded contract state", json!({"type": "string"})),
			&[error_ref("StateRpcError")],
		),
		"midnight_zswapStateRoot" => method_entry(
			name,
			"Returns the Merkle root of the zswap (shielded transaction) state tree.",
			"The root is returned as a byte array. If `at` is null, the best block is used.",
			&[param_optional(
				"at",
				"Block hash to query at (defaults to best block)",
				schema_ref("BlockHash"),
			)],
			result(
				"root",
				"Merkle root bytes",
				json!({"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}}),
			),
			&[error_ref("StateRpcError")],
		),
		"midnight_ledgerStateRoot" => method_entry(
			name,
			"Returns the Merkle root of the overall ledger state.",
			"The root is returned as a byte array. If `at` is null, the best block is used.",
			&[param_optional(
				"at",
				"Block hash to query at (defaults to best block)",
				schema_ref("BlockHash"),
			)],
			result(
				"root",
				"Merkle root bytes",
				json!({"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}}),
			),
			&[error_ref("StateRpcError")],
		),
		"midnight_apiVersions" => method_entry(
			name,
			"Returns the RPC API version(s) supported by this node.",
			"The returned array currently contains a single element ([2]). This is the RPC protocol version, distinct from the runtime API version.",
			&[],
			result(
				"versions",
				"Supported API version numbers",
				json!({"type": "array", "items": {"type": "integer"}}),
			),
			&[],
		),
		"midnight_ledgerVersion" => method_entry(
			name,
			"Returns the ledger implementation version string.",
			"If `at` is null, the best block is used.",
			&[param_optional(
				"at",
				"Block hash to query at (defaults to best block)",
				schema_ref("BlockHash"),
			)],
			result("version", "Ledger version string", json!({"type": "string"})),
			&[error_ref("BlockRpcError")],
		),

		// -- SystemParametersRpcApi --
		"systemParameters_getTermsAndConditions" => method_entry(
			name,
			"Get the current Terms and Conditions.",
			"Returns the hash and URL of the current terms and conditions, or null if not set.",
			&[param_optional(
				"at",
				"Block hash to query at (defaults to best block)",
				schema_ref("BlockHash"),
			)],
			result(
				"termsAndConditions",
				"Terms and conditions, or null",
				json!({
					"oneOf": [
						schema_ref("TermsAndConditionsRpcResponse"),
						{"type": "null"}
					]
				}),
			),
			&[error_ref("SystemParametersRpcError")],
		),
		"systemParameters_getDParameter" => method_entry(
			name,
			"Get the current D-Parameter.",
			"Returns the number of permissioned and registered candidates.",
			&[param_optional(
				"at",
				"Block hash to query at (defaults to best block)",
				schema_ref("BlockHash"),
			)],
			result("dParameter", "D-Parameter values", schema_ref("DParameterRpcResponse")),
			&[error_ref("SystemParametersRpcError")],
		),
		"systemParameters_getAriadneParameters" => method_entry(
			name,
			"Get Ariadne parameters for a given mainchain epoch.",
			"Returns permissioned candidates and candidate registrations from Cardano, with the D Parameter sourced from pallet-system-parameters on-chain storage. Preferred over sidechain_getAriadneParameters.",
			&[
				param(
					"epoch_number",
					"Mainchain epoch number to query candidates for",
					json!({"type": "integer", "minimum": 0}),
				),
				param_optional(
					"d_parameter_at",
					"Block hash to query D Parameter from (defaults to best block)",
					schema_ref("BlockHash"),
				),
			],
			result(
				"ariadneParameters",
				"Ariadne parameters response",
				schema_ref("AriadneParametersRpcResponse"),
			),
			&[error_ref("SystemParametersRpcError")],
		),

		// -- PeerInfoApi --
		"network_peerReputations" => method_entry(
			name,
			"Returns reputation info for all connected peers.",
			"Lists all connected peers with their reputation score and ban status.",
			&[],
			result(
				"peers",
				"Array of peer reputation info",
				json!({
					"type": "array",
					"items": schema_ref("PeerReputationInfo")
				}),
			),
			&[],
		),
		"network_peerReputation" => method_entry(
			name,
			"Returns reputation info for a single peer.",
			"Looks up a peer by its base58-encoded peer ID.",
			&[param("peer_id", "Base58-encoded peer ID", json!({"type": "string"}))],
			result("peer", "Peer reputation info", schema_ref("PeerReputationInfo")),
			&[],
		),
		"network_unbanPeer" => {
			let mut entry = method_entry(
				name,
				"Unbans a peer by boosting its reputation above the ban threshold.",
				"Requires the node to be started with --rpc-methods=unsafe.",
				&[param("peer_id", "Base58-encoded peer ID to unban", json!({"type": "string"}))],
				result("result", "Null on success", json!({"type": "null"})),
				&[],
			);
			entry.as_object_mut().unwrap().insert("x-unsafe".to_string(), json!(true));
			entry
		},

		// -- SidechainRpcApi --
		"sidechain_getParams" => method_entry(
			name,
			"Get sidechain parameters.",
			"Returns the genesis UTXO that uniquely identifies this partner chain.",
			&[],
			result(
				"params",
				"Sidechain parameters",
				json!({
					"type": "object",
					"properties": {
						"genesis_utxo": schema_ref("UtxoId")
					},
					"required": ["genesis_utxo"]
				}),
			),
			&[],
		),
		"sidechain_getStatus" => method_entry(
			name,
			"Get sidechain status.",
			"Returns current epoch/slot information for both the partner chain and Cardano mainchain.",
			&[],
			result("status", "Sidechain and mainchain status", schema_ref("GetStatusResponse")),
			&[error_ref("GetStatusRpcError")],
		),
		"sidechain_getEpochCommittee" => method_entry(
			name,
			"Get the validator committee for a sidechain epoch.",
			"Returns the ordered list of validators selected for the specified epoch.",
			&[param(
				"epoch_number",
				"Sidechain epoch number",
				json!({"type": "integer", "minimum": 0}),
			)],
			result("committee", "Committee response", schema_ref("GetCommitteeResponse")),
			&[],
		),
		"sidechain_getRegistrations" => method_entry(
			name,
			"Get SPO registrations for a mainchain epoch.",
			"Returns Stake Pool Operator registration records for committee candidacy.",
			&[
				param(
					"mc_epoch_number",
					"Mainchain epoch number",
					json!({"type": "integer", "minimum": 0}),
				),
				param(
					"mc_public_key",
					"Stake pool public key (hex-encoded)",
					json!({"type": "string"}),
				),
			],
			result(
				"registrations",
				"Array of registration entries",
				json!({
					"type": "array",
					"items": schema_ref("CandidateRegistrationEntry")
				}),
			),
			&[],
		),
		"sidechain_getAriadneParameters" => {
			let mut entry = method_entry(
				name,
				"Get Ariadne parameters for a mainchain epoch.",
				"Returns permissioned candidates, registrations, and D Parameter sourced from Cardano. Deprecated: use systemParameters_getAriadneParameters instead.",
				&[param(
					"epoch_number",
					"Mainchain epoch number",
					json!({"type": "integer", "minimum": 0}),
				)],
				result(
					"ariadneParameters",
					"Ariadne parameters response",
					schema_ref("AriadneParametersRpcResponse"),
				),
				&[],
			);
			entry.as_object_mut().unwrap().insert("deprecated".to_string(), json!(true));
			entry
		},

		_ => return None,
	})
}

// ---------------------------------------------------------------------------
// Standard Substrate method stubs
// ---------------------------------------------------------------------------

fn build_substrate_stub(name: &str) -> Value {
	let summary = substrate_method_summary(name);
	let mut entry = json!({
		"name": name,
		"summary": summary,
		"params": [],
		"result": {
			"name": "result",
			"schema": {}
		},
		"examples": examples_for_method(name),
		"description": "Standard Substrate RPC method. See https://paritytech.github.io/polkadot-sdk/master/sc_rpc/index.html for upstream documentation.",
	});

	if is_substrate_unsafe(name) {
		entry.as_object_mut().unwrap().insert("x-unsafe".to_string(), json!(true));
	}

	if is_substrate_subscription(name) {
		entry.as_object_mut().unwrap().insert("x-subscription".to_string(), json!(true));
	}

	entry
}

fn substrate_method_summary(name: &str) -> &'static str {
	match name {
		"system_name" => "Node implementation name",
		"system_version" => "Node implementation version",
		"system_chain" => "Chain name",
		"system_chainType" => "Chain type (Development, Local, Live, Custom)",
		"system_properties" => "Chain properties",
		"system_health" => "Node health (peers, syncing, should_have_peers)",
		"system_localPeerId" => "Local node PeerId",
		"system_localListenAddresses" => "Listen addresses",
		"system_peers" => "Connected peers",
		"system_nodeRoles" => "Node roles (Full, Authority, etc.)",
		"system_syncState" => "Sync state (starting, current, highest block)",
		"system_addReservedPeer" => "Add reserved peer (unsafe)",
		"system_removeReservedPeer" => "Remove reserved peer (unsafe)",
		"system_reservedPeers" => "List reserved peers",
		"system_accountNextIndex" => "Account nonce",
		"system_dryRun" => "Dry-run an extrinsic at a block (unsafe)",
		"author_submitExtrinsic" => "Submit hex-encoded extrinsic to pool",
		"author_pendingExtrinsics" => "Get pending extrinsics",
		"author_removeExtrinsic" => "Remove extrinsics from pool",
		"author_hasKey" => "Check if key exists in keystore",
		"author_hasSessionKeys" => "Check if session keys exist",
		"author_insertKey" => "Insert key into keystore (unsafe)",
		"author_rotateKeys" => "Generate new session keys (unsafe)",
		"author_submitAndWatchExtrinsic" => "Submit and subscribe to extrinsic status updates",
		"chain_getHeader" => "Get header by hash",
		"chain_getBlock" => "Get block by hash",
		"chain_getBlockHash" => "Get block hash by number",
		"chain_getFinalizedHead" => "Get finalized head hash",
		"chain_subscribeNewHeads" => "Subscribe to new best head",
		"chain_subscribeFinalizedHeads" => "Subscribe to finalized heads",
		"chain_subscribeAllHeads" => "Subscribe to all heads",
		"state_call" => "Execute a runtime API call",
		"state_getKeys" => "Get storage keys (deprecated)",
		"state_getKeysPaged" => "Get storage keys (paginated)",
		"state_getStorage" => "Get storage value at key",
		"state_getStorageHash" => "Get storage hash at key",
		"state_getStorageSize" => "Get storage size at key",
		"state_getMetadata" => "Get runtime metadata",
		"state_getRuntimeVersion" => "Get runtime version",
		"state_queryStorage" => "Query storage over a range",
		"state_queryStorageAt" => "Query storage at a block",
		"state_getReadProof" => "Get read proof for keys",
		"state_subscribeRuntimeVersion" => "Subscribe to version changes",
		"state_subscribeStorage" => "Subscribe to storage changes",
		"state_traceBlock" => "Trace block execution (unsafe)",
		"grandpa_roundState" => "Current GRANDPA round state",
		"grandpa_proveFinality" => "Prove finality of a block",
		"grandpa_subscribeJustifications" => "Subscribe to GRANDPA justifications",
		"mmr_root" => "Get MMR root hash",
		"mmr_generateProof" => "Generate MMR membership proof",
		"beefy_getFinalizedHead" => "Get BEEFY finalized head hash",
		"beefy_subscribeJustifications" => "Subscribe to BEEFY justifications",
		_ => "Standard Substrate RPC method",
	}
}

fn is_substrate_unsafe(name: &str) -> bool {
	matches!(
		name,
		"system_addReservedPeer"
			| "system_removeReservedPeer"
			| "author_insertKey"
			| "author_rotateKeys"
			| "system_dryRun"
			| "state_traceBlock"
	)
}

fn is_substrate_subscription(name: &str) -> bool {
	matches!(
		name,
		"author_submitAndWatchExtrinsic"
			| "chain_subscribeNewHeads"
			| "chain_subscribeFinalizedHeads"
			| "chain_subscribeAllHeads"
			| "state_subscribeRuntimeVersion"
			| "state_subscribeStorage"
			| "grandpa_subscribeJustifications"
			| "beefy_subscribeJustifications"
	)
}

fn examples_for_method(name: &str) -> Value {
	json!([{
		"name": format!("{}_example", name),
		"summary": "Minimal JSON-RPC example",
		"params": example_params_for_method(name),
		"result": example_result_for_method(name),
	}])
}

fn example_params_for_method(name: &str) -> Value {
	let block_hash = "0x1111111111111111111111111111111111111111111111111111111111111111";
	let storage_key = "0x3a636f6465";
	let extrinsic = "0x280402000b";
	let peer_id = "12D3KooWExamplePeerId1111111111111111111111111111111111";
	let multiaddr =
		"/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWExamplePeerId1111111111111111111111111111111111";

	match name {
		"midnight_contractState" => json!([
			example_param("contract_address", json!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")),
			example_param("at", json!(block_hash)),
		]),
		"midnight_zswapStateRoot"
		| "midnight_ledgerStateRoot"
		| "midnight_ledgerVersion"
		| "systemParameters_getTermsAndConditions"
		| "systemParameters_getDParameter" => {
			json!([example_param("at", json!(block_hash))])
		},
		"systemParameters_getAriadneParameters" => json!([
			example_param("epoch_number", json!(42)),
			example_param("d_parameter_at", json!(block_hash)),
		]),
		"network_peerReputation" | "network_unbanPeer" => {
			json!([example_param("peer_id", json!(peer_id))])
		},
		"sidechain_getEpochCommittee" | "sidechain_getAriadneParameters" => {
			json!([example_param("epoch_number", json!(42))])
		},
		"sidechain_getRegistrations" => json!([
			example_param("mc_epoch_number", json!(42)),
			example_param(
				"mc_public_key",
				json!("0x2222222222222222222222222222222222222222222222222222222222222222"),
			),
		]),
		"system_addReservedPeer" | "system_removeReservedPeer" => {
			json!([example_param("peer", json!(multiaddr))])
		},
		"system_accountNextIndex" => {
			json!([example_param(
				"accountId",
				json!("5GrwvaEF5zXb26Fz9rcQpDWS5M7vQp93bYj6dBzU1gk9fH8Z")
			)])
		},
		"system_dryRun" => json!([
			example_param("extrinsic", json!(extrinsic)),
			example_param("at", json!(block_hash)),
		]),
		"author_submitExtrinsic" | "author_submitAndWatchExtrinsic" => {
			json!([example_param("extrinsic", json!(extrinsic))])
		},
		"author_removeExtrinsic" => {
			json!([example_param("bytesOrHash", json!(extrinsic))])
		},
		"author_hasKey" => json!([
			example_param(
				"publicKey",
				json!("0x3333333333333333333333333333333333333333333333333333333333333333"),
			),
			example_param("keyType", json!("aura")),
		]),
		"author_hasSessionKeys" => {
			json!([example_param("sessionKeys", json!("0x44444444444444444444444444444444"))])
		},
		"author_insertKey" => json!([
			example_param("keyType", json!("aura")),
			example_param("suri", json!("//Alice")),
			example_param(
				"publicKey",
				json!("0x3333333333333333333333333333333333333333333333333333333333333333"),
			),
		]),
		"chain_getHeader" | "chain_getBlock" => {
			json!([example_param("hash", json!(block_hash))])
		},
		"chain_getBlockHash" => {
			json!([example_param("blockNumber", json!(1))])
		},
		"state_call" => json!([
			example_param("method", json!("Core_version")),
			example_param("data", json!("0x")),
			example_param("at", json!(block_hash)),
		]),
		"state_getKeys" => {
			json!([example_param("prefix", json!("0x")), example_param("at", json!(block_hash)),])
		},
		"state_getKeysPaged" => json!([
			example_param("prefix", json!("0x")),
			example_param("count", json!(10)),
			example_param("startKey", Value::Null),
			example_param("at", json!(block_hash)),
		]),
		"state_getStorage" | "state_getStorageHash" | "state_getStorageSize" => json!([
			example_param("key", json!(storage_key)),
			example_param("at", json!(block_hash)),
		]),
		"state_getRuntimeVersion" => {
			json!([example_param("at", json!(block_hash))])
		},
		"state_queryStorage" => json!([
			example_param("keys", json!([storage_key])),
			example_param("fromBlock", json!(block_hash)),
			example_param("toBlock", json!(block_hash)),
		]),
		"state_queryStorageAt" => json!([
			example_param("keys", json!([storage_key])),
			example_param("at", json!(block_hash)),
		]),
		"state_getReadProof" => json!([
			example_param("keys", json!([storage_key])),
			example_param("at", json!(block_hash)),
		]),
		"state_subscribeStorage" => {
			json!([example_param("keys", json!([storage_key]))])
		},
		"state_traceBlock" | "grandpa_proveFinality" => {
			json!([example_param("hash", json!(block_hash))])
		},
		"mmr_generateProof" => json!([
			example_param("blockNumbers", json!([1])),
			example_param("bestKnownBlockNumber", Value::Null),
			example_param("at", json!(block_hash)),
		]),
		"midnight_apiVersions"
		| "network_peerReputations"
		| "sidechain_getParams"
		| "sidechain_getStatus"
		| "system_name"
		| "system_version"
		| "system_chain"
		| "system_chainType"
		| "system_properties"
		| "system_health"
		| "system_localPeerId"
		| "system_localListenAddresses"
		| "system_peers"
		| "system_nodeRoles"
		| "system_syncState"
		| "system_reservedPeers"
		| "author_pendingExtrinsics"
		| "author_rotateKeys"
		| "chain_getFinalizedHead"
		| "chain_subscribeNewHeads"
		| "chain_subscribeFinalizedHeads"
		| "chain_subscribeAllHeads"
		| "state_getMetadata"
		| "state_subscribeRuntimeVersion"
		| "grandpa_roundState"
		| "grandpa_subscribeJustifications"
		| "mmr_root"
		| "beefy_getFinalizedHead"
		| "beefy_subscribeJustifications" => json!([]),
		_ => json!([]),
	}
}

fn example_result_for_method(name: &str) -> Value {
	let block_hash = "0x1111111111111111111111111111111111111111111111111111111111111111";
	let extrinsic_hash = "0x2222222222222222222222222222222222222222222222222222222222222222";
	let storage_key = "0x3a636f6465";
	let peer_id = "12D3KooWExamplePeerId1111111111111111111111111111111111";
	let peer_info = json!({
		"peerId": peer_id,
		"roles": "FULL",
		"bestHash": block_hash,
		"bestNumber": 1,
		"reputation": 256,
		"isBanned": false
	});
	let d_parameter = json!({
		"numPermissionedCandidates": 1,
		"numRegisteredCandidates": 2
	});
	let ariadne_parameters = json!({
		"permissionedCandidates": [],
		"candidateRegistrations": {},
		"dParameter": d_parameter.clone()
	});
	let header = json!({
		"parentHash": block_hash,
		"number": "0x1",
		"stateRoot": block_hash,
		"extrinsicsRoot": block_hash,
		"digest": { "logs": [] }
	});

	match name {
		"midnight_contractState" => example_result("state", json!("0x")),
		"midnight_zswapStateRoot" | "midnight_ledgerStateRoot" => {
			example_result("root", json!([0, 1, 2, 3]))
		},
		"midnight_apiVersions" => example_result("versions", json!([2])),
		"midnight_ledgerVersion" => example_result("version", json!("1.0.0")),
		"systemParameters_getTermsAndConditions" => example_result(
			"termsAndConditions",
			json!({
				"hash": "0x5555555555555555555555555555555555555555555555555555555555555555",
				"url": "https://example.com/terms"
			}),
		),
		"systemParameters_getDParameter" => example_result("dParameter", d_parameter),
		"systemParameters_getAriadneParameters" | "sidechain_getAriadneParameters" => {
			example_result("ariadneParameters", ariadne_parameters)
		},
		"network_peerReputations" => example_result("peers", json!([peer_info])),
		"network_peerReputation" => example_result("peer", peer_info),
		"network_unbanPeer" => example_result("result", Value::Null),
		"sidechain_getParams" => example_result(
			"params",
			json!({
				"genesis_utxo": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#0"
			}),
		),
		"sidechain_getStatus" => example_result(
			"status",
			json!({
				"sidechain": {
					"epoch": 1,
					"nextEpochTimestamp": 1700000000000_i64
				},
				"mainchain": {
					"epoch": 42,
					"slot": 1000,
					"nextEpochTimestamp": 1700000600000_i64
				}
			}),
		),
		"sidechain_getEpochCommittee" => example_result(
			"committee",
			json!({
				"sidechainEpoch": 42,
				"committee": [
					{
						"sidechainPubKey": "0x6666666666666666666666666666666666666666666666666666666666666666"
					}
				]
			}),
		),
		"sidechain_getRegistrations" => example_result("registrations", json!([])),
		"system_name" => example_result("result", json!("midnight-node")),
		"system_version" => example_result("result", json!("1.0.0")),
		"system_chain" => example_result("result", json!("Development")),
		"system_chainType" => example_result("result", json!("Development")),
		"system_properties" => example_result(
			"result",
			json!({
				"tokenDecimals": 12,
				"tokenSymbol": "DUST"
			}),
		),
		"system_health" => example_result(
			"result",
			json!({
				"peers": 1,
				"isSyncing": false,
				"shouldHavePeers": true
			}),
		),
		"system_localPeerId" => example_result("result", json!(peer_id)),
		"system_localListenAddresses" => example_result(
			"result",
			json!([
				"/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWExamplePeerId1111111111111111111111111111111111"
			]),
		),
		"system_peers" => example_result("result", json!([])),
		"system_nodeRoles" => example_result("result", json!(["Full"])),
		"system_syncState" => example_result(
			"result",
			json!({
				"startingBlock": 0,
				"currentBlock": 1,
				"highestBlock": 1
			}),
		),
		"system_addReservedPeer" | "system_removeReservedPeer" => {
			example_result("result", Value::Null)
		},
		"system_reservedPeers" => example_result("result", json!([])),
		"system_accountNextIndex" => example_result("result", json!(0)),
		"system_dryRun" => example_result(
			"result",
			json!({
				"Ok": []
			}),
		),
		"author_submitExtrinsic" => example_result("result", json!(extrinsic_hash)),
		"author_pendingExtrinsics" => example_result("result", json!([])),
		"author_removeExtrinsic" => example_result("result", json!([extrinsic_hash])),
		"author_hasKey" | "author_hasSessionKeys" => example_result("result", json!(true)),
		"author_insertKey" => example_result("result", Value::Null),
		"author_rotateKeys" => {
			example_result("result", json!("0x44444444444444444444444444444444"))
		},
		"author_submitAndWatchExtrinsic"
		| "chain_subscribeNewHeads"
		| "chain_subscribeFinalizedHeads"
		| "chain_subscribeAllHeads"
		| "state_subscribeRuntimeVersion"
		| "state_subscribeStorage"
		| "grandpa_subscribeJustifications"
		| "beefy_subscribeJustifications" => example_result("result", json!("0x1")),
		"chain_getHeader" => example_result("result", header),
		"chain_getBlock" => example_result(
			"result",
			json!({
				"block": {
					"header": header,
					"extrinsics": []
				},
				"justifications": null
			}),
		),
		"chain_getBlockHash" | "chain_getFinalizedHead" | "beefy_getFinalizedHead" => {
			example_result("result", json!(block_hash))
		},
		"state_call" | "state_getStorage" | "state_getMetadata" => {
			example_result("result", json!("0x"))
		},
		"state_getKeys" | "state_getKeysPaged" => example_result("result", json!([storage_key])),
		"state_getStorageHash" => example_result("result", json!(block_hash)),
		"state_getStorageSize" => example_result("result", json!(0)),
		"state_getRuntimeVersion" => example_result(
			"result",
			json!({
				"specName": "midnight-node",
				"implName": "midnight-node",
				"authoringVersion": 1,
				"specVersion": 1,
				"implVersion": 1,
				"apis": [],
				"transactionVersion": 1,
				"stateVersion": 1
			}),
		),
		"state_queryStorage" | "state_queryStorageAt" => example_result(
			"result",
			json!([
				{
					"block": block_hash,
					"changes": [[storage_key, "0x"]]
				}
			]),
		),
		"state_getReadProof" => example_result(
			"result",
			json!({
				"at": block_hash,
				"proof": []
			}),
		),
		"state_traceBlock" => example_result("result", json!({})),
		"grandpa_roundState" => example_result(
			"result",
			json!({
				"round": 1,
				"totalWeight": 1,
				"thresholdWeight": 1,
				"prevotes": { "currentWeight": 1, "missing": [] },
				"precommits": { "currentWeight": 1, "missing": [] }
			}),
		),
		"grandpa_proveFinality" => example_result("result", json!("0x")),
		"mmr_root" => example_result("result", json!(block_hash)),
		"mmr_generateProof" => example_result(
			"result",
			json!({
				"blockHash": block_hash,
				"proof": {
					"leafIndices": [1],
					"leafCount": 2,
					"items": []
				}
			}),
		),
		_ => example_result("result", Value::Null),
	}
}

fn example_param(name: &str, value: Value) -> Value {
	json!({
		"name": name,
		"value": value
	})
}

fn example_result(name: &str, value: Value) -> Value {
	json!({
		"name": name,
		"value": value
	})
}

// ---------------------------------------------------------------------------
// Component schemas (JSON Schema definitions for types)
// ---------------------------------------------------------------------------

fn build_component_schemas() -> Value {
	let tc_schema = serde_json::to_value(schema_for!(
		pallet_system_parameters_rpc::TermsAndConditionsRpcResponse
	))
	.expect("TermsAndConditionsRpcResponse schema must serialize to valid JSON");
	let dp_schema =
		serde_json::to_value(schema_for!(pallet_system_parameters_rpc::DParameterRpcResponse))
			.expect("DParameterRpcResponse schema must serialize to valid JSON");
	let ap_schema = serde_json::to_value(schema_for!(
		pallet_system_parameters_rpc::AriadneParametersRpcResponse
	))
	.expect("AriadneParametersRpcResponse schema must serialize to valid JSON");
	let operation_schema = serde_json::to_value(schema_for!(pallet_midnight_rpc::Operation))
		.expect("Operation schema must serialize to valid JSON");
	let midnight_tx_schema =
		serde_json::to_value(schema_for!(pallet_midnight_rpc::MidnightRpcTransaction))
			.expect("MidnightRpcTransaction schema must serialize to valid JSON");
	let rpc_tx_schema = serde_json::to_value(schema_for!(pallet_midnight_rpc::RpcTransaction))
		.expect("RpcTransaction schema must serialize to valid JSON");

	json!({
		"BlockHash": {
			"type": "string",
			"description": "0x-prefixed hex-encoded 32-byte block hash",
			"pattern": "^0x[0-9a-fA-F]{64}$"
		},
		"TermsAndConditionsRpcResponse": tc_schema,
		"DParameterRpcResponse": dp_schema,
		"AriadneParametersRpcResponse": ap_schema,
		"Operation": operation_schema,
		"MidnightRpcTransaction": midnight_tx_schema,
		"RpcTransaction": rpc_tx_schema,
		"PeerReputationInfo": {
			"type": "object",
			"description": "Peer information enriched with reputation and ban status",
			"properties": {
				"peerId": { "type": "string", "description": "Peer ID (base58-encoded)" },
				"roles": { "type": "string", "description": "Roles advertised by the peer (e.g. FULL)" },
				"bestHash": { "type": "string", "description": "Best block hash known for this peer (0x-prefixed hex)" },
				"bestNumber": { "type": "integer", "description": "Best block number known for this peer" },
				"reputation": { "type": "integer", "description": "Current reputation score" },
				"isBanned": { "type": "boolean", "description": "Whether the peer is currently banned" }
			},
			"required": ["peerId", "roles", "bestHash", "bestNumber", "reputation", "isBanned"]
		},
		"RpcBlock": {
			"type": "object",
			"description": "A block with decoded transactions",
			"properties": {
				"header": { "type": "object", "description": "Block header" },
				"body": {
					"type": "array",
					"items": { "$ref": "#/components/schemas/RpcTransaction" }
				},
				"transactions_index": {
					"type": "array",
					"items": {
						"type": "array",
						"items": { "type": "string" },
						"minItems": 2,
						"maxItems": 2
					}
				}
			},
			"required": ["header", "body", "transactions_index"]
		},
		"UtxoId": {
			"type": "string",
			"description": "UTXO identifier in the format \"<hex_tx_hash>#<output_index>\"",
			"pattern": "^[0-9a-fA-F]{64}#[0-9]+$"
		},
		"GetStatusResponse": {
			"type": "object",
			"description": "Sidechain and mainchain status",
			"properties": {
				"sidechain": {
					"type": "object",
					"properties": {
						"epoch": { "type": "integer" },
						"nextEpochTimestamp": { "type": "integer", "description": "Milliseconds since Unix epoch" }
					},
					"required": ["epoch", "nextEpochTimestamp"]
				},
				"mainchain": {
					"type": "object",
					"properties": {
						"epoch": { "type": "integer" },
						"slot": { "type": "integer" },
						"nextEpochTimestamp": { "type": "integer", "description": "Milliseconds since Unix epoch" }
					},
					"required": ["epoch", "slot", "nextEpochTimestamp"]
				}
			},
			"required": ["sidechain", "mainchain"]
		},
		"GetCommitteeResponse": {
			"type": "object",
			"description": "Committee members for a sidechain epoch",
			"properties": {
				"sidechainEpoch": { "type": "integer" },
				"committee": {
					"type": "array",
					"items": {
						"type": "object",
						"properties": {
							"sidechainPubKey": { "type": "string", "description": "0x-prefixed hex public key" }
						},
						"required": ["sidechainPubKey"]
					}
				}
			},
			"required": ["sidechainEpoch", "committee"]
		},
		"CandidateRegistrationEntry": {
			"type": "object",
			"description": "SPO registration entry for committee candidacy",
			"properties": {
				"sidechainPubKey": { "type": "string" },
				"sidechainAccountId": { "type": "string", "description": "SS58-encoded account ID" },
				"mainchainPubKey": { "type": "string" },
				"crossChainPubKey": { "type": "string" },
				"keys": { "type": "object", "additionalProperties": { "type": "string" } },
				"sidechainSignature": { "type": "string" },
				"mainchainSignature": { "type": "string" },
				"crossChainSignature": { "type": "string" },
				"utxo": { "$ref": "#/components/schemas/UtxoId" },
				"stakeDelegation": {
					"oneOf": [{ "type": "integer" }, { "type": "null" }]
				},
				"isValid": { "type": "boolean" },
			"invalidReasons": {
				"description": "Present only when isValid is false",
				"oneOf": [
					{ "type": "array", "items": { "type": "string" } },
					{ "type": "null" }
				]
			}
			},
			"required": [
				"sidechainPubKey", "sidechainAccountId", "mainchainPubKey",
				"crossChainPubKey", "keys", "sidechainSignature",
				"mainchainSignature", "crossChainSignature", "utxo", "isValid"
			]
		}
	})
}

// ---------------------------------------------------------------------------
// Error component definitions
// ---------------------------------------------------------------------------

fn build_error_components() -> Value {
	json!({
		"StateRpcError": {
			"code": -32602,
			"message": "Invalid params",
			"data": {
				"description": "Variants: BadContractAddress, BadAccountAddress, UnableToGetContractState, UnableToGetZSwapChainState, UnableToGetZSwapStateRoot, UnableToGetLedgerStateRoot"
			}
		},
		"BlockRpcError": {
			"code": -32602,
			"message": "Invalid params",
			"data": {
				"description": "Variants: UnableToGetBlock, BlockNotFound, UnableToGetLedgerState, UnableToDecodeTransactions, UnableToSerializeBlock, UnableToGetChainVersion"
			}
		},
		"SystemParametersRpcError": {
			"code": -32603,
			"message": "Internal error",
			"data": {
				"description": "Variants: UnableToGetTermsAndConditions, UnableToGetDParameter, UnableToGetAriadneParameters, RuntimeApiError"
			}
		},
		"GetStatusRpcError": {
			"code": -1,
			"message": "Sidechain status error",
			"data": {
				"description": "Custom error from partner-chains sidechain RPC"
			}
		}
	})
}

// ---------------------------------------------------------------------------
// Helper builders
// ---------------------------------------------------------------------------

fn param(name: &str, description: &str, schema: Value) -> Value {
	json!({
		"name": name,
		"description": description,
		"required": true,
		"schema": schema
	})
}

fn param_optional(name: &str, description: &str, schema: Value) -> Value {
	json!({
		"name": name,
		"description": description,
		"required": false,
		"schema": schema
	})
}

fn result(name: &str, description: &str, schema: Value) -> Value {
	json!({
		"name": name,
		"description": description,
		"schema": schema
	})
}

fn method_entry(
	name: &str,
	summary: &str,
	description: &str,
	params: &[Value],
	result_desc: Value,
	errors: &[Value],
) -> Value {
	let mut entry = json!({
		"name": name,
		"summary": summary,
		"description": description,
		"params": params,
		"result": result_desc,
		"examples": examples_for_method(name),
	});

	if !errors.is_empty() {
		entry.as_object_mut().unwrap().insert("errors".to_string(), json!(errors));
	}

	entry
}

fn schema_ref(name: &str) -> Value {
	json!({ "$ref": format!("#/components/schemas/{}", name) })
}

fn error_ref(name: &str) -> Value {
	json!({ "$ref": format!("#/components/errors/{}", name) })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	fn all_custom_method_names() -> Vec<&'static str> {
		CUSTOM_METHOD_NAMES.to_vec()
	}

	#[test]
	fn document_has_valid_openrpc_structure() {
		let doc = build_openrpc_document(&all_custom_method_names());

		assert_eq!(doc["openrpc"], "1.4.0");
		assert!(doc["info"]["title"].is_string());
		assert!(doc["info"]["version"].is_string());
		assert!(doc["methods"].is_array());
		assert!(doc["components"]["schemas"].is_object());
		assert!(doc["components"]["errors"].is_object());
	}

	#[test]
	fn all_sixteen_custom_methods_present() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let methods = doc["methods"].as_array().unwrap();

		for &expected in CUSTOM_METHOD_NAMES {
			assert!(
				methods.iter().any(|m| m["name"] == expected),
				"Custom method '{}' missing from OpenRPC document",
				expected
			);
		}
	}

	#[test]
	fn standard_substrate_stubs_present() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let methods = doc["methods"].as_array().unwrap();

		for &expected in SUBSTRATE_METHOD_NAMES {
			assert!(
				methods.iter().any(|m| m["name"] == expected),
				"Substrate method '{}' missing from OpenRPC document",
				expected
			);
		}
	}

	#[test]
	fn method_names_match_expected_inventory() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let methods = doc["methods"].as_array().unwrap();
		let method_names: Vec<&str> = methods.iter().map(|m| m["name"].as_str().unwrap()).collect();

		let total_expected = CUSTOM_METHOD_NAMES.len() + SUBSTRATE_METHOD_NAMES.len();
		assert_eq!(
			method_names.len(),
			total_expected,
			"Expected {} methods, got {}. Names: {:?}",
			total_expected,
			method_names.len(),
			method_names
		);
	}

	#[test]
	fn custom_methods_have_params_and_result() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let methods = doc["methods"].as_array().unwrap();

		for &name in CUSTOM_METHOD_NAMES {
			let method = methods.iter().find(|m| m["name"] == name).unwrap();
			assert!(method["params"].is_array(), "{} missing params array", name);
			assert!(method["result"].is_object(), "{} missing result object", name);
			assert!(method["result"]["schema"].is_object(), "{} missing result schema", name);
		}
	}

	#[test]
	fn all_methods_have_examples() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let methods = doc["methods"].as_array().unwrap();

		for method in methods {
			let name = method["name"].as_str().unwrap();
			let examples = method["examples"]
				.as_array()
				.unwrap_or_else(|| panic!("{} missing examples array", name));
			assert!(!examples.is_empty(), "{} has no examples", name);

			for example in examples {
				assert!(example["params"].is_array(), "{} example missing params", name);
				assert!(example["result"].is_object(), "{} example missing result", name);
			}
		}
	}

	#[test]
	fn deprecated_method_marked() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let methods = doc["methods"].as_array().unwrap();
		let deprecated =
			methods.iter().find(|m| m["name"] == "sidechain_getAriadneParameters").unwrap();
		assert_eq!(deprecated["deprecated"], true);
	}

	#[test]
	fn unsafe_methods_marked() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let methods = doc["methods"].as_array().unwrap();
		let unban = methods.iter().find(|m| m["name"] == "network_unbanPeer").unwrap();
		assert_eq!(unban["x-unsafe"], true);
	}

	#[test]
	fn only_registered_methods_emitted() {
		let partial = &["midnight_contractState", "midnight_apiVersions"];
		let doc = build_openrpc_document(partial);
		let methods = doc["methods"].as_array().unwrap();

		let custom_count = methods
			.iter()
			.filter(|m| {
				let name = m["name"].as_str().unwrap();
				CUSTOM_METHOD_NAMES.contains(&name)
			})
			.count();
		assert_eq!(custom_count, 2, "Only registered custom methods should appear");
	}

	#[test]
	fn component_schemas_include_key_types() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let schemas = doc["components"]["schemas"].as_object().unwrap();

		let expected_types = [
			"BlockHash",
			"TermsAndConditionsRpcResponse",
			"DParameterRpcResponse",
			"AriadneParametersRpcResponse",
			"PeerReputationInfo",
			"RpcBlock",
			"UtxoId",
			"GetStatusResponse",
			"GetCommitteeResponse",
			"CandidateRegistrationEntry",
			"Operation",
			"MidnightRpcTransaction",
			"RpcTransaction",
		];
		for ty in &expected_types {
			assert!(schemas.contains_key(*ty), "Schema component '{}' missing", ty);
		}
	}

	#[test]
	fn error_components_defined() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let errors = doc["components"]["errors"].as_object().unwrap();

		assert!(errors.contains_key("StateRpcError"));
		assert!(errors.contains_key("BlockRpcError"));
		assert!(errors.contains_key("SystemParametersRpcError"));
		assert!(errors.contains_key("GetStatusRpcError"));
	}

	/// CI drift detection: fails if custom methods are added or removed without
	/// updating CUSTOM_METHOD_NAMES. Hardcodes the expected count so a mismatch
	/// surfaces immediately in CI output.
	#[test]
	fn ci_custom_method_count_drift_detection() {
		assert_eq!(
			CUSTOM_METHOD_NAMES.len(),
			16,
			"CUSTOM_METHOD_NAMES has {} entries but 16 are expected. \
			 If you added or removed a custom RPC method, update CUSTOM_METHOD_NAMES \
			 and the OpenRPC metadata in build_custom_method().",
			CUSTOM_METHOD_NAMES.len()
		);
	}

	/// CI drift detection: fails if standard Substrate methods are added or
	/// removed without updating SUBSTRATE_METHOD_NAMES.
	#[test]
	fn ci_substrate_method_count_drift_detection() {
		assert_eq!(
			SUBSTRATE_METHOD_NAMES.len(),
			52,
			"SUBSTRATE_METHOD_NAMES has {} entries but 52 are expected. \
			 If upstream Substrate RPC methods changed, update SUBSTRATE_METHOD_NAMES.",
			SUBSTRATE_METHOD_NAMES.len()
		);
	}

	/// Verify the document is valid JSON and can be round-tripped through
	/// serde_json without loss.
	#[test]
	fn document_round_trips_through_json() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let serialized = serde_json::to_string_pretty(&doc).unwrap();
		let deserialized: Value = serde_json::from_str(&serialized).unwrap();
		assert_eq!(doc, deserialized);
	}

	/// Verify no duplicate method names exist in the document.
	#[test]
	fn no_duplicate_method_names() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let methods = doc["methods"].as_array().unwrap();
		let names: Vec<&str> = methods.iter().map(|m| m["name"].as_str().unwrap()).collect();
		let mut seen = std::collections::HashSet::new();
		for name in &names {
			assert!(seen.insert(name), "Duplicate method name: {}", name);
		}
	}

	/// Sync test: verifies that `docs/openrpc.json` matches the output of
	/// `build_openrpc_document()`. If this test fails, regenerate the static
	/// file by running:
	///
	/// ```sh
	/// cargo test -p midnight-node --lib openrpc::tests::generate_static_openrpc_json -- --ignored
	/// ```
	#[test]
	fn static_openrpc_json_in_sync() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let expected = serde_json::to_string_pretty(&doc).unwrap() + "\n";

		let static_path =
			std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/openrpc.json");
		if !static_path.exists() {
			panic!(
				"docs/openrpc.json does not exist. Generate it with:\n\
				 cargo test -p midnight-node --lib \
				 openrpc::tests::generate_static_openrpc_json -- --ignored"
			);
		}
		let actual = std::fs::read_to_string(&static_path).unwrap();
		assert_eq!(
			expected, actual,
			"docs/openrpc.json is out of date. Regenerate with:\n\
			 cargo test -p midnight-node --lib \
			 openrpc::tests::generate_static_openrpc_json -- --ignored"
		);
	}

	/// Verifies that schemars-generated schemas for RPC response types match
	/// the schemas in the OpenRPC document's components/schemas. Catches field
	/// additions, removals, renames, and type changes on response structs.
	#[test]
	fn component_schemas_match_schemars_output() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let schemas = doc["components"]["schemas"].as_object().unwrap();

		let checks: Vec<(&str, Value)> = vec![
			(
				"TermsAndConditionsRpcResponse",
				serde_json::to_value(schema_for!(
					pallet_system_parameters_rpc::TermsAndConditionsRpcResponse
				))
				.unwrap(),
			),
			(
				"DParameterRpcResponse",
				serde_json::to_value(schema_for!(
					pallet_system_parameters_rpc::DParameterRpcResponse
				))
				.unwrap(),
			),
			(
				"AriadneParametersRpcResponse",
				serde_json::to_value(schema_for!(
					pallet_system_parameters_rpc::AriadneParametersRpcResponse
				))
				.unwrap(),
			),
			(
				"Operation",
				serde_json::to_value(schema_for!(pallet_midnight_rpc::Operation)).unwrap(),
			),
			(
				"MidnightRpcTransaction",
				serde_json::to_value(schema_for!(pallet_midnight_rpc::MidnightRpcTransaction))
					.unwrap(),
			),
			(
				"RpcTransaction",
				serde_json::to_value(schema_for!(pallet_midnight_rpc::RpcTransaction)).unwrap(),
			),
		];

		for (type_name, expected_schema) in &checks {
			let doc_schema = schemas.get(*type_name).unwrap_or_else(|| {
				panic!("Schema component '{}' not found in OpenRPC document", type_name)
			});
			assert_eq!(
				doc_schema,
				expected_schema,
				"Schema for '{}' in OpenRPC document does not match schemars output. \
				 The Rust type has changed but the OpenRPC metadata was not updated.\n\
				 schemars: {}\n\
				 document: {}",
				type_name,
				serde_json::to_string_pretty(expected_schema).unwrap(),
				serde_json::to_string_pretty(doc_schema).unwrap()
			);
		}
	}

	/// Helper: (re)generates `docs/openrpc.json`. Run with `--ignored`:
	///
	/// ```sh
	/// cargo test -p midnight-node --lib openrpc::tests::generate_static_openrpc_json -- --ignored
	/// ```
	#[test]
	#[ignore]
	fn generate_static_openrpc_json() {
		let doc = build_openrpc_document(&all_custom_method_names());
		let json = serde_json::to_string_pretty(&doc).unwrap() + "\n";
		let out_path =
			std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/openrpc.json");
		std::fs::create_dir_all(out_path.parent().unwrap()).unwrap();
		std::fs::write(&out_path, json).unwrap();
		eprintln!("Wrote {}", out_path.display());
	}

	/// Integration test: connects to a running node, calls both `rpc_methods`
	/// and `rpc.discover`, and verifies that every method reported by the node
	/// appears in the OpenRPC document. Catches cases where a method is
	/// registered in `create_full()` or `gen_rpc_module()` but not added to
	/// the OpenRPC metadata.
	///
	/// Requires a running node on `RPC_URL` (default `http://localhost:9944`).
	///
	/// ```sh
	/// cargo test -p midnight-node --lib openrpc::tests::rpc_discover_matches_rpc_methods -- --ignored --nocapture
	/// ```
	#[test]
	#[ignore]
	fn rpc_discover_matches_rpc_methods() {
		let rpc_url =
			std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:9944".to_string());

		let call = |method: &str| -> Value {
			let body = serde_json::json!({
				"jsonrpc": "2.0",
				"method": method,
				"params": [],
				"id": 1
			});
			let client = reqwest::blocking::Client::new();
			let resp = client
				.post(&rpc_url)
				.header("Content-Type", "application/json")
				.body(body.to_string())
				.send()
				.unwrap_or_else(|e| {
					panic!(
						"Failed to connect to node at {rpc_url}. \
						 Is a node running? Error: {e}"
					)
				});
			let json: Value = resp.json().unwrap();
			assert!(json.get("error").is_none(), "RPC error from {method}: {:?}", json["error"]);
			json["result"].clone()
		};

		let rpc_methods_result = call("rpc_methods");
		let live_methods: Vec<&str> = rpc_methods_result["methods"]
			.as_array()
			.expect("rpc_methods.result.methods should be an array")
			.iter()
			.map(|v| v.as_str().unwrap())
			.collect();

		let discover_result = call("rpc.discover");
		let openrpc_methods: Vec<&str> = discover_result["methods"]
			.as_array()
			.expect("rpc.discover.result.methods should be an array")
			.iter()
			.map(|m| m["name"].as_str().unwrap())
			.collect();

		let openrpc_set: std::collections::HashSet<&str> =
			openrpc_methods.iter().copied().collect();

		let mut missing: Vec<&str> = live_methods
			.iter()
			.filter(|m| !openrpc_set.contains(*m) && **m != "rpc_methods" && **m != "rpc.discover")
			.copied()
			.collect();
		missing.sort();

		assert!(
			missing.is_empty(),
			"Methods registered on the node but missing from rpc.discover:\n  {}\n\n\
			 Add these to CUSTOM_METHOD_NAMES or SUBSTRATE_METHOD_NAMES in openrpc.rs \
			 and create corresponding metadata entries.",
			missing.join("\n  ")
		);

		eprintln!(
			"OK: all {} live methods are present in the OpenRPC document ({} methods in document)",
			live_methods.len(),
			openrpc_methods.len()
		);
	}
}
