use rand::RngCore;

use midnight_node_metadata::midnight_metadata::{
	self as mn_meta, council, native_token_observation::{self, calls::types::process_tokens},
	runtime_types::{
		bounded_collections::bounded_vec::BoundedVec, pallet_native_token_observation,
		sidechain_domain::byte_string,
	},
};
use subxt::{blocks::ExtrinsicEvents, events::EventDetails, ext::scale_encode::EncodeAsType, OnlineClient, SubstrateConfig};

pub fn new_dust_hex(bytes: usize) -> String {
	let mut a = vec![0u8; bytes];
	rand::rng().fill_bytes(&mut a);
	a.iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

pub async fn subscribe_to_cngd_registration_event(
	tx_id: &[u8],
) -> Result<ExtrinsicEvents<SubstrateConfig>, Box<dyn std::error::Error>> {
	// Create a client to use:
	let api = OnlineClient::<SubstrateConfig>::from_url("ws://127.0.0.1:9933").await?;

	// Subscribe to all finalized blocks:
	let mut blocks_sub = api.blocks().subscribe_finalized().await?;

	// For each block, print a bunch of information about it:
	while let Some(block) = blocks_sub.next().await {
		let block = block?;

		let block_number = block.header().number;
		let block_hash = block.hash();

		println!("Block #{block_number}:");
		println!("  Hash: {block_hash}");
		println!("  Extrinsics:");

		// Log each of the extrinsic with it's associated events:
		let extrinsics = block.extrinsics().await?;
		for ext in extrinsics.iter() {
			let idx = ext.index();
			let events = ext.events().await?;
			let bytes_hex = format!("0x{}", hex::encode(ext.bytes()));

			// See the API docs for more ways to decode extrinsics:
			let decoded_ext = ext.as_root_extrinsic::<mn_meta::Call>();
			let runtime_event = decoded_ext.unwrap();
			match &runtime_event {
				mn_meta::Call::NativeTokenObservation(e) => {
					println!("  NativeTokenObservation{:?}", e);
					match e {
						native_token_observation::Call::process_tokens { utxos, .. } => {
							println!("    process_tokens {:?}", utxos);
							if utxos.is_empty() {
								println!("      No UTXOs found");
								continue;
							} else {
								for utxo in utxos {
									let utxo_tx_id = utxo.header.tx_hash.0;
									println!("      UTXO tx_id: 0x{}", hex::encode(utxo_tx_id.clone()));
									if utxo_tx_id == tx_id {
										println!("*** Found UTXO with matching tx_id: 0x{} ***", hex::encode(tx_id));
										return Ok(events);
										// return Ok(events
										// 	.iter()
										// 	.filter_map(|e| e.ok())
										// 	.find_map(|evt| {
										// 		evt.as_event::<native_token_observation::events::Registration>().ok().flatten()
										// 	})
										// 	.map(|registration| {
										// 		let registration_cardano_address = registration.0.cardano_address;
										// 		let registration_dust_address = registration.0.dust_address;
										// 		println!("          Cardano Address: 0x{}", hex::encode(registration_cardano_address.0.clone()));
										// 		println!("          DUST Address: 0x{}", hex::encode(registration_dust_address.clone()));
										// 	}).expect("Failed to extract registration details"));
									}
								}
							}
						}
						_ => {}
					}
					// if let native_token_observation::Call::process_tokens { utxos, next_cardano_position } = e {
					// 	println!("    process_tokens: utxos: {:?}, next_cardano_position: {:?}", utxos, next_cardano_position);
					// }
				},
				_ => {
					continue;
				},
			}

			// println!("    Extrinsic #{idx}:");
			// println!("      Bytes: {bytes_hex}");
			// println!("      Decoded: {decoded_ext:?}");

			println!("      Events:");
			for evt in events.iter() {
				let evt = evt?;
				let pallet_name = evt.pallet_name();
				let event_name = evt.variant_name();
				let event_values = evt.field_values()?;
				// if let Some(msg) = decode_native_token_event(&evt) {
				// 	println!("        NativeTokenObservation::{}", msg);
				// }
				// let Some(registration) = evt.as_event::<native_token_observation::events::Registration>().unwrap() else {
				// 	continue;
				// };
				if let Ok(Some(registration)) =
					evt.as_event::<native_token_observation::events::Registration>()
				{
					println!("        NativeTokenObservation::Registration: {:?}", registration);
					let registration_cardano_address = registration.0.cardano_address;
					let registration_dust_address = registration.0.dust_address;
					println!("          Cardano Address: 0x{}", hex::encode(registration_cardano_address.0.clone()));
					println!("          DUST Address: 0x{}", hex::encode(registration_dust_address.clone()));
					// Check if this is the registration we are looking for
					// if registration_cardano_address.0 == cardano_address.clone() {
					// 	println!("*** Found Registration event for address: 0x{} with DUST address 0x{} ***", hex::encode(cardano_address), hex::encode(registration_dust_address));
					// 	return Ok(());
					// }
				}
			}
		}
	}
	Err("Did not find registration event".into())

	// if let Ok(Some(pallet_native_token_observation::pallet::Registration { cardano_address: addr, .. })) = evt.as_event::<native_token_observation::events::Registration>() {
	// 	if addr.0 == byte_string(cardano_address.to_vec()) {
	// 		println!("*** Found Registration event for address: 0x{} ***", hex::encode(cardano_address));
	// 		return Ok(());
	// 	}
	// }

	// println!("        {pallet_name}_{event_name}");
	// println!("          {event_values}");
}

fn decode_native_token_event(evt: &EventDetails<SubstrateConfig>) -> Option<String> {
	if let Ok(Some(e)) = evt.as_event::<native_token_observation::events::Registration>() {
		return Some(format!("Registration: {:?}", e));
	}
	if let Ok(Some(e)) = evt.as_event::<native_token_observation::events::MappingAdded>() {
		return Some(format!("MappingAdded: {:?}", e));
	}
	// Add more event types as needed
	None
}
