use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use crate::MappingEntry;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CNightGenesis {
	pub addresses: CNightAddresses,
	pub observed_utxos: ObservedUtxos,
	pub mappings: BTreeMap<Vec<u8>, Vec<MappingEntry>>,
	pub utxo_owners: BTreeMap<Vec<u8>, Vec<u8>>,
	pub next_cardano_position: CardanoPosition,
	pub system_tx: Option<SystemTx>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SystemTx(#[serde(with = "hex")] pub Vec<u8>);
