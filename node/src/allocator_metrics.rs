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

//! Publishes jemalloc's memory statistics to Prometheus.

use prometheus_endpoint::{GaugeVec, Opts, PrometheusError, Registry, U64, register};
use std::time::Duration;
use tikv_jemalloc_ctl::{epoch, epoch_mib, stats};

const LOG_TARGET: &str = "allocator-metrics";

/// How often the allocator statistics are refreshed.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);

/// Handles to jemalloc's statistics and the gauge they are published to.
pub struct AllocatorMetrics {
	epoch: epoch_mib,
	allocated: stats::allocated_mib,
	active: stats::active_mib,
	metadata: stats::metadata_mib,
	resident: stats::resident_mib,
	mapped: stats::mapped_mib,
	retained: stats::retained_mib,
	bytes: GaugeVec<U64>,
}

/// Failure to set up allocator metrics.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("jemalloc stats unavailable: {0}")]
	Jemalloc(tikv_jemalloc_ctl::Error),
	#[error(transparent)]
	Prometheus(#[from] PrometheusError),
}

impl AllocatorMetrics {
	/// Registers the gauge with the given Prometheus registry.
	pub fn register(registry: &Registry) -> Result<Self, Error> {
		Ok(Self {
			epoch: epoch::mib().map_err(Error::Jemalloc)?,
			allocated: stats::allocated::mib().map_err(Error::Jemalloc)?,
			active: stats::active::mib().map_err(Error::Jemalloc)?,
			metadata: stats::metadata::mib().map_err(Error::Jemalloc)?,
			resident: stats::resident::mib().map_err(Error::Jemalloc)?,
			mapped: stats::mapped::mib().map_err(Error::Jemalloc)?,
			retained: stats::retained::mib().map_err(Error::Jemalloc)?,
			bytes: register(
				GaugeVec::new(
					Opts::new(
						"midnight_memory_allocator_bytes",
						"jemalloc memory statistics in bytes, by jemalloc `stats.*` name",
					),
					&["stat"],
				)?,
				registry,
			)?,
		})
	}

	fn update(&self) -> Result<(), tikv_jemalloc_ctl::Error> {
		// jemalloc caches its statistics until the epoch is advanced.
		self.epoch.advance()?;
		for (stat, value) in [
			("allocated", self.allocated.read()?),
			("active", self.active.read()?),
			("metadata", self.metadata.read()?),
			("resident", self.resident.read()?),
			("mapped", self.mapped.read()?),
			("retained", self.retained.read()?),
		] {
			self.bytes.with_label_values(&[stat]).set(value as u64);
		}
		Ok(())
	}

	/// Refreshes the gauge every `interval`.
	pub async fn run(self, interval: Duration) {
		loop {
			if let Err(err) = self.update() {
				log::debug!(target: LOG_TARGET, "Failed to read jemalloc stats: {err}");
			}
			tokio::time::sleep(interval).await;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn publishes_every_stat() {
		let registry = Registry::new();
		let metrics = AllocatorMetrics::register(&registry).unwrap();
		metrics.update().unwrap();

		let families = registry.gather();
		let family = families
			.iter()
			.find(|f| f.get_name() == "midnight_memory_allocator_bytes")
			.expect("gauge is registered");
		let mut stats: Vec<_> = family
			.get_metric()
			.iter()
			.map(|m| m.get_label()[0].get_value().to_owned())
			.collect();
		stats.sort();
		assert_eq!(stats, ["active", "allocated", "mapped", "metadata", "resident", "retained"]);
	}
}
