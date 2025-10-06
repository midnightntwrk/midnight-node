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

use crate::indexer::IndexerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolVersion {
	V0_17_0,
}
impl ProtocolVersion {
	pub const GENESIS: Self = Self::V0_17_0;
}
impl TryFrom<u32> for ProtocolVersion {
	type Error = IndexerError;
	fn try_from(value: u32) -> Result<Self, Self::Error> {
		match value {
			000_017_000 => Ok(Self::V0_17_0),
			_ => Err(IndexerError::UnsupportedBlockVersion(value)),
		}
	}
}

impl<'a> TryFrom<&'a [u8]> for ProtocolVersion {
	type Error = IndexerError;

	fn try_from(mut value: &'a [u8]) -> Result<Self, Self::Error> {
		use parity_scale_codec::Decode;
		match u32::decode(&mut value) {
			Ok(version) => Self::try_from(version),
			Err(e) => Err(IndexerError::InvalidProtocolVersion(e)),
		}
	}
}

pub trait MidnightProtocol {
	type Call: subxt::ext::scale_decode::DecodeAsType;
	type SystemTransactionAppliedEvent: subxt::ext::subxt_core::events::StaticEvent;

	fn send_mn_transaction(call: &Self::Call) -> Option<Vec<u8>>;
	fn send_mn_system_transaction(call: &Self::Call) -> Option<Vec<u8>>;
	fn timestamp_set(call: &Self::Call) -> Option<u64>;
	fn check_for_events(call: &Self::Call) -> bool;
	fn system_transaction_applied(event: Self::SystemTransactionAppliedEvent) -> Vec<u8>;
}

use midnight_node_metadata::midnight_metadata_0_17_0 as mn_meta_0_17_0;

pub struct MidnightProtocol0_17_0;
impl MidnightProtocol for MidnightProtocol0_17_0 {
	type Call = mn_meta_0_17_0::Call;
	type SystemTransactionAppliedEvent =
		mn_meta_0_17_0::midnight_system::events::SystemTransactionApplied;

	fn send_mn_transaction(call: &Self::Call) -> Option<Vec<u8>> {
		if let mn_meta_0_17_0::Call::Midnight(
			mn_meta_0_17_0::midnight::Call::send_mn_transaction { midnight_tx },
		) = call
		{
			Some(midnight_tx.clone())
		} else {
			None
		}
	}

	fn send_mn_system_transaction(call: &Self::Call) -> Option<Vec<u8>> {
		if let mn_meta_0_17_0::Call::MidnightSystem(
			mn_meta_0_17_0::midnight_system::Call::send_mn_system_transaction {
				midnight_system_tx,
			},
		) = call
		{
			Some(midnight_system_tx.clone())
		} else {
			None
		}
	}

	fn timestamp_set(call: &Self::Call) -> Option<u64> {
		if let mn_meta_0_17_0::Call::Timestamp(mn_meta_0_17_0::timestamp::Call::set { now }) = call
		{
			Some(*now)
		} else {
			None
		}
	}

	fn check_for_events(call: &Self::Call) -> bool {
		matches!(call, mn_meta_0_17_0::Call::NativeTokenObservation(_))
	}

	fn system_transaction_applied(event: Self::SystemTransactionAppliedEvent) -> Vec<u8> {
		event.0.serialized_system_transaction
	}
}
