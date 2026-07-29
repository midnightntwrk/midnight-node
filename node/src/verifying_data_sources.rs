// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Shadow-verification wrappers for the main-chain follower data sources.
//!
//! In `DbSyncEmbeddedVerify` mode every data-source call is answered by the
//! primary (db-sync) backend exactly as in `DbSync` mode — consensus behaviour
//! is bit-identical — while the same query is replayed off the hot path
//! against a shadow backend and the answers compared. Divergences are counted
//! (`midnight_mc_follower_verify_total`) and logged; `OnDivergence::Halt`
//! additionally exits the node, for soak tests that must fail loudly.
//!
//! Comparison outcomes are three-valued, not two: queries anchored to an
//! explicit block hash / epoch must match *strictly* (ordering included —
//! if the node consumes an ordering, the ordering is the spec), while
//! tip-relative queries and shadow-not-yet-synced answers are counted as
//! skew/lag, never divergence. Both backends legitimately race on tip.

use std::{error::Error, fmt::Debug, future::Future, sync::Arc};

use authority_selection_inherents::{AriadneParameters, AuthoritySelectionDataSource};
use log::warn;
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use midnight_primitives_federated_authority_observation::{
	FederatedAuthorityData, FederatedAuthorityObservationConfig,
};
use midnight_primitives_mainchain_follower::{
	FederatedAuthorityObservationDataSource, MidnightCNightObservationDataSource,
};
use pallet_sidechain_rpc::SidechainRpcDataSource;
use prometheus_endpoint::{CounterVec, Opts, PrometheusError, Registry, U64, register};
use serde::{Deserialize, Serialize};
use sidechain_domain::{
	CandidateRegistrations, EpochNonce, MainchainAddress, MainchainBlock, McBlockHash,
	McEpochNumber, PolicyId,
};
use sidechain_mc_hash::{McHashDataSource, StableBlockByHashResult};
use sp_timestamp::Timestamp;

use crate::main_chain_follower::DataSources;

const LOG_TARGET: &str = "mc-follower-verify";

/// Upper bound on concurrently running shadow comparisons. When exhausted,
/// further comparisons are dropped (counted as `compare_dropped`) rather than
/// queued, so a stalled shadow backend can never exert backpressure on the
/// primary path.
const MAX_INFLIGHT_COMPARES: usize = 64;

type DataSourceError = Box<dyn Error + Send + Sync>;
type DataSourceResult<T> = Result<T, DataSourceError>;

/// Policy applied when the shadow backend contradicts the primary on an
/// anchored query.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum OnDivergence {
	/// Record the divergence in metrics and the error log; keep running.
	#[default]
	Log,
	/// Log, then exit the node. For soak tests where a divergence must fail
	/// loudly rather than scroll past.
	Halt,
}

/// Prometheus counters for shadow-verification outcomes, labelled by data
/// source method and outcome.
#[derive(Clone)]
pub struct VerifyMetrics {
	outcomes: CounterVec<U64>,
}

impl VerifyMetrics {
	/// Register `midnight_mc_follower_verify_total{method,outcome}` on `registry`.
	pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			outcomes: register(
				CounterVec::new(
					Opts::new(
						"midnight_mc_follower_verify_total",
						"Outcomes of shadow-verified main-chain follower queries",
					),
					&["method", "outcome"],
				)?,
				registry,
			)?,
		})
	}

	/// Like [`Self::register`] but degrades to metric-less operation (with a
	/// warning) when there is no registry or registration fails — verify mode
	/// still logs divergences without metrics.
	pub fn register_warn_errors(registry: Option<&Registry>) -> Option<Self> {
		registry.and_then(|registry| match Self::register(registry) {
			Ok(metrics) => Some(metrics),
			Err(err) => {
				warn!("Failed registering mc-follower verify metrics: {err}");
				None
			},
		})
	}
}

/// How to interpret a primary/shadow mismatch for a given method.
///
/// Only `Divergence` is an error signal; the other two record that the
/// backends were looking at different tips, which is expected.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Mismatch {
	/// Same anchored question, both synced, different answer.
	Divergence,
	/// The shadow has not yet indexed the data the primary answered from.
	ShadowLag,
	/// Tip-relative query; the backends legitimately race.
	TipSkew,
	/// The answers differ only in fields the node is known not to consume.
	Benign,
}

/// Strictly-anchored query: any mismatch is a divergence.
fn strict<T>(_: &T, _: &T) -> Mismatch {
	Mismatch::Divergence
}

/// Anchored query returning `Option`: a missing shadow answer is lag, a
/// missing primary answer is (primary) skew, two different `Some`s diverge.
fn anchored_option<T>(primary: &Option<T>, shadow: &Option<T>) -> Mismatch {
	match (primary, shadow) {
		(Some(_), None) => Mismatch::ShadowLag,
		(None, Some(_)) => Mismatch::TipSkew,
		_ => Mismatch::Divergence,
	}
}

/// Tip-relative query: mismatches are expected, never a divergence.
fn tip_relative<T>(_: &T, _: &T) -> Mismatch {
	Mismatch::TipSkew
}

/// Ariadne parameters: only the candidate list is compared. The d-parameter
/// is dead weight in this node — the runtime overwrites it from
/// pallet_system_parameters (see `runtime/src/lib.rs`) — and the embedded
/// indexer does not index the d-parameter datum, so it reports a placeholder.
fn ariadne_parameters(p: &AriadneParameters, s: &AriadneParameters) -> Mismatch {
	if p.permissioned_candidates == s.permissioned_candidates {
		Mismatch::Benign
	} else {
		Mismatch::Divergence
	}
}

/// Block-by-hash stability lookup: the stability *classification* depends on
/// the backend's tip (confirmations), so classification mismatches are lag.
/// But two `BlockStable` answers for the same hash must carry the same block.
fn stable_block_by_hash(p: &StableBlockByHashResult, s: &StableBlockByHashResult) -> Mismatch {
	use StableBlockByHashResult::*;
	match (p, s) {
		(BlockStable { .. }, BlockStable { .. }) => Mismatch::Divergence,
		_ => Mismatch::ShadowLag,
	}
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Outcome {
	Match,
	Divergence,
	ShadowLag,
	TipSkew,
	BenignMismatch,
	ShadowError,
}

impl Outcome {
	fn as_str(&self) -> &'static str {
		match self {
			Outcome::Match => "match",
			Outcome::Divergence => "divergence",
			Outcome::ShadowLag => "shadow_lag",
			Outcome::TipSkew => "tip_skew",
			Outcome::BenignMismatch => "benign_mismatch",
			Outcome::ShadowError => "shadow_error",
		}
	}
}

/// Pure comparison step, factored out of the spawned task for testability.
fn classify_results<T: PartialEq>(
	primary: &T,
	shadow: &DataSourceResult<T>,
	classify: fn(&T, &T) -> Mismatch,
) -> Outcome {
	match shadow {
		Err(_) => Outcome::ShadowError,
		Ok(shadow) if shadow == primary => Outcome::Match,
		Ok(shadow) => match classify(primary, shadow) {
			Mismatch::Divergence => Outcome::Divergence,
			Mismatch::ShadowLag => Outcome::ShadowLag,
			Mismatch::TipSkew => Outcome::TipSkew,
			Mismatch::Benign => Outcome::BenignMismatch,
		},
	}
}

/// Shared state threaded through every verifying wrapper.
#[derive(Clone)]
pub struct VerifyContext {
	metrics: Option<VerifyMetrics>,
	on_divergence: OnDivergence,
	inflight: Arc<tokio::sync::Semaphore>,
}

impl VerifyContext {
	/// Bounds in-flight shadow comparisons at `MAX_INFLIGHT_COMPARES` (64);
	/// beyond that, comparisons are dropped (counted), never queued.
	pub fn new(metrics: Option<VerifyMetrics>, on_divergence: OnDivergence) -> Self {
		Self {
			metrics,
			on_divergence,
			inflight: Arc::new(tokio::sync::Semaphore::new(MAX_INFLIGHT_COMPARES)),
		}
	}

	fn record(&self, method: &'static str, outcome: &'static str) {
		if let Some(metrics) = &self.metrics {
			metrics.outcomes.with_label_values(&[method, outcome]).inc();
		}
	}

	/// Replay `shadow_call` off the hot path and compare against the
	/// primary's answer. `primary` is `None` when the primary call itself
	/// failed — nothing to verify then. Never blocks the caller.
	fn spawn_compare<T, F>(
		&self,
		method: &'static str,
		args: String,
		primary: Option<T>,
		shadow_call: F,
		classify: fn(&T, &T) -> Mismatch,
	) where
		T: PartialEq + Debug + Send + 'static,
		F: Future<Output = DataSourceResult<T>> + Send + 'static,
	{
		let Some(primary) = primary else {
			self.record(method, "primary_error");
			return;
		};
		let Ok(permit) = Arc::clone(&self.inflight).try_acquire_owned() else {
			self.record(method, "compare_dropped");
			return;
		};
		let ctx = self.clone();
		tokio::spawn(async move {
			let _permit = permit;
			let shadow = shadow_call.await;
			let outcome = classify_results(&primary, &shadow, classify);
			ctx.record(method, outcome.as_str());
			match (outcome, &shadow) {
				(Outcome::ShadowError, Err(err)) => {
					log::debug!(target: LOG_TARGET, "shadow backend error in {method}({args}): {err}");
				},
				(Outcome::ShadowLag | Outcome::TipSkew | Outcome::BenignMismatch, _) => {
					log::debug!(
						target: LOG_TARGET,
						"{} in {method}({args})", outcome.as_str()
					);
				},
				(Outcome::Divergence, Ok(shadow)) => {
					log::error!(
						target: LOG_TARGET,
						"main-chain follower divergence in {method}({args}):\n  primary: {primary:?}\n  shadow:  {shadow:?}"
					);
					if ctx.on_divergence == OnDivergence::Halt {
						log::error!(
							target: LOG_TARGET,
							"halting node: mc_follower_on_divergence = Halt"
						);
						std::process::exit(1);
					}
				},
				_ => {},
			}
		});
	}
}

/// Wrap `primary` so that every call is shadow-verified against `shadow`.
///
/// The bridge data source is passed through unverified: no shadow
/// implementation exists for it yet.
pub fn verify_data_sources(
	primary: DataSources,
	shadow: DataSources,
	ctx: VerifyContext,
) -> DataSources {
	DataSources {
		mc_hash: Arc::new(VerifyingMcHashDataSource {
			primary: primary.mc_hash,
			shadow: shadow.mc_hash,
			ctx: ctx.clone(),
		}),
		authority_selection: Arc::new(VerifyingAuthoritySelectionDataSource {
			primary: primary.authority_selection,
			shadow: shadow.authority_selection,
			ctx: ctx.clone(),
		}),
		cnight_observation: Arc::new(VerifyingCNightObservationDataSource {
			primary: primary.cnight_observation,
			shadow: shadow.cnight_observation,
			ctx: ctx.clone(),
		}),
		sidechain_rpc: Arc::new(VerifyingSidechainRpcDataSource {
			primary: primary.sidechain_rpc,
			shadow: shadow.sidechain_rpc,
			ctx: ctx.clone(),
		}),
		federated_authority_observation: Arc::new(
			VerifyingFederatedAuthorityObservationDataSource {
				primary: primary.federated_authority_observation,
				shadow: shadow.federated_authority_observation,
				ctx,
			},
		),
		bridge: primary.bridge,
	}
}

struct VerifyingMcHashDataSource {
	primary: Arc<dyn McHashDataSource + Send + Sync>,
	shadow: Arc<dyn McHashDataSource + Send + Sync>,
	ctx: VerifyContext,
}

#[async_trait::async_trait]
impl McHashDataSource for VerifyingMcHashDataSource {
	async fn get_latest_stable_block_for(
		&self,
		reference_timestamp: Timestamp,
	) -> DataSourceResult<Option<MainchainBlock>> {
		let primary = self.primary.get_latest_stable_block_for(reference_timestamp).await;
		let shadow = Arc::clone(&self.shadow);
		self.ctx.spawn_compare(
			"mc_hash.get_latest_stable_block_for",
			format!("reference_timestamp={reference_timestamp:?}"),
			primary.as_ref().ok().cloned(),
			async move { shadow.get_latest_stable_block_for(reference_timestamp).await },
			tip_relative,
		);
		primary
	}

	async fn get_stable_block_for(
		&self,
		hash: McBlockHash,
		reference_timestamp: Timestamp,
	) -> DataSourceResult<StableBlockByHashResult> {
		let primary = self.primary.get_stable_block_for(hash.clone(), reference_timestamp).await;
		let shadow = Arc::clone(&self.shadow);
		let args = format!("hash={hash}, reference_timestamp={reference_timestamp:?}");
		self.ctx.spawn_compare(
			"mc_hash.get_stable_block_for",
			args,
			primary.as_ref().ok().cloned(),
			async move { shadow.get_stable_block_for(hash, reference_timestamp).await },
			stable_block_by_hash,
		);
		primary
	}

	async fn get_block_by_hash(
		&self,
		hash: McBlockHash,
	) -> DataSourceResult<Option<MainchainBlock>> {
		let primary = self.primary.get_block_by_hash(hash.clone()).await;
		let shadow = Arc::clone(&self.shadow);
		let args = format!("hash={hash}");
		self.ctx.spawn_compare(
			"mc_hash.get_block_by_hash",
			args,
			primary.as_ref().ok().cloned(),
			async move { shadow.get_block_by_hash(hash).await },
			anchored_option,
		);
		primary
	}

	// Health probes ask about *this backend's* infrastructure, not about
	// chain data — the primary's and shadow's health are independent
	// questions, so there is nothing meaningful to cross-check.
	async fn is_cardano_tip_fresh(&self) -> DataSourceResult<bool> {
		self.primary.is_cardano_tip_fresh().await
	}

	async fn is_cardano_ok(&self) -> DataSourceResult<bool> {
		self.primary.is_cardano_ok().await
	}
}

struct VerifyingAuthoritySelectionDataSource {
	primary: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	shadow: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	ctx: VerifyContext,
}

#[async_trait::async_trait]
impl AuthoritySelectionDataSource for VerifyingAuthoritySelectionDataSource {
	async fn get_ariadne_parameters(
		&self,
		epoch_number: McEpochNumber,
		d_parameter: PolicyId,
		permissioned_candidates: PolicyId,
	) -> DataSourceResult<AriadneParameters> {
		let primary = self
			.primary
			.get_ariadne_parameters(
				epoch_number,
				d_parameter.clone(),
				permissioned_candidates.clone(),
			)
			.await;
		let shadow = Arc::clone(&self.shadow);
		let args = format!("epoch={epoch_number:?}");
		self.ctx.spawn_compare(
			"authority_selection.get_ariadne_parameters",
			args,
			primary.as_ref().ok().cloned(),
			async move {
				shadow
					.get_ariadne_parameters(epoch_number, d_parameter, permissioned_candidates)
					.await
			},
			ariadne_parameters,
		);
		primary
	}

	async fn get_candidates(
		&self,
		epoch: McEpochNumber,
		committee_candidate_address: MainchainAddress,
	) -> DataSourceResult<Vec<CandidateRegistrations>> {
		let primary = self.primary.get_candidates(epoch, committee_candidate_address.clone()).await;
		let shadow = Arc::clone(&self.shadow);
		let args = format!("epoch={epoch:?}");
		self.ctx.spawn_compare(
			"authority_selection.get_candidates",
			args,
			primary.as_ref().ok().cloned(),
			async move { shadow.get_candidates(epoch, committee_candidate_address).await },
			strict,
		);
		primary
	}

	async fn get_epoch_nonce(&self, epoch: McEpochNumber) -> DataSourceResult<Option<EpochNonce>> {
		let primary = self.primary.get_epoch_nonce(epoch).await;
		let shadow = Arc::clone(&self.shadow);
		self.ctx.spawn_compare(
			"authority_selection.get_epoch_nonce",
			format!("epoch={epoch:?}"),
			primary.as_ref().ok().cloned(),
			async move { shadow.get_epoch_nonce(epoch).await },
			anchored_option,
		);
		primary
	}

	async fn data_epoch(&self, for_epoch: McEpochNumber) -> DataSourceResult<McEpochNumber> {
		let primary = self.primary.data_epoch(for_epoch).await;
		let shadow = Arc::clone(&self.shadow);
		self.ctx.spawn_compare(
			"authority_selection.data_epoch",
			format!("for_epoch={for_epoch:?}"),
			primary.as_ref().ok().cloned(),
			async move { shadow.data_epoch(for_epoch).await },
			strict,
		);
		primary
	}
}

struct VerifyingCNightObservationDataSource {
	primary: Arc<dyn MidnightCNightObservationDataSource + Send + Sync>,
	shadow: Arc<dyn MidnightCNightObservationDataSource + Send + Sync>,
	ctx: VerifyContext,
}

#[async_trait::async_trait]
impl MidnightCNightObservationDataSource for VerifyingCNightObservationDataSource {
	async fn get_utxos_up_to_capacity(
		&self,
		config: &CNightAddresses,
		start_position: &CardanoPosition,
		current_tip: McBlockHash,
		tx_capacity: usize,
		utxo_overestimate: usize,
	) -> DataSourceResult<ObservedUtxos> {
		let primary = self
			.primary
			.get_utxos_up_to_capacity(
				config,
				start_position,
				current_tip.clone(),
				tx_capacity,
				utxo_overestimate,
			)
			.await;
		let shadow = Arc::clone(&self.shadow);
		let config = config.clone();
		let start_position = start_position.clone();
		let args = format!(
			"start={start_position}, tip={current_tip}, tx_capacity={tx_capacity}, utxo_overestimate={utxo_overestimate}"
		);
		self.ctx.spawn_compare(
			"cnight_observation.get_utxos_up_to_capacity",
			args,
			primary.as_ref().ok().cloned(),
			async move {
				shadow
					.get_utxos_up_to_capacity(
						&config,
						&start_position,
						current_tip,
						tx_capacity,
						utxo_overestimate,
					)
					.await
			},
			strict,
		);
		primary
	}
}

struct VerifyingSidechainRpcDataSource {
	primary: Arc<dyn SidechainRpcDataSource + Send + Sync>,
	shadow: Arc<dyn SidechainRpcDataSource + Send + Sync>,
	ctx: VerifyContext,
}

#[async_trait::async_trait]
impl SidechainRpcDataSource for VerifyingSidechainRpcDataSource {
	async fn get_latest_block_info(&self) -> DataSourceResult<MainchainBlock> {
		let primary = self.primary.get_latest_block_info().await;
		let shadow = Arc::clone(&self.shadow);
		self.ctx.spawn_compare(
			"sidechain_rpc.get_latest_block_info",
			String::new(),
			primary.as_ref().ok().cloned(),
			async move { shadow.get_latest_block_info().await },
			tip_relative,
		);
		primary
	}
}

struct VerifyingFederatedAuthorityObservationDataSource {
	primary: Arc<dyn FederatedAuthorityObservationDataSource + Send + Sync>,
	shadow: Arc<dyn FederatedAuthorityObservationDataSource + Send + Sync>,
	ctx: VerifyContext,
}

#[async_trait::async_trait]
impl FederatedAuthorityObservationDataSource for VerifyingFederatedAuthorityObservationDataSource {
	async fn get_federated_authority_data(
		&self,
		config: &FederatedAuthorityObservationConfig,
		mc_block_hash: &McBlockHash,
	) -> DataSourceResult<FederatedAuthorityData> {
		let primary = self.primary.get_federated_authority_data(config, mc_block_hash).await;
		let shadow = Arc::clone(&self.shadow);
		let config = config.clone();
		let mc_block_hash = mc_block_hash.clone();
		let args = format!("mc_block_hash={mc_block_hash}");
		self.ctx.spawn_compare(
			"federated_authority_observation.get_federated_authority_data",
			args,
			primary.as_ref().ok().cloned(),
			async move { shadow.get_federated_authority_data(&config, &mc_block_hash).await },
			strict,
		);
		primary
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ok<T>(value: T) -> DataSourceResult<T> {
		Ok(value)
	}

	fn err<T>() -> DataSourceResult<T> {
		Err("shadow backend unavailable".into())
	}

	#[test]
	fn equal_answers_match() {
		assert_eq!(classify_results(&1u32, &ok(1u32), strict), Outcome::Match);
	}

	#[test]
	fn strict_mismatch_is_divergence() {
		assert_eq!(classify_results(&1u32, &ok(2u32), strict), Outcome::Divergence);
	}

	#[test]
	fn shadow_error_is_not_divergence() {
		assert_eq!(classify_results(&1u32, &err(), strict), Outcome::ShadowError);
	}

	#[test]
	fn anchored_option_shadow_none_is_lag() {
		assert_eq!(classify_results(&Some(1u32), &ok(None), anchored_option), Outcome::ShadowLag);
	}

	#[test]
	fn anchored_option_primary_none_is_skew() {
		assert_eq!(classify_results(&None, &ok(Some(1u32)), anchored_option), Outcome::TipSkew);
	}

	#[test]
	fn anchored_option_different_somes_diverge() {
		assert_eq!(
			classify_results(&Some(1u32), &ok(Some(2u32)), anchored_option),
			Outcome::Divergence
		);
	}

	#[test]
	fn tip_relative_mismatch_is_skew() {
		assert_eq!(classify_results(&1u32, &ok(2u32), tip_relative), Outcome::TipSkew);
	}

	#[test]
	fn ariadne_d_parameter_mismatch_is_benign() {
		use sidechain_domain::DParameter;
		let p = AriadneParameters {
			d_parameter: DParameter {
				num_permissioned_candidates: 3,
				num_registered_candidates: 7,
			},
			permissioned_candidates: None,
		};
		let s = AriadneParameters {
			d_parameter: DParameter {
				num_permissioned_candidates: 0,
				num_registered_candidates: 0,
			},
			permissioned_candidates: None,
		};
		assert_eq!(classify_results(&p, &ok(s), ariadne_parameters), Outcome::BenignMismatch);
	}

	#[test]
	fn ariadne_candidate_mismatch_is_divergence() {
		use sidechain_domain::DParameter;
		let d = DParameter { num_permissioned_candidates: 0, num_registered_candidates: 0 };
		let p = AriadneParameters { d_parameter: d.clone(), permissioned_candidates: Some(vec![]) };
		let s = AriadneParameters { d_parameter: d, permissioned_candidates: None };
		assert_eq!(classify_results(&p, &ok(s), ariadne_parameters), Outcome::Divergence);
	}

	#[test]
	fn stable_block_classification_mismatch_is_lag() {
		let block = MainchainBlock::default();
		let primary = StableBlockByHashResult::BlockStable { info: block.clone() };
		let shadow = StableBlockByHashResult::NotEnoughConfirmations { info: block };
		assert_eq!(
			classify_results(&primary, &ok(shadow), stable_block_by_hash),
			Outcome::ShadowLag
		);
	}

	#[test]
	fn stable_block_same_classification_different_block_diverges() {
		let primary = StableBlockByHashResult::BlockStable { info: MainchainBlock::default() };
		let shadow = StableBlockByHashResult::BlockStable {
			info: MainchainBlock {
				number: sidechain_domain::McBlockNumber(42),
				..Default::default()
			},
		};
		assert_eq!(
			classify_results(&primary, &ok(shadow), stable_block_by_hash),
			Outcome::Divergence
		);
	}

	/// A diverging shadow must never change what the caller sees: the
	/// primary's answer is returned as-is.
	#[tokio::test(flavor = "multi_thread")]
	async fn wrapper_returns_primary_answer_even_when_shadow_diverges() {
		struct Fixed(McEpochNumber);
		#[async_trait::async_trait]
		impl AuthoritySelectionDataSource for Fixed {
			async fn get_ariadne_parameters(
				&self,
				_: McEpochNumber,
				_: PolicyId,
				_: PolicyId,
			) -> DataSourceResult<AriadneParameters> {
				err()
			}
			async fn get_candidates(
				&self,
				_: McEpochNumber,
				_: MainchainAddress,
			) -> DataSourceResult<Vec<CandidateRegistrations>> {
				Ok(vec![])
			}
			async fn get_epoch_nonce(
				&self,
				_: McEpochNumber,
			) -> DataSourceResult<Option<EpochNonce>> {
				Ok(None)
			}
			async fn data_epoch(&self, _: McEpochNumber) -> DataSourceResult<McEpochNumber> {
				Ok(self.0)
			}
		}

		let wrapper = VerifyingAuthoritySelectionDataSource {
			primary: Arc::new(Fixed(McEpochNumber(7))),
			shadow: Arc::new(Fixed(McEpochNumber(8))),
			ctx: VerifyContext::new(None, OnDivergence::Log),
		};
		let got = wrapper.data_epoch(McEpochNumber(1)).await.unwrap();
		assert_eq!(got, McEpochNumber(7));
	}

	/// A primary error propagates unchanged; the shadow is never consulted
	/// (nothing to verify against).
	#[tokio::test(flavor = "multi_thread")]
	async fn wrapper_propagates_primary_error() {
		struct Failing;
		#[async_trait::async_trait]
		impl SidechainRpcDataSource for Failing {
			async fn get_latest_block_info(&self) -> DataSourceResult<MainchainBlock> {
				err()
			}
		}
		struct Answering;
		#[async_trait::async_trait]
		impl SidechainRpcDataSource for Answering {
			async fn get_latest_block_info(&self) -> DataSourceResult<MainchainBlock> {
				Ok(MainchainBlock::default())
			}
		}

		let wrapper = VerifyingSidechainRpcDataSource {
			primary: Arc::new(Failing),
			shadow: Arc::new(Answering),
			ctx: VerifyContext::new(None, OnDivergence::Log),
		};
		assert!(wrapper.get_latest_block_info().await.is_err());
	}
}
