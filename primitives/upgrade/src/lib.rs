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

#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_inherents::{InherentData, InherentIdentifier};

#[cfg(feature = "std")]
use {
	sc_executor::{error::WasmError, read_embedded_version},
	sc_executor_common::runtime_blob::RuntimeBlob,
	sp_core::hashing::blake2_256,
	sp_partner_chains_consensus_aura::InherentDigest,
	sp_runtime::DigestItem,
	std::error::Error,
	std::fmt::{Display, Formatter},
};

#[derive(
	Clone,
	Debug,
	Decode,
	DecodeWithMemTracking,
	Default,
	Encode,
	Eq,
	MaxEncodedLen,
	PartialEq,
	TypeInfo,
)]
pub struct UpgradeProposal {
	pub spec_version: u32,
	pub runtime_hash: H256,
}

pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"mnrupgrd";
const MIDNIGHT_UPGRADE_DIGEST_ID: [u8; 4] = *b"mnup";

pub type InherentType = UpgradeProposal;

#[cfg(feature = "std")]
impl UpgradeProposal {
	pub fn new(spec_version: u32, runtime_hash: H256) -> Self {
		Self { spec_version, runtime_hash }
	}

	/// Interprets an upgrade proposal based on provided runtime WASM bytes
	pub fn from_embedded_runtime(runtime_code: &[u8]) -> Result<Self, WasmError> {
		let runtime_hash = H256::from_slice(&blake2_256(runtime_code));
		let runtime_blob = RuntimeBlob::uncompress_if_needed(runtime_code)?;
		let version_maybe = read_embedded_version(&runtime_blob)?;
		let spec_version = version_maybe.ok_or(WasmError::CodeNotFound)?.spec_version;

		Ok(Self { spec_version, runtime_hash })
	}

	// Functioning for getting struct from an input such as a CLI, where not all arguments are guaranteed
	pub fn from_optional_args(
		spec_version: Option<u32>,
		runtime_hash: Option<H256>,
	) -> Option<Self> {
		match (spec_version, runtime_hash) {
			(Some(spec_version), Some(runtime_hash)) => Some(Self::new(spec_version, runtime_hash)),
			_ => None,
		}
	}

	pub fn from_upgrade_proposal(upgrade_proposal: UpgradeProposal) -> Vec<DigestItem> {
		vec![DigestItem::PreRuntime(MIDNIGHT_UPGRADE_DIGEST_ID, upgrade_proposal.encode())]
	}
}

#[cfg(feature = "std")]
impl InherentDigest for UpgradeProposal {
	type Value = UpgradeProposal;

	fn from_inherent_data(
		inherent_data: &InherentData,
	) -> Result<Vec<sp_runtime::DigestItem>, Box<dyn Error + Send + Sync>> {
		let upgrade_proposal = inherent_data
			.get_data::<UpgradeProposal>(&INHERENT_IDENTIFIER)
			.map_err(|err| {
				format!("Failed to retrieve upgrade proposal from inherent data: {err}")
			})?
			.ok_or("Upgrade proposal missing from inherent data".to_string())?;
		Ok(Self::from_upgrade_proposal(upgrade_proposal))
	}

	fn value_from_digest(
		digest: &[DigestItem],
	) -> Result<Self::Value, Box<dyn Error + Send + Sync>> {
		for item in digest {
			if let DigestItem::PreRuntime(id, data) = item {
				if *id == MIDNIGHT_UPGRADE_DIGEST_ID {
					let upgrade_proposal: UpgradeProposal = Decode::decode(&mut &data[..])?;
					return Ok(upgrade_proposal);
				}
			}
		}
		Err("Main chain block hash missing from digest".into())
	}
}

#[cfg(feature = "std")]
impl Display for UpgradeProposal {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"[spec_version: {}, hash: 0x{}]",
			self.spec_version,
			hex::encode(self.runtime_hash)
		)
	}
}

/// Auxiliary trait to extract runtime upgrade proposal inherent data.
pub trait UpgradeProposalInherentData {
	/// Get runtime upgrade proposal inherent data.
	fn upgrade_proposal_inherent_data(&self) -> Result<Option<InherentType>, sp_inherents::Error>;
}

impl UpgradeProposalInherentData for InherentData {
	fn upgrade_proposal_inherent_data(&self) -> Result<Option<InherentType>, sp_inherents::Error> {
		self.get_data(&INHERENT_IDENTIFIER)
	}
}

#[cfg(feature = "std")]
pub struct InherentDataProvider(Option<InherentType>);

#[cfg(feature = "std")]
impl InherentDataProvider {
	pub fn propose(proposal: InherentType) -> Self {
		Self(Some(proposal))
	}

	pub fn skip() -> Self {
		Self(None)
	}
}

#[cfg(feature = "std")]
impl std::ops::Deref for InherentDataProvider {
	type Target = Option<InherentType>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for InherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		if let Some(ref proposal) = self.0 {
			inherent_data.put_data(INHERENT_IDENTIFIER, proposal)
		} else {
			Ok(())
		}
	}

	async fn try_handle_error(
		&self,
		identifier: &InherentIdentifier,
		_error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		if *identifier == INHERENT_IDENTIFIER {
			panic!("BUG: {INHERENT_IDENTIFIER:?} inherent shouldn't return any errors")
		} else {
			None
		}
	}
}
