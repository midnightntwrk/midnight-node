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
	let api = OnlineClient::<SubstrateConfig>::from_url("ws://127.0.0.1:9933").await?;

	let mut blocks_sub = api.blocks().subscribe_finalized().await?;

	// TODO: add timeout
	// e.g. after 30 seconds, return Err("Timeout waiting for registration event".into())
	// or use tokio::time::timeout
	// let timeout = tokio::time::sleep(Duration::from_secs(30));
	// tokio::select! {
	// 	_ = timeout => {
	// 		return Err("Timeout waiting for registration event".into());
	// 	}
	// 	_ = async {
	// 		while let Some(block) = blocks_sub.next().await {
	// 			// ...
	// 		}
	// 	} => {}
	// }
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
}
