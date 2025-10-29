use crate::cfg::load_config;
use midnight_node_metadata::midnight_metadata_latest::{
	self as mn_meta, federated_authority_observation,
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
					mn_meta::Call::NativeTokenObservation(e) => if let native_token_observation::Call::process_tokens { utxos, .. } = e {
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
						}
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

pub async fn subscribe_to_federated_authority_events() -> Result<(), Box<dyn std::error::Error>> {
	println!("Subscribing to federated authority observation events");
	let url = load_config().node_url;
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&url).await?;

	let mut blocks_sub = api.blocks().subscribe_finalized().await?;

	use tokio::time::{timeout, Duration};
	let result = timeout(Duration::from_secs(120), async {
		while let Some(block) = blocks_sub.next().await {
			let block = block?;
			let block_number = block.header().number;
			println!("Checking block #{block_number} for federated authority events");

			let events = block.events().await?;

			// Check for CouncilMembersReset event
			let council_reset = events
				.find::<federated_authority_observation::events::CouncilMembersReset>()
				.flatten()
				.next();

			// Check for TechnicalCommitteeMembersReset event
			let tech_committee_reset = events
				.find::<federated_authority_observation::events::TechnicalCommitteeMembersReset>()
				.flatten()
				.next();

			if council_reset.is_some() || tech_committee_reset.is_some() {
				if let Some(event) = council_reset {
					println!(
						"✓ Found CouncilMembersReset event with {} members",
						event.members.0.len()
					);
				}
				if let Some(event) = tech_committee_reset {
					println!(
						"✓ Found TechnicalCommitteeMembersReset event with {} members",
						event.members.0.len()
					);
				}

				return Ok(());
			}
		}
		Err("Did not find federated authority events".into())
	})
	.await;

	match result {
		Ok(res) => res,
		Err(_) => Err("Timeout waiting for federated authority events".into()),
	}
}
