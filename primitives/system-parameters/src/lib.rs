//! # System Parameters Primitives
//!
//! This module provides primitives for system parameters configuration.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use sidechain_domain::DParameter;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// Denominator of [`SystemParametersConfig::tx_weight_factor_permille`]: a value of `1000` is
/// 1.0x, i.e. no rescaling.
pub const TX_WEIGHT_FACTOR_ONE: u32 = 1000;

/// Serde default for [`SystemParametersConfig::tx_weight_factor_permille`], so network configs
/// that predate the field keep their unscaled block capacity.
#[cfg(feature = "std")]
fn default_tx_weight_factor_permille() -> u32 {
	TX_WEIGHT_FACTOR_ONE
}

/// Configuration for Terms and Conditions (used for JSON parsing)
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermsAndConditionsConfig {
	/// SHA-256 hash of the terms and conditions document (hex-encoded with 0x prefix)
	pub hash: alloc::string::String,
	/// URL where the terms and conditions can be found
	pub url: alloc::string::String,
}

/// Configuration for D-Parameter (used for JSON parsing)
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DParameterConfig {
	/// Expected number of permissioned candidates selected for a committee
	pub num_permissioned_candidates: u16,
	/// Expected number of registered candidates selected for a committee
	pub num_registered_candidates: u16,
}

#[cfg(feature = "std")]
impl From<DParameterConfig> for DParameter {
	fn from(config: DParameterConfig) -> Self {
		DParameter::new(config.num_permissioned_candidates, config.num_registered_candidates)
	}
}

/// Configuration for System Parameters (used for JSON parsing)
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemParametersConfig {
	/// Terms and conditions configuration
	pub terms_and_conditions: TermsAndConditionsConfig,
	/// D-Parameter configuration
	pub d_parameter: DParameterConfig,
	/// How many ledger transactions a block should hold, relative to the ledger's own block
	/// limits, in permille ([`TX_WEIGHT_FACTOR_ONE`] = 1000 = unscaled, the default).
	///
	/// `500` means "half the weight per transaction", i.e. roughly twice as many transactions
	/// per block. It is consumed at genesis-build time in two places, and both must agree:
	///
	/// * `generate-genesis` divides the ledger's `limits.block_limits` by it, so the ledger
	///   itself admits proportionally more transactions per block. That also rescales the
	///   ledger-derived part of a transaction's runtime weight, which is the transaction's cost
	///   *normalised against those limits*.
	/// * the chain spec passes it to `pallet_midnight`, which applies it to the flat,
	///   ledger-independent per-transaction weight (`ConfigurableTransactionSizeWeight`) — the
	///   one term that does not follow the block limits.
	///
	/// Intended for test and perf networks. `0` would make ledger transactions weightless and
	/// is rejected by the genesis generator.
	#[serde(default = "default_tx_weight_factor_permille")]
	pub tx_weight_factor_permille: u32,
}

#[cfg(feature = "std")]
impl SystemParametersConfig {
	/// Parse the hash string to bytes (expects 0x-prefixed hex string for 32-byte hash)
	pub fn terms_and_conditions_hash_bytes(&self) -> Result<[u8; 32], &'static str> {
		let hash_str = self
			.terms_and_conditions
			.hash
			.strip_prefix("0x")
			.unwrap_or(&self.terms_and_conditions.hash);
		let bytes = hex::decode(hash_str).map_err(|_| "Invalid hex encoding for hash")?;
		if bytes.len() != 32 {
			return Err("Hash must be 32 bytes (SHA-256)");
		}
		let mut arr = [0u8; 32];
		arr.copy_from_slice(&bytes);
		Ok(arr)
	}
}
