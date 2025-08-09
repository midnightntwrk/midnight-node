use clap::Args;
use midnight_node_ledger_helpers::{
	DefaultDB, NetworkId, TransactionWithContext, deserialize, serialize,
};
use midnight_node_toolkit::{
	ProofType, SignatureType,
	cli_parsers::{self as cli},
};

#[derive(Args)]
pub struct GetTxFromContextArgs {
	/// Target network
	#[arg(long, value_parser = cli::network_id_decode)]
	network: NetworkId,
	/// Serialized Transaction
	#[arg(long, short)]
	src_file: String,
	/// Destination file to save the address
	#[arg(long, short)]
	pub dest_file: String,
	/// Select if the transactions to show is saved as bytes
	#[arg(long, default_value = "false")]
	from_bytes: bool,
}

pub fn execute(args: &GetTxFromContextArgs) -> Result<(Vec<u8>, u64), Box<dyn std::error::Error>> {
	let network = args.network;

	let deserialized_tx_with_context: TransactionWithContext<SignatureType, ProofType, DefaultDB> =
		if !args.from_bytes {
			deserialize_from_bytes(&args.src_file, network)?
		} else {
			let bytes = std::fs::read(&args.src_file)?;
			deserialize(bytes.as_slice(), network)?
		};

	let tx = deserialized_tx_with_context.tx;
	let serialized_tx = serialize(&tx, network)?;
	let timestamp = deserialized_tx_with_context.block_context.tblock.to_secs();

	Ok((serialized_tx, timestamp))
}

fn deserialize_from_bytes(
	src_file: &str,
	network: NetworkId,
) -> Result<TransactionWithContext<SignatureType, ProofType, DefaultDB>, Box<dyn std::error::Error>>
{
	// Read single tx from file
	let file_content = std::fs::read(src_file).expect("failed to read file");
	let tx_hex = String::from_utf8_lossy(&file_content);
	// Some IDEs auto-add an extra empty line at the end of the file
	let sanitized_hex_tx: String = tx_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();

	let tx_with_context = hex::decode(&sanitized_hex_tx)?;
	let bytes = tx_with_context.as_slice();

	let value = deserialize(bytes, network)?;

	Ok(value)
}

#[cfg(test)]
mod test {
	use super::{GetTxFromContextArgs, NetworkId, execute};

	#[test_case::test_case(
        NetworkId::Undeployed,
        "../../res/test-contract/contract_tx_1_deploy_undeployed.mn",
        1752166483;
        "undeployed deploy case"
    )]
	#[test_case::test_case(
        NetworkId::Undeployed,
        "../../res/test-contract/contract_tx_2_store_undeployed.mn",
        1752166494;
        "undeployed store case"
    )]
	#[test_case::test_case(
        NetworkId::Undeployed,
        "../../res/test-contract/contract_tx_3_check_undeployed.mn",
        1752166504;
        "undeployed check case"
    )]
	fn test_get_tx_from_context(network: NetworkId, src_file: &str, timestamp_param: u64) {
		let args = GetTxFromContextArgs {
			network,
			src_file: src_file.to_string(),
			dest_file: "output.mn".to_string(),
			from_bytes: true,
		};

		let (_, timestamp) = execute(&args).expect("all good");
		assert_eq!(timestamp, timestamp_param);
	}
}
