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

//! Periodically runs the [`BabeKeyProbe`] and publishes the result as a
//! Prometheus gauge.

use prometheus_endpoint::{Gauge, PrometheusError, Registry, U64, register};
use std::time::Duration;

use super::{LOG_TARGET, probe::BabeKeyProbe};

/// How often the reporter probes in production.
pub const DEFAULT_PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// Prometheus metric published by the reporter.
#[derive(Clone)]
pub struct BabeKeyMetrics {
	ready: Gauge<U64>,
}

impl BabeKeyMetrics {
	/// Registers the metric with the given Prometheus registry.
	pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			ready: register(
				Gauge::new(
					"midnight_babe_key_registered",
					"1 if the keystore holds a BABE key registered for a permissioned candidate on Cardano, 0 otherwise",
				)?,
				registry,
			)?,
		})
	}

	fn set(&self, matching: bool) {
		self.ready.set(u64::from(matching));
	}
}

/// Runs the probe every `interval` and publishes the result.
pub async fn run(probe: BabeKeyProbe, metrics: BabeKeyMetrics, interval: Duration) {
	loop {
		match probe.matching_babe_keys().await {
			Ok(keys) => {
				let matching = !keys.is_empty();
				metrics.set(matching);
			},
			Err(err) => log::warn!(
				target: LOG_TARGET,
				"Failed to check whether this node's BABE keys are registered on Cardano: {err}",
			),
		}

		tokio::time::sleep(interval).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn metric_starts_at_zero_and_tracks_the_probe_result() {
		let registry = Registry::new();
		let metrics = BabeKeyMetrics::register(&registry).unwrap();
		assert_eq!(metrics.ready.get(), 0);

		metrics.set(true);
		assert_eq!(metrics.ready.get(), 1);

		metrics.set(false);
		assert_eq!(metrics.ready.get(), 0);
	}

	#[test]
	fn metric_can_only_be_registered_once_per_registry() {
		let registry = Registry::new();
		BabeKeyMetrics::register(&registry).unwrap();
		assert!(BabeKeyMetrics::register(&registry).is_err());
	}
}
