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
use strum::{EnumIter, IntoEnumIterator as _};
use subxt::{
	config::substrate::{ConsensusEngineId, DigestItem, SubstrateHeader},
	utils::H256,
};

#[derive(thiserror::Error, Debug)]
pub enum RuntimeVersionError {
	#[error("indexer received a block with invalid node version: {0}")]
	InvalidProtocolVersion(parity_scale_codec::Error),
	#[error("indexer received a block made with unsupported node version {0}")]
	UnsupportedBlockVersion(u32),
	#[error("block header is missing the MNSV runtime-version digest")]
	MissingDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
pub enum RuntimeVersion {
	V0_21_0,
	V0_22_0,
	V1_0_0,
	V1_0_3,
	V2_0_0,
	V2_1_0,
	V2_1_100,
	V3_0_0,
	V4_0_0,
}
impl TryFrom<u32> for RuntimeVersion {
	type Error = RuntimeVersionError;
	fn try_from(value: u32) -> Result<Self, Self::Error> {
		match value {
			000_021_000 => Ok(Self::V0_21_0),
			000_022_000 => Ok(Self::V0_22_0),
			001_000_000 => Ok(Self::V1_0_0),
			// devnet ran spec 001_000_300 ("1.0.300") on the 1.0.x line, alongside the
			// 001_000_003 ("1.0.3") releases. Both are ledger-8 era: `LedgerVersion::
			// from_spec_version` classifies the whole 000_022_000..=001_999_999 range as
			// Ledger8, so they share a decoder and mapping them to one variant is exact.
			001_000_003 | 001_000_300 => Ok(Self::V1_0_3),
			002_000_000 => Ok(Self::V2_0_0),
			002_001_000 => Ok(Self::V2_1_0),
			002_001_100 => Ok(Self::V2_1_100),
			003_000_000 => Ok(Self::V3_0_0),
			004_000_000 => Ok(Self::V4_0_0),
			_ => Err(RuntimeVersionError::UnsupportedBlockVersion(value)),
		}
	}
}

impl RuntimeVersion {
	/// Convert back to the raw spec version number.
	pub fn to_spec_version(self) -> u32 {
		match self {
			Self::V0_21_0 => 000_021_000,
			Self::V0_22_0 => 000_022_000,
			Self::V1_0_0 => 001_000_000,
			Self::V1_0_3 => 001_000_003,
			Self::V2_0_0 => 002_000_000,
			Self::V2_1_0 => 002_001_000,
			Self::V2_1_100 => 002_001_100,
			Self::V3_0_0 => 003_000_000,
			Self::V4_0_0 => 004_000_000,
		}
	}

	pub fn latest_version() -> Self {
		RuntimeVersion::iter().max().unwrap()
	}

	/// Extract the runtime version from a block header by parsing the `MNSV`
	/// consensus digest written by the `version` pallet.
	pub fn from_header(header: &SubstrateHeader<H256>) -> Result<Self, RuntimeVersionError> {
		const VERSION_ID: ConsensusEngineId = *b"MNSV";
		header
			.digest
			.logs
			.iter()
			.find_map(|item| match item {
				DigestItem::Consensus(id, data) if *id == VERSION_ID => {
					Some(RuntimeVersion::try_from(data.as_slice()))
				},
				_ => None,
			})
			.ok_or(RuntimeVersionError::MissingDigest)?
	}
}

impl<'a> TryFrom<&'a [u8]> for RuntimeVersion {
	type Error = RuntimeVersionError;

	fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
		Self::try_from(decode_spec_version(value)?)
	}
}

/// Decodes the raw `spec_version` carried by the `MNSV` digest.
///
/// [`RuntimeVersion`] buckets several spec versions into one variant and
/// [`RuntimeVersion::to_spec_version`] cannot round-trip, so callers that must tell two
/// runtimes inside a bucket apart use this instead.
pub fn decode_spec_version(mut value: &[u8]) -> Result<u32, RuntimeVersionError> {
	use parity_scale_codec::Decode;
	u32::decode(&mut value).map_err(RuntimeVersionError::InvalidProtocolVersion)
}

pub trait MidnightMetadata {
	type Call: subxt::ext::scale_decode::DecodeAsType;
	type SystemTransactionAppliedEvent: subxt::events::DecodeAsEvent;

	fn send_mn_transaction(call: &Self::Call) -> Option<Vec<u8>>;
	fn send_mn_system_transaction(call: &Self::Call) -> Option<Vec<u8>>;
	fn timestamp_set(call: &Self::Call) -> Option<u64>;
	fn system_transaction_applied(event: Self::SystemTransactionAppliedEvent) -> Vec<u8>;
}

macro_rules! impl_midnight_metadata {
	($struct_name:ident, $meta_ident:ident, $meta_module:path) => {
		use $meta_module as $meta_ident;

		pub struct $struct_name;

		impl MidnightMetadata for $struct_name {
			type Call = $meta_ident::Call;
			type SystemTransactionAppliedEvent =
				$meta_ident::midnight_system::events::SystemTransactionApplied;

			fn send_mn_transaction(call: &Self::Call) -> Option<Vec<u8>> {
				if let $meta_ident::Call::Midnight(
					$meta_ident::midnight::Call::send_mn_transaction { midnight_tx },
				) = call
				{
					Some(midnight_tx.clone())
				} else {
					None
				}
			}

			fn send_mn_system_transaction(call: &Self::Call) -> Option<Vec<u8>> {
				if let $meta_ident::Call::MidnightSystem(
					$meta_ident::midnight_system::Call::send_mn_system_transaction {
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
				if let $meta_ident::Call::Timestamp($meta_ident::timestamp::Call::set { now }) =
					call
				{
					Some(*now)
				} else {
					None
				}
			}

			fn system_transaction_applied(event: Self::SystemTransactionAppliedEvent) -> Vec<u8> {
				event.0.serialized_system_transaction
			}
		}
	};
}

impl_midnight_metadata!(
	MidnightMetadata0_21_0,
	mn_meta_0_21_0,
	midnight_node_metadata::midnight_metadata_0_21_0
);

impl_midnight_metadata!(
	MidnightMetadata0_22_0,
	mn_meta_0_22_0,
	midnight_node_metadata::midnight_metadata_0_22_0
);

impl_midnight_metadata!(
	MidnightMetadata1_0_0,
	mn_meta_1_0_0,
	midnight_node_metadata::midnight_metadata_1_0_0
);

impl_midnight_metadata!(
	MidnightMetadata1_0_3,
	mn_meta_1_0_3,
	midnight_node_metadata::midnight_metadata_1_0_3
);

// The ledger-9 runtime (spec 2_000_000) keeps the same extrinsic envelope
// (send_mn_transaction / send_mn_system_transaction / timestamp.set), so the
// 1.0.0 subxt metadata decodes it correctly; only the inner ledger tx bytes
// differ, which are dispatched separately via LedgerVersion.
impl_midnight_metadata!(
	MidnightMetadata2_0_0,
	mn_meta_2_0_0,
	midnight_node_metadata::midnight_metadata_2_0_0
);

impl_midnight_metadata!(
	MidnightMetadata2_1_0,
	mn_meta_2_1_0,
	midnight_node_metadata::midnight_metadata_2_1_0
);

impl_midnight_metadata!(
	MidnightMetadata2_1_100,
	mn_meta_2_1_100,
	midnight_node_metadata::midnight_metadata_2_1_100
);

impl_midnight_metadata!(
	MidnightMetadata3_0_0,
	mn_meta_3_0_0,
	midnight_node_metadata::midnight_metadata_3_0_0
);

impl_midnight_metadata!(
	MidnightMetadata4_0_0,
	mn_meta_4_0_0,
	midnight_node_metadata::midnight_metadata_4_0_0
);

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Encode;

	/// Keying the tblock-correction lookup on the bucket would apply the first runtime
	/// resolved in it to every other runtime in it.
	#[test]
	#[allow(clippy::zero_prefixed_literal)]
	fn bucket_collapses_distinct_runtimes_but_raw_spec_version_does_not() {
		assert_eq!(RuntimeVersion::try_from(001_000_003).unwrap(), RuntimeVersion::V1_0_3);
		assert_eq!(RuntimeVersion::try_from(001_000_300).unwrap(), RuntimeVersion::V1_0_3);

		// Lossy on the way back: a block that ran 001_000_300 reports as 001_000_003.
		assert_eq!(RuntimeVersion::V1_0_3.to_spec_version(), 001_000_003);

		// The digest still carries the real number.
		assert_eq!(decode_spec_version(&001_000_300u32.encode()).unwrap(), 001_000_300);
		assert_eq!(decode_spec_version(&001_000_000u32.encode()).unwrap(), 001_000_000);
	}

	/// Node 2.1.101 ships runtime 2.1.100, whose metadata differs from the 2.1.0 RCs'.
	#[test]
	#[allow(clippy::zero_prefixed_literal)]
	fn runtime_2_1_100_round_trips() {
		let version = RuntimeVersion::try_from(002_001_100).unwrap();
		assert_eq!(version, RuntimeVersion::V2_1_100);
		assert_eq!(version.to_spec_version(), 002_001_100);
	}
}
