pub mod raw_block_data;

#[cfg(feature = "can-panic")]
use crate::fork::raw_block_data::LedgerVersion;

#[cfg(feature = "can-panic")]
pub mod fork_7_to_8;
#[cfg(feature = "can-panic")]
pub mod fork_aware_context;

#[cfg(feature = "can-panic")]
pub fn network_id_and_ledger_version_from_tx_bytes(
	tx_bytes: &[u8],
) -> Result<(String, LedgerVersion), std::io::Error> {
	let res8 = crate::ledger_8::network_id_from_transaction_bytes(tx_bytes);
	if let Ok(ref network_id) = res8 {
		return Ok((network_id.to_string(), LedgerVersion::Ledger8));
	}

	let network_id = crate::ledger_7::network_id_from_transaction_bytes(tx_bytes)?;
	Ok((network_id.to_string(), LedgerVersion::Ledger7))
}
