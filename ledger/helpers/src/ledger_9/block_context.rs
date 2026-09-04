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

pub fn make_block_context(
	tblock: crate::ledger_9::base_crypto::time::Timestamp,
	parent_block_hash: crate::ledger_9::base_crypto::hash::HashOutput,
	last_block_time: crate::ledger_9::base_crypto::time::Timestamp,
) -> crate::ledger_9::onchain_runtime::context::BlockContext {
	crate::ledger_9::onchain_runtime::context::BlockContext {
		tblock,
		tblock_err: 30,
		parent_block_hash,
		last_block_time,
	}
}
