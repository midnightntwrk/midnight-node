use midnight_primitives_cnight_observation::{CNightAddresses, ObservedUtxos};
use std::collections::HashMap;

use crate::MappingEntry;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CNightGenesis {
	pub cardano_addresses: CNightAddresses,
	pub initial_utxos: ObservedUtxos,
	pub initial_mappings: HashMap<Vec<u8>, Vec<MappingEntry>>,
	#[serde(with = "hex")]
	pub system_tx: Vec<u8>,
}
