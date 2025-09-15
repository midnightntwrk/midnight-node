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

use prometheus_endpoint::{
	self as prometheus, HistogramOpts, HistogramVec, PrometheusError, Registry,
};
use std::{
	path::PathBuf,
	sync::{Arc, Mutex},
};

const LOG_TARGET: &str = "ledger::primitives";
/// Ledger metrics exposed through Prometheus
#[derive(Clone, Debug)]
pub struct LedgerMetrics {
	/// Transactions processing time
	pub txs_processing_time: HistogramVec,
	/// System Transactions processing time
	pub system_txs_processing_time: HistogramVec,
	/// Transactions validation time
	pub txs_validating_time: HistogramVec,
	/// Transactions size
	pub txs_size: HistogramVec,
	/// Storage fetch time
	pub storage_fetch_time: HistogramVec,
	/// Storage flush time
	pub storage_flush_time: HistogramVec,
}

/// Time constants to build a Prometheus Histogram bucket
const TIME_INTERVAL_LINEAR: f64 = 0.05; // 50 ms
const TIME_MAX_LINEAR: f64 = 1.0; // 1 second
const TIME_INCREASE_EXP: f64 = 1.5; // Increase by 50% each step
const TIME_MAX_EXP: f64 = 60.0; // 1 minute

/// Size constants to build a Prometheus Histogram bucket
const KB: f64 = 1024.0; // 1 KiB = 1024 bytes
const MB: f64 = KB * 1024.0; // 1 MiB = 1024 KB
const SIZE_INTERVAL_LINEAR: f64 = 10.0 * KB; // 10 KiB
const SIZE_MAX_LINEAR: f64 = 200.0 * KB; // 200 KiB
const SIZE_INCREASE_EXP: f64 = 1.5; // Increase by 50% each step
const SIZE_MAX_EXP: f64 = 5.0 * MB; // 5 MiB

/// Combine linear and exponential buckets to get more precise measurements for
/// short transactions while still efficiently capturing longer ones.
fn hybrid_buckets(
	interval_linear: f64,
	max_linear: f64,
	increase_exp: f64,
	max_exp: f64,
) -> Vec<f64> {
	let mut buckets = Vec::new();

	// Linear buckets from 0 to `max_linear` second (every 10ms)
	for i in 0..(max_linear / interval_linear) as u64 {
		let interval = f64::trunc(i as f64 * interval_linear * 100.0) / 100.0; // trunc to 2 decimals
		buckets.push(interval);
	}

	// Exponential buckets from `max_linear` onward
	let mut value = max_linear;
	while value < max_exp {
		// Capture up to `max_exp` second transactions
		buckets.push(value);
		value *= increase_exp; // Increase by `increase_exp` each step
		value = f64::trunc(value * 100.0) / 100.0; // trunc to 2 decimals
	}

	buckets
}

impl LedgerMetrics {
	pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		let time_buckets =
			hybrid_buckets(TIME_INTERVAL_LINEAR, TIME_MAX_LINEAR, TIME_INCREASE_EXP, TIME_MAX_EXP);
		let size_buckets =
			hybrid_buckets(SIZE_INTERVAL_LINEAR, SIZE_MAX_LINEAR, SIZE_INCREASE_EXP, SIZE_MAX_EXP);

		Ok(Self {
			txs_processing_time: prometheus::register(
				HistogramVec::new(
					HistogramOpts::new(
						"ledger_txs_processing_time",
						"Time spent for processing a transaction",
					)
					.buckets(time_buckets.clone()),
					&["tx_type"],
				)?,
				registry,
			)?,
			system_txs_processing_time: prometheus::register(
				HistogramVec::new(
					HistogramOpts::new(
						"ledger_system_txs_processing_time",
						"Time spent for processing a system transaction",
					)
					.buckets(time_buckets.clone()),
					&["tx_type"],
				)?,
				registry,
			)?,
			txs_validating_time: prometheus::register(
				HistogramVec::new(
					HistogramOpts::new(
						"ledger_txs_validating_time",
						"Time spent for validating a transaction",
					)
					.buckets(time_buckets.clone()),
					&["tx_type"],
				)?,
				registry,
			)?,
			txs_size: prometheus::register(
				HistogramVec::new(
					HistogramOpts::new("ledger_txs_size", "Transaction size").buckets(size_buckets),
					&["tx_type"],
				)?,
				registry,
			)?,
			storage_fetch_time: prometheus::register(
				HistogramVec::new(
					HistogramOpts::new(
						"storage_fetch_time",
						"Time spent fetching the ledger state",
					)
					.buckets(time_buckets.clone()),
					&["storage"],
				)?,
				registry,
			)?,
			storage_flush_time: prometheus::register(
				HistogramVec::new(
					HistogramOpts::new(
						"storage_flush_time",
						"Time spent flushing the ledger state to disk",
					)
					.buckets(time_buckets.clone()),
					&["storage"],
				)?,
				registry,
			)?,
		})
	}
}

sp_externalities::decl_extension! {
	/// The `LedgerMetrics`` extension to register/retrieve from the externalities.
	#[derive(Debug)]
	pub struct LedgerMetricsExt(Arc<Mutex<Option<LedgerMetrics>>>);
}

impl LedgerMetricsExt {
	pub fn new(metrics: Arc<Mutex<Option<LedgerMetrics>>>) -> Self {
		LedgerMetricsExt(metrics)
	}

	fn observe<F>(&mut self, op: F)
	where
		F: FnOnce(&LedgerMetrics),
	{
		let metrics = self.0.clone();
		let metrics_result = metrics.lock();

		if let Ok(write_metrics) = metrics_result {
			if let Some(m) = write_metrics.as_ref() {
				op(m);
			}
		} else {
			log::error!(target: LOG_TARGET, "Ledger Metrics's lock is already held by the current thread");
		}
	}

	pub fn observe_system_txs_processing_time(&mut self, time: f64, label: &'static str) {
		self.observe(|m| {
			m.system_txs_processing_time.with_label_values(&[label]).observe(time);
		});
	}

	pub fn observe_txs_processing_time(&mut self, time: f64, label: &'static str) {
		self.observe(|m| {
			m.txs_processing_time.with_label_values(&[label]).observe(time);
		});
	}

	pub fn observe_txs_validating_time(&mut self, time: f64, label: &'static str) {
		self.observe(|m| {
			m.txs_validating_time.with_label_values(&[label]).observe(time);
		});
	}

	pub fn observe_txs_size(&mut self, size: f64, label: &'static str) {
		self.observe(|m| {
			m.txs_size.with_label_values(&[label]).observe(size);
		});
	}

	pub fn observe_storage_fetch_time(&mut self, time: f64, label: &'static str) {
		self.observe(|m| {
			m.storage_fetch_time.with_label_values(&[label]).observe(time);
		});
	}

	pub fn observe_storage_flush_time(&mut self, time: f64, label: &'static str) {
		self.observe(|m| {
			m.storage_flush_time.with_label_values(&[label]).observe(time);
		});
	}
}

/// Ledger Storage info to be sent to host functions
#[derive(Clone, Debug)]
pub struct LedgerStorage {
	pub db_path: PathBuf,
	pub cache_size: usize,
}

impl LedgerStorage {
	pub fn new(db_path: PathBuf, cache_size: usize) -> Self {
		Self { db_path, cache_size }
	}
}

sp_externalities::decl_extension! {
	/// The `LedgerStorageExt`` extension to set default `Storage` in case of a Ledger's hard-fork.
	#[derive(Debug)]
	pub struct LedgerStorageExt(LedgerStorage);
}

impl LedgerStorageExt {
	pub fn new(storage: LedgerStorage) -> Self {
		LedgerStorageExt(storage)
	}
}
