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

//! Shared plumbing: elapsed-time logging, the request deadline, the tuning bounds,
//! and the error/status-code helpers every other module in this backend uses.

use std::error::Error;
use std::time::{Duration, Instant};

use blockfrost::BlockfrostError;

pub(crate) type BoxError = Box<dyn Error + Send + Sync>;

pub(crate) const RETRY_AMOUNT: u64 = 10;
pub(crate) const PAGE_SIZE: usize = 100;

// The timing and page-count bounds are shortened under `cfg(test)` so the fake-server
// tests can reach them in milliseconds instead of minutes. Only the `not(test)` values
// ever ship; the tests assert the behaviour at the bound, not the bound itself.
#[cfg(not(test))]
pub(crate) const RETRY_DELAY: Duration = Duration::from_secs(1);
#[cfg(test)]
pub(crate) const RETRY_DELAY: Duration = Duration::from_millis(50);

/// Per-request deadline for both HTTP clients, and the bound on a whole retry loop.
#[cfg(not(test))]
pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(2);

/// Backstop for the page-walking loops, which otherwise only stop on a short
/// page. Blockfrost returns an empty page past the end, but a backend that
/// ignores `page` and keeps answering with full pages would spin forever and
/// grow memory without bound. Far above any real result set: 1M rows.
#[cfg(not(test))]
pub(crate) const MAX_PAGES: usize = 10_000;
#[cfg(test)]
pub(crate) const MAX_PAGES: usize = 3;

/// Logs the elapsed time of the enclosing scope under the `blockfrost` log target on drop.
pub(crate) struct Timer {
	label: String,
	start: Instant,
}

impl Timer {
	pub(crate) fn new(label: impl Into<String>) -> Self {
		Self { label: label.into(), start: Instant::now() }
	}
}

impl Drop for Timer {
	fn drop(&mut self) {
		log::debug!(target: "blockfrost", "{}: {}ms", self.label, self.start.elapsed().as_millis());
	}
}

pub(crate) fn is_404(e: &BlockfrostError) -> bool {
	matches!(e, BlockfrostError::Response { reason, .. } if reason.status_code == 404)
}

/// Blockfrost answers 402 once a project passes its request limit. Nothing recovers by
/// itself until the quota resets, so the node would otherwise sit there retrying forever
/// behind a bare status code. Say what happened and what it means for sync.
pub(crate) const OVER_QUOTA_MESSAGE: &str = "Blockfrost project is over its request limit (HTTP 402). Cardano main chain data \
	 cannot be read, so block import and authoring will not progress until the daily \
	 quota resets or the project's plan is upgraded.";

pub(crate) fn is_over_quota(e: &BlockfrostError) -> bool {
	matches!(e, BlockfrostError::Response { reason, .. } if reason.status_code == 402)
}

pub(crate) fn box_err(e: BlockfrostError) -> BoxError {
	// Logged as well as returned: the inherent data providers retry every block, and the
	// returned error is not always surfaced where an operator will see it.
	if is_over_quota(&e) {
		log::error!("{OVER_QUOTA_MESSAGE}");
		return OVER_QUOTA_MESSAGE.into();
	}
	format!("Blockfrost error: {e}").into()
}

/// Applies the deadline to one Blockfrost call, preserving the inner error so callers
/// can still treat a 404 as an empty result. The SDK builds its HTTP client internally,
/// on a different `reqwest` version, so the deadline cannot be configured there; without
/// one a stalled server blocks the caller indefinitely. Wrapping a call that retries
/// internally bounds the retries too, which is why the raw path uses it as well.
pub(crate) async fn deadline<T, E>(
	label: &str,
	fut: impl Future<Output = Result<T, E>>,
) -> Result<Result<T, E>, BoxError> {
	match tokio::time::timeout(HTTP_TIMEOUT, fut).await {
		Ok(result) => Ok(result),
		Err(_) => {
			Err(format!("Blockfrost request timed out after {}s: {label}", HTTP_TIMEOUT.as_secs())
				.into())
		},
	}
}

/// Error for a page-walk that hit [`MAX_PAGES`] without seeing a short page.
pub(crate) fn too_many_pages(what: &str) -> BoxError {
	format!(
		"Blockfrost returned {MAX_PAGES} full pages for {what} without reaching the end; \
		 the backend may not honor the `page` parameter"
	)
	.into()
}
