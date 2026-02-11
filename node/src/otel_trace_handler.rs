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

//! OpenTelemetry trace handler for bridging Substrate spans to Datadog.
//!
//! This module provides a [`TraceHandler`] implementation that converts Substrate's
//! native tracing spans into OpenTelemetry spans and exports them to Datadog.

use opentelemetry::{
	global,
	trace::{SpanBuilder, SpanKind, TraceContextExt},
	Context, KeyValue,
};
use sc_tracing::{SpanDatum, TraceEvent, TraceHandler};
use std::{
	collections::HashMap,
	sync::{
		atomic::{AtomicU64, Ordering},
		Mutex,
	},
	time::{Duration, SystemTime},
};

/// Minimum duration for a span to be exported (filters out micro-operations)
const MIN_SPAN_DURATION: Duration = Duration::from_micros(100);

/// Default sample rate (1.0 = 100%, 0.01 = 1%)
/// Can be overridden with OTEL_TRACE_SAMPLE_RATE env var
const DEFAULT_SAMPLE_RATE: f64 = 0.01;

/// Low-level operations to filter out (noise reduction)
const FILTERED_OPERATIONS: &[&str] = &[
	"malloc_version_1",
	"free_version_1",
	"blake2_256_version_1",
	"blake2_128_version_1",
	"twox_128_version_1",
	"twox_64_version_1",
	"max_level_version_1",
	"get_version_1",
];

/// Operations to always include regardless of duration
const IMPORTANT_OPERATIONS: &[&str] = &[
	"import_block",
	"execute_block",
	"validate_transaction",
	"apply_extrinsic",
	"on_initialize",
	"on_finalize",
	"propose",
	"seal",
];

/// A trace handler that exports Substrate spans to OpenTelemetry/Datadog.
///
/// This handler receives completed span data from Substrate's tracing system
/// and converts them into OpenTelemetry spans with proper timing and attributes.
pub struct OpenTelemetryTraceHandler {
	service_name: String,
	/// Maps Substrate span IDs to OpenTelemetry contexts for parent-child relationships
	span_contexts: Mutex<HashMap<u64, Context>>,
	/// Debug counters
	spans_received: AtomicU64,
	spans_filtered: AtomicU64,
	spans_exported: AtomicU64,
	/// Sample rate for non-important spans (0.0 to 1.0)
	/// Important operations are always sampled at 100%
	sample_rate: f64,
}

impl OpenTelemetryTraceHandler {
	/// Creates a new OpenTelemetry trace handler.
	///
	/// # Arguments
	/// * `service_name` - The service name to use for spans (e.g., "midnight-node")
	pub fn new(service_name: &str) -> Self {
		let sample_rate = std::env::var("OTEL_TRACE_SAMPLE_RATE")
			.ok()
			.and_then(|s| s.parse::<f64>().ok())
			.unwrap_or(DEFAULT_SAMPLE_RATE)
			.clamp(0.0, 1.0);

		eprintln!(
			"[midnight-node] OpenTelemetry trace handler initialized (sample_rate: {}%)",
			sample_rate * 100.0
		);

		Self {
			service_name: service_name.to_string(),
			span_contexts: Mutex::new(HashMap::new()),
			spans_received: AtomicU64::new(0),
			spans_filtered: AtomicU64::new(0),
			spans_exported: AtomicU64::new(0),
			sample_rate,
		}
	}

	/// Check if a span should be filtered out
	fn should_filter_span(name: &str, duration: Duration) -> bool {
		// Always include important operations
		if IMPORTANT_OPERATIONS.iter().any(|op| name.contains(op)) {
			return false;
		}

		// Filter out known noisy low-level operations
		if FILTERED_OPERATIONS.iter().any(|op| name == *op) {
			return true;
		}

		// Filter out very short spans (likely internal housekeeping)
		if duration < MIN_SPAN_DURATION {
			return true;
		}

		false
	}

	/// Categorize a span for better Datadog grouping
	fn categorize_span(name: &str, target: &str) -> (&'static str, String) {
		// Return (category, resource_name)
		if name.contains("block") || target.contains("block") {
			("block", format!("block.{}", name))
		} else if name.contains("transaction") || name.contains("extrinsic") {
			("transaction", format!("tx.{}", name))
		} else if target.starts_with("sc_consensus") || name.contains("consensus") {
			("consensus", format!("consensus.{}", name))
		} else if target.starts_with("sc_network") || name.contains("network") {
			("network", format!("network.{}", name))
		} else if target.contains("runtime")
			|| target.starts_with("frame")
			|| target.starts_with("pallet")
		{
			("runtime", format!("runtime.{}", name))
		} else if target.starts_with("sp_io") {
			("wasm", format!("wasm.{}", name))
		} else if name.contains("db") || name.contains("storage") || target.contains("db") {
			("storage", format!("storage.{}", name))
		} else {
			("substrate", name.to_string())
		}
	}

	/// Converts a tracing Level to an OpenTelemetry-compatible string.
	fn level_to_string(level: &tracing::Level) -> &'static str {
		match *level {
			tracing::Level::ERROR => "ERROR",
			tracing::Level::WARN => "WARN",
			tracing::Level::INFO => "INFO",
			tracing::Level::DEBUG => "DEBUG",
			tracing::Level::TRACE => "TRACE",
		}
	}

	/// Converts Substrate span values to OpenTelemetry attributes.
	fn values_to_attributes(values: &sc_tracing::Values) -> Vec<KeyValue> {
		let mut attrs = Vec::new();

		for (k, v) in &values.bool_values {
			attrs.push(KeyValue::new(k.clone(), *v));
		}
		for (k, v) in &values.i64_values {
			attrs.push(KeyValue::new(k.clone(), *v));
		}
		for (k, v) in &values.u64_values {
			attrs.push(KeyValue::new(k.clone(), *v as i64));
		}
		for (k, v) in &values.string_values {
			attrs.push(KeyValue::new(k.clone(), v.clone()));
		}

		attrs
	}

	/// Calculates the start time from overall duration.
	/// Since we receive completed spans, we need to work backwards from now.
	fn calculate_start_time(overall_time: Duration) -> SystemTime {
		SystemTime::now().checked_sub(overall_time).unwrap_or_else(SystemTime::now)
	}
}

impl TraceHandler for OpenTelemetryTraceHandler {
	fn handle_span(&self, span_datum: &SpanDatum) {
		let received = self.spans_received.fetch_add(1, Ordering::Relaxed) + 1;

		// Debug: print first span and periodic stats
		if received == 1 {
			eprintln!(
				"[otel-debug] FIRST SPAN RECEIVED! name={}, target={}, duration={:?}",
				span_datum.name, span_datum.target, span_datum.overall_time
			);
		}

		// Check if this span should be filtered out
		if Self::should_filter_span(&span_datum.name, span_datum.overall_time) {
			self.spans_filtered.fetch_add(1, Ordering::Relaxed);
			return;
		}

		// Apply sampling for non-important operations
		// Important operations (import_block, execute_block, etc.) are always sampled
		let is_important = IMPORTANT_OPERATIONS.iter().any(|op| span_datum.name.contains(op));
		if !is_important && self.sample_rate < 1.0 {
			// Use counter-based sampling: export every Nth span where N = 1/sample_rate
			let sample_interval = (1.0 / self.sample_rate) as u64;
			if sample_interval > 0 && received % sample_interval != 0 {
				self.spans_filtered.fetch_add(1, Ordering::Relaxed);
				return;
			}
		}

		let tracer = global::tracer(self.service_name.clone());

		// Categorize the span for better Datadog organization
		let (category, resource_name) = Self::categorize_span(&span_datum.name, &span_datum.target);

		// Build attributes from span data
		let mut attributes = Self::values_to_attributes(&span_datum.values);
		attributes.push(KeyValue::new("substrate.target", span_datum.target.clone()));
		attributes.push(KeyValue::new("substrate.level", Self::level_to_string(&span_datum.level)));
		attributes.push(KeyValue::new("substrate.line", span_datum.line as i64));
		attributes.push(KeyValue::new("substrate.span_id", span_datum.id.into_u64() as i64));
		attributes.push(KeyValue::new("category", category));
		attributes.push(KeyValue::new("resource.name", resource_name.clone()));

		// Add duration in milliseconds as an attribute for easier filtering
		attributes.push(KeyValue::new("duration_ms", span_datum.overall_time.as_millis() as i64));

		// Calculate timing
		let start_time = Self::calculate_start_time(span_datum.overall_time);
		let end_time = SystemTime::now();

		// Get parent context if available
		let parent_context = span_datum.parent_id.as_ref().and_then(|parent_id| {
			self.span_contexts
				.lock()
				.ok()
				.and_then(|contexts| contexts.get(&parent_id.into_u64()).cloned())
		});

		// Create the span builder with categorized name
		let span_name = resource_name.clone();
		let span_builder = SpanBuilder::from_name(resource_name)
			.with_kind(SpanKind::Internal)
			.with_start_time(start_time)
			.with_attributes(attributes);

		// Build and start the span with proper parent context
		let span = if let Some(ref parent_ctx) = parent_context {
			span_builder.start_with_context(&tracer, parent_ctx)
		} else {
			span_builder.start(&tracer)
		};

		// Create context for potential child spans
		let cx = Context::current_with_span(span);

		// Store the context for child spans to reference
		if let Ok(mut contexts) = self.span_contexts.lock() {
			contexts.insert(span_datum.id.into_u64(), cx.clone());

			// Clean up old contexts to prevent memory leaks
			// Keep only recent contexts (parent might still be referenced)
			if contexts.len() > 10000 {
				// Simple cleanup: remove oldest entries
				// In production, you might want a more sophisticated LRU cache
				let keys_to_remove: Vec<_> =
					contexts.keys().take(contexts.len() / 2).cloned().collect();
				for key in keys_to_remove {
					contexts.remove(&key);
				}
			}
		}

		// End the span with the calculated end time
		// The span is automatically ended when dropped, but we want explicit timing
		cx.span().end_with_timestamp(end_time);

		let exported = self.spans_exported.fetch_add(1, Ordering::Relaxed) + 1;

		// Debug: print stats every 1000 exported spans
		if exported == 1 || exported % 1000 == 0 {
			let received = self.spans_received.load(Ordering::Relaxed);
			let filtered = self.spans_filtered.load(Ordering::Relaxed);
			eprintln!(
				"[otel-debug] Stats: received={}, filtered={}, exported={} (sample_rate={}%)",
				received, filtered, exported, self.sample_rate * 100.0
			);
		}
	}

	fn handle_event(&self, event: &TraceEvent) {
		// For events, we're more selective - only export important events
		// Events are typically log messages, not operations we want to trace
		let dominated_by_noise = event.target.starts_with("sp_io")
			|| event.target.contains("allocator")
			|| event.name.contains("malloc")
			|| event.name.contains("free");

		if dominated_by_noise {
			return;
		}

		let tracer = global::tracer(self.service_name.clone());

		// Categorize the event
		let (category, resource_name) = Self::categorize_span(&event.name, &event.target);

		// Convert event to a short-lived span (events don't have duration)
		let mut attributes = Self::values_to_attributes(&event.values);
		attributes.push(KeyValue::new("substrate.target", event.target.clone()));
		attributes.push(KeyValue::new("substrate.level", Self::level_to_string(&event.level)));
		attributes.push(KeyValue::new("event.name", event.name.clone()));
		attributes.push(KeyValue::new("category", category));
		attributes.push(KeyValue::new("span.type", "event"));

		// Get parent context if available
		let parent_context = event.parent_id.as_ref().and_then(|parent_id| {
			self.span_contexts
				.lock()
				.ok()
				.and_then(|contexts| contexts.get(&parent_id.into_u64()).cloned())
		});

		let now = SystemTime::now();
		let span_builder = SpanBuilder::from_name(format!("event: {}", resource_name))
			.with_kind(SpanKind::Internal)
			.with_start_time(now)
			.with_attributes(attributes);

		let span = if let Some(ref parent_ctx) = parent_context {
			span_builder.start_with_context(&tracer, parent_ctx)
		} else {
			span_builder.start(&tracer)
		};

		// Events are instantaneous, so end immediately
		let cx = Context::current_with_span(span);
		cx.span().end_with_timestamp(now);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_level_to_string() {
		assert_eq!(OpenTelemetryTraceHandler::level_to_string(&tracing::Level::ERROR), "ERROR");
		assert_eq!(OpenTelemetryTraceHandler::level_to_string(&tracing::Level::INFO), "INFO");
	}

	#[test]
	fn test_calculate_start_time() {
		let duration = Duration::from_secs(1);
		let start = OpenTelemetryTraceHandler::calculate_start_time(duration);
		let now = SystemTime::now();

		// Start time should be approximately 1 second before now
		let elapsed = now.duration_since(start).unwrap();
		assert!(elapsed >= Duration::from_millis(900));
		assert!(elapsed <= Duration::from_millis(1100));
	}

	#[test]
	fn test_handler_creation() {
		let handler = OpenTelemetryTraceHandler::new("test-service");
		assert_eq!(handler.service_name, "test-service");
	}

	#[test]
	fn test_should_filter_span() {
		// Low-level operations should be filtered
		assert!(OpenTelemetryTraceHandler::should_filter_span(
			"malloc_version_1",
			Duration::from_micros(50)
		));
		assert!(OpenTelemetryTraceHandler::should_filter_span(
			"free_version_1",
			Duration::from_micros(50)
		));
		assert!(OpenTelemetryTraceHandler::should_filter_span(
			"blake2_256_version_1",
			Duration::from_micros(50)
		));

		// Important operations should never be filtered
		assert!(!OpenTelemetryTraceHandler::should_filter_span(
			"import_block",
			Duration::from_micros(50)
		));
		assert!(!OpenTelemetryTraceHandler::should_filter_span(
			"execute_block",
			Duration::from_millis(100)
		));

		// Short unknown operations should be filtered
		assert!(OpenTelemetryTraceHandler::should_filter_span(
			"unknown_op",
			Duration::from_micros(50)
		));

		// Longer operations should pass
		assert!(!OpenTelemetryTraceHandler::should_filter_span(
			"some_operation",
			Duration::from_millis(1)
		));
	}

	#[test]
	fn test_categorize_span() {
		let (cat, _) = OpenTelemetryTraceHandler::categorize_span("import_block", "sc_consensus");
		assert_eq!(cat, "block");

		let (cat, _) =
			OpenTelemetryTraceHandler::categorize_span("validate_transaction", "pallet_midnight");
		assert_eq!(cat, "transaction");

		let (cat, _) = OpenTelemetryTraceHandler::categorize_span("propose", "sc_consensus_aura");
		assert_eq!(cat, "consensus");
	}
}
