use clap::Args;

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
	let tx_bytes = if !args.from_bytes {
		read_hex_file(&args.src_file)?
	} else {
		std::fs::read(&args.src_file)?
	};

	// Try ledger_8 first, fall back to ledger_7
	crate::commands::fork::ledger_8::get_tx_from_context::extract_tx_from_context(&tx_bytes)
		.or_else(|_| {
			crate::commands::fork::ledger_7::get_tx_from_context::extract_tx_from_context(
				&tx_bytes,
			)
		})
}

fn read_hex_file(
	src_file: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
	let file_content = std::fs::read(src_file)?;
	let tx_hex = String::from_utf8_lossy(&file_content);
	// Some IDEs auto-add an extra empty line at the end of the file
	let sanitized_hex_tx: String = tx_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
	let bytes = hex::decode(&sanitized_hex_tx)?;
	Ok(bytes)
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
