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

//! Property tests against a running node.
//!
//! `tx_properties.rs` checks randomly shaped transfers against the ledger in memory. This file
//! sends the same kind of transfers to a live chain and uses the toolkit's own sync as the oracle:
//! the replay that reads the chain back compares its derived ledger state root with the node's
//! `Midnight::StateKey` storage on every block, so a divergence between what the toolkit thinks a
//! transaction did and what the node did fails the sync (and so the test) itself. On top of that
//! the properties check NIGHT balances move exactly as specified, a double spend never moves
//! funds twice, and a replayed transaction never moves funds again.
//!
//! Every operation runs the toolkit binary as its own process, the way the e2e scripts do. Using
//! the library in-process leaks one WebSocket client per fetch worker and per send until the
//! process exits, and a node's default connection limit (100) is reached after a handful of cases.
//!
//! Skipped unless `MN_LIVE_NODE_WS` is set. The wallets come from `MN_LIVE_SEEDS` (comma-separated
//! hex seeds) and default to the four the micro-dao e2e funds in its local genesis; the network id
//! from `MN_LIVE_NETWORK` (default `local`); the binary from `TOOLKIT_BIN` (default the release
//! build). Every case costs at least one block, so the case counts are small.

use proptest::prelude::*;
use std::{
	collections::{BTreeMap, HashMap},
	path::{Path, PathBuf},
	process::Command,
	sync::{Mutex, OnceLock, PoisonError},
};

const DEFAULT_SEEDS: [&str; 4] = [
	"0000000000000000000000000000000000000000000000000000000000000002",
	"0000000000000000000000000000000000000000000000000000000000000003",
	"0000000000000000000000000000000000000000000000000000000000000004",
	"0000000000000000000000000000000000000000000000000000000000000005",
];
const NIGHT_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";

struct Live {
	node_ws: String,
	network: String,
	seeds: Vec<String>,
	bin: PathBuf,
	work_dir: tempfile::TempDir,
	addresses: Mutex<HashMap<usize, String>>,
}

fn live() -> Option<&'static Live> {
	static LIVE: OnceLock<Option<Live>> = OnceLock::new();
	LIVE.get_or_init(|| {
		let node_ws = std::env::var("MN_LIVE_NODE_WS").ok()?;
		let seeds: Vec<String> = std::env::var("MN_LIVE_SEEDS")
			.map(|s| s.split(',').map(str::to_owned).collect())
			.unwrap_or_else(|_| DEFAULT_SEEDS.iter().map(|s| s.to_string()).collect());
		assert!(seeds.len() >= 2, "need at least two funded seeds");
		let bin = std::env::var("TOOLKIT_BIN").map(PathBuf::from).unwrap_or_else(|_| {
			PathBuf::from(env!("CARGO_MANIFEST_DIR"))
				.join("../../target/release/midnight-node-toolkit")
		});
		assert!(bin.exists(), "toolkit binary not found at {}", bin.display());
		Some(Live {
			node_ws,
			network: std::env::var("MN_LIVE_NETWORK").unwrap_or_else(|_| "local".into()),
			seeds,
			bin,
			work_dir: tempfile::tempdir().expect("tempdir"),
			addresses: Mutex::new(HashMap::new()),
		})
	})
	.as_ref()
}

impl Live {
	/// One toolkit process. Wallet caches are per seed because both caches are single-writer.
	fn toolkit(&self, seed: usize, args: &[&str]) -> anyhow::Result<String> {
		let cache = self.work_dir.path().join(format!("cache-{seed}"));
		std::fs::create_dir_all(&cache)?;
		let output = Command::new(&self.bin)
			.args(args)
			.env("MN_FETCH_BEST", "true")
			.env("MN_NO_WAIT_FINALITY", "true")
			.env("MN_LEDGER_CACHE_DB", cache.join("ledger_state_db"))
			.output()?;
		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			let tail: Vec<&str> = stderr.lines().rev().take(6).collect();
			anyhow::bail!(
				"toolkit {} failed ({}): {}",
				args.first().copied().unwrap_or(""),
				output.status,
				tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
			);
		}
		Ok(String::from_utf8(output.stdout)?)
	}

	fn fetch_cache(&self, seed: usize) -> String {
		format!("redb:{}", self.work_dir.path().join(format!("cache-{seed}/fetch.redb")).display())
	}

	fn address(&self, seed: usize) -> anyhow::Result<String> {
		if let Some(a) = self.addresses.lock().unwrap_or_else(PoisonError::into_inner).get(&seed) {
			return Ok(a.clone());
		}
		let out = self.toolkit(
			seed,
			&[
				"show-address",
				"--network",
				&self.network,
				"--seed",
				&self.seeds[seed],
				"--unshielded",
			],
		)?;
		let address = out.trim().to_string();
		self.addresses
			.lock()
			.unwrap_or_else(PoisonError::into_inner)
			.insert(seed, address.clone());
		Ok(address)
	}

	/// NIGHT held by one wallet, from a fresh sync (which also checks every block's state root).
	fn night(&self, seed: usize) -> anyhow::Result<u128> {
		let out = self.toolkit(
			seed,
			&[
				"show-wallet",
				"--seed",
				&self.seeds[seed],
				"-s",
				&self.node_ws,
				"--fetch-cache",
				&self.fetch_cache(seed),
			],
		)?;
		let wallet: serde_json::Value = serde_json::from_str(&out)?;
		let utxos = wallet["utxos"].as_array().ok_or_else(|| anyhow::anyhow!("no utxos array"))?;
		Ok(utxos
			.iter()
			.filter(|u| u["token_type"].as_str() == Some(NIGHT_HEX))
			.map(|u| u["value"].as_u64().unwrap_or(0) as u128)
			.sum())
	}

	fn balances(&self) -> anyhow::Result<Vec<u128>> {
		(0..self.seeds.len()).map(|i| self.night(i)).collect()
	}

	/// Builds the transfer against the chain as it is now and writes it to a file.
	fn build(&self, spec: &TxSpec, name: &str) -> anyhow::Result<PathBuf> {
		let file = self.work_dir.path().join(format!("{name}.tx.mn"));
		let mut args = vec![
			"generate-txs".to_string(),
			"-s".into(),
			self.node_ws.clone(),
			"--fetch-cache".into(),
			self.fetch_cache(spec.sender),
			"--dest-file".into(),
			file.to_string_lossy().into_owned(),
			"single-tx".into(),
			"--source-seed".into(),
			self.seeds[spec.sender].clone(),
			"--coin-selection".into(),
			spec.coin_selection.into(),
			"--rng-seed".into(),
			hex::encode(spec.rng_seed),
		];
		for (receiver, amount) in spec.per_receiver() {
			args.push("--output".into());
			args.push(format!("addr={},amount={amount}", self.address(receiver)?));
		}
		let args: Vec<&str> = args.iter().map(String::as_str).collect();
		self.toolkit(spec.sender, &args)?;
		Ok(file)
	}

	/// Submits a built transaction and waits for best-block inclusion; `Err` when the pool or
	/// the block author rejected it.
	fn send(&self, file: &Path) -> anyhow::Result<()> {
		let file = file.to_string_lossy().to_string();
		self.toolkit(
			0,
			&["generate-txs", "--src-file", &file, "-r", "1", "send", "-d", &self.node_ws],
		)
		.map(|_| ())
	}
}

#[derive(Clone, Debug)]
struct TxSpec {
	sender: usize,
	/// (receiver index, amount); receivers may repeat and never equal the sender
	outputs: Vec<(usize, u128)>,
	coin_selection: &'static str,
	rng_seed: [u8; 32],
}

impl TxSpec {
	fn total(&self) -> u128 {
		self.outputs.iter().map(|(_, a)| a).sum()
	}

	fn per_receiver(&self) -> BTreeMap<usize, u128> {
		let mut totals = BTreeMap::new();
		for (receiver, amount) in &self.outputs {
			*totals.entry(*receiver).or_default() += amount;
		}
		totals
	}

	/// Signed NIGHT change expected for every wallet; fees are paid in DUST.
	fn expected_deltas(&self, wallets: usize) -> Vec<i128> {
		let mut deltas = vec![0i128; wallets];
		deltas[self.sender] -= self.total() as i128;
		for (r, a) in &self.outputs {
			deltas[*r] += *a as i128;
		}
		deltas
	}
}

fn tx_spec_strategy(wallets: usize) -> impl Strategy<Value = TxSpec> {
	(0..wallets).prop_flat_map(move |sender| {
		let receiver = (0..wallets - 1).prop_map(move |i| if i >= sender { i + 1 } else { i });
		let output = (receiver, 1_000u128..=5_000_000u128);
		(
			Just(sender),
			prop::collection::vec(output, 1..=3),
			prop_oneof![Just("largest-first"), Just("smallest-first")],
			any::<[u8; 32]>(),
		)
			.prop_map(|(sender, outputs, coin_selection, rng_seed)| TxSpec {
				sender,
				outputs,
				coin_selection,
				rng_seed,
			})
	})
}

fn deltas(before: &[u128], after: &[u128]) -> Vec<i128> {
	before.iter().zip(after).map(|(b, a)| *a as i128 - *b as i128).collect()
}

fn fail(e: anyhow::Error) -> TestCaseError {
	TestCaseError::fail(format!("{e:#}"))
}

fn wallets() -> usize {
	live().map_or(2, |l| l.seeds.len())
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 4, max_shrink_iters: 0, .. ProptestConfig::default() })]

	/// Whatever the node included, the toolkit's replay agrees with it block by block (else the
	/// balance reads fail), and NIGHT moved exactly as the transfer said.
	#[test]
	fn live_transfer_moves_exactly_what_it_says(spec in tx_spec_strategy(wallets())) {
		let Some(live) = live() else { return Ok(()) };
		(|| {
			let before = live.balances()?;
			let tx = live.build(&spec, "transfer")?;
			live.send(&tx)?;
			let after = live.balances()?;
			let expected = spec.expected_deltas(live.seeds.len());
			anyhow::ensure!(
				deltas(&before, &after) == expected,
				"balances moved {:?}, expected {expected:?} for {spec:?}",
				deltas(&before, &after)
			);
			Ok(())
		})()
		.map_err(fail)?;
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 3, max_shrink_iters: 0, .. ProptestConfig::default() })]

	/// Two transfers built from the same chain state spend the same coins. Whatever the pool and
	/// the block author do with the second one, funds move once.
	#[test]
	fn live_double_spend_moves_funds_once(spec in tx_spec_strategy(wallets())) {
		let Some(live) = live() else { return Ok(()) };
		(|| {
			let before = live.balances()?;
			let first = live.build(&spec, "first")?;
			let mut rival_spec = spec.clone();
			rival_spec.rng_seed.reverse();
			let rival = live.build(&rival_spec, "rival")?;
			live.send(&first)?;
			let rival_outcome = live.send(&rival).map_err(|e| e.to_string());
			let after = live.balances()?;
			let expected = spec.expected_deltas(live.seeds.len());
			anyhow::ensure!(
				deltas(&before, &after) == expected,
				"double spend moved funds twice: {:?}, expected {expected:?} (rival send: {rival_outcome:?})",
				deltas(&before, &after)
			);
			Ok(())
		})()
		.map_err(fail)?;
	}

	/// Sending an included transaction again never moves funds again.
	#[test]
	fn live_replay_moves_nothing(spec in tx_spec_strategy(wallets())) {
		let Some(live) = live() else { return Ok(()) };
		(|| {
			let tx = live.build(&spec, "replayed")?;
			live.send(&tx)?;
			let after_first = live.balances()?;
			let replay_outcome = live.send(&tx).map_err(|e| e.to_string());
			let after_replay = live.balances()?;
			anyhow::ensure!(
				after_first == after_replay,
				"replay moved funds: {:?} (replay send: {replay_outcome:?})",
				deltas(&after_first, &after_replay)
			);
			Ok(())
		})()
		.map_err(fail)?;
	}
}
