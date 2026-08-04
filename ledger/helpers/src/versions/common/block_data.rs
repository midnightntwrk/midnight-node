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

use super::{BlockContext, DB, HashOutput, ProofKind, SerdeTransaction, SignatureKind, Tagged};
use serde::{Deserialize, Serialize};

/// Block data - struct containing all Ledger-relevant data for each block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockData<S: SignatureKind<D> + Tagged, P: ProofKind<D>, D: DB> {
	pub hash: HashOutput,
	pub parent_hash: HashOutput,
	pub number: u64,
	#[serde(bound(
		deserialize = "Vec<SerdeTransaction<S, P, D>>: Deserialize<'de>",
		serialize = "Vec<SerdeTransaction<S, P, D>>: Serialize"
	))]
	pub transactions: Vec<SerdeTransaction<S, P, D>>,
	pub context: BlockContext,
	pub state_root: Option<Vec<u8>>,
}
