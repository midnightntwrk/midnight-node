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
use prometheus_endpoint::{
	CounterVec, HistogramOpts, HistogramVec, Opts, PrometheusError, Registry, U64, register,
};

pub type MetricsRegistry = Registry;

/// Prometheus metrics client for Midnight-specific data sources.
///
/// Provides public accessor methods, unlike the upstream `McFollowerMetrics`
/// whose accessors are crate-private in partner-chains.
#[derive(Clone)]
pub struct MidnightDataSourceMetrics {
	time_elapsed: HistogramVec,
	call_count: CounterVec<U64>,
}

impl MidnightDataSourceMetrics {
	pub fn time_elapsed(&self) -> &HistogramVec {
		&self.time_elapsed
	}

	pub fn call_count(&self) -> &CounterVec<U64> {
		&self.call_count
	}

	pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			time_elapsed: register(
				HistogramVec::new(
					HistogramOpts::new(
						"midnight_data_source_method_time_elapsed",
						"Time spent in a midnight data source method call",
					),
					&["method_name"],
				)?,
				registry,
			)?,
			call_count: register(
				CounterVec::new(
					Opts::new(
						"midnight_data_source_method_call_count",
						"Total number of midnight data source method calls",
					),
					&["method_name"],
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
