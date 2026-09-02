// This file is part of midnight-indexer.
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

//! Helpers shared by the periodic polling of the shielded and unshielded
//! transaction subscriptions: interval jitter (so concurrent subscribers do not
//! synchronise their database hits) and idle backoff (so an unchanging
//! subscription polls less often).

use chacha20poly1305::aead::{OsRng, rand_core::RngCore};
use futures::{Stream, stream};
use std::time::Duration;
use tokio::time::sleep;

/// Fraction by which an interval is randomly shortened or lengthened (±20%).
const JITTER_FRACTION: f64 = 0.2; // TODO: make configurable if tuning is needed.

/// Upper bound on idle backoff, expressed as a multiple of the base interval.
const IDLE_BACKOFF_MAX_MULTIPLE: u32 = 8; // TODO: make configurable if tuning is needed.

/// Apply ±[`JITTER_FRACTION`] random jitter to `base`, so that subscribers that
/// started at the same time do not keep firing their periodic work in lockstep.
pub(super) fn jittered(base: Duration) -> Duration {
    let unit = f64::from(OsRng.next_u32()) / f64::from(u32::MAX);
    let factor = 1.0 + JITTER_FRACTION * (2.0 * unit - 1.0);
    base.mul_f64(factor)
}

/// An unbounded stream that yields `()` immediately, then after each jittered
/// `base` interval. Like [`tokio::time::interval`] the first item fires
/// immediately (so the first keep-alive refreshes `last_active` right away);
/// only the subsequent, steady-state ticks carry jitter.
pub(super) fn jittered_interval(base: Duration) -> impl Stream<Item = ()> {
    stream::unfold(true, move |first| async move {
        if !first {
            sleep(jittered(base)).await;
        }
        Some(((), false))
    })
}

/// Next polling interval for idle backoff: reset to `base` when the polled state
/// changed since the previous tick, otherwise double the current interval up to
/// a cap of [`IDLE_BACKOFF_MAX_MULTIPLE`] × `base`.
pub(super) fn next_poll_interval(
    current_interval: Duration,
    base: Duration,
    changed: bool,
) -> Duration {
    if changed {
        base
    } else {
        (current_interval * 2).min(base * IDLE_BACKOFF_MAX_MULTIPLE)
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::api::v4::subscription::polling::{
        IDLE_BACKOFF_MAX_MULTIPLE, JITTER_FRACTION, jittered, next_poll_interval,
    };
    use std::time::Duration;

    #[test]
    fn jittered_stays_within_bounds() {
        let base = Duration::from_secs(30);
        let lo = base.mul_f64(1.0 - JITTER_FRACTION);
        let hi = base.mul_f64(1.0 + JITTER_FRACTION);
        for _ in 0..10_000 {
            let d = jittered(base);
            assert!(d >= lo && d <= hi, "{d:?} not in [{lo:?}, {hi:?}]");
        }
    }

    #[test]
    fn backoff_resets_to_base_on_change() {
        let base = Duration::from_secs(30);
        assert_eq!(next_poll_interval(base * 4, base, true), base);
    }

    #[test]
    fn backoff_doubles_when_idle_up_to_cap() {
        let base = Duration::from_secs(30);
        let cap = base * IDLE_BACKOFF_MAX_MULTIPLE;
        assert_eq!(next_poll_interval(base, base, false), base * 2);
        assert_eq!(next_poll_interval(base * 2, base, false), base * 4);
        assert_eq!(next_poll_interval(base * 4, base, false), cap);
        // Already at or beyond the cap stays capped.
        assert_eq!(next_poll_interval(cap, base, false), cap);
        assert_eq!(next_poll_interval(base * 6, base, false), cap);
    }
}
