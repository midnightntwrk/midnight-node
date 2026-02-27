use std::fmt;

use clap::Args;

use crate::tx_generator::source::GetTxsFromFile;
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

pub struct ShowTransactionResult {
	display: String,
	size: usize,
}

impl fmt::Display for ShowTransactionResult {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		writeln!(f)?;
		writeln!(f, "Tx {}", self.display)?;
		writeln!(f)?;
		write!(f, "Size {:?}", self.size)
	}
}

#[derive(Args)]
pub struct ShowTransactionArgs {
	/// Serialized Transaction
	#[arg(long, short)]
	src_file: String,
}

pub fn execute(
	args: ShowTransactionArgs,
) -> Result<ShowTransactionResult, Box<dyn std::error::Error + Send + Sync>> {
	let txs = GetTxsFromFile::load_single_or_multiple(&args.src_file)?;
	let (display, size) = exec_inner(&txs)?;
	Ok(ShowTransactionResult { display, size })
}

pub fn exec_inner(
	txs: &SerializedTxBatches,
) -> Result<(String, usize), Box<dyn std::error::Error + Send + Sync>> {
	// Try ledger_8 first (most common), fall back to ledger_7
	crate::commands::fork::ledger_8::show_transaction::show_transactions(&txs)
		.or_else(|_| crate::commands::fork::ledger_7::show_transaction::show_transactions(&txs))
}

#[cfg(test)]
mod test {
	use crate::commands::show_transaction::ShowTransactionArgs;

	#[test]
	fn test_show_transaction_funcs() {
		let result = super::execute(ShowTransactionArgs {
			src_file: "../../res/test-tx-deserialize/serialized_tx.mn".to_string(),
		})
		.unwrap();
		assert!(result.size > 0);
		assert!(!result.display.is_empty());
	}
}
