use crate::cfg::load_config;
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use midnight_node_metadata::midnight_metadata_latest::{
	self as mn_meta,
	c_night_observation::{self},
};
use rand::RngCore;
use subxt::{blocks::ExtrinsicEvents, OnlineClient, SubstrateConfig};
use tokio::time::{sleep, Duration, Instant};

pub fn new_dust_hex(bytes: usize) -> String {
	let mut a = vec![0u8; bytes];
	rand::rng().fill_bytes(&mut a);
	a.iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

pub async fn subscribe_to_cnight_observation_events(
	tx_id: &[u8],
) -> Result<ExtrinsicEvents<SubstrateConfig>, Box<dyn std::error::Error>> {
	println!("Subscribing for cNIGHT observation extrinsic with tx_id: 0x{}", hex::encode(tx_id));
	let url = load_config().node_url;
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&url).await?;

	let mut blocks_sub = api.blocks().subscribe_finalized().await?;

	use tokio::time::{timeout, Duration};
	let result = timeout(Duration::from_secs(60), async {
		while let Some(block) = blocks_sub.next().await {
			let block = block?;
			let block_number = block.header().number;
			println!("Block #{block_number}:");

			let extrinsics = block.extrinsics().await?;
			for ext in extrinsics.iter() {
				let events = ext.events().await?;

				let decoded_ext = ext.as_root_extrinsic::<mn_meta::Call>();
				let runtime_call = decoded_ext.unwrap();
				match &runtime_call {
					mn_meta::Call::CNightObservation(e) => {
						if let c_night_observation::Call::process_tokens { utxos, .. } = e {
							println!(
								"  CNightObservation::process_tokens called with {} UTXOs",
								utxos.len()
							);
							if utxos.is_empty() {
								continue;
							} else {
								for utxo in utxos {
									let utxo_tx_id = utxo.header.tx_hash.0;
									if utxo_tx_id == tx_id {
										println!(
											"*** Found UTXO with matching tx hash: 0x{} ***",
											hex::encode(tx_id)
										);
										return Ok(events);
									} else {
										println!(
											"Tx hash 0x{} does not match expected tx hash 0x{}",
											hex::encode(utxo_tx_id),
											hex::encode(tx_id)
										);
									}
								}
							}
						}
					},
					_ => {
						continue;
					},
				}
			}
		}
		Err("Did not find cNIGHT observation event".into())
	})
	.await;

	match result {
		Ok(res) => res,
		Err(_) => Err("Timeout waiting for cNIGHT observation event".into()),
	}
}

pub fn calculate_nonce(prefix: &[u8], tx_hash: [u8; 32], tx_index: u16) -> String {
	let tx_index_bytes = tx_index.to_be_bytes();
	let mut data = Vec::new();
	data.extend_from_slice(prefix);
	data.extend_from_slice(&tx_hash);
	data.extend_from_slice(&tx_index_bytes);

	let mut hasher = Blake2bVar::new(32).unwrap();
	hasher.update(&data);
	let nonce = hasher.finalize_boxed();
	hex::encode(&nonce)
}

pub async fn query_night_utxo_owners(
	utxo: String,
) -> Result<
	Option<mn_meta::c_night_observation::storage::types::utxo_owners::UtxoOwners>,
	Box<dyn std::error::Error>,
> {
	let url = load_config().node_url;
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&url).await?;
	let nonce = hex::decode(&utxo).unwrap();
	let storage_address = mn_meta::storage()
		.c_night_observation()
		.utxo_owners(mn_meta::runtime_types::bounded_collections::bounded_vec::BoundedVec(nonce));

	let owners = api.storage().at_latest().await?.fetch(&storage_address).await?;

	Ok(owners)
}

pub async fn poll_utxo_owners_until_change(
	utxo: String,
	initial_value: Option<mn_meta::c_night_observation::storage::types::utxo_owners::UtxoOwners>,
	timeout_secs: u64,
	poll_interval_ms: u64,
) -> Result<
	Option<mn_meta::c_night_observation::storage::types::utxo_owners::UtxoOwners>,
	Box<dyn std::error::Error>,
> {
	let start = Instant::now();
	loop {
		let current_value = query_night_utxo_owners(utxo.clone()).await?;
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
