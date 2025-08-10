use clap::Args;
use midnight_node_ledger_helpers::{
	DefaultDB, NetworkId, TransactionWithContext, mn_ledger_serialize, serialize,
};
use midnight_node_toolkit::{
	ProofType, SignatureType,
	cli_parsers::{self as cli},
};
use std::{fs, path::Path};

#[derive(Args, Clone)]
pub struct ContractAddressArgs {
	/// Target network
	#[arg(long, value_parser = cli::network_id_decode)]
	network: NetworkId,
	/// Serialized Transaction
	#[arg(long, short)]
	src_file: String,
	/// Destination file to save the address
	#[arg(long, short)]
	dest_file: String,
}

pub fn execute(args: ContractAddressArgs) -> Result<(), Box<dyn std::error::Error>> {
	let bytes = fs::read(&args.src_file).expect("failed to read file");
	let tx_with_context: TransactionWithContext<SignatureType, ProofType, DefaultDB> =
		mn_ledger_serialize::deserialize(bytes.as_slice(), args.network)?;

	let (_, deploy) = tx_with_context
		.tx
		.deploys()
		.next()
		.expect("There is not any `ContractDeploy` in the tx");

	let deserialized_address = deploy.address();
	let address = hex::encode(serialize(&deserialized_address, args.network)?);

	println!("\nDeserialized Address {:?}\n", deserialized_address.0);
	println!("\nSerialized Address {:?}\n", address);

	let full_path = Path::new(&args.dest_file);
	if let Some(directory) = full_path.parent() {
		fs::create_dir_all(directory).expect("failed to create directories");
	}

	fs::write(full_path, address.as_bytes()).expect("failed to create file");

	Ok(())
}

#[cfg(test)]
mod test {
	use super::{ContractAddressArgs, NetworkId, execute};
	use std::{
		env::temp_dir,
		fs::{self, remove_file},
	};

	// todo: need more samples
	#[test_case::test_case(
        NetworkId::Undeployed,
        "../../res/test-contract/contract_tx_1_deploy_undeployed.mn",
        "000200e99d4445695a6244a01ab00d592825e2703c3f9a928f01429561585ce2db1e78" ;
        "undeployed case"
    )]
	fn test_contract_address(network: NetworkId, src_file: &str, hex_addr: &str) {
		let path = temp_dir().join("example.mn");
		let path_str = path.as_os_str().to_str().expect("failed to convert to path");

		let args = ContractAddressArgs {
			network,
			src_file: src_file.to_string(),
			dest_file: path_str.to_string(),
		};

		let _ = execute(args).expect("execution failed");

		// check if file exists
		let file_exist = fs::exists(&path).expect("should return ok");
		assert!(file_exist);

		let bytes = fs::read(&path).expect("failed to return address");
		let bytes_to_string = String::from_utf8(bytes).expect("failed to convert to string");
		assert_eq!(bytes_to_string, hex_addr);

		remove_file(path).expect("It should be removed");
	}
}
