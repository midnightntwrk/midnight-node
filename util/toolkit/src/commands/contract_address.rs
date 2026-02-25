use crate::tx_generator::source::GetTxsFromFile;
use clap::Args;
use hex::ToHex;
use midnight_node_ledger_helpers::{
	DefaultDB, FinalizedTransaction, fork::raw_block_data::RawTransaction, mn_ledger_serialize,
	serialize, serialize_untagged,
};
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

pub fn execute(args: ContractAddressArgs) -> Result<String, ContractAddressError> {
	let tx = GetTxsFromFile::load_single(&args.src_file)
		.map_err(ContractAddressError::TransactionLoadError)?;

	let RawTransaction::Midnight(tx) = tx.tx else {
		return Err(ContractAddressError::TransactionIsSystemTransaction);
	};

	let mn_tx: FinalizedTransaction<DefaultDB> =
		mn_ledger_serialize::tagged_deserialize(tx.as_slice())
			.map_err(ContractAddressError::LedgerSerializeError)?;

	let (_, deploy) = mn_tx.deploys().next().ok_or(ContractAddressError::NoContractDeployFound)?;

	let both = ContractAddressBoth {
		tagged: serialize(&deploy.address())
			.map_err(ContractAddressError::LedgerSerializeError)?
			.encode_hex(),
		untagged: serialize_untagged(&deploy.address())
			.map_err(ContractAddressError::LedgerSerializeError)?
			.encode_hex(),
	};

	if args.untagged {
		eprintln!("Warning: `--untagged` flag is deprecated (now default)");
	}

	if args.tagged { Ok(both.tagged) } else { Ok(both.untagged) }
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
