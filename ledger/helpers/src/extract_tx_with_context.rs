#[cfg(feature = "can-panic")]
use crate::fork::raw_block_data::{RawTransaction, SerializedTx};

#[cfg(feature = "can-panic")]
pub fn extract_tx_with_context_ledger_hf(
	bytes: &[u8],
) -> (Vec<u8>, crate::hard_fork_test::BlockContext) {
	use base_crypto_hf::{hash::HashOutput, time::Timestamp};

	let serialized_tx: SerializedTx =
		serde_json::from_slice(bytes).expect("failed to deserialize as SerializedTx");
	let RawTransaction::Midnight(tx_bytes) = serialized_tx.tx else {
		panic!("expected test to run against midnight transaction");
	};
	let block_context = crate::hard_fork_test::BlockContext {
		tblock: Timestamp::from_secs(serialized_tx.context.tblock.to_secs()),
		tblock_err: serialized_tx.context.tblock_err,
		parent_block_hash: HashOutput(serialized_tx.context.parent_block_hash.0),
		last_block_time: Timestamp::from_secs(serialized_tx.context.last_block_time.to_secs()),
	};

	(tx_bytes, block_context)
}

#[cfg(feature = "can-panic")]
pub fn extract_tx_with_context_ledger_8(bytes: &[u8]) -> (Vec<u8>, crate::ledger_8::BlockContext) {
	let serialized_tx: SerializedTx =
		serde_json::from_slice(bytes).expect("failed to deserialize as SerializedTx");
	let RawTransaction::Midnight(tx_bytes) = serialized_tx.tx else {
		panic!("expected test to run against midnight transaction");
	};
	let block_context = serialized_tx.context;

	(tx_bytes, block_context)
}

#[cfg(feature = "can-panic")]
pub fn extract_tx_with_context_ledger_7(bytes: &[u8]) -> (Vec<u8>, crate::ledger_7::BlockContext) {
	use crate::fork::raw_block_data::RawTransaction;
	use crate::ledger_7::base_crypto::{hash::HashOutput, time::Timestamp};

	let serialized_tx: SerializedTx =
		serde_json::from_slice(bytes).expect("failed to deserialize as SerializedTx");
	let RawTransaction::Midnight(tx_bytes) = serialized_tx.tx else {
		panic!("expected test to run against midnight transaction");
	};

	let block_context = crate::ledger_7::BlockContext {
		tblock: Timestamp::from_secs(serialized_tx.context.tblock.to_secs()),
		tblock_err: serialized_tx.context.tblock_err,
		parent_block_hash: HashOutput(serialized_tx.context.parent_block_hash.0),
	};

	(tx_bytes, block_context)
}
