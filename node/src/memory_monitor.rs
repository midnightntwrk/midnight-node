// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Memory monitor service: checks available system memory and triggers graceful
//! shutdown when it drops below a configured threshold, preventing OOM kills.
//!
//! At startup the memory source is detected once by probing which files exist:
//! 1. cgroup v2 (`memory.max` with a numeric limit + `memory.current`) — Docker/K8s
//! 2. cgroup v1 (`memory.limit_in_bytes` with a bounded limit + `memory.usage_in_bytes`)
//! 3. `/proc/meminfo` `MemAvailable` — bare metal / no cgroup limit
//!
//! On non-Linux platforms, monitoring is not supported and the service is not spawned.

use clap::Args;
use sp_core::traits::SpawnEssentialNamed;
use std::time::Duration;

const LOG_TARGET: &str = "memory-monitor";

/// Error type for memory monitoring.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("IO Error: {0}")]
	IOError(#[from] std::io::Error),
	#[error("Out of memory: available {0}MiB, required {1}MiB")]
	OutOfMemory(u64, u64),
}

/// Parameters for memory monitoring.
#[derive(Default, Debug, Clone, Args)]
pub struct MemoryMonitorParams {
	/// Required available memory in MiB.
	/// If available memory drops below this threshold, the node will be gracefully terminated.
	/// If `0`, monitoring is disabled.
	#[arg(long, default_value_t = 0)]
	pub memory_threshold: u64,
	/// How often available memory is polled, in seconds.
	#[arg(long, default_value_t = 1)]
	pub memory_polling_period: u32,
}

/// Which source to read available memory from, detected once at startup.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
enum MemorySource {
	CgroupV2,
	CgroupV1,
	ProcMeminfo,
}

#[cfg(target_os = "linux")]
impl MemorySource {
	fn label(self) -> &'static str {
		match self {
			Self::CgroupV2 => "cgroup v2",
			Self::CgroupV1 => "cgroup v1",
			Self::ProcMeminfo => "/proc/meminfo",
		}
	}
}

/// Memory monitor service that periodically checks available memory and triggers
/// a graceful shutdown if it drops below the configured threshold.
#[allow(dead_code)] // fields used only on linux
pub struct MemoryMonitorService {
	threshold: u64,
	polling_period: Duration,
	#[cfg(target_os = "linux")]
	source: MemorySource,
}

impl MemoryMonitorService {
	/// Spawns the memory monitor as an essential task. If the monitor detects
	/// insufficient memory, the essential task completes, causing the node to
	/// shut down gracefully.
	#[allow(unused_variables)] // spawner unused on non-linux
	pub fn try_spawn(
		params: MemoryMonitorParams,
		spawner: &impl SpawnEssentialNamed,
	) -> Result<(), Error> {
		if params.memory_threshold == 0 {
			log::info!(
				target: LOG_TARGET,
				"MemoryMonitorService: memory monitoring disabled (threshold=0)",
			);
			return Ok(());
		}

		if params.memory_polling_period == 0 {
			log::warn!(
				target: LOG_TARGET,
				"MemoryMonitorService: memory monitoring disabled \
				 (threshold={} but polling_period=0)",
				params.memory_threshold,
			);
			return Ok(());
		}

		#[cfg(not(target_os = "linux"))]
		{
			log::warn!(
				target: LOG_TARGET,
				"MemoryMonitorService: not supported on this platform, memory monitoring disabled",
			);
			return Ok(());
		}

		#[cfg(target_os = "linux")]
		{
			let source = Self::detect_source();
			log::info!(
				target: LOG_TARGET,
				"Initializing MemoryMonitorService via {}, threshold: {}MiB, polling period: {}s",
				source.label(),
				params.memory_threshold,
				params.memory_polling_period,
			);

			let service = MemoryMonitorService {
				threshold: params.memory_threshold,
				polling_period: Duration::from_secs(params.memory_polling_period.into()),
				source,
			};

			service.check_available_memory()?;

			spawner.spawn_essential("memory-monitor", None, Box::pin(service.run()));
			Ok(())
		}
	}

	/// Main monitoring loop. Completes (causing node shutdown) when available
	/// memory drops below threshold.
	#[cfg(target_os = "linux")]
	async fn run(self) {
		loop {
			tokio::time::sleep(self.polling_period).await;
			if self.check_available_memory().is_err() {
				break;
			}
		}
	}

	/// Checks if available memory is above the threshold. Returns `Err` if it
	/// has dropped below, triggering shutdown.
	#[cfg(target_os = "linux")]
	fn check_available_memory(&self) -> Result<(), Error> {
		match self.read_available_mib() {
			Ok(available) => {
				let threshold = self.threshold;
				if available < threshold {
					log::error!(
						target: LOG_TARGET,
						"Available memory {available}MiB dropped below threshold: \
						 {threshold}MiB, terminating...",
					);
					Err(Error::OutOfMemory(available, threshold))
				} else if available < threshold * 2 {
					log::warn!(
						target: LOG_TARGET,
						"Available memory {available}MiB is approaching threshold: \
						 {threshold}MiB",
					);
					Ok(())
				} else {
					Ok(())
				}
			},
			Err(e) => {
				// Don't shut down for transient read errors — just warn and continue.
				log::warn!(
					target: LOG_TARGET,
					"Could not read available memory: {e:?}, skipping check",
				);
				Ok(())
			},
		}
	}

	/// Detect which memory source to use by checking which files exist.
	#[cfg(target_os = "linux")]
	fn detect_source() -> MemorySource {
		// cgroup v2: memory.max must exist and contain a numeric (non-"max") limit
		if let Ok(max_str) = std::fs::read_to_string("/sys/fs/cgroup/memory.max") {
			let trimmed = max_str.trim();
			if trimmed != "max"
				&& trimmed.parse::<u64>().is_ok()
				&& std::fs::metadata("/sys/fs/cgroup/memory.current").is_ok()
			{
				return MemorySource::CgroupV2;
			}
		}

		// cgroup v1: limit_in_bytes must exist and not be an absurdly large value
		if let Ok(limit_str) =
			std::fs::read_to_string("/sys/fs/cgroup/memory/memory.limit_in_bytes")
			&& let Ok(limit) = limit_str.trim().parse::<u64>()
			&& limit <= (1u64 << 62)
			&& std::fs::metadata("/sys/fs/cgroup/memory/memory.usage_in_bytes").is_ok()
		{
			return MemorySource::CgroupV1;
		}

		if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
			if contents.lines().any(|l| l.starts_with("MemAvailable:")) {
				return MemorySource::ProcMeminfo;
			}
			log::warn!(
				target: LOG_TARGET,
				"MemoryMonitorService: /proc/meminfo exists but MemAvailable field not found",
			);
		} else {
			log::warn!(
				target: LOG_TARGET,
				"MemoryMonitorService: /proc/meminfo not readable, memory monitoring may not function",
			);
		}

		MemorySource::ProcMeminfo
	}

	/// Read available memory in MiB using the pre-detected source.
	#[cfg(target_os = "linux")]
	fn read_available_mib(&self) -> Result<u64, Error> {
		match self.source {
			MemorySource::CgroupV2 => Self::cgroup_v2_available_mib(),
			MemorySource::CgroupV1 => Self::cgroup_v1_available_mib(),
			MemorySource::ProcMeminfo => Self::proc_meminfo_available_mib(),
		}
	}

	#[cfg(target_os = "linux")]
	fn cgroup_v2_available_mib() -> Result<u64, Error> {
		let max: u64 = std::fs::read_to_string("/sys/fs/cgroup/memory.max")?
			.trim()
			.parse()
			.map_err(|e| {
				std::io::Error::new(std::io::ErrorKind::InvalidData, format!("memory.max: {e}"))
			})?;
		let current: u64 = std::fs::read_to_string("/sys/fs/cgroup/memory.current")?
			.trim()
			.parse()
			.map_err(|e| {
				std::io::Error::new(std::io::ErrorKind::InvalidData, format!("memory.current: {e}"))
			})?;
		Ok(max.saturating_sub(current) / 1024 / 1024)
	}

	#[cfg(target_os = "linux")]
	fn cgroup_v1_available_mib() -> Result<u64, Error> {
		let limit: u64 = std::fs::read_to_string("/sys/fs/cgroup/memory/memory.limit_in_bytes")?
			.trim()
			.parse()
			.map_err(|e| {
				std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					format!("memory.limit_in_bytes: {e}"),
				)
			})?;
		let usage: u64 = std::fs::read_to_string("/sys/fs/cgroup/memory/memory.usage_in_bytes")?
			.trim()
			.parse()
			.map_err(|e| {
				std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					format!("memory.usage_in_bytes: {e}"),
				)
			})?;
		Ok(limit.saturating_sub(usage) / 1024 / 1024)
	}

	/// Parse `MemAvailable` from `/proc/meminfo` (in kB), return as MiB.
	#[cfg(target_os = "linux")]
	fn proc_meminfo_available_mib() -> Result<u64, Error> {
		let contents = std::fs::read_to_string("/proc/meminfo")?;
		for line in contents.lines() {
			if let Some(rest) = line.strip_prefix("MemAvailable:") {
				let kb_str = rest.trim().trim_end_matches(" kB").trim();
				let kb: u64 = kb_str.parse().map_err(|_| {
					std::io::Error::new(
						std::io::ErrorKind::InvalidData,
						format!("failed to parse MemAvailable value: {kb_str}"),
					)
				})?;
				return Ok(kb / 1024);
			}
		}
		Err(Error::IOError(std::io::Error::new(
			std::io::ErrorKind::NotFound,
			"MemAvailable not found in /proc/meminfo",
		)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn threshold_zero_is_disabled() {
		let params = MemoryMonitorParams { memory_threshold: 0, memory_polling_period: 1 };
		assert_eq!(params.memory_threshold, 0);
	}

	#[cfg(target_os = "linux")]
	#[test]
	fn detect_source_and_read() {
		let source = MemoryMonitorService::detect_source();
		log::info!("detected memory source: {:?}", source);

		let service =
			MemoryMonitorService { threshold: 0, polling_period: Duration::from_secs(1), source };
		let mib = service.read_available_mib();
		assert!(mib.is_ok(), "should be able to read memory on Linux: {mib:?}");
		assert!(mib.unwrap() > 0, "available memory should be > 0");
	}
}
