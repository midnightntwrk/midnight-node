use frame_support::inherent::ProvideInherent;
use midnight_primitives_cnight_observation::{
	CNightAddresses, CardanoPosition, INHERENT_IDENTIFIER, ObservedUtxos, TimestampUnixMillis,
};
use midnight_primitives_mainchain_follower::{
	MidnightCNightObservationDataSource, MidnightObservationTokenMovement, ObservedUtxo,
};
use pallet_cnight_observation::{
	MappingEntry, Mappings, NextCardanoPosition, UtxoOwners,
	config::{CNightGenesis, SystemTx},
};
use pallet_cnight_observation_mock::mock_with_capture as mock;
use sidechain_domain::McBlockHash;
use sp_inherents::InherentData;
use sp_runtime::traits::Dispatchable;
use sp_std::collections::btree_map::BTreeMap;
use std::{path::Path, sync::Arc};

use serde_json;
use tokio::{fs::File, io::AsyncWriteExt};

const UTXO_CAPACITY: usize = 1000;

#[derive(Debug, thiserror::Error)]
pub enum CNightGenesisError {
	#[error("Failed to query UTXOs: {0}")]
	UtxoQueryError(Box<dyn std::error::Error + Send + Sync>),

	#[error("Failed to serialize UTXOs to JSON: {0}")]
	SerdeError(#[from] serde_json::Error),

	#[error("I/O error: {0}")]
	IoError(#[from] std::io::Error),
}

fn create_inherent(
	utxos: Vec<ObservedUtxo>,
	next_cardano_position: CardanoPosition,
) -> InherentData {
	let mut inherent_data = InherentData::new();
	inherent_data
		.put_data(
			INHERENT_IDENTIFIER,
			&MidnightObservationTokenMovement { utxos, next_cardano_position },
		)
		.expect("inherent data insertion should not fail");
	inherent_data
}

struct PalletExecResult {
	mappings: BTreeMap<Vec<u8>, Vec<MappingEntry>>,
	utxo_owners: BTreeMap<Vec<u8>, Vec<u8>>,
	next_cardano_position: CardanoPosition,
	system_tx: Option<Vec<u8>>,
}

fn exec_pallet(utxos: &ObservedUtxos) -> PalletExecResult {
	mock::new_test_ext().execute_with(|| {
		let inherent_data = create_inherent(utxos.utxos.clone(), utxos.end);
		let call = mock::CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = mock::RuntimeCall::CNightObservation(call);
		assert!(call.dispatch(frame_system::RawOrigin::None.into()).is_ok());

		PalletExecResult {
			mappings: Mappings::<mock::Test>::iter().map(|(k, v)| (k.into(), v)).collect(),
			utxo_owners: UtxoOwners::<mock::Test>::iter().map(|(k, v)| (k.into(), v)).collect(),
			next_cardano_position: NextCardanoPosition::<mock::Test>::get(),
			system_tx: mock::MidnightSystemTx::pop_captured_system_txs().pop(),
		}
	})
}

pub async fn generate_cnight_genesis(
	addresses: CNightAddresses,
	cnight_observation_data_source: Arc<dyn MidnightCNightObservationDataSource>,
	// Cardano block hash("mc hash") which is assumed to be the tip for the queries
	cardano_tip: McBlockHash,
	output_path: impl AsRef<Path>,
) -> Result<(), CNightGenesisError> {
	let mut current_position = CardanoPosition {
		// Required to fulfill struct, but value will be unused
		block_hash: [0; 32],
		block_number: 0,
		block_timestamp: TimestampUnixMillis(0),
		tx_index_in_block: 0,
	};

	let mut all_utxos = Vec::new();

	loop {
		let observed = cnight_observation_data_source
			.get_utxos_up_to_capacity(
				&addresses,
				current_position,
				cardano_tip.clone(),
				UTXO_CAPACITY,
			)
			.await
			.map_err(CNightGenesisError::UtxoQueryError)?;

		current_position = observed.end;
		log::info!(
			"Fetched {} cNight utxos. Current tip: {current_position:?}",
			observed.utxos.len(),
		);
		all_utxos.extend(observed.utxos);

		// Optional: break early if position is past the tip
		if current_position.block_hash == cardano_tip.0 {
			break;
		}
	}

	let observed_utxos = ObservedUtxos {
		start: CardanoPosition::default(),
		end: current_position,
		utxos: all_utxos,
	};

	let PalletExecResult { mappings, utxo_owners, next_cardano_position, system_tx } =
		exec_pallet(&observed_utxos);

	let config = CNightGenesis {
		addresses,
		observed_utxos,
		mappings,
		utxo_owners,
		next_cardano_position,
		system_tx: system_tx.map(SystemTx),
	};

	let json = serde_json::to_string_pretty(&config)?;
	let mut file = File::create(output_path.as_ref()).await?;
	file.write_all(json.as_bytes()).await?;
	log::info!("Wrote cNIGHT Generates Dust genesis to {}", output_path.as_ref().display());
	Ok(())
}
