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

use subxt_signer::SecretUriError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UpgraderError {
	#[error("Secret URI parse error: {0}")]
	UriParseFailed(#[from] SecretUriError),
	#[error("Subxt signer error: {0}")]
	SubxtSignerError(#[from] subxt_signer::sr25519::Error),
	#[error("Subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("BIP error: {0}")]
	BipError(#[from] bip39::Error),
	#[error("serialization error: {0}")]
	SerializationError(std::io::Error),
	#[error("deserialization error: {0}")]
	DeserializationError(std::io::Error),
	#[error("Code upgrade failed: Missing code updated event")]
	CodeUpgradeFailed,
	#[error("Proposal index not found in events")]
	ProposalIndexNotFound,
	#[error("Encoding error: {0}")]
	EncodingError(String),
}

impl actix_web::ResponseError for UpgraderError {}
