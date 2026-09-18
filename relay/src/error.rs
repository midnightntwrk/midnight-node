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

use subxt::rpcs;

use crate::BlockNumber;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Failed to read Beefy keys from {0}")]
	InvalidKeysFile(String),

	#[error("Failed to parse {0}")]
	JsonDecodeError(String),

	#[error("Subxt Error: {0}")]
	Subxt(#[from] subxt::Error),
	#[error("online client error: {0}")]
	OnlineClientError(#[from] subxt::error::OnlineClientError),
	#[error("online client at block error: {0}")]
	OnlineClientAtBlockError(#[from] subxt::error::OnlineClientAtBlockError),
	#[error("runtime api error: {0}")]
	RuntimeApiError(#[from] subxt::error::RuntimeApiError),

	#[error("Rpc Error: {0}")]
	Rpc(#[from] rpcs::Error),

	#[error("Codec Error: {0}")]
	Codec(#[from] parity_scale_codec::Error),

	#[error("Block({0}): commitment signature(1) does not match the validator set")]
	NoMatchingSignature(BlockNumber, u32),

	#[error("Failed to create proof of authorities list")]
	InvalidAuthoritiesProofCreation,

	#[error("No \"Current\" Beefy Stakes found in the payload")]
	MissingCurrentBeefyStakes,

	#[error("No Current Beefy AuthoritySet found in the payload")]
	MissingCurrentAuthoritySet,

	#[error("No \"Next\" Beefy Stakes found in the payload")]
	MissingNextBeefyStakes,

	#[error("No \"Next\" Beefy AuthoritySet found in the payload")]
	MissingNextAuthoritySet,

	#[error("Chain did not return any validator set")]
	EmptyValidatorSet,
}
