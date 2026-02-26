use std::fmt;

use clap::Args;

use crate::{serde_def::BuiltTransactions, tx_generator::source::GetTxsFromFile};

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
	txs: &BuiltTransactions,
) -> Result<(String, usize), Box<dyn std::error::Error + Send + Sync>> {
	// Try ledger_8 first (most common), fall back to ledger_7
	crate::commands::fork::ledger_8::show_transaction::show_transactions(&txs)
		.or_else(|_| crate::commands::fork::ledger_7::show_transaction::show_transactions(&txs))
}

// TODO: Re-enable this test
// #[cfg(test)]
// mod test {
// 	use super::InnerReturnType;
// 	use test_case::test_case;
//
// 	#[test_case(
// 		"../../res/test-tx-deserialize/serialized_tx_no_context.mn",
// 		false,
// 		tx_from_bytes;
// 		"transaction no context"
// 	)]
// 	#[test_case(
// 		"../../res/test-tx-deserialize/serialized_tx_with_context.mn",
// 		true,
// 		tx_from_bytes;
// 		"transaction with context"
// 	)]
// 	fn test_show_transaction_funcs<F>(src_file: &str, with_context: bool, func: F)
// 	where
// 		F: Fn(String, bool) -> InnerReturnType,
// 	{
// 		let result = func(src_file.to_string(), with_context).expect("should be ok");
// 		assert!(result.size > 0);
// 		assert!(!result.display.is_empty());
// 	}
// }
