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

// //! Benchmarking setup for pallet-midnight

use super::Pallet as Midnight;
use super::*;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{Hooks, StorageInstance},
};
use frame_system::RawOrigin;
use hex::FromHex;
use midnight_node_ledger::{
	host_api::ledger_bridge as LedgerApi,
	types::{
		BenchmarkClaimMintTxBuilder, BenchmarkComponents, BenchmarkStandardTxBuilder, GasCost,
		TransactionDetails,
	},
};
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

const MAX_GCI: u32 = 6;
const MAX_GCO: u32 = 6;
const MAX_GCT: u32 = 6;
const MAX_FCI: u32 = 6;
const MAX_FCO: u32 = 6;
const MAX_FCT: u32 = 6;
const MAX_D: u32 = 6;
const MAX_RA: u32 = 6;
const MAX_VKR: u32 = 6;
const MAX_VKI: u32 = 6;
const MAX_CO: u32 = 6;

const GENESIS_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000037";
const WALLET_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000042";

const MINT_AMOUNT: u128 = 100;
const FOR_FEES: u128 = 5;

const FEE_TOKEN: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const ALT_TOKEN: &str = "0000000000000000000000000000000000000000000000000000000000000001";

const BENEFICIARY: &str = "04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e";
const REWARD: u128 = 500000;

fn build_standard_txs(
	network_id: &Vec<u8>,
	ledger_state_key: &BoundedVec<u8, MaxStateLength>,
	benchmark_components: BenchmarkComponents,
) -> (Vec<u8>, Vec<u8>) {
	let tx_args = BenchmarkStandardTxBuilder {
		genesis_seed: Vec::from_hex(GENESIS_SEED).expect("Invalid genesis seed hex string"),
		wallet_seed: Vec::from_hex(WALLET_SEED).expect("Invalid wallet seed hex string"),
		mint_amount: MINT_AMOUNT,
		fee_per_tx: FOR_FEES,
		token: Vec::from_hex(FEE_TOKEN).expect("Invalid token hex string"),
		alt_token: Vec::from_hex(ALT_TOKEN).expect("Invalid alt token hex string"),
		benchmark_components,
	};
	LedgerApi::build_standard_transactions(network_id, ledger_state_key, tx_args)
		.expect("Transaction should be properly built")
}

fn get_transaction_details<T>(network_id: &Vec<u8>, tx: &Vec<u8>) -> TransactionDetails
where
	_GeneratedPrefixForStorageStateKey<T>: StorageInstance,
{
	let state_key = StateKey::<T>::get().expect("Failed to get state key");
	let runtime_version = 0; // We can ignore the runtime version for benchmarks.

	let (_tx_hash, tx_details) =
		LedgerApi::validate_transaction(network_id, &state_key, &tx, runtime_version).unwrap();

	tx_details
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn send_standard_transaction(
		a: Linear<0, MAX_GCI>, // guaranteed coins inputs
		b: Linear<0, MAX_GCO>, // guaranteed coins outputs
		c: Linear<0, MAX_GCT>, // guaranteed coins transient
		d: Linear<0, MAX_FCI>, // fallible coins inputs
		e: Linear<0, MAX_FCO>, // fallible coins outputs
		f: Linear<0, MAX_FCT>, // fallible coins transient
		g: Linear<0, MAX_D>,   // contract call deploys
		h: Linear<0, MAX_RA>,  // contract call maintain update replace authority
		i: Linear<0, MAX_VKR>, // contract call maintain update verifier key remove
		j: Linear<0, MAX_VKI>, // contract call maintain update verifier key insert
		k: Linear<0, MAX_CO>,  // contract calls operations
	) {
		// SET UP CODE
		let network_id = Midnight::<T>::get_network_id();
		let ledger_state_key = Midnight::<T>::state_key().expect("Ledger State should exists");

		// Build the tx to prefund wallets
		let benchmark_components = BenchmarkComponents {
			num_guaranteed_inputs: a,
			num_guaranteed_outputs: b,
			num_guaranteed_transients: c,
			num_fallible_inputs: d,
			num_fallible_outputs: e,
			num_fallible_transients: f,
			num_contracts_deploy: g,
			num_contract_replace_auth: h,
			num_contract_key_remove: i,
			num_contract_key_insert: j,
			num_contract_operations: k,
		};

		let (setup_tx, tx) =
			build_standard_txs(&network_id, &ledger_state_key, benchmark_components.clone());

		// Send setup_tx
		assert_ok!(Midnight::<T>::send_mn_transaction(RawOrigin::None.into(), setup_tx));

		// CODE TO BENCHMARK
		#[extrinsic_call]
		send_mn_transaction(RawOrigin::None, tx.clone());

		// VERIFICATION
		// Verify the tx details correspond with the built tx
		let tx_details = get_transaction_details::<T>(&network_id, &tx);
		let received_benchmark_components =
			BenchmarkComponents::try_from(&tx_details).expect("Tx should be Standard");

		assert!(benchmark_components == received_benchmark_components);
	}

	#[benchmark]
	fn send_claim_mint_transaction() {
		// SET UP CODE
		let network_id = Midnight::<T>::get_network_id();
		let ledger_state_key = Midnight::<T>::state_key().expect("Ledger State should exists");
		let beneficiary: [u8; 32] = hex::decode(BENEFICIARY)
			.expect("Invalid hex string")
			.try_into()
			.expect("Invalid length, expected 32 bytes");

		assert_ok!(LedgerApi::mint_coins(&network_id, &ledger_state_key, REWARD, &beneficiary[..]));

		let tx_args = BenchmarkClaimMintTxBuilder {
			wallet_seed: Vec::from_hex(WALLET_SEED).expect("Invalid wallet seed hex string"),
			claim_amount: REWARD,
			token: Vec::from_hex(FEE_TOKEN).expect("Invalid token hex string"),
		};

		let tx = LedgerApi::build_claim_mint_transactions(&network_id, &ledger_state_key, tx_args)
			.expect("Transaction should be properly built");

		#[extrinsic_call]
		send_mn_transaction(RawOrigin::None, tx.clone());

		// VERIFICATION
		// Verify the tx details correspond with the built tx
		let tx_details = get_transaction_details::<T>(&network_id, &tx);

		assert!(tx_details == TransactionDetails::ClaimRewards);
	}

	#[benchmark]
	fn execute_contract_call() {
		// SET UP CODE
		let network_id = Midnight::<T>::get_network_id();
		let ledger_state_key = Midnight::<T>::state_key().expect("Ledger State should exists");

		// Build the txs to call one contract
		let benchmark_components = BenchmarkComponents {
			num_guaranteed_inputs: 0,
			num_guaranteed_outputs: 0,
			num_guaranteed_transients: 0,
			num_fallible_inputs: 0,
			num_fallible_outputs: 0,
			num_fallible_transients: 0,
			num_contracts_deploy: 0,
			num_contract_replace_auth: 0,
			num_contract_key_remove: 0,
			num_contract_key_insert: 0,
			num_contract_operations: 1, // One contract call
		};

		let (setup_tx, tx) =
			build_standard_txs(&network_id, &ledger_state_key, benchmark_components.clone());

		// Send setup_tx (deploy a contract)
		assert_ok!(Midnight::<T>::send_mn_transaction(RawOrigin::None.into(), setup_tx));

		// Query the state again after the contract deployment
		let ledger_state_key = Midnight::<T>::state_key().expect("Ledger State should exists");

		// Benchmark the contract call
		#[block]
		{
			LedgerApi::execute_contract_call(&network_id, &ledger_state_key, &tx).unwrap();
		}

		// VERIFICATION
		// Verify the tx details correspond with the built tx
		let tx_details = get_transaction_details::<T>(&network_id, &tx);
		let gas_cost: GasCost = tx_details.clone().into();

		log::info!(target: "benchmarking", "GAS COST: {:?}", gas_cost);

		let received_benchmark_components =
			BenchmarkComponents::try_from(&tx_details).expect("Tx should be Standard");

		assert!(benchmark_components == received_benchmark_components);
	}

	#[benchmark]
	fn deserialize_transaction() {
		// SET UP CODE
		let network_id = Midnight::<T>::get_network_id();
		let ledger_state_key = Midnight::<T>::state_key().expect("Ledger State should exists");

		// Build a heavy tx to better benchmark deserealization
		let benchmark_components = BenchmarkComponents {
			num_guaranteed_inputs: MAX_GCI,
			num_guaranteed_outputs: MAX_GCO,
			num_guaranteed_transients: MAX_GCT,
			num_fallible_inputs: MAX_FCI,
			num_fallible_outputs: MAX_FCO,
			num_fallible_transients: MAX_FCT,
			num_contracts_deploy: MAX_D,
			num_contract_replace_auth: MAX_RA,
			num_contract_key_remove: MAX_VKR,
			num_contract_key_insert: MAX_VKI,
			num_contract_operations: MAX_CO,
		};

		let (setup_tx, tx) =
			build_standard_txs(&network_id, &ledger_state_key, benchmark_components.clone());

		// Send setup_tx
		assert_ok!(Midnight::<T>::send_mn_transaction(RawOrigin::None.into(), setup_tx));

		let tx_bytes = tx.clone();

		// Benchmark the tx deserealization
		#[block]
		{
			LedgerApi::deserialize_transaction(&network_id, &tx_bytes).expect("Tx should be valid");
		}

		// VERIFICATION
		// Verify the tx details correspond with the built tx
		let tx_details = get_transaction_details::<T>(&network_id, &tx);
		let tx_size = tx_bytes.len();

		log::info!(target: "benchmarking", "TX SIZE: {:?}", tx_size);

		let received_benchmark_components =
			BenchmarkComponents::try_from(&tx_details).expect("Tx should be Standard");

		assert!(benchmark_components == received_benchmark_components);
	}

	#[benchmark]
	fn on_finalize() {
		#[block]
		{
			let block_number: u32 = 1;
			Midnight::<T>::on_finalize(block_number.into());
		}
	}
}
