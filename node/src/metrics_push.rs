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

//! Prometheus Remote Write functionality.
//!
//! This module provides the ability to push metrics to a Prometheus-compatible
//! remote write endpoint (e.g., Thanos Receive, Cortex, Mimir).
//!
//! Uses the Prometheus Remote Write protocol:
//! - Protobuf encoding
//! - Snappy compression
//! - HTTP POST to `/api/v1/receive`

use prometheus_endpoint::{Registry, prometheus::proto::MetricType};
use prost::Message;
use std::time::Duration;

/// Configuration for the metrics remote write task.
#[derive(Clone, Debug)]
pub struct MetricsPushConfig {
	/// URL of the remote write endpoint (e.g., "https://thanos.example.com/api/v1/receive")
	pub endpoint: String,
	/// Interval between metric pushes
	pub interval: Duration,
	/// Job name to identify this node's metrics
	pub job_name: String,
	/// Peer ID derived from node_key (unique, stable identifier)
	pub peer_id: String,
	/// Node name (from --name CLI argument or auto-generated)
	pub node_name: String,
}

// Prometheus Remote Write protobuf types
// Based on: https://github.com/prometheus/prometheus/blob/main/prompb/remote.proto

#[derive(Clone, PartialEq, Message)]
pub struct WriteRequest {
	#[prost(message, repeated, tag = "1")]
	pub timeseries: Vec<TimeSeries>,
}

#[derive(Clone, PartialEq, Message)]
pub struct TimeSeries {
	#[prost(message, repeated, tag = "1")]
	pub labels: Vec<Label>,
	#[prost(message, repeated, tag = "2")]
	pub samples: Vec<Sample>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Label {
	#[prost(string, tag = "1")]
	pub name: String,
	#[prost(string, tag = "2")]
	pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct Sample {
	#[prost(double, tag = "1")]
	pub value: f64,
	#[prost(int64, tag = "2")]
	pub timestamp: i64,
}

/// Runs a background task that periodically pushes metrics via Prometheus Remote Write.
///
/// This task will run indefinitely, pushing metrics at the configured interval.
/// Errors during push are logged but do not stop the task.
pub async fn run_metrics_push_task(registry: Registry, config: MetricsPushConfig) {
	// Get hostname
	let hostname = hostname::get()
		.map(|h| h.to_string_lossy().into_owned())
		.unwrap_or_else(|_| "unknown".to_string());

	// Get local IP address
	let ip = local_ip_address::local_ip()
		.map(|ip| ip.to_string())
		.unwrap_or_else(|_| "unknown".to_string());

	log::info!(
		"Starting Prometheus remote write to {} every {:?} (job='{}', peer_id='{}', node_name='{}', hostname='{}', ip='{}')",
		config.endpoint,
		config.interval,
		config.job_name,
		config.peer_id,
		config.node_name,
		hostname,
		ip
	);

	let client = match reqwest::Client::builder().timeout(Duration::from_secs(30)).build() {
		Ok(client) => client,
		Err(e) => {
			log::error!("Failed to create HTTP client for remote write: {}", e);
			return;
		},
	};

	let mut interval = tokio::time::interval(config.interval);

	loop {
		interval.tick().await;

		match push_metrics(&registry, &client, &config, &hostname, &ip).await {
			Ok(()) => {
				log::debug!("Successfully pushed metrics to {}", config.endpoint);
			},
			Err(e) => {
				log::warn!("Failed to push metrics to {}: {}", config.endpoint, e);
			},
		}
	}
}

/// Push metrics to the remote write endpoint.
async fn push_metrics(
	registry: &Registry,
	client: &reqwest::Client,
	config: &MetricsPushConfig,
	hostname: &str,
	ip: &str,
) -> Result<(), MetricsPushError> {
	// Get current timestamp in milliseconds
	let timestamp = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|d| d.as_millis() as i64)
		.unwrap_or(0);

	// Gather all metrics from the registry
	let metric_families = registry.gather();

	// Convert to remote write format
	let mut timeseries = Vec::new();

	for family in metric_families {
		let metric_name = family.get_name();
		let metric_type = family.get_field_type();

		for metric in family.get_metric() {
			let mut labels = vec![
				Label { name: "__name__".to_string(), value: metric_name.to_string() },
				Label { name: "hostname".to_string(), value: hostname.to_string() },
				Label { name: "ip".to_string(), value: ip.to_string() },
				Label { name: "job".to_string(), value: config.job_name.clone() },
				Label { name: "node_name".to_string(), value: config.node_name.clone() },
				Label { name: "peer_id".to_string(), value: config.peer_id.clone() },
			];

			// Add metric-specific labels
			for label in metric.get_label() {
				labels.push(Label {
					name: label.get_name().to_string(),
					value: label.get_value().to_string(),
				});
			}

			// Sort labels by name (required by some remote write implementations)
			labels.sort_by(|a, b| a.name.cmp(&b.name));

			// Get the metric value based on type
			match metric_type {
				MetricType::COUNTER => {
					let value = metric.get_counter().get_value();
					timeseries
						.push(TimeSeries { labels, samples: vec![Sample { value, timestamp }] });
				},
				MetricType::GAUGE => {
					let value = metric.get_gauge().get_value();
					timeseries
						.push(TimeSeries { labels, samples: vec![Sample { value, timestamp }] });
				},
				MetricType::HISTOGRAM => {
					// For histograms, emit multiple timeseries
					let histogram = metric.get_histogram();

					// _sum
					let mut sum_labels = labels.clone();
					sum_labels[0].value = format!("{}_sum", metric_name);
					timeseries.push(TimeSeries {
						labels: sum_labels,
						samples: vec![Sample { value: histogram.get_sample_sum(), timestamp }],
					});

					// _count
					let mut count_labels = labels.clone();
					count_labels[0].value = format!("{}_count", metric_name);
					timeseries.push(TimeSeries {
						labels: count_labels,
						samples: vec![Sample {
							value: histogram.get_sample_count() as f64,
							timestamp,
						}],
					});

					// _bucket for each bucket
					for bucket in histogram.get_bucket() {
						let mut bucket_labels = labels.clone();
						bucket_labels[0].value = format!("{}_bucket", metric_name);
						bucket_labels.push(Label {
							name: "le".to_string(),
							value: bucket.get_upper_bound().to_string(),
						});
						bucket_labels.sort_by(|a, b| a.name.cmp(&b.name));
						timeseries.push(TimeSeries {
							labels: bucket_labels,
							samples: vec![Sample {
								value: bucket.get_cumulative_count() as f64,
								timestamp,
							}],
						});
					}
				},
				MetricType::SUMMARY => {
					let summary = metric.get_summary();

					// _sum
					let mut sum_labels = labels.clone();
					sum_labels[0].value = format!("{}_sum", metric_name);
					timeseries.push(TimeSeries {
						labels: sum_labels,
						samples: vec![Sample { value: summary.get_sample_sum(), timestamp }],
					});

					// _count
					let mut count_labels = labels.clone();
					count_labels[0].value = format!("{}_count", metric_name);
					timeseries.push(TimeSeries {
						labels: count_labels,
						samples: vec![Sample {
							value: summary.get_sample_count() as f64,
							timestamp,
						}],
					});
				},
				// UNTYPED and GAUGE_HISTOGRAM are less common, skip them
				_ => continue,
			}
		}
	}

	if timeseries.is_empty() {
		return Ok(());
	}

	// Create the write request
	let write_request = WriteRequest { timeseries };

	// Encode to protobuf
	let mut proto_buf = Vec::new();
	write_request
		.encode(&mut proto_buf)
		.map_err(|e| MetricsPushError::EncodeError(e.to_string()))?;

	// Snappy compress
	let compressed = snap::raw::Encoder::new()
		.compress_vec(&proto_buf)
		.map_err(|e| MetricsPushError::EncodeError(format!("Snappy compression failed: {}", e)))?;

	// POST to the remote write endpoint
	let response = client
		.post(&config.endpoint)
		.header("Content-Type", "application/x-protobuf")
		.header("Content-Encoding", "snappy")
		.header("X-Prometheus-Remote-Write-Version", "0.1.0")
		.body(compressed)
		.send()
		.await
		.map_err(|e| MetricsPushError::HttpError(e.to_string()))?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(MetricsPushError::PushFailed(format!("HTTP {}: {}", status, body)));
	}

	Ok(())
}

/// Errors that can occur during metrics push.
#[derive(Debug, thiserror::Error)]
pub enum MetricsPushError {
	#[error("Failed to encode metrics: {0}")]
	EncodeError(String),

	#[error("HTTP request failed: {0}")]
	HttpError(String),

	#[error("Remote write endpoint rejected metrics: {0}")]
	PushFailed(String),
}
