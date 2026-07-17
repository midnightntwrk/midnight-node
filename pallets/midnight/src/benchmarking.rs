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

// //! Benchmarking setup for pallet-midnight

use super::*;
use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use midnight_node_ledger::types::{LedgerEvent, LedgerEventSource};

/// Per-event payload size for the event-emission benchmark, in bytes.
///
/// Sized to a deploy-heavy `ContractDeploy`/`ContractLog` event (~4 KiB of
/// tagged `content_tagged_bytes`).
const BENCH_EVENT_PAYLOAD_BYTES: usize = 4 * 1024;

/// The `bytesChurned` per-block ceiling from the shipped network configs
/// (`res/*/ledger-parameters-config.json`). Event volume is transitively bounded
/// by this limit, so the worst-case block the guardrail fills must approximate it.
const BYTES_CHURNED_CEILING: usize = 50_000_000;

/// Upper bound on the number of ledger events deposited in a single block for the
/// worst-case benchmark, anchored to the real `bytesChurned` ceiling:
/// `MAX_BENCH_EVENTS * BENCH_EVENT_PAYLOAD_BYTES` ≈ 50 MB, the effective ceiling a
/// block can carry — not the ~1 MiB the guardrail previously exercised.
const MAX_BENCH_EVENTS: u32 = (BYTES_CHURNED_CEILING / BENCH_EVENT_PAYLOAD_BYTES) as u32;

/// Build `count` synthetic `LedgerEvent`s with worst-case-sized opaque
/// payloads. The payload bytes are not a decodable event — this benchmark
/// measures the runtime-side deposit cost (state-trie write into
/// `frame_system::Events`), which is agnostic to the payload's internal shape.
fn generate_ledger_events(count: u32) -> Vec<LedgerEvent> {
	(0..count)
		.map(|i| {
			let mut transaction_hash = [0u8; 32];
			transaction_hash[0..4].copy_from_slice(&i.to_be_bytes());
			LedgerEvent {
				source: LedgerEventSource {
					transaction_hash,
					logical_segment: 0,
					physical_segment: (i % u16::MAX as u32) as u16,
				},
				content_tagged_bytes: alloc::vec![0u8; BENCH_EVENT_PAYLOAD_BYTES],
			}
		})
		.collect()
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn todo() {
		#[extrinsic_call]
		send_mn_transaction(RawOrigin::None, Vec::default());
	}

	/// Worst-case event-emission guardrail
	///
	/// Fills a block with `n` worst-case-sized ledger events and measures the
	/// cost of depositing them as runtime events.
	///
	/// Component `n`: number of ledger events in the block (0..MAX_BENCH_EVENTS).
	#[benchmark]
	fn bench_block_full_of_events(n: Linear<0, MAX_BENCH_EVENTS>) {
		let events = generate_ledger_events(n);

		#[block]
		{
			for event in events {
				Pallet::<T>::deposit_event(Event::LedgerEvent(event));
			}
		}

		assert_eq!(frame_system::Pallet::<T>::event_count(), n);
	}
}
