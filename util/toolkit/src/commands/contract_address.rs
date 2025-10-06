use clap::Args;
use hex::ToHex;
use midnight_node_ledger_helpers::{
	DefaultDB, NetworkId, TransactionWithContext, mn_ledger_serialize, serialize,
	serialize_untagged,
};
use midnight_node_toolkit::{
	ProofType, SignatureType,
	cli_parsers::{self as cli},
};
use serde::Serialize;
use std::fs;

#[derive(Args, Clone)]
pub struct ContractAddressArgs {
	/// Target network
	#[arg(long, value_parser = cli::network_id_decode)]
	network: NetworkId,
	/// Serialized Transaction
	#[arg(long, short)]
	src_file: String,
	/// Serialize Tagged
	#[arg(long, conflicts_with = "untagged")]
	tagged: bool,
	/// Serialize Untagged
	#[arg(long, conflicts_with = "tagged")]
	untagged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractAddressBoth {
	tagged: String,
	untagged: String,
}

#[derive(Debug)]
pub enum ContractAddressValue {
	Either(String),
	Both(ContractAddressBoth),
}

pub fn execute(
	args: ContractAddressArgs,
) -> Result<ContractAddressValue, Box<dyn std::error::Error + Send + Sync>> {
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

	if args.tagged {
		Ok(ContractAddressValue::Either(both.tagged))
	} else if args.untagged {
		Ok(ContractAddressValue::Either(both.untagged))
	} else {
		Ok(ContractAddressValue::Both(both))
	}
}

#[cfg(test)]
mod test {
	use super::{ContractAddressArgs, ContractAddressValue, NetworkId, execute};

	// todo: need more samples
	#[test_case::test_case(
        NetworkId::Undeployed,
        "../../res/test-contract/contract_tx_1_deploy_undeployed.mn",
"6d69646e696768743a636f6e74726163742d616464726573735b76325d3a67d664a2055a72472e8ec00b1225204540daa3afba8e847bf1c79057f795f870",
        "67d664a2055a72472e8ec00b1225204540daa3afba8e847bf1c79057f795f870" ;
        "undeployed case"
    )]
	fn test_contract_address(network: NetworkId, src_file: &str, tagged: &str, untagged: &str) {
		let args = ContractAddressArgs {
			network,
			src_file: src_file.to_string(),
			tagged: false,
			untagged: false,
		};

		let res = execute(args).expect("execution failed");

		assert!(matches!(res, ContractAddressValue::Both(_)));

		if let ContractAddressValue::Both(both) = res {
			assert_eq!(both.tagged, tagged);
			assert_eq!(both.untagged, untagged);
		} else {
			panic!("incorrect return");
		};
	}
}
