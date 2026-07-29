// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

//! In-process acropolis Cardano indexer.
//!
//! Runs the same Caryatid module fleet as acropolis's standalone
//! `midnight_indexer` process — Mithril/peer ingest through `midnight_state`
//! — on the node's tokio runtime, replacing the external
//! cardano-node + db-sync + postgres stack with in-process state.
//!
//! The data sources call the indexer's query service directly (see
//! `IndexerHandle::Direct`) — no gRPC transport, no codec, no local port.
//! Its TCP gRPC server stays available for external clients but the shipped
//! configs disable it (`grpc-bind-address = "disabled"`).
//!
//! The query service is available as soon as this returns; the *state*
//! behind it back-fills from Mithril + peer sync, and queries for
//! not-yet-indexed data fail cleanly until it catches up.

use std::sync::Arc;
use std::time::Duration;

use acropolis_common::messages::Message;
use caryatid_process::Process;
use config::{Config, Environment, File};

use acropolis_module_midnight_state::grpc::service::MidnightStateService;
use acropolis_module_midnight_state::{MidnightState, embedded_service};

use acropolis_module_genesis_bootstrapper::GenesisBootstrapper;
use acropolis_module_snapshot_bootstrapper::SnapshotBootstrapper;

use acropolis_module_mithril_snapshot_fetcher::MithrilSnapshotFetcher;
use acropolis_module_peer_network_interface::PeerNetworkInterface;

use acropolis_module_block_unpacker::BlockUnpacker;
use acropolis_module_tx_unpacker::TxUnpacker;

use acropolis_module_accounts_state::AccountsState;
use acropolis_module_drep_state::DRepState;
use acropolis_module_epochs_state::EpochsState;
use acropolis_module_governance_state::GovernanceState;
use acropolis_module_parameters_state::ParametersState;
use acropolis_module_spo_state::SPOState;
use acropolis_module_stake_delta_filter::StakeDeltaFilter;
use acropolis_module_utxo_state::UTXOState;

use acropolis_module_block_kes_validator::BlockKesValidator;
use acropolis_module_block_vrf_validator::BlockVrfValidator;
use acropolis_module_consensus::Consensus;

use acropolis_module_chain_store::ChainStore;
use acropolis_module_spdd_state::SPDDState;

use caryatid_module_clock::Clock;
use caryatid_module_spy::Spy;

/// How long to wait for the indexer's query service to be published before
/// giving up. It appears during module init, well before any chain sync, so
/// this only guards against outright startup failure.
const SERVICE_READY_TIMEOUT: Duration = Duration::from_secs(30);
const SERVICE_READY_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// A running in-process indexer.
pub struct EmbeddedIndexer {
	/// Query service of the indexer, called directly by the data sources.
	pub service: MidnightStateService,
}

/// Spawn the acropolis midnight-indexer module fleet on the current tokio
/// runtime, configured from the TOML at `config_path` (same format as
/// acropolis's `processes/midnight_indexer` configs; `ACROPOLIS__`-prefixed
/// environment variables override). Returns once the indexer's gRPC endpoint
/// accepts connections.
pub async fn spawn_embedded_indexer(
	config_path: &str,
) -> Result<EmbeddedIndexer, Box<dyn std::error::Error + Send + Sync>> {
	let config = Config::builder()
		.add_source(File::with_name(config_path))
		.add_source(Environment::with_prefix("ACROPOLIS"))
		.build()
		.map_err(|e| format!("failed to load acropolis config `{config_path}`: {e}"))?;

	let mut process = Process::<Message>::create(Arc::new(config)).await;

	// Same fleet as acropolis processes/midnight_indexer/src/main.rs — the
	// `[module.*]` sections in the config decide which are active.
	GenesisBootstrapper::register(&mut process);
	SnapshotBootstrapper::register(&mut process);
	MithrilSnapshotFetcher::register(&mut process);
	BlockUnpacker::register(&mut process);
	PeerNetworkInterface::register(&mut process);
	TxUnpacker::register(&mut process);
	UTXOState::register(&mut process);
	SPOState::register(&mut process);
	DRepState::register(&mut process);
	GovernanceState::register(&mut process);
	ParametersState::register(&mut process);
	StakeDeltaFilter::register(&mut process);
	EpochsState::register(&mut process);
	AccountsState::register(&mut process);
	SPDDState::register(&mut process);
	Consensus::register(&mut process);
	ChainStore::register(&mut process);
	BlockVrfValidator::register(&mut process);
	BlockKesValidator::register(&mut process);
	MidnightState::register(&mut process);
	Clock::<Message>::register(&mut process);
	Spy::<Message>::register(&mut process);

	tokio::spawn(async move {
		match process.run().await {
			Ok(()) => log::warn!("embedded acropolis indexer exited"),
			Err(e) => log::error!("embedded acropolis indexer failed: {e:#}"),
		}
	});

	let service = wait_for_service_ready().await?;
	log::info!("embedded acropolis indexer up, query service published");

	Ok(EmbeddedIndexer { service })
}

async fn wait_for_service_ready()
-> Result<MidnightStateService, Box<dyn std::error::Error + Send + Sync>> {
	let deadline = tokio::time::Instant::now() + SERVICE_READY_TIMEOUT;
	while tokio::time::Instant::now() < deadline {
		if let Some(service) = embedded_service() {
			return Ok(service);
		}
		tokio::time::sleep(SERVICE_READY_POLL_INTERVAL).await;
	}
	Err(format!(
		"embedded indexer query service not published within {SERVICE_READY_TIMEOUT:?}; \
		 is [module.midnight-state] present in the acropolis config?"
	)
	.into())
}

#[cfg(test)]
mod tests {
	// This workspace force-enables serde_json's `arbitrary_precision`
	// (sp-genesis-builder, ogmios-client, cardano-serialization-lib), which
	// breaks derived `#[serde(untagged)]` number matching. The indexer's
	// genesis parsing must survive that, or the whole module fleet stalls
	// at bootstrap. These reproduce the failure the derived impls had.
	#[test]
	fn chameleon_fraction_parses_under_arbitrary_precision() {
		use acropolis_common::rational_number::ChameleonFraction;
		let f: ChameleonFraction = serde_json::from_str("0.05").expect("float form");
		assert!(matches!(f, ChameleonFraction::Float(v) if (v - 0.05).abs() < 1e-6));
		let f: ChameleonFraction =
			serde_json::from_str(r#"{"numerator": 1, "denominator": 20}"#).expect("fraction form");
		assert!(matches!(f, ChameleonFraction::Fraction { numerator: 1, denominator: 20 }));
	}
}
