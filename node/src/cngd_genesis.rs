use frame_support::inherent::ProvideInherent;
use midnight_node_res::native_token_observation_consts::{
	TEST_CNIGHT_ASSET_NAME, TEST_CNIGHT_CURRENCY_POLICY_ID, TEST_CNIGHT_MAPPING_VALIDATOR_ADDRESS,
	TEST_CNIGHT_REDEMPTION_VALIDATOR_ADDRESS,
};
use midnight_primitives_mainchain_follower::{
	MidnightNativeTokenObservationDataSource, MidnightObservationTokenMovement, ObservedUtxo,
	data_source::ObservedUtxos,
};
use midnight_primitives_native_token_observation::{
	CardanoPosition, INHERENT_IDENTIFIER, TimestampUnixMillis, TokenObservationConfig,
};
use pallet_native_token_observation::{MappingEntry, Mappings, mock};
use serde::{Deserialize, Serialize};
use sidechain_domain::McBlockHash;
use sp_inherents::InherentData;
use sp_runtime::traits::Dispatchable;
use std::{collections::HashMap, sync::Arc};

use serde_json;
use tokio::{fs::File, io::AsyncWriteExt};

const UTXO_CAPACITY: usize = 1000;

#[derive(Serialize, Deserialize)]
pub struct CNightGeneratesDustConfig {
	cardano_addresses: TokenObservationConfig,
	initial_utxos: ObservedUtxos,
	initial_mappings: HashMap<Vec<u8>, Vec<MappingEntry>>,
}

#[derive(Debug, thiserror::Error)]
pub enum CngdGenesisError {
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

pub fn get_mappings(utxos: &ObservedUtxos) -> HashMap<Vec<u8>, Vec<MappingEntry>> {
	mock::new_test_ext().execute_with(|| {
		let inherent_data = create_inherent(utxos.utxos.clone(), utxos.end);
		let call = mock::NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = mock::RuntimeCall::NativeTokenObservation(call);
		assert!(call.dispatch(frame_system::RawOrigin::None.into()).is_ok());

		Mappings::<mock::Test>::iter().map(|(k, v)| (k.into(), v)).collect()
	})
}

pub async fn get_cngd_genesis(
	native_token_observation_data_source: Arc<dyn MidnightNativeTokenObservationDataSource>,
	// Cardano block hash("mc hash") which is assumed to be the tip for the queries
	initial_cardano_tip_hash: McBlockHash,
) -> Result<(), CngdGenesisError> {
	let mut current_position = CardanoPosition {
		// Required to fulfill struct, but value will be unused
		block_hash: [0; 32],
		block_number: 0,
		block_timestamp: TimestampUnixMillis(0),
		tx_index_in_block: 0,
	};

	let mut all_utxos = Vec::new();
	let initial_cardano_tip_hash = initial_cardano_tip_hash.clone();

	let token_observation_config = TokenObservationConfig {
		mapping_validator_address: TEST_CNIGHT_MAPPING_VALIDATOR_ADDRESS.to_string(),
		redemption_validator_address: TEST_CNIGHT_REDEMPTION_VALIDATOR_ADDRESS.to_string(),
		policy_id: hex::encode(TEST_CNIGHT_CURRENCY_POLICY_ID),
		asset_name: TEST_CNIGHT_ASSET_NAME.to_string(),
	};

	// TODO: Ensure that cTime matches Cardano UTXO creation time, not the current pallet time
	loop {
		let observed = native_token_observation_data_source
			.get_utxos_up_to_capacity(
				&token_observation_config,
				current_position,
				initial_cardano_tip_hash.clone(),
				UTXO_CAPACITY,
			)
			.await
			.map_err(CngdGenesisError::UtxoQueryError)?;

		current_position = observed.end;
		log::info!(
			"Fetched {} cNight utxos. Current tip: {current_position:?}",
			observed.utxos.len(),
		);
		all_utxos.extend(observed.utxos);

		// Optional: break early if position is past the tip
		if current_position.block_hash == initial_cardano_tip_hash.0 {
			break;
		}
	}

	let initial_utxos = ObservedUtxos {
		start: CardanoPosition::default(),
		end: current_position,
		utxos: all_utxos,
	};

	let initial_mappings = get_mappings(&initial_utxos);

	let config = CNightGeneratesDustConfig {
		cardano_addresses: token_observation_config,
		initial_utxos,
		initial_mappings,
	};

	let json = serde_json::to_string_pretty(&config)?;
	let mut file = File::create("cngd-config.json").await?;
	file.write_all(json.as_bytes()).await?;
	log::info!("Wrote cNIGHT Generates Dust genesis to cngd-config.json");
	Ok(())
}
