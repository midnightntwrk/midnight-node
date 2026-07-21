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

//! `replay-check`: replay a chain from a source (RPC or file) and test every
//! block, its intermediate ledger states, and its individual transactions
//! against a registry of per-ledger-version [`Predicate`]s, so that known,
//! detectable vulnerabilities can be scanned for across real chain history.
//!
//! [`Predicate`]: crate::commands::fork::ledger_8::replay_check::Predicate

use core::fmt::Display;

use clap::Args;
use midnight_node_ledger_helpers::fork::{
	fork_aware_context::{ForkAwareLedgerContext, fork_context_7_to_8, fork_context_8_to_9},
	raw_block_data::{LedgerVersion, RawBlockData},
};
use serde::Serialize;

use crate::commands::fork::{ledger_7, ledger_8, ledger_9};
use crate::progress::Progress;
use crate::{TxGenerator, source::Source};

#[derive(Args)]
pub struct ReplayCheckArgs {
	#[command(flatten)]
	pub source: Source,
	/// Only run predicates whose name contains this substring (repeatable).
	/// Default: run all registered predicates.
	#[arg(long)]
	pub predicate: Vec<String>,
	/// Stop at the first block that produces a violation instead of scanning
	/// the whole history and reporting all violations at the end
	#[arg(long)]
	pub fail_fast: bool,
	/// List the registered predicates per ledger version and exit
	#[arg(long)]
	pub list_predicates: bool,
	/// Only observe blocks with number >= this (earlier blocks are still
	/// replayed to reconstruct state, but predicates don't run on them)
	#[arg(long)]
	pub from_block: Option<u64>,
	/// Stop replaying after this block number
	#[arg(long)]
	pub to_block: Option<u64>,
	/// Output the report as JSON
	#[arg(long)]
	pub json: bool,
	/// Dry-run - don't fetch or replay anything, just print out settings
	#[arg(long)]
	pub dry_run: bool,
}

/// A single predicate finding, reported at the end of the scan.
#[derive(Clone, Debug, Serialize)]
pub struct Violation {
	/// Name of the predicate that fired.
	pub predicate: String,
	pub block_number: u64,
	#[serde(with = "hex")]
	pub block_hash: [u8; 32],
	/// Index of the offending transaction within the block; `None` for
	/// block-level findings.
	pub tx_index: Option<usize>,
	#[serde(serialize_with = "serialize_opt_hex")]
	pub tx_hash: Option<[u8; 32]>,
	pub message: String,
}

fn serialize_opt_hex<S: serde::Serializer>(
	value: &Option<[u8; 32]>,
	serializer: S,
) -> Result<S::Ok, S::Error> {
	match value {
		Some(bytes) => serializer.serialize_some(&hex::encode(bytes)),
		None => serializer.serialize_none(),
	}
}

impl Violation {
	/// A block-level finding (no specific transaction).
	pub fn block_level(predicate: &str, block: &RawBlockData, message: impl Into<String>) -> Self {
		Self {
			predicate: predicate.to_string(),
			block_number: block.number,
			block_hash: block.hash,
			tx_index: None,
			tx_hash: None,
			message: message.into(),
		}
	}

	/// A finding against one transaction of the block.
	pub fn tx_level(
		predicate: &str,
		block: &RawBlockData,
		tx_index: usize,
		tx_hash: [u8; 32],
		message: impl Into<String>,
	) -> Self {
		Self {
			predicate: predicate.to_string(),
			block_number: block.number,
			block_hash: block.hash,
			tx_index: Some(tx_index),
			tx_hash: Some(tx_hash),
			message: message.into(),
		}
	}
}

impl Display for Violation {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"[{}] block #{} (0x{})",
			self.predicate,
			self.block_number,
			hex::encode(self.block_hash)
		)?;
		if let Some(tx_index) = self.tx_index {
			write!(f, " tx #{tx_index}")?;
			if let Some(tx_hash) = self.tx_hash {
				write!(f, " (0x{})", hex::encode(tx_hash))?;
			}
		}
		write!(f, ": {}", self.message)
	}
}

#[derive(Debug, Serialize)]
pub struct ReplayCheckReport {
	/// Blocks replayed (applied to the ledger state).
	pub blocks_scanned: u64,
	/// Blocks the predicates observed (within `--from-block` bounds).
	pub blocks_observed: u64,
	/// True when `--fail-fast` stopped the scan at the first violation.
	pub aborted: bool,
	pub violations: Vec<Violation>,
}

impl Display for ReplayCheckReport {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(
			f,
			"replay-check: {} block(s) replayed, {} observed{}",
			self.blocks_scanned,
			self.blocks_observed,
			if self.aborted { " (aborted at first violation)" } else { "" }
		)?;
		if self.violations.is_empty() {
			write!(f, "No violations found")
		} else {
			writeln!(f, "{} violation(s) found:", self.violations.len())?;
			for violation in &self.violations {
				writeln!(f, "  {violation}")?;
			}
			Ok(())
		}
	}
}

#[derive(Debug, Serialize)]
pub struct PredicateInfo {
	pub name: String,
	pub description: String,
}

#[derive(Debug, Serialize)]
pub struct PredicateListing {
	pub ledger_7: Vec<PredicateInfo>,
	pub ledger_8: Vec<PredicateInfo>,
	pub ledger_9: Vec<PredicateInfo>,
}

impl Display for PredicateListing {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		for (version, infos) in [
			("ledger_7", &self.ledger_7),
			("ledger_8", &self.ledger_8),
			("ledger_9", &self.ledger_9),
		] {
			writeln!(f, "{version}:")?;
			if infos.is_empty() {
				writeln!(f, "  (none)")?;
			}
			for info in infos {
				writeln!(f, "  {} - {}", info.name, info.description)?;
			}
		}
		Ok(())
	}
}

pub enum ReplayCheckResult {
	Human(ReplayCheckReport),
	Json(ReplayCheckReport),
	ListHuman(PredicateListing),
	ListJson(PredicateListing),
	DryRun(()),
}

/// The per-ledger-version predicate sets a scan runs with. [`execute`] uses
/// the default registries from `fork::ledger_{7,8,9}::predicates()`; tests
/// (and future embedders) can inject their own via [`execute_with_registries`].
pub struct PredicateRegistries {
	pub ledger_7: Vec<Box<dyn ledger_7::replay_check::Predicate>>,
	pub ledger_8: Vec<Box<dyn ledger_8::replay_check::Predicate>>,
	pub ledger_9: Vec<Box<dyn ledger_9::replay_check::Predicate>>,
}

impl PredicateRegistries {
	pub fn defaults() -> Self {
		Self {
			ledger_7: ledger_7::predicates(),
			ledger_8: ledger_8::predicates(),
			ledger_9: ledger_9::predicates(),
		}
	}

	/// Keep only predicates whose name contains any of `patterns`
	/// (no patterns = keep all).
	fn filter(&mut self, patterns: &[String]) {
		if patterns.is_empty() {
			return;
		}
		let matches = |name: &str| patterns.iter().any(|p| name.contains(p.as_str()));
		self.ledger_7.retain(|p| matches(p.name()));
		self.ledger_8.retain(|p| matches(p.name()));
		self.ledger_9.retain(|p| matches(p.name()));
	}

	fn listing(&self) -> PredicateListing {
		fn infos<P: ?Sized>(
			predicates: &[Box<P>],
			name: impl Fn(&P) -> &'static str,
			description: impl Fn(&P) -> &'static str,
		) -> Vec<PredicateInfo> {
			predicates
				.iter()
				.map(|p| PredicateInfo {
					name: name(p).to_string(),
					description: description(p).to_string(),
				})
				.collect()
		}
		PredicateListing {
			ledger_7: infos(&self.ledger_7, |p| p.name(), |p| p.description()),
			ledger_8: infos(&self.ledger_8, |p| p.name(), |p| p.description()),
			ledger_9: infos(&self.ledger_9, |p| p.name(), |p| p.description()),
		}
	}
}

pub async fn execute(
	args: ReplayCheckArgs,
) -> Result<ReplayCheckResult, Box<dyn std::error::Error + Send + Sync>> {
	execute_with_registries(args, PredicateRegistries::defaults()).await
}

pub async fn execute_with_registries(
	args: ReplayCheckArgs,
	mut registries: PredicateRegistries,
) -> Result<ReplayCheckResult, Box<dyn std::error::Error + Send + Sync>> {
	registries.filter(&args.predicate);

	if args.list_predicates {
		let listing = registries.listing();
		return Ok(if args.json {
			ReplayCheckResult::ListJson(listing)
		} else {
			ReplayCheckResult::ListHuman(listing)
		});
	}

	// Construct the source eagerly so source-argument validation runs before
	// the dry-run short circuit (see `dust_balance::execute_many`).
	let src = TxGenerator::source(args.source, args.dry_run).await?;

	if args.dry_run {
		log::info!("Dry-run: replay-check with fail_fast={}", args.fail_fast);
		log::info!("Dry-run: predicate filter: {:?}", args.predicate);
		log::info!("Dry-run: block bounds: {:?}..={:?}", args.from_block, args.to_block);
		return Ok(ReplayCheckResult::DryRun(()));
	}

	let source_blocks = src.get_txs().await?;

	// Blocks arrive sorted by height, so `--to-block` is a prefix cut.
	let mut blocks: &[RawBlockData] = &source_blocks.blocks;
	if let Some(to) = args.to_block {
		blocks = &blocks[..blocks.partition_point(|b| b.number <= to)];
	}

	// Split into per-ledger-version runs, mirroring `replay_blocks`
	// (`tx_generator/builder/mod.rs`).
	let fork_7_to_8_idx = blocks.partition_point(|b| b.ledger_version() == LedgerVersion::Ledger7);
	let (l7_blocks, l8_and_l9_blocks) = blocks.split_at(fork_7_to_8_idx);
	let fork_8_to_9_idx =
		l8_and_l9_blocks.partition_point(|b| b.ledger_version() == LedgerVersion::Ledger8);
	let (l8_blocks, l9_blocks) = l8_and_l9_blocks.split_at(fork_8_to_9_idx);

	// Seedless context: predicates inspect ledger state, not wallets, so
	// per-block wallet/dust work is skipped entirely.
	let fork_ctx = ForkAwareLedgerContext::new(
		blocks.first().map(|b| b.ledger_version()).unwrap_or_default(),
		&source_blocks.network_id,
	);

	let progress = Progress::new(blocks.len(), "replay-check: replaying blocks");
	let mut violations: Vec<Violation> = Vec::new();
	let mut blocks_scanned = 0u64;
	let mut blocks_observed = 0u64;
	let mut aborted = false;

	// Run one ledger version's blocks through its `observe_blocks` driver.
	macro_rules! run_version {
		($module:ident, $ctx:expr, $blocks:expr, $predicates:expr) => {{
			let outcome = $module::replay_check::observe_blocks(
				$ctx,
				$blocks,
				$predicates,
				args.from_block,
				args.fail_fast,
				&progress,
				&mut violations,
			)?;
			blocks_scanned += outcome.blocks_applied;
			blocks_observed += outcome.blocks_observed;
			aborted |= outcome.aborted;
		}};
	}

	match fork_ctx {
		ForkAwareLedgerContext::Ledger7(ctx7) => {
			run_version!(ledger_7, &ctx7, l7_blocks, &registries.ledger_7);
			if !l8_blocks.is_empty() && !aborted {
				let ctx8 =
					fork_context_7_to_8(ctx7).map_err(|e| format!("fork 7 to 8 failed: {e}"))?;
				run_version!(ledger_8, &ctx8, l8_blocks, &registries.ledger_8);
				if !l9_blocks.is_empty() && !aborted {
					let ctx9 = fork_context_8_to_9(ctx8)
						.map_err(|e| format!("fork 8 to 9 failed: {e}"))?;
					run_version!(ledger_9, &ctx9, l9_blocks, &registries.ledger_9);
				}
			}
		},
		ForkAwareLedgerContext::Ledger8(ctx8) => {
			run_version!(ledger_8, &ctx8, l8_blocks, &registries.ledger_8);
			if !l9_blocks.is_empty() && !aborted {
				let ctx9 =
					fork_context_8_to_9(ctx8).map_err(|e| format!("fork 8 to 9 failed: {e}"))?;
				run_version!(ledger_9, &ctx9, l9_blocks, &registries.ledger_9);
			}
		},
		ForkAwareLedgerContext::Ledger9(ctx9) => {
			run_version!(ledger_9, &ctx9, l9_blocks, &registries.ledger_9);
		},
	}

	progress.finish(format!(
		"replay-check: {blocks_scanned} block(s) replayed, {} violation(s)",
		violations.len()
	));

	let report = ReplayCheckReport { blocks_scanned, blocks_observed, aborted, violations };
	Ok(if args.json { ReplayCheckResult::Json(report) } else { ReplayCheckResult::Human(report) })
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::commands::fork::ledger_9::replay_check::{BlockObservation, Predicate};
	use crate::tx_generator::source::FetchCacheConfig;

	/// Test data
	fn td(filepath: &str) -> String {
		[env!("CARGO_MANIFEST_DIR"), "/test-data/", &filepath].concat().to_string()
	}

	fn source_for(src_files: Vec<String>) -> Source {
		Source {
			src_url: None,
			fetch_concurrency: 1,
			fetch_compute_concurrency: None,
			src_files: Some(src_files),
			dust_warp: false,
			ignore_block_context: false,
			fetch_only_cached: false,
			fetch_cache: FetchCacheConfig::InMemory,
			ledger_state_db: String::new(),
		}
	}

	fn args_for(src_files: Vec<String>) -> ReplayCheckArgs {
		ReplayCheckArgs {
			source: source_for(src_files),
			predicate: Vec::new(),
			fail_fast: false,
			list_predicates: false,
			from_block: None,
			to_block: None,
			json: false,
			dry_run: false,
		}
	}

	/// Fires one block-level violation on every observed block.
	struct AlwaysFail;
	impl Predicate for AlwaysFail {
		fn name(&self) -> &'static str {
			"always-fail"
		}
		fn observe_block(&self, obs: &BlockObservation<'_>, out: &mut Vec<Violation>) {
			out.push(Violation::block_level(self.name(), obs.block, "always fails"));
		}
	}

	/// Never fires.
	struct AlwaysPass;
	impl Predicate for AlwaysPass {
		fn name(&self) -> &'static str {
			"always-pass"
		}
		fn observe_block(&self, _obs: &BlockObservation<'_>, _out: &mut Vec<Violation>) {}
	}

	fn registries_with(predicate: Box<dyn Predicate>) -> PredicateRegistries {
		// The genesis fixture is a ledger-9 source, so only that registry
		// needs populating.
		PredicateRegistries { ledger_7: vec![], ledger_8: vec![], ledger_9: vec![predicate] }
	}

	#[tokio::test]
	async fn always_fail_predicate_fires_once_per_block() {
		let args = args_for(vec![td("genesis/genesis_block_undeployed.mn")]);
		let result = execute_with_registries(args, registries_with(Box::new(AlwaysFail)))
			.await
			.expect("replay-check should succeed");

		let ReplayCheckResult::Human(report) = result else {
			panic!("expected a human report");
		};
		assert!(report.blocks_scanned > 0, "fixture must produce at least one block");
		assert_eq!(report.blocks_observed, report.blocks_scanned);
		assert_eq!(
			report.violations.len() as u64,
			report.blocks_observed,
			"always-fail must produce exactly one violation per observed block"
		);
		assert!(report.violations.iter().all(|v| v.predicate == "always-fail"));
		assert!(!report.aborted);
	}

	#[tokio::test]
	async fn always_pass_predicate_reports_nothing() {
		let args = args_for(vec![td("genesis/genesis_block_undeployed.mn")]);
		let result = execute_with_registries(args, registries_with(Box::new(AlwaysPass)))
			.await
			.expect("replay-check should succeed");

		let ReplayCheckResult::Human(report) = result else {
			panic!("expected a human report");
		};
		assert!(report.blocks_scanned > 0);
		assert!(report.violations.is_empty(), "always-pass must not produce violations");
	}

	#[tokio::test]
	async fn fail_fast_stops_at_first_violating_block() {
		let mut args = args_for(vec![td("genesis/genesis_block_undeployed.mn")]);
		args.fail_fast = true;
		let result = execute_with_registries(args, registries_with(Box::new(AlwaysFail)))
			.await
			.expect("replay-check should succeed");

		let ReplayCheckResult::Human(report) = result else {
			panic!("expected a human report");
		};
		assert!(report.aborted, "fail-fast must abort on the first violating block");
		assert_eq!(report.violations.len(), 1);
		assert_eq!(report.blocks_observed, 1);
	}

	#[tokio::test]
	async fn default_example_predicates_pass_on_genesis() {
		let args = args_for(vec![td("genesis/genesis_block_undeployed.mn")]);
		let result = execute(args).await.expect("replay-check should succeed");

		let ReplayCheckResult::Human(report) = result else {
			panic!("expected a human report");
		};
		assert!(
			report.violations.is_empty(),
			"example predicates must not fire on healthy genesis history: {:?}",
			report.violations
		);
	}

	#[tokio::test]
	async fn decode_block_txs_round_trips_fixture_blocks() {
		let src =
			TxGenerator::source(source_for(vec![td("genesis/genesis_block_undeployed.mn")]), false)
				.await
				.expect("build source");
		let source_blocks = src.get_txs().await.expect("get_txs");

		assert!(!source_blocks.blocks.is_empty());
		for block in &source_blocks.blocks {
			let txs = crate::commands::fork::ledger_9::replay_check::decode_block_txs(block)
				.expect("fixture transactions must decode");
			assert_eq!(
				txs.len(),
				block.transactions.len(),
				"one decoded tx per raw tx in block {}",
				block.number
			);
		}
	}

	#[test]
	fn violation_serializes_hashes_as_hex() {
		let violation = Violation {
			predicate: "p".into(),
			block_number: 7,
			block_hash: [0xab; 32],
			tx_index: Some(1),
			tx_hash: Some([0xcd; 32]),
			message: "m".into(),
		};
		let json = serde_json::to_value(&violation).unwrap();
		assert_eq!(json["block_hash"], serde_json::json!("ab".repeat(32)));
		assert_eq!(json["tx_hash"], serde_json::json!("cd".repeat(32)));

		let block_level = Violation { tx_index: None, tx_hash: None, ..violation };
		let json = serde_json::to_value(&block_level).unwrap();
		assert_eq!(json["tx_hash"], serde_json::Value::Null);
	}
}
