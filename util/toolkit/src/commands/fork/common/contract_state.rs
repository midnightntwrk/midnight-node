use super::ledger_helpers_local::{
	self, ContractAddress, DefaultDB, HashOutput, serialize, serialize_untagged,
};

pub fn get_contract_state(
	context: &ledger_helpers_local::context::LedgerContext<DefaultDB>,
	contract_address: midnight_node_ledger_helpers::ContractAddress,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
	// ContractAddress is a HashOutput newtype in both coin-structure 2.x and 3.x;
	// identity when compiled as ledger_9.
	let contract_address = ContractAddress(HashOutput(contract_address.0.0));
	let state = context
		.with_ledger_state(|ledger_state| ledger_state.index(contract_address))
		.expect("contract state for address does not exist");

	log::info!("Contract address: {}", hex::encode(serialize_untagged(&contract_address)?));
	for operation in state.operations.keys() {
		log::info!("Op: {} ({})", String::from_utf8_lossy(&operation.0), hex::encode(&operation.0));
	}
	for key in &state.maintenance_authority.committee {
		log::info!("Authority VerifyingKey: {}", hex::encode(serialize_untagged(&key)?));
	}
	log::info!("Authority Threshold: {}", state.maintenance_authority.threshold);
	log::info!("Authority Counter: {}", state.maintenance_authority.counter);

	let serialized_state = serialize(&state)?;
	Ok(serialized_state)
}
