use super::ledger_helpers_local::{self, DefaultDB, FinalizedTransaction, mn_ledger_serialize};
use crate::commands::contract_address::{ContractAddressBoth, ContractAddressError};
use hex::ToHex;

pub fn extract_contract_address(
	tx_bytes: &[u8],
) -> Result<ContractAddressBoth, ContractAddressError> {
	let mn_tx: FinalizedTransaction<DefaultDB> = mn_ledger_serialize::tagged_deserialize(tx_bytes)
		.map_err(ContractAddressError::LedgerSerializeError)?;

	let (_, deploy) = mn_tx.deploys().next().ok_or(ContractAddressError::NoContractDeployFound)?;

	let tagged = ledger_helpers_local::serialize(&deploy.address())
		.map_err(ContractAddressError::LedgerSerializeError)?
		.encode_hex();
	let untagged = ledger_helpers_local::serialize_untagged(&deploy.address())
		.map_err(ContractAddressError::LedgerSerializeError)?
		.encode_hex();

	Ok(ContractAddressBoth::new(tagged, untagged))
}
