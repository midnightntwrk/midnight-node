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

//! Subscription bounding infrastructure for finality RPC handlers.
//!
//! Provides [`SubscriptionTracker`] for enforcing a global limit on the number
//! of concurrent finality subscriptions (GRANDPA + BEEFY), and
//! [`SubscriptionMetrics`] for Prometheus monitoring.

use prometheus_endpoint::{Counter, Gauge, PrometheusError, Registry, U64, register};
use std::sync::{
	Arc,
	atomic::{AtomicU32, Ordering},
};

/// Optional Prometheus metrics for subscription tracking.
#[derive(Clone)]
pub struct SubscriptionMetrics {
	active: Gauge<U64>,
	rejected: Counter<U64>,
}

impl SubscriptionMetrics {
	/// Register metrics with the given Prometheus registry.
	pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			active: register(
				Gauge::new(
					"midnight_rpc_finality_subscriptions_active",
					"Number of active finality RPC subscriptions (GRANDPA + BEEFY)",
				)?,
				registry,
			)?,
			rejected: register(
				Counter::new(
					"midnight_rpc_finality_subscriptions_rejected_total",
					"Total finality RPC subscriptions rejected due to limit",
				)?,
				registry,
			)?,
		})
	}
}

/// Tracks the number of active finality subscriptions and enforces a global limit.
///
/// Shared across all RPC handler instances via `Arc`. When a subscription is
/// acquired, a [`SubscriptionGuard`] is returned that decrements the counter
/// on drop (RAII).
#[derive(Clone)]
pub struct SubscriptionTracker {
	inner: Arc<SubscriptionTrackerInner>,
}

struct SubscriptionTrackerInner {
	active: AtomicU32,
	max: u32,
	metrics: Option<SubscriptionMetrics>,
}

impl SubscriptionTracker {
	/// Create a new tracker with the given limit and optional metrics.
	pub fn new(max: u32, metrics: Option<SubscriptionMetrics>) -> Self {
		Self {
			inner: Arc::new(SubscriptionTrackerInner { active: AtomicU32::new(0), max, metrics }),
		}
	}

	/// Try to acquire a subscription slot. Returns a [`SubscriptionGuard`] on
	/// success, or `None` if the global limit has been reached.
	pub fn try_acquire(&self) -> Option<SubscriptionGuard> {
		loop {
			let current = self.inner.active.load(Ordering::Relaxed);
			if current >= self.inner.max {
				log::warn!(
					"Finality subscription rejected: limit of {} reached ({} active)",
					self.inner.max,
					current,
				);
				if let Some(m) = &self.inner.metrics {
					m.rejected.inc();
				}
				return None;
			}
			if self
				.inner
				.active
				.compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Relaxed)
				.is_ok()
			{
				if let Some(m) = &self.inner.metrics {
					m.active.inc();
				}
				return Some(SubscriptionGuard { tracker: self.inner.clone() });
			}
		}
	}

	/// Current number of active subscriptions (for diagnostics / tests).
	pub fn active_count(&self) -> u32 {
		self.inner.active.load(Ordering::Relaxed)
	}
}

/// RAII guard that decrements the subscription counter when dropped.
pub struct SubscriptionGuard {
	tracker: Arc<SubscriptionTrackerInner>,
}

impl Drop for SubscriptionGuard {
	fn drop(&mut self) {
		self.tracker.active.fetch_sub(1, Ordering::AcqRel);
		if let Some(m) = &self.tracker.metrics {
			m.active.dec();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tracker_allows_under_limit() {
		let tracker = SubscriptionTracker::new(3, None);
		let g1 = tracker.try_acquire();
		let g2 = tracker.try_acquire();
		let g3 = tracker.try_acquire();
		assert!(g1.is_some());
		assert!(g2.is_some());
		assert!(g3.is_some());
		assert_eq!(tracker.active_count(), 3);
	}

	#[test]
	fn tracker_rejects_at_limit() {
		let tracker = SubscriptionTracker::new(2, None);
		let _g1 = tracker.try_acquire().unwrap();
		let _g2 = tracker.try_acquire().unwrap();
		assert!(tracker.try_acquire().is_none());
	}

	#[test]
	fn guard_decrements_on_drop() {
		let tracker = SubscriptionTracker::new(2, None);
		let g1 = tracker.try_acquire().unwrap();
		assert_eq!(tracker.active_count(), 1);
		drop(g1);
		assert_eq!(tracker.active_count(), 0);
		// Slot is now available again
		assert!(tracker.try_acquire().is_some());
	}

	#[test]
	fn tracker_zero_limit_rejects_all() {
		let tracker = SubscriptionTracker::new(0, None);
		assert!(tracker.try_acquire().is_none());
	}

	#[test]
	fn tracker_concurrent_access() {
		let tracker = SubscriptionTracker::new(100, None);
		let barrier = Arc::new(std::sync::Barrier::new(101));
		let handles: Vec<_> = (0..100)
			.map(|_| {
				let t = tracker.clone();
				let b = barrier.clone();
				std::thread::spawn(move || {
					let guard = t.try_acquire();
					b.wait();
					guard
				})
			})
			.collect();
		barrier.wait();
		assert_eq!(tracker.active_count(), 100);
		assert!(tracker.try_acquire().is_none());
		let guards: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
		assert!(guards.iter().all(|g| g.is_some()));
	}
}
