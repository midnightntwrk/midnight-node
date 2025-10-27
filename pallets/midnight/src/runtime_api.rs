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

//! Runtime API definition for Midnight pallet

use midnight_node_ledger::types::{GasCost, StorageCost, Tx, active_version::LedgerApiError};
use scale_info::prelude::string::String;
use sp_std::vec::Vec;

sp_api::decl_runtime_apis! {
	#[api_version(5)]
	pub trait MidnightRuntimeApi {
		#[changed_in(2)]
		fn get_contract_state(contract_address: Vec<u8>) -> Vec<u8>;
		fn get_contract_state(contract_address: Vec<u8>) -> Result<Vec<u8>, LedgerApiError>;
		#[changed_in(2)]
		fn get_decoded_transaction(transaction_bytes: Vec<u8>) -> Option<Tx>;
		fn get_decoded_transaction(transaction_bytes: Vec<u8>) -> Result<Tx, LedgerApiError>;
		#[changed_in(2)]
		fn get_zswap_chain_state(contract_address: Vec<u8>) -> Vec<u8>;
		fn get_zswap_chain_state(contract_address: Vec<u8>) -> Result<Vec<u8>, LedgerApiError>;
		#[changed_in(5)]
		fn get_network_id() -> Vec<u8>;
		fn get_network_id() -> String;
		fn get_ledger_version() -> Vec<u8>;
		#[changed_in(2)]
		fn get_unclaimed_amount(beneficiary: Vec<u8>) -> u128;
		fn get_unclaimed_amount(beneficiary: Vec<u8>) -> Result<u128, LedgerApiError>;
		fn get_ledger_parameters() -> Result<Vec<u8>, LedgerApiError>;
		fn get_transaction_cost(transaction_bytes: Vec<u8>) -> Result<(StorageCost, GasCost), LedgerApiError>;
		fn get_zswap_state_root() -> Result<Vec<u8>, LedgerApiError>;
	}
}
