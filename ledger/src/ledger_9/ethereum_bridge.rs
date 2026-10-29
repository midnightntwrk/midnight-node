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

//! Read the compiler-0.34 escrow layout from committed ledger state, without running Compact.
use super::transient_crypto_local::commitment::PureGeneratorPedersen;
use super::{
	Bridge, api, base_crypto_local, coin_structure_local, ledger_storage_local, mn_ledger_local,
	onchain_runtime_local,
};
use crate::ethereum_bridge::{EscrowConfig, EscrowError, Redemption};
use base_crypto_local::{
	fab::{AlignedValue, AlignmentAtom, AlignmentSegment},
	hash::HashOutput,
};
use coin_structure_local::{coin::TokenType, contract::ContractAddress};
use ledger_storage_local::db::DB;
use midnight_serialize::Tagged;
use mn_ledger_local::structure::{ProofMarker, SignatureKind};
use onchain_runtime_local::state::{ContractState, StateValue};
use sha2::digest::{OutputSizeUser, consts::U32};

const CAPACITY: u64 = 64;
const MAX_AMOUNT: u128 = 1_000_000_000_000;

fn cell<D: DB>(state: &StateValue<D>) -> Result<&AlignedValue, EscrowError> {
	match state {
		StateValue::Cell(value) => Ok(value),
		_ => Err(EscrowError::Schema),
	}
}

fn bytes<const N: usize>(value: &AlignedValue, index: usize) -> Result<[u8; N], EscrowError> {
	if value.alignment.0.get(index)
		!= Some(&AlignmentSegment::Atom(AlignmentAtom::Bytes { length: N as u32 }))
	{
		return Err(EscrowError::Schema);
	}
	let atom = value.value.0.get(index).ok_or(EscrowError::Schema)?;
	if atom.0.len() > N {
		return Err(EscrowError::Schema);
	}
	let mut result = [0; N];
	result[..atom.0.len()].copy_from_slice(&atom.0);
	Ok(result)
}

fn single<const N: usize, D: DB>(state: &StateValue<D>) -> Result<[u8; N], EscrowError> {
	let value = cell(state)?;
	if value.value.0.len() != 1 || value.alignment.0.len() != 1 {
		return Err(EscrowError::Schema);
	}
	bytes(value, 0)
}

fn checked_fields<'a, D: DB>(
	state: &'a ContractState<D>,
	config: &EscrowConfig,
) -> Result<&'a ledger_storage_local::storage::Array<StateValue<D>, D>, EscrowError>
where
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	if !state.maintenance_authority.committee.is_empty()
		|| state.maintenance_authority.threshold != 1
		|| state.maintenance_authority.counter != 0
	{
		return Err(EscrowError::Unsealed);
	}
	if state.operations.size() != 1 {
		return Err(EscrowError::Operation);
	}
	let operation =
		state.operations.get(&b"retire".to_vec().into()).ok_or(EscrowError::Operation)?;
	if <[u8; 32]>::from(operation.hash().0) != config.retire_operation {
		return Err(EscrowError::Operation);
	}
	let StateValue::Array(fields) = state.data.get_ref() else { return Err(EscrowError::Schema) };
	if fields.len() != 6 {
		return Err(EscrowError::Schema);
	}
	if single::<1, D>(fields.get(0).ok_or(EscrowError::Schema)?)? != [1] {
		return Err(EscrowError::Schema);
	}
	let domain = single::<32, D>(fields.get(1).ok_or(EscrowError::Schema)?)?;
	let namespace = single::<32, D>(fields.get(2).ok_or(EscrowError::Schema)?)?;
	if domain != config.bridge_domain
		|| namespace != config.token_domain
		|| ContractAddress(HashOutput(config.escrow))
			.custom_unshielded_token_type(HashOutput(namespace))
			.0
			.0 != config.color
	{
		return Err(EscrowError::Configuration);
	}
	Ok(fields)
}

/// Validate an empty, immutable escrow before activating a bridge registry.
pub fn validate_escrow<D: DB>(
	state: &ContractState<D>,
	config: &EscrowConfig,
) -> Result<(), EscrowError>
where
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	let fields = checked_fields(state, config)?;
	if single::<8, D>(fields.get(3).ok_or(EscrowError::Schema)?)? != [0; 8]
		|| single::<16, D>(fields.get(4).ok_or(EscrowError::Schema)?)? != [0; 16]
	{
		return Err(EscrowError::Range);
	}
	let StateValue::Map(records) = fields.get(5).ok_or(EscrowError::Schema)? else {
		return Err(EscrowError::Schema);
	};
	if records.size() != 0 || state.balance.size() != 0 {
		return Err(EscrowError::Balance);
	}
	Ok(())
}

/// Six fixed array fields, one map lookup, one operation hash and one balance lookup.
/// The enclosing runtime must supply its frozen registry and canonical state key.
pub fn read_redemption<D: DB>(
	state: &ContractState<D>,
	config: &EscrowConfig,
	sequence: u64,
) -> Result<Redemption, EscrowError>
where
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	let fields = checked_fields(state, config)?;
	let domain = config.bridge_domain;
	let namespace = config.token_domain;
	let count = u64::from_le_bytes(single::<8, D>(fields.get(3).ok_or(EscrowError::Schema)?)?);
	let total = u128::from_le_bytes(single::<16, D>(fields.get(4).ok_or(EscrowError::Schema)?)?);
	let color = ContractAddress(HashOutput(config.escrow))
		.custom_unshielded_token_type(HashOutput(namespace));
	if domain != config.bridge_domain
		|| namespace != config.token_domain
		|| color.0.0 != config.color
	{
		return Err(EscrowError::Configuration);
	}
	if count > CAPACITY || sequence >= count || total == 0 || total > u128::from(count) * MAX_AMOUNT
	{
		return Err(EscrowError::Range);
	}
	let StateValue::Map(records) = fields.get(5).ok_or(EscrowError::Schema)? else {
		return Err(EscrowError::Schema);
	};
	if records.size() != count as usize {
		return Err(EscrowError::Schema);
	}
	let record = records.get(&AlignedValue::from(sequence)).ok_or(EscrowError::RecordMissing)?;
	let value = cell(&record)?;
	if value.value.0.len() != 2 || value.alignment.0.len() != 2 {
		return Err(EscrowError::Schema);
	}
	let amount = u128::from_le_bytes(bytes::<16>(value, 0)?);
	let recipient = bytes::<20>(value, 1)?;
	if amount == 0 || amount > MAX_AMOUNT || amount > total || recipient == [0; 20] {
		return Err(EscrowError::Range);
	}
	let balance = state.balance.get(&TokenType::Unshielded(color)).map(|v| *v).unwrap_or(0);
	if balance < total {
		return Err(EscrowError::Balance);
	}
	Ok(Redemption {
		magic: *b"MNBR",
		version: 1,
		bridge_domain: domain,
		escrow: config.escrow,
		sequence,
		amount,
		recipient,
		color: config.color,
		total_retired: total,
		count: count as u16,
		ledger_balance: balance,
	})
}

impl<S: SignatureKind<D> + core::fmt::Debug, D: DB> Bridge<S, D>
where
	mn_ledger_local::structure::Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Tagged,
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	pub fn validate_bridge_escrow(
		state_key: &[u8],
		config: EscrowConfig,
	) -> Result<(), EscrowError> {
		let ledger =
			Self::get_ledger(&api::new(), state_key).map_err(|_| EscrowError::LedgerUnavailable)?;
		let contract = ledger
			.state
			.index(ContractAddress(HashOutput(config.escrow)))
			.ok_or(EscrowError::ContractMissing)?;
		validate_escrow(&contract, &config)
	}

	pub fn get_bridge_redemption(
		state_key: &[u8],
		config: EscrowConfig,
		sequence: u64,
	) -> Result<Redemption, EscrowError> {
		let ledger =
			Self::get_ledger(&api::new(), state_key).map_err(|_| EscrowError::LedgerUnavailable)?;
		let contract = ledger
			.state
			.index(ContractAddress(HashOutput(config.escrow)))
			.ok_or(EscrowError::ContractMissing)?;
		read_redemption(&contract, &config, sequence)
	}
}

#[cfg(feature = "ethereum-bridge-poc")]
impl<S: SignatureKind<D> + core::fmt::Debug, D: DB> Bridge<S, D>
where
	mn_ledger_local::structure::Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Tagged,
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	pub fn create_ethereum_credit(
		credit: crate::ethereum_bridge::EthereumCredit,
	) -> Result<alloc::vec::Vec<u8>, super::types::LedgerApiError> {
		use coin_structure_local::coin::{UnshieldedTokenType, UserAddress};
		use mn_ledger_local::structure::{IntentHash, SystemTransaction};
		api::new().tagged_serialize(&SystemTransaction::CreditEthereumAsset {
			deposit_id: HashOutput(credit.deposit_id),
			output_id: IntentHash(HashOutput(credit.output_id)),
			token_type: UnshieldedTokenType(HashOutput(credit.color)),
			amount: credit.amount,
			recipient: UserAddress(HashOutput(credit.recipient)),
		})
	}

	pub fn apply_ethereum_system_transaction(
		externalities: &mut dyn sp_externalities::Externalities,
		state_key: &[u8],
		tx: &[u8],
		context: super::BlockContext,
	) -> Result<crate::types::SystemTransactionAppliedStateRoot, super::types::LedgerApiError> {
		Self::apply_system_transaction_checked(externalities, state_key, tx, context, |tx| {
			matches!(tx, mn_ledger_local::structure::SystemTransaction::CreditEthereumAsset { .. })
		})
	}
}
