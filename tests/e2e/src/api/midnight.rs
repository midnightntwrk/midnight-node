use rand::RngCore;

use midnight_node_metadata::midnight_metadata as mn_meta;
use subxt::{OnlineClient, SubstrateConfig};

pub fn new_dust_hex(bytes: usize) -> String {
	let mut a = vec![0u8; bytes];
	rand::rng().fill_bytes(&mut a);
	a.iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

pub async fn subscribe_to_blocks() -> Result<(), Box<dyn std::error::Error>> {
	// Create a client to use:
	let api = OnlineClient::<SubstrateConfig>::new().await?;

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

			println!("    Extrinsic #{idx}:");
			println!("      Bytes: {bytes_hex}");
			println!("      Decoded: {decoded_ext:?}");

			println!("      Events:");
			for evt in events.iter() {
				let evt = evt?;
				let pallet_name = evt.pallet_name();
				let event_name = evt.variant_name();
				let event_values = evt.field_values()?;

				println!("        {pallet_name}_{event_name}");
				println!("          {event_values}");
			}

			println!("      Transaction Extensions:");
			if let Some(transaction_extensions) = ext.transaction_extensions() {
				for transaction_extension in transaction_extensions.iter() {
					let name = transaction_extension.name();
					let value = transaction_extension.value()?.to_string();
					println!("        {name}: {value}");
				}
			}
		}
	}

	Ok(())
}
