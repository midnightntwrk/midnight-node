use std::io::Write as _;

use midnight_node_ledger_helpers::fork::raw_block_data::RawTransaction;

use midnight_node_ledger_helpers::fork::raw_block_data::{SerializedTx, SerializedTxBatches};

use super::ledger_helpers_local::{
	self, DefaultDB, PureGeneratorPedersen, SystemTransaction, deserialize,
};

type Signature = ledger_helpers_local::Signature;
type ProofMarker = ledger_helpers_local::ProofMarker;
type Transaction =
	ledger_helpers_local::Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;

pub fn show_transactions(
	built_txs: &SerializedTxBatches,
) -> Result<(String, usize), Box<dyn std::error::Error + Send + Sync>> {
	let mut displays = Vec::new();
	let mut total_size = 0;
	for tx in built_txs.batches.iter().flatten() {
		let (display, size) = show_transaction(tx)?;
		displays.push(display);
		total_size += size;
	}
	let display = displays.join("\n");
	Ok((display, total_size))
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
