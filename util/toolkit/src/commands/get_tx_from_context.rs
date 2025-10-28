use clap::Args;
use midnight_node_ledger_helpers::{DefaultDB, TransactionWithContext, deserialize};
use midnight_node_toolkit::{ProofType, SignatureType};

#[derive(Args)]
pub struct GetTxFromContextArgs {
	/// Target network
	#[arg(long)]
	network: String,
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

pub fn execute(
	args: &GetTxFromContextArgs,
) -> Result<(Vec<u8>, u64), Box<dyn std::error::Error + Send + Sync>> {
	let deserialized_tx_with_context: TransactionWithContext<SignatureType, ProofType, DefaultDB> =
		if !args.from_bytes {
			deserialize_from_bytes(&args.src_file)?
		} else {
			let bytes = std::fs::read(&args.src_file)?;
			deserialize(bytes.as_slice())?
		};

	let tx = deserialized_tx_with_context.tx;
	let serialized_tx = tx.serialize_inner()?;
	let timestamp = deserialized_tx_with_context.block_context.tblock.to_secs();

	Ok((serialized_tx, timestamp))
}

fn deserialize_from_bytes(
	src_file: &str,
) -> Result<
	TransactionWithContext<SignatureType, ProofType, DefaultDB>,
	Box<dyn std::error::Error + Send + Sync>,
> {
	// Read single tx from file
	let file_content = std::fs::read(src_file).expect("failed to read file");
	let tx_hex = String::from_utf8_lossy(&file_content);
	// Some IDEs auto-add an extra empty line at the end of the file
	let sanitized_hex_tx: String = tx_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();

	let tx_with_context = hex::decode(&sanitized_hex_tx)?;
	let bytes = tx_with_context.as_slice();

	let value = deserialize(bytes)?;

	Ok(value)
}

#[cfg(test)]
mod test {
	use std::time::{SystemTime, UNIX_EPOCH};

	use super::{GetTxFromContextArgs, execute};

	#[test_case::test_case(
        "undeployed",
        "../../res/test-contract/contract_tx_1_deploy_undeployed.mn";
        "undeployed deploy case"
    )]
	#[test_case::test_case(
        "undeployed",
        "../../res/test-contract/contract_tx_2_store_undeployed.mn";
        "undeployed store case"
    )]
	#[test_case::test_case(
        "undeployed",
        "../../res/test-contract/contract_tx_3_check_undeployed.mn";
        "undeployed check case"
    )]
	fn test_get_tx_from_context(network: &str, src_file: &str) {
		let args = GetTxFromContextArgs {
			network: network.to_string(),
			src_file: src_file.to_string(),
			dest_file: "output.mn".to_string(),
			from_bytes: true,
		};

		let (tx, timestamp) = execute(&args).expect("all good");
		assert!(!tx.is_empty());
		assert!(timestamp < SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs());
	}
}
