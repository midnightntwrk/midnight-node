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

//! Coherence invariants on the mainchain timing configuration.
//!
//! The mainchain timing parameters carried by [`MainchainEpochConfig`] feed directly into
//! consensus-critical epoch/slot derivation in the partner-chains
//! `sidechain_domain::mainchain_epoch` module. Several of those derivations divide by, or
//! integer-truncate against, these parameters, so an internally-incoherent configuration becomes
//! a consensus input that can either panic at a division site or silently compute divergent
//! epoch/slot boundaries for the same wall-clock time. These guards reject such a configuration
//! before it reaches consensus.
//!
//! The self-contained invariants (`I1`–`I4`, `I6`) are properties of the mainchain values alone and
//! are checked by [`check_mainchain_epoch_invariants`]. The sidechain↔mainchain cross-field invariant
//! (`I5`) additionally needs the sidechain slot configuration, which is only available at service
//! construction; it is checked by [`check_sidechain_mainchain_coherence`].

use sidechain_domain::mainchain_epoch::MainchainEpochConfig;
use sidechain_slots::ScSlotConfig;

/// The mainchain slot duration upstream `slots_per_epoch()` assumes when it divides the epoch
/// duration by a hardcoded `1000` (`mainchain_epoch.rs`). `I3` exists to keep that truncation from
/// reaching zero.
const MIN_EPOCH_DURATION_MILLIS: u64 = 1000;

/// A violated mainchain-configuration coherence invariant.
///
/// Each variant carries the offending values so the operator-facing message names the exact
/// parameters to correct. The messages are phrased for an operator reading a node start-up failure,
/// not for a developer reading a stack trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainchainEpochConfigError {
	/// `I1`: `mc__epoch_duration_millis` is zero.
	///
	/// Upstream `epochs_passed` divides elapsed time by the epoch duration; a zero divisor panics
	/// on every block produced or imported.
	EpochDurationZero,
	/// `I2`: `mc__slot_duration_millis` is zero.
	///
	/// Upstream `timestamp_to_mainchain_slot_number` divides elapsed time by the slot duration; a
	/// zero divisor panics on every block produced or imported.
	SlotDurationZero,
	/// `I3`: `mc__epoch_duration_millis` is below one second.
	///
	/// Upstream `slots_per_epoch` truncates `epoch_duration_millis / 1000` to zero for any
	/// sub-second epoch duration, which then panics as a second division-by-zero in
	/// `epoch_for_slot`.
	EpochDurationBelowMinimum {
		/// The configured `mc__epoch_duration_millis`.
		epoch_duration_millis: u64,
		/// The minimum coherent value (`1000`).
		minimum_millis: u64,
	},
	/// `I4`: `mc__epoch_duration_millis` is not an exact multiple of `mc__slot_duration_millis`.
	///
	/// A non-divisible pair makes the epoch and slot maps round differently, so two nodes with the
	/// same wall-clock time can disagree on epoch/slot boundaries with no panic and no error.
	EpochNotDivisibleBySlot {
		/// The configured `mc__epoch_duration_millis`.
		epoch_duration_millis: u64,
		/// The configured `mc__slot_duration_millis`.
		slot_duration_millis: u64,
	},
	/// `I6`: `mc__slot_duration_millis` is not exactly `1000`.
	///
	/// Upstream `slots_per_epoch` derives the slot count from `epoch_duration_millis / 1000`, a
	/// hardcoded 1000 ms slot, while `timestamp_to_mainchain_slot_number` divides by the configured
	/// `slot_duration_millis`. For any slot duration other than 1000 ms these two derivations
	/// disagree on epoch/slot boundaries with no panic and no error, so `1000` is the only mainchain
	/// slot duration coherent with the vendored upstream math.
	UnsupportedSlotDuration {
		/// The configured `mc__slot_duration_millis`.
		slot_duration_millis: u64,
	},
}

impl core::fmt::Display for MainchainEpochConfigError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::EpochDurationZero => write!(
				f,
				"mc__epoch_duration_millis must be greater than zero; a zero mainchain epoch duration divides by zero when deriving the current epoch"
			),
			Self::SlotDurationZero => write!(
				f,
				"mc__slot_duration_millis must be greater than zero; a zero mainchain slot duration divides by zero when deriving the current slot"
			),
			Self::EpochDurationBelowMinimum { epoch_duration_millis, minimum_millis } => write!(
				f,
				"mc__epoch_duration_millis ({epoch_duration_millis}) must be at least {minimum_millis}; a sub-second mainchain epoch duration truncates to zero slots per epoch and divides by zero when deriving the epoch of a slot"
			),
			Self::EpochNotDivisibleBySlot { epoch_duration_millis, slot_duration_millis } => {
				write!(
					f,
					"mc__epoch_duration_millis ({epoch_duration_millis}) must be an exact multiple of mc__slot_duration_millis ({slot_duration_millis}); a non-divisible pair makes mainchain epoch and slot boundaries round inconsistently between nodes"
				)
			},
			Self::UnsupportedSlotDuration { slot_duration_millis } => write!(
				f,
				"mc__slot_duration_millis ({slot_duration_millis}) must be exactly 1000; the mainchain slot/epoch derivation assumes a 1000 ms slot, so any other value makes mainchain epoch and slot boundaries derive inconsistently"
			),
		}
	}
}

impl std::error::Error for MainchainEpochConfigError {}

/// A violated sidechain↔mainchain cross-field coherence invariant (`I5`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusConfigCoherenceError {
	/// `I5`: the mainchain epoch duration does not partition into a whole number of sidechain
	/// epochs.
	///
	/// A sidechain epoch spans `slots_per_epoch * sc_slot_duration_millis` milliseconds. When the
	/// mainchain epoch duration is not an exact multiple of that span, the two timing domains drift
	/// against each other and mainchain epoch boundaries no longer coincide with sidechain epoch
	/// boundaries.
	MainchainEpochNotDivisibleBySidechainEpoch {
		/// `mc__epoch_duration_millis`.
		mc_epoch_duration_millis: u64,
		/// The sidechain epoch span in milliseconds (`slots_per_epoch * sc_slot_duration_millis`).
		sc_epoch_duration_millis: u128,
	},
	/// The sidechain epoch span is zero (zero slots per epoch and/or zero slot duration), so the
	/// divisibility relation is undefined.
	SidechainEpochZero {
		/// The configured sidechain `slots_per_epoch`.
		slots_per_epoch: u32,
		/// The configured sidechain slot duration in milliseconds.
		sc_slot_duration_millis: u64,
	},
}

impl core::fmt::Display for ConsensusConfigCoherenceError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::MainchainEpochNotDivisibleBySidechainEpoch {
				mc_epoch_duration_millis,
				sc_epoch_duration_millis,
			} => write!(
				f,
				"mc__epoch_duration_millis ({mc_epoch_duration_millis}) must be an exact multiple of the sidechain epoch duration ({sc_epoch_duration_millis} ms = slots_per_epoch * sc_slot_duration_millis) so mainchain epoch boundaries coincide with sidechain epoch boundaries"
			),
			Self::SidechainEpochZero { slots_per_epoch, sc_slot_duration_millis } => write!(
				f,
				"sidechain epoch duration must be greater than zero, but slots_per_epoch ({slots_per_epoch}) * sc_slot_duration_millis ({sc_slot_duration_millis}) is zero"
			),
		}
	}
}

impl std::error::Error for ConsensusConfigCoherenceError {}

/// Validates the self-contained mainchain timing invariants (`I1`–`I4`, `I6`) on `config`.
///
/// Expressed against [`MainchainEpochConfig`] — the same type the consensus code consumes — so the
/// guard sits on the consensus input rather than on ad-hoc decomposed fields. The first violated
/// invariant is returned; invariants are checked in the order `I1`, `I2`, `I3`, `I4`, `I6` so the
/// most fundamental coherence failure (a zero divisor) is reported first.
pub fn check_mainchain_epoch_invariants(
	config: &MainchainEpochConfig,
) -> Result<(), MainchainEpochConfigError> {
	let epoch_duration_millis = config.epoch_duration_millis.millis();
	let slot_duration_millis = config.slot_duration_millis.millis();

	// I1
	if epoch_duration_millis == 0 {
		return Err(MainchainEpochConfigError::EpochDurationZero);
	}
	// I2
	if slot_duration_millis == 0 {
		return Err(MainchainEpochConfigError::SlotDurationZero);
	}
	// I3
	if epoch_duration_millis < MIN_EPOCH_DURATION_MILLIS {
		return Err(MainchainEpochConfigError::EpochDurationBelowMinimum {
			epoch_duration_millis,
			minimum_millis: MIN_EPOCH_DURATION_MILLIS,
		});
	}
	// I4 — slot_duration is a non-zero divisor here (guaranteed by I2 above).
	if !epoch_duration_millis.is_multiple_of(slot_duration_millis) {
		return Err(MainchainEpochConfigError::EpochNotDivisibleBySlot {
			epoch_duration_millis,
			slot_duration_millis,
		});
	}
	// I6 — the upstream `MainchainEpochConfig::slots_per_epoch()` derives the slot count from a
	// hardcoded `epoch_duration_millis / 1000` (a 1000 ms slot), whereas
	// `timestamp_to_mainchain_slot_number` divides by the configured `slot_duration_millis`. For any
	// value other than 1000 ms these derivations disagree, so 1000 ms is the only mainchain slot
	// duration coherent with the vendored partner-chains math. Revisit (and relax to honour the
	// configured slot duration) if the partner-chains dependency is upgraded to a version whose
	// `slots_per_epoch` divides by the configured slot duration rather than a hardcoded 1000.
	//
	// Checked after I4 so the I4 non-divisibility property remains reachable (and testable) for slot
	// durations other than 1000; I4 with the slot pinned to 1000 additionally guarantees the epoch is
	// a whole number of seconds, matching the exact `epoch / 1000` upstream truncation.
	if slot_duration_millis != 1000 {
		return Err(MainchainEpochConfigError::UnsupportedSlotDuration { slot_duration_millis });
	}

	Ok(())
}

/// Validates the sidechain↔mainchain cross-field coherence invariant (`I5`).
///
/// The relation `mc_epoch_duration % (slots_per_epoch * sc_slot_duration) == 0` is the
/// [`SlotsPerEpoch`](sidechain_slots::SlotsPerEpoch) coherence property: mainchain epoch boundaries
/// must coincide with sidechain epoch boundaries. This cannot be checked at config-parse time
/// because the sidechain slot configuration is only known once the runtime API is available at
/// service construction.
///
/// The sidechain epoch span is computed in `u128` to avoid overflow when multiplying a `u32` slot
/// count by a `u64` slot duration.
pub fn check_sidechain_mainchain_coherence(
	mc_epoch_config: &MainchainEpochConfig,
	sc_slot_config: &ScSlotConfig,
) -> Result<(), ConsensusConfigCoherenceError> {
	let mc_epoch_duration_millis = mc_epoch_config.epoch_duration_millis.millis();
	let slots_per_epoch = sc_slot_config.slots_per_epoch.0;
	let sc_slot_duration_millis = sc_slot_config.slot_duration.as_millis();

	let sc_epoch_duration_millis =
		u128::from(slots_per_epoch) * u128::from(sc_slot_duration_millis);

	if sc_epoch_duration_millis == 0 {
		return Err(ConsensusConfigCoherenceError::SidechainEpochZero {
			slots_per_epoch,
			sc_slot_duration_millis,
		});
	}

	if u128::from(mc_epoch_duration_millis) % sc_epoch_duration_millis != 0 {
		return Err(ConsensusConfigCoherenceError::MainchainEpochNotDivisibleBySidechainEpoch {
			mc_epoch_duration_millis,
			sc_epoch_duration_millis,
		});
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use sidechain_slots::{SlotDuration, SlotsPerEpoch};
	use sp_core::offchain::{Duration, Timestamp};

	/// A representative, internally-coherent mainchain config: 1000 ms slots, a 5-day epoch
	/// (Cardano mainnet shape), divisible by the slot duration and at least one second.
	fn good_mc_config() -> MainchainEpochConfig {
		MainchainEpochConfig {
			epoch_duration_millis: Duration::from_millis(432_000_000),
			slot_duration_millis: Duration::from_millis(1000),
			first_epoch_timestamp_millis: Timestamp::from_unix_millis(1_596_059_091_000),
			first_epoch_number: 208,
			first_slot_number: 4_492_800,
		}
	}

	#[test]
	fn known_good_mainchain_config_passes() {
		assert_eq!(check_mainchain_epoch_invariants(&good_mc_config()), Ok(()));
	}

	#[test]
	fn rejects_zero_epoch_duration_i1() {
		let mut cfg = good_mc_config();
		cfg.epoch_duration_millis = Duration::from_millis(0);
		assert_eq!(
			check_mainchain_epoch_invariants(&cfg),
			Err(MainchainEpochConfigError::EpochDurationZero)
		);
	}

	#[test]
	fn rejects_zero_slot_duration_i2() {
		let mut cfg = good_mc_config();
		cfg.slot_duration_millis = Duration::from_millis(0);
		assert_eq!(
			check_mainchain_epoch_invariants(&cfg),
			Err(MainchainEpochConfigError::SlotDurationZero)
		);
	}

	#[test]
	fn rejects_sub_second_epoch_duration_i3() {
		let mut cfg = good_mc_config();
		// Divisible by a 1 ms slot duration, so it is rejected for being below 1000 ms (I3),
		// not for non-divisibility (I4).
		cfg.epoch_duration_millis = Duration::from_millis(999);
		cfg.slot_duration_millis = Duration::from_millis(1);
		assert_eq!(
			check_mainchain_epoch_invariants(&cfg),
			Err(MainchainEpochConfigError::EpochDurationBelowMinimum {
				epoch_duration_millis: 999,
				minimum_millis: 1000,
			})
		);
	}

	#[test]
	fn rejects_non_divisible_epoch_slot_pair_i4() {
		let mut cfg = good_mc_config();
		cfg.epoch_duration_millis = Duration::from_millis(10_000);
		cfg.slot_duration_millis = Duration::from_millis(3000);
		assert_eq!(
			check_mainchain_epoch_invariants(&cfg),
			Err(MainchainEpochConfigError::EpochNotDivisibleBySlot {
				epoch_duration_millis: 10_000,
				slot_duration_millis: 3000,
			})
		);
	}

	#[test]
	fn rejects_non_1000ms_slot_duration_i6() {
		// epoch 432_000_000 ms is an exact multiple of a 2000 ms slot (432_000_000 % 2000 == 0), so
		// this pair satisfies the I4 divisibility check, yet the vendored upstream `slots_per_epoch`
		// hardcodes `/ 1000`. The I6 guard rejects the non-1000 ms slot duration that I4 alone admits.
		let mut cfg = good_mc_config();
		cfg.slot_duration_millis = Duration::from_millis(2000);
		assert_eq!(
			check_mainchain_epoch_invariants(&cfg),
			Err(MainchainEpochConfigError::UnsupportedSlotDuration { slot_duration_millis: 2000 })
		);
	}

	fn sc_config(slots_per_epoch: u32, slot_duration_millis: u64) -> ScSlotConfig {
		ScSlotConfig {
			slots_per_epoch: SlotsPerEpoch(slots_per_epoch),
			slot_duration: SlotDuration::from_millis(slot_duration_millis),
		}
	}

	#[test]
	fn coherent_sidechain_mainchain_pair_passes_i5() {
		// 432_000_000 ms mainchain epoch; sidechain epoch = 60 * 6000 = 360_000 ms; divides evenly.
		let mc = good_mc_config();
		let sc = sc_config(60, 6000);
		assert_eq!(check_sidechain_mainchain_coherence(&mc, &sc), Ok(()));
	}

	#[test]
	fn rejects_incoherent_sidechain_mainchain_pair_i5() {
		// 432_000_000 ms mainchain epoch; sidechain epoch = 60 * 7000 = 420_000 ms; does not divide.
		let mc = good_mc_config();
		let sc = sc_config(60, 7000);
		assert_eq!(
			check_sidechain_mainchain_coherence(&mc, &sc),
			Err(ConsensusConfigCoherenceError::MainchainEpochNotDivisibleBySidechainEpoch {
				mc_epoch_duration_millis: 432_000_000,
				sc_epoch_duration_millis: 420_000,
			})
		);
	}

	#[test]
	fn rejects_zero_sidechain_epoch_span_i5() {
		let mc = good_mc_config();
		let sc = sc_config(0, 6000);
		assert_eq!(
			check_sidechain_mainchain_coherence(&mc, &sc),
			Err(ConsensusConfigCoherenceError::SidechainEpochZero {
				slots_per_epoch: 0,
				sc_slot_duration_millis: 6000,
			})
		);
	}
}
