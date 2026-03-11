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

use log::warn;
use prometheus_endpoint::{HistogramOpts, HistogramVec, PrometheusError, Registry, register};

pub type MetricsRegistry = Registry;

/// Prometheus metrics for Midnight-specific data source SQL queries.
#[derive(Clone)]
pub struct MidnightDataSourceMetrics {
	time_elapsed: HistogramVec,
}

impl MidnightDataSourceMetrics {
	pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			time_elapsed: register(
				HistogramVec::new(
					HistogramOpts::new(
						"midnight_data_source_query_time_elapsed",
						"Time spent in a midnight data source SQL query",
					),
					&["query_name"],
				)?,
				registry,
			)?,
		})
	}

	pub fn register_warn_errors(metrics_registry_opt: Option<&Registry>) -> Option<Self> {
		metrics_registry_opt.and_then(|registry| match Self::register(registry) {
			Ok(metrics) => Some(metrics),
			Err(err) => {
				warn!("Failed registering midnight data source metrics: {}", err);
				None
			},
		})
	}
}

/// Starts a Prometheus sub-query timer if metrics are available.
/// The returned guard records the elapsed time to the histogram when dropped.
pub fn start_sub_query_timer(
	metrics_opt: &Option<MidnightDataSourceMetrics>,
	label: &str,
) -> Option<SubQueryTimer> {
	metrics_opt.as_ref().map(|m| SubQueryTimer {
		start: std::time::Instant::now(),
		histogram: m.time_elapsed.with_label_values(&[label]),
	})
}

/// RAII guard that records elapsed time to a Prometheus histogram on drop.
pub struct SubQueryTimer {
	start: std::time::Instant,
	histogram: prometheus_endpoint::Histogram,
}

impl Drop for SubQueryTimer {
	fn drop(&mut self) {
		self.histogram.observe(self.start.elapsed().as_secs_f64());
	}
}
