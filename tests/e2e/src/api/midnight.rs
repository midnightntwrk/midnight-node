use crate::configuration::NodeClientSettings;
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation::storage::types::utxo_owners::UtxoOwners;
use midnight_node_metadata::midnight_metadata_latest::runtime_types::bounded_collections::bounded_vec::BoundedVec;
use midnight_node_metadata::midnight_metadata_latest::runtime_types::midnight_primitives_cnight_observation::ObservedUtxo;
use midnight_node_metadata::midnight_metadata_latest::{
	self as mn_meta,
	c_night_observation::{self},
};
use std::time::Duration;
use subxt::blocks::ExtrinsicEvents;
use subxt::{OnlineClient, SubstrateConfig};
use tokio::time::{sleep, timeout, Instant};

pub struct MidnightClient {
	pub online_client: OnlineClient<SubstrateConfig>,
}

impl MidnightClient {
	pub async fn new(node_settings: NodeClientSettings) -> Self {
		let online_client =
			OnlineClient::<SubstrateConfig>::from_url(node_settings.base_url).await.unwrap();
		Self { online_client }
	}

	pub fn new_dust_hex() -> String {
		let bytes: [u8; 32] = rand::random();
		hex::encode(bytes)
	}

	pub async fn subscribe_to_cnight_observation_events(
		&self,
		tx_id: &[u8],
	) -> Result<ExtrinsicEvents<SubstrateConfig>, Box<dyn std::error::Error>> {
		println!(
			"Subscribing for cNIGHT observation extrinsic with tx_id: 0x{}",
			hex::encode(tx_id)
		);
		let mut blocks_sub = self.online_client.blocks().subscribe_finalized().await?;

		let inner = async {
			while let Some(block_result) = blocks_sub.next().await {
				let block = block_result?;

				let block_number = block.header().number;
				println!("Finalized block #{}", block_number);

				let extrinsic = block.extrinsics().await?;

				for ext in extrinsic.iter() {
					let Ok(decoded) = ext.as_root_extrinsic::<mn_meta::Call>() else {
						continue;
					};

					let Some(utxos) = MidnightClient::extract_process_tokens_utxos(&decoded) else {
						continue;
					};

					println!(
						"  NativeTokenObservation::process_tokens called with {} UTXOs",
						utxos.len()
					);

					if utxos.is_empty() {
						continue;
					}

					if utxos.iter().any(|u| u.header.tx_hash.0 == tx_id) {
						println!(
							"*** Found UTXO with matching registration tx hash: 0x{} ***",
							hex::encode(tx_id)
						);
						let events = ext.events().await?;
						return Ok(events);
					} else {
						for u in utxos {
							let seen = u.header.tx_hash.0;
							println!(
								"Tx hash 0x{} does not match expected registration tx hash 0x{}",
								hex::encode(seen),
								hex::encode(tx_id)
							);
						}
					}
				}
			}
			Err("Did not find registration event".into())
		};

		timeout(Duration::from_secs(60), inner)
			.await
			.unwrap_or_else(|_| Err("Timeout waiting for registration event".into()))
	}

	pub fn calculate_nonce(prefix: &[u8], tx_hash: [u8; 32], tx_index: u16) -> String {
		let mut hasher = Blake2bVar::new(32).expect("valid output size");

		hasher.update(&prefix);
		hasher.update(&tx_hash);
		hasher.update(&tx_index.to_be_bytes());

		let mut out = [0u8; 32];
		hasher.finalize_variable(&mut out).expect("finalize succeeds");
		hex::encode(&out)
	}

	fn extract_process_tokens_utxos(call: &mn_meta::Call) -> Option<&Vec<ObservedUtxo>> {
		match call {
			mn_meta::Call::CNightObservation(c_night_observation::Call::process_tokens {
				utxos,
				..
			}) => Some(utxos),
			_ => None,
		}
	}

	pub async fn query_night_utxo_owners(
		&self,
		utxo: String,
	) -> Result<Option<UtxoOwners>, Box<dyn std::error::Error>> {
		let nonce = hex::decode(&utxo).unwrap();
		let storage_address =
			mn_meta::storage().c_night_observation().utxo_owners(BoundedVec(nonce));

		let owners =
			self.online_client.storage().at_latest().await?.fetch(&storage_address).await?;

		Ok(owners)
	}

	pub async fn poll_utxo_owners_until_change(
		&self,
		utxo: String,
		initial_value: Option<UtxoOwners>,
		timeout_secs: u64,
		poll_interval_ms: u64,
	) -> Result<Option<UtxoOwners>, Box<dyn std::error::Error>> {
		let start = Instant::now();
		loop {
			let current_value = self.query_night_utxo_owners(utxo.clone()).await?;
			if current_value != initial_value {
				println!("UtxoOwners storage changed: {:?}", current_value);
				return Ok(current_value);
			}
			if start.elapsed() > Duration::from_secs(timeout_secs) {
				println!("Timeout reached without change");
				return Ok(current_value);
			}
			sleep(Duration::from_millis(poll_interval_ms)).await;
		}
	}
}
