use midnight_primitives_cnight_observation::{CNightAddresses, ObservedUtxos};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::MappingEntry;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CNightGenesis {
	pub addresses: CNightAddresses,
	pub initial_utxos: ObservedUtxos,
	pub initial_mappings: HashMap<Vec<u8>, Vec<MappingEntry>>,
	pub system_tx: Option<SystemTx>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SystemTx(#[serde(with = "hex")] pub Vec<u8>);
