use crate::tx_generator::source::GetTxsFromFile;
use clap::Args;
use midnight_node_ledger_helpers::fork::raw_block_data::RawTransaction;
use serde::Serialize;

#[derive(Args, Clone)]
pub struct ContractAddressArgs {
	/// Serialize Tagged
	#[arg(long)]
	tagged: bool,
	/// Serialize Untagged
	#[arg(long)]
	untagged: bool,
	/// Serialized Transaction
	#[arg(long, short)]
	src_file: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ContractAddressError {
	#[error("failed to load tx")]
	TransactionLoadError(std::io::Error),
	#[error("ledger de/ser failed")]
	LedgerSerializeError(std::io::Error),
	#[error("transaction type is a System Transaction")]
	TransactionIsSystemTransaction,
	#[error("no contract deploy found in transaction")]
	NoContractDeployFound,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractAddressBoth {
	tagged: String,
	untagged: String,
}

impl ContractAddressBoth {
	pub fn new(tagged: String, untagged: String) -> Self {
		Self { tagged, untagged }
	}

	pub fn tagged(&self) -> &str {
		&self.tagged
	}

	pub fn untagged(&self) -> &str {
		&self.untagged
	}
}

pub fn execute(args: ContractAddressArgs) -> Result<String, ContractAddressError> {
	let tx = GetTxsFromFile::load_single(&args.src_file)
		.map_err(ContractAddressError::TransactionLoadError)?;

	let RawTransaction::Midnight(tx_bytes) = tx.tx else {
		return Err(ContractAddressError::TransactionIsSystemTransaction);
	};

	// Try ledger_8 first, fall back to ledger_7
	let both = crate::commands::fork::ledger_8::contract_address::extract_contract_address(
		tx_bytes.as_slice(),
	)
	.or_else(|_| {
		crate::commands::fork::ledger_7::contract_address::extract_contract_address(
			tx_bytes.as_slice(),
		)
	})?;

	if args.untagged {
		eprintln!("Warning: `--untagged` flag is deprecated (now default)");
	}

	if args.tagged { Ok(both.tagged().to_string()) } else { Ok(both.untagged().to_string()) }
}

#[cfg(test)]
mod test {
	use super::{ContractAddressArgs, execute};

	// todo: need more samples
	#[test_case::test_case(
		"../../res/test-contract/contract_tx_1_deploy_undeployed.mn",
		"../../res/test-contract/contract_address_undeployed.mn";
		"undeployed case"
	)]
	fn test_contract_address(src_file: &str, untagged_address_file: &str) {
		let args =
			ContractAddressArgs { src_file: src_file.to_string(), tagged: false, untagged: false };
		let res = execute(args).expect("execution failed");

		let untagged =
			std::fs::read_to_string(untagged_address_file).expect("failed to read address file");
		assert_eq!(res, untagged.trim());

		let args =
			ContractAddressArgs { src_file: src_file.to_string(), tagged: true, untagged: true };
		let res = execute(args).expect("execution failed");
		assert!(res.len() > untagged.trim().len());
	}
}
