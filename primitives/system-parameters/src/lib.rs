//! # System Parameters Primitives
//!
//! This module provides primitives for system parameters configuration.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use sidechain_domain::DParameter;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

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
