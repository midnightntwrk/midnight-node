use crate::cfg::load_config;
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use midnight_node_metadata::midnight_metadata::{
	self as mn_meta,
	native_token_observation::{self},
};
use rand::RngCore;
use subxt::{blocks::ExtrinsicEvents, OnlineClient, SubstrateConfig};

pub fn new_dust_hex(bytes: usize) -> String {
	let mut a = vec![0u8; bytes];
	rand::rng().fill_bytes(&mut a);
	a.iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

pub async fn subscribe_to_cngd_registration_extrinsic(
	tx_id: &[u8],
) -> Result<ExtrinsicEvents<SubstrateConfig>, Box<dyn std::error::Error>> {
	println!("Subscribing for registration extrinsic with tx_id: 0x{}", hex::encode(tx_id));
	let url = load_config().node_url;
	let api = OnlineClient::<SubstrateConfig>::from_url(&url).await?;

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
					mn_meta::Call::NativeTokenObservation(e) => match e {
						native_token_observation::Call::process_tokens { utxos, .. } => {
							println!(
								"  NativeTokenObservation::process_tokens called with {} UTXOs",
								utxos.len()
							);
							if utxos.is_empty() {
								continue;
							} else {
								for utxo in utxos {
									let utxo_tx_id = utxo.header.tx_hash.0;
									if utxo_tx_id == tx_id {
										println!("*** Found UTXO with matching registration tx hash: 0x{} ***", hex::encode(tx_id));
										return Ok(events);
									} else {
										println!("Tx hash 0x{} does not match expected registration tx hash 0x{}", hex::encode(utxo_tx_id), hex::encode(tx_id));
									}
								}
							}
						},
						_ => {},
					},
					_ => {
						continue;
					},
				}
			}
		}
		Err("Did not find registration event".into())
	}).await;

	match result {
		Ok(res) => res,
		Err(_) => Err("Timeout waiting for registration event".into()),
	}
}

pub fn calculate_nonce(prefix: &[u8], tx_hash: [u8; 32], tx_index: u16) -> String {
	let tx_index_bytes = tx_index.to_be_bytes();
	let mut data = Vec::new();
	data.extend_from_slice(&prefix);
	data.extend_from_slice(&tx_hash);
	data.extend_from_slice(&tx_index_bytes);

	let mut hasher = Blake2bVar::new(32).unwrap();
	hasher.update(&data);
	let nonce = hasher.finalize_boxed();
	hex::encode(&nonce)
}
