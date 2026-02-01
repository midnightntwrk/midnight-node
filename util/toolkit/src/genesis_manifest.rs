// Copyright 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0

//! Genesis manifest generation for linking chain_spec to input configs.
//!
//! The manifest provides integrity verification by including SHA256 hashes
//! of all input config files used during genesis generation.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Manifest that links a generated chain_spec to its input configurations.
///
/// This enables verification that a chain_spec was generated using specific
/// config files by comparing hashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisManifest {
	/// Filename of the generated chain_spec (relative to manifest location)
	pub chain_spec_state: String,
	/// Filename of the generated genesis block (relative to manifest location)
	pub chain_spec_block: String,
	/// SHA256 hash of the genesis state file (hex-encoded)
	pub chain_spec_state_hash: String,
	/// SHA256 hash of the genesis block file (hex-encoded)
	pub chain_spec_block_hash: String,
	/// Treasury config filename (if used)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub treasury_config: Option<String>,
	/// SHA256 hash of the treasury config file (hex-encoded)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub treasury_config_hash: Option<String>,
	/// cNight generates dust config filename (if used)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cnight_generates_dust_config: Option<String>,
	/// SHA256 hash of the cNight generates dust config file (hex-encoded)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cnight_generates_dust_config_hash: Option<String>,
	/// Network name used for generation
	pub network: String,
	/// ISO 8601 timestamp of when the genesis was generated
	pub generated_at: String,
}

impl GenesisManifest {
	/// Create a new manifest builder
	pub fn builder(network: &str) -> GenesisManifestBuilder {
		GenesisManifestBuilder {
			network: network.to_string(),
			chain_spec_state: None,
			chain_spec_block: None,
			chain_spec_state_hash: None,
			chain_spec_block_hash: None,
			treasury_config: None,
			treasury_config_hash: None,
			cnight_generates_dust_config: None,
			cnight_generates_dust_config_hash: None,
		}
	}

	/// Write the manifest to a file
	pub fn write_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
		let json = serde_json::to_string_pretty(self)
			.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
		std::fs::write(path, json)
	}

	/// Read a manifest from a file
	pub fn read_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
		let json = std::fs::read_to_string(path)?;
		let manifest: Self = serde_json::from_str(&json)?;
		Ok(manifest)
	}
}

/// Builder for creating a GenesisManifest
pub struct GenesisManifestBuilder {
	network: String,
	chain_spec_state: Option<String>,
	chain_spec_block: Option<String>,
	chain_spec_state_hash: Option<String>,
	chain_spec_block_hash: Option<String>,
	treasury_config: Option<String>,
	treasury_config_hash: Option<String>,
	cnight_generates_dust_config: Option<String>,
	cnight_generates_dust_config_hash: Option<String>,
}

impl GenesisManifestBuilder {
	/// Set the chain_spec state file info (computes hash from file)
	pub fn with_chain_spec_state(
		mut self,
		filename: &str,
		path: &Path,
	) -> Result<Self, std::io::Error> {
		self.chain_spec_state = Some(filename.to_string());
		self.chain_spec_state_hash = Some(hash_file(path)?);
		Ok(self)
	}

	/// Set the chain_spec block file info (computes hash from file)
	pub fn with_chain_spec_block(
		mut self,
		filename: &str,
		path: &Path,
	) -> Result<Self, std::io::Error> {
		self.chain_spec_block = Some(filename.to_string());
		self.chain_spec_block_hash = Some(hash_file(path)?);
		Ok(self)
	}

	/// Set the treasury config file info (computes hash from file)
	pub fn with_treasury_config(
		mut self,
		filename: &str,
		path: &Path,
	) -> Result<Self, std::io::Error> {
		self.treasury_config = Some(filename.to_string());
		self.treasury_config_hash = Some(hash_file(path)?);
		Ok(self)
	}

	/// Set the cNight generates dust config file info (computes hash from file)
	pub fn with_cnight_generates_dust_config(
		mut self,
		filename: &str,
		path: &Path,
	) -> Result<Self, std::io::Error> {
		self.cnight_generates_dust_config = Some(filename.to_string());
		self.cnight_generates_dust_config_hash = Some(hash_file(path)?);
		Ok(self)
	}

	/// Build the manifest
	pub fn build(self) -> Result<GenesisManifest, &'static str> {
		let chain_spec_state = self.chain_spec_state.ok_or("chain_spec_state is required")?;
		let chain_spec_block = self.chain_spec_block.ok_or("chain_spec_block is required")?;
		let chain_spec_state_hash =
			self.chain_spec_state_hash.ok_or("chain_spec_state_hash is required")?;
		let chain_spec_block_hash =
			self.chain_spec_block_hash.ok_or("chain_spec_block_hash is required")?;

		// Generate ISO 8601 timestamp
		let now = std::time::SystemTime::now();
		let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
		let secs = duration.as_secs();
		// Simple ISO 8601 format: YYYY-MM-DDTHH:MM:SSZ
		// We'll use a simple calculation for UTC time
		let days_since_epoch = secs / 86400;
		let time_of_day = secs % 86400;
		let hours = time_of_day / 3600;
		let minutes = (time_of_day % 3600) / 60;
		let seconds = time_of_day % 60;

		// Calculate year/month/day from days since epoch (1970-01-01)
		let (year, month, day) = days_to_ymd(days_since_epoch);
		let generated_at = format!(
			"{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
			year, month, day, hours, minutes, seconds
		);

		Ok(GenesisManifest {
			chain_spec_state,
			chain_spec_block,
			chain_spec_state_hash,
			chain_spec_block_hash,
			treasury_config: self.treasury_config,
			treasury_config_hash: self.treasury_config_hash,
			cnight_generates_dust_config: self.cnight_generates_dust_config,
			cnight_generates_dust_config_hash: self.cnight_generates_dust_config_hash,
			network: self.network,
			generated_at,
		})
	}
}

/// Convert days since Unix epoch to year/month/day
fn days_to_ymd(days: u64) -> (u32, u32, u32) {
	// Simplified algorithm for dates after 1970
	let mut remaining_days = days as i64;
	let mut year = 1970u32;

	loop {
		let days_in_year = if is_leap_year(year) { 366 } else { 365 };
		if remaining_days < days_in_year {
			break;
		}
		remaining_days -= days_in_year;
		year += 1;
	}

	let leap = is_leap_year(year);
	let days_in_months: [i64; 12] = if leap {
		[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
	} else {
		[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
	};

	let mut month = 1u32;
	for days_in_month in days_in_months.iter() {
		if remaining_days < *days_in_month {
			break;
		}
		remaining_days -= days_in_month;
		month += 1;
	}

	let day = remaining_days as u32 + 1;
	(year, month, day)
}

fn is_leap_year(year: u32) -> bool {
	(year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Compute SHA256 hash of a file, returning hex-encoded string
pub fn hash_file(path: &Path) -> Result<String, std::io::Error> {
	let contents = std::fs::read(path)?;
	let hash = Sha256::digest(&contents);
	Ok(format!("sha256:{}", hex::encode(hash)))
}

/// Compute SHA256 hash of bytes, returning hex-encoded string
pub fn hash_bytes(data: &[u8]) -> String {
	let hash = Sha256::digest(data);
	format!("sha256:{}", hex::encode(hash))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;
	use tempfile::NamedTempFile;

	#[test]
	fn test_hash_file() {
		let mut file = NamedTempFile::new().unwrap();
		file.write_all(b"test content").unwrap();
		file.flush().unwrap();

		let hash = hash_file(file.path()).unwrap();
		assert!(hash.starts_with("sha256:"));
		assert_eq!(hash.len(), 7 + 64); // "sha256:" + 64 hex chars
	}

	#[test]
	fn test_hash_bytes() {
		let hash = hash_bytes(b"test content");
		assert!(hash.starts_with("sha256:"));
		assert_eq!(hash.len(), 7 + 64);
	}

	#[test]
	fn test_manifest_builder() {
		let mut state_file = NamedTempFile::new().unwrap();
		state_file.write_all(b"state data").unwrap();
		state_file.flush().unwrap();

		let mut block_file = NamedTempFile::new().unwrap();
		block_file.write_all(b"block data").unwrap();
		block_file.flush().unwrap();

		let manifest = GenesisManifest::builder("testnet")
			.with_chain_spec_state("genesis_state.mn", state_file.path())
			.unwrap()
			.with_chain_spec_block("genesis_block.mn", block_file.path())
			.unwrap()
			.build()
			.unwrap();

		assert_eq!(manifest.network, "testnet");
		assert_eq!(manifest.chain_spec_state, "genesis_state.mn");
		assert!(manifest.chain_spec_state_hash.starts_with("sha256:"));
		assert!(manifest.treasury_config.is_none());
	}

	#[test]
	fn test_manifest_with_treasury_config() {
		let mut state_file = NamedTempFile::new().unwrap();
		state_file.write_all(b"state").unwrap();
		state_file.flush().unwrap();

		let mut block_file = NamedTempFile::new().unwrap();
		block_file.write_all(b"block").unwrap();
		block_file.flush().unwrap();

		let mut treasury_file = NamedTempFile::new().unwrap();
		treasury_file.write_all(b"treasury config").unwrap();
		treasury_file.flush().unwrap();

		let manifest = GenesisManifest::builder("preview")
			.with_chain_spec_state("state.mn", state_file.path())
			.unwrap()
			.with_chain_spec_block("block.mn", block_file.path())
			.unwrap()
			.with_treasury_config("treasury.json", treasury_file.path())
			.unwrap()
			.build()
			.unwrap();

		assert!(manifest.treasury_config.is_some());
		assert!(manifest.treasury_config_hash.is_some());
		assert!(manifest.treasury_config_hash.unwrap().starts_with("sha256:"));
	}

	#[test]
	fn test_manifest_serialization() {
		let mut state_file = NamedTempFile::new().unwrap();
		state_file.write_all(b"state").unwrap();
		state_file.flush().unwrap();

		let mut block_file = NamedTempFile::new().unwrap();
		block_file.write_all(b"block").unwrap();
		block_file.flush().unwrap();

		let manifest = GenesisManifest::builder("dev")
			.with_chain_spec_state("state.mn", state_file.path())
			.unwrap()
			.with_chain_spec_block("block.mn", block_file.path())
			.unwrap()
			.build()
			.unwrap();

		let json = serde_json::to_string_pretty(&manifest).unwrap();
		assert!(json.contains("chain_spec_state"));
		assert!(json.contains("sha256:"));

		// Verify it doesn't include None fields
		assert!(!json.contains("treasury_config"));
	}
}
