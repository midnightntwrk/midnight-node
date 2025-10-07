use clap::Args;
use hex::ToHex;
use midnight_node_ledger_helpers::{
	DefaultDB, TransactionWithContext, mn_ledger_serialize, serialize, serialize_untagged,
};
use midnight_node_toolkit::{ProofType, SignatureType};
use serde::Serialize;
use std::fs;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractAddressBoth {
	tagged: String,
	untagged: String,
}

pub fn execute(
	args: ContractAddressArgs,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
	let bytes = fs::read(&args.src_file).expect("failed to read file");
	let tx_with_context: TransactionWithContext<SignatureType, ProofType, DefaultDB> =
		mn_ledger_serialize::tagged_deserialize(bytes.as_slice())?;

	let (_, deploy) = tx_with_context
		.tx
		.as_midnight()
		.expect("Not called with a standard midnight transaction")
		.deploys()
		.next()
		.expect("There is not any `ContractDeploy` in the tx");

	let both = ContractAddressBoth {
		tagged: serialize(&deploy.address())?.encode_hex(),
		untagged: serialize_untagged(&deploy.address())?.encode_hex(),
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
"6d69646e696768743a636f6e74726163742d616464726573735b76325d3a67d664a2055a72472e8ec00b1225204540daa3afba8e847bf1c79057f795f870",
        "67d664a2055a72472e8ec00b1225204540daa3afba8e847bf1c79057f795f870" ;
        "undeployed case"
    )]
	fn test_contract_address(src_file: &str, tagged: &str, untagged: &str) {
		let args =
			ContractAddressArgs { src_file: src_file.to_string(), tagged: false, untagged: false };
		let res = execute(args).expect("execution failed");
		assert_eq!(res, untagged);

		let args =
			ContractAddressArgs { src_file: src_file.to_string(), tagged: true, untagged: true };
		let res = execute(args).expect("execution failed");
		assert_eq!(res, tagged);
	}
}
