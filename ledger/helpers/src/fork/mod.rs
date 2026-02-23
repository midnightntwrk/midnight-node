use crate::fork::raw_block_data::LedgerVersion;

pub mod fork_7_to_8;
pub mod fork_aware_context;
pub mod fork_to_hf;
pub mod raw_block_data;

pub fn network_id_and_ledger_version_from_tx_bytes(tx_bytes: &[u8]) -> (String, LedgerVersion) {
	let res8 = crate::ledger_8::network_id_from_transaction_bytes(tx_bytes);
	if let Ok(ref network_id) = res8 {
		return (network_id.to_string(), LedgerVersion::Ledger8);
	}

	let res7 = crate::ledger_7::network_id_from_transaction_bytes(tx_bytes);
	if let Ok(network_id) = res7 {
		return (network_id.to_string(), LedgerVersion::Ledger7);
	}

	panic!(
		"transaction bytes does not deserialize into either ledger 8 or ledger 7 type transaction. Results: {res7:?} {res8:?}"
	);
}
