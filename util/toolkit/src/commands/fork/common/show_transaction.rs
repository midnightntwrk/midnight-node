use std::io::Write as _;

use midnight_node_ledger_helpers::fork::raw_block_data::RawTransaction;

use midnight_node_ledger_helpers::fork::raw_block_data::{SerializedTx, SerializedTxBatches};
use serde::{Deserialize, Serialize};

use super::ledger_helpers_local::{
	self, DefaultDB, PureGeneratorPedersen, SystemTransaction, deserialize,
};

type Signature = ledger_helpers_local::Signature;
type ProofMarker = ledger_helpers_local::ProofMarker;
type Transaction =
	ledger_helpers_local::Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;

#[derive(Debug, Serialize, Deserialize)]
pub struct ShowTransaction {
	pub tx_type: String,
	pub size_bytes: usize,
	#[serde(with = "hex")]
	pub hash: [u8; 32],
	pub debug_str: String,
}

impl TryFrom<&RawTransaction> for ShowTransaction {
	type Error = std::io::Error;

	fn try_from(value: &RawTransaction) -> Result<Self, Self::Error> {
		let size_bytes = value.as_bytes().len();
		match value {
			RawTransaction::Midnight(tx_bytes) => {
				let tx: Transaction = deserialize(tx_bytes.as_slice())?;
				let hash = tx.transaction_hash().0.0;
				Ok(ShowTransaction {
					tx_type: "Midnight".to_string(),
					size_bytes,
					hash,
					debug_str: format!("{tx:#?}"),
				})
			},
			RawTransaction::System(tx_bytes) => {
				let tx: SystemTransaction = deserialize(tx_bytes.as_slice())?;
				let hash = tx.transaction_hash().0.0;
				Ok(ShowTransaction {
					tx_type: "Midnight".to_string(),
					size_bytes,
					hash,
					debug_str: format!("{tx:#?}"),
				})
			},
		}
	}
}

pub fn show_transactions(
	built_txs: &SerializedTxBatches,
) -> Result<Vec<ShowTransaction>, std::io::Error> {
	built_txs
		.batches
		.iter()
		.flatten()
		.map(|tx| ShowTransaction::try_from(&tx.tx))
		.collect()
}

pub fn show_transaction(
	serialized_tx: &SerializedTx,
) -> Result<(String, usize), Box<dyn std::error::Error + Send + Sync>> {
	let mut out_str = Vec::new();

	writeln!(&mut out_str, "{{")?;
	writeln!(&mut out_str, "hash: {}", hex::encode(serialized_tx.tx_hash))?;
	writeln!(&mut out_str, "context: {:#?}", serialized_tx.context)?;
	match &serialized_tx.tx {
		RawTransaction::Midnight(tx) => {
			let tx: Transaction = deserialize(tx.as_slice())?;
			writeln!(&mut out_str, "{tx:#?}")?;
		},
		RawTransaction::System(tx) => {
			let tx: SystemTransaction = deserialize(tx.as_slice())?;
			writeln!(&mut out_str, "{tx:#?}")?;
		},
	}

	writeln!(&mut out_str, "}}")?;
	let size = serialized_tx.tx_byte_len();
	Ok((String::from_utf8_lossy(&out_str).to_string(), size))
}
