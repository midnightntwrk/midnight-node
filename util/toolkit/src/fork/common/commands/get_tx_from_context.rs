use super::ledger_helpers_local::{self, DefaultDB, deserialize};

type Signature = ledger_helpers_local::Signature;
type ProofMarker = ledger_helpers_local::ProofMarker;
type TransactionWithContext =
	ledger_helpers_local::TransactionWithContext<Signature, ProofMarker, DefaultDB>;

pub fn extract_tx_from_context(
	bytes: &[u8],
) -> Result<(Vec<u8>, u64), Box<dyn std::error::Error + Send + Sync>> {
	let deserialized_tx_with_context: TransactionWithContext = deserialize(bytes)?;

	let tx = deserialized_tx_with_context.tx;
	let serialized_tx = tx.serialize_inner()?;
	let timestamp = deserialized_tx_with_context.block_context.tblock.to_secs();

	Ok((serialized_tx, timestamp))
}
