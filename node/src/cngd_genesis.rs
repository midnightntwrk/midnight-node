use midnight_node_res::native_token_observation_consts::{
	TEST_CNIGHT_ASSET_NAME, TEST_CNIGHT_CURRENCY_POLICY_ID, TEST_CNIGHT_MAPPING_VALIDATOR_ADDRESS,
	TEST_CNIGHT_REDEMPTION_VALIDATOR_ADDRESS,
};
use midnight_primitives_mainchain_follower::MidnightNativeTokenObservationDataSource;
use midnight_primitives_native_token_observation::{CardanoPosition, TokenObservationConfig};
use sidechain_domain::McBlockHash;
use std::sync::Arc;

use serde_json;
use tokio::{fs::File, io::AsyncWriteExt};

const UTXO_CAPACITY: usize = 100_000;

#[derive(Debug, thiserror::Error)]
pub enum CngdGenesisError {
	#[error("Failed to query UTXOs: {0}")]
	UtxoQueryError(Box<dyn std::error::Error + Send + Sync>),

	#[error("Failed to serialize UTXOs to JSON: {0}")]
	SerdeError(#[from] serde_json::Error),

	#[error("I/O error: {0}")]
	IoError(#[from] std::io::Error),
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

		// TODO: Check for inclusivity
		current_position = observed.end;
		all_utxos.extend(observed.utxos);

		// Optional: break early if position is past the tip
		if current_position.block_hash == initial_cardano_tip_hash.0 {
			break;
		}
	}

	let json = serde_json::to_string_pretty(&all_utxos)?;
	let mut file = File::create("observed_utxos.json").await?;
	file.write_all(json.as_bytes()).await?;
	println!("Wrote cNIGHT Generates Dust genesis to observed_utxos.json");
	Ok(())
}
