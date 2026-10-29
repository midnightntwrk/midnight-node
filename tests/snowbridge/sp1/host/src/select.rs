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

use midnight_beefy::{
	verify, verify_committee, BatchInput, Checkpoint, CommitteeProof, Input, Result,
};

/// Retain authority changes, including announcements omitted by later commitments.
/// Intermediate updates with identical authority state do not need separate proofs.
pub fn select(
	checkpoint: Checkpoint,
	mut captures: Vec<(u32, Vec<u8>, CommitteeProof)>,
	target: u32,
) -> Result<BatchInput> {
	captures.sort_by_key(|(block, _, _)| *block);
	let mut state = checkpoint.clone();
	let mut justifications = Vec::new();
	let mut committee_proofs = Vec::new();
	for (block, bytes, committee) in captures {
		if block <= state.block_number || block > target {
			continue;
		}
		let Ok(output) = verify(&Input { checkpoint: state.clone(), justification: bytes.clone() })
		else {
			continue;
		};
		// A rotation-block MMR may still contain the preceding session snapshot.
		// Wait for a later commitment that confirms the successor in runtime state.
		if verify_committee(&bytes, &committee).is_err() {
			continue;
		}
		if output.new_state.current != state.current
			|| output.new_state.next != state.next
			|| block == target
		{
			justifications.push(bytes);
			committee_proofs.push(committee);
		}
		state = output.new_state;
	}
	if state.block_number != target {
		return Err("missing authenticated authority transition or invalid target proof");
	}
	if justifications.is_empty() || justifications.len() > 64 {
		return Err("batch limit exceeded");
	}
	Ok(BatchInput { checkpoint, justifications, committee_proofs })
}
