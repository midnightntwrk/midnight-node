use super::ledger_helpers_local::{self, ContractAddress, DefaultDB, serialize};

pub fn get_contract_state(
	context: &ledger_helpers_local::context::LedgerContext<DefaultDB>,
	contract_address: ContractAddress,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
	let state = context
		.with_ledger_state(|ledger_state| ledger_state.index(contract_address))
		.expect("contract state for address does not exist");

	let serialized_state = serialize(&state)?;
	Ok(serialized_state)
}
