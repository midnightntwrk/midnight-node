use std::fmt;

use clap::Args;

type InnerReturnType = Result<ShowTransactionResult, Box<dyn std::error::Error + Send + Sync>>;

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
	/// Select if the transactions to show is saved as bytes
	#[arg(long, default_value = "false")]
	from_bytes: bool,
	/// Select if the transaction should be shown with context
	#[arg(long, default_value = "false")]
	with_context: bool,
}

pub fn execute(args: ShowTransactionArgs) -> InnerReturnType {
	if args.from_bytes {
		tx_from_bytes(args.src_file, args.with_context)
	} else {
		tx_from_hex(args.src_file, args.with_context)
	}
}

fn deserialize_tx(
	tx_bytes: &[u8],
	with_context: bool,
) -> Result<(String, usize), Box<dyn std::error::Error + Send + Sync>> {
	// Try ledger_8 first (most common), fall back to ledger_7
	crate::commands::fork::ledger_8::show_transaction::show_transaction(tx_bytes, with_context)
		.or_else(|_| {
			crate::commands::fork::ledger_7::show_transaction::show_transaction(
				tx_bytes,
				with_context,
			)
		})
}

fn tx_from_bytes(src_file: String, with_context: bool) -> InnerReturnType {
	let tx_bytes = std::fs::read(&src_file)?;
	let (display, size) = deserialize_tx(&tx_bytes, with_context)?;
	Ok(ShowTransactionResult { display, size })
}

fn tx_from_hex(src_file: String, with_context: bool) -> InnerReturnType {
	let file_content = std::fs::read(&src_file)?;
	// Some IDEs auto-add an extra empty line at the end of the file
	let tx_hex = String::from_utf8(file_content)?.trim().to_string();
	let tx_bytes = hex::decode(&tx_hex)?;
	let (display, size) = deserialize_tx(&tx_bytes, with_context)?;
	Ok(ShowTransactionResult { display, size })
}

#[cfg(test)]
mod test {
	use super::{InnerReturnType, tx_from_bytes};
	use test_case::test_case;

	#[test_case(
		"../../res/test-tx-deserialize/serialized_tx_no_context.mn",
		false,
		tx_from_bytes;
		"transaction no context"
	)]
	#[test_case(
		"../../res/test-tx-deserialize/serialized_tx_with_context.mn",
		true,
		tx_from_bytes;
		"transaction with context"
	)]
	fn test_show_transaction_funcs<F>(src_file: &str, with_context: bool, func: F)
	where
		F: Fn(String, bool) -> InnerReturnType,
	{
		let result = func(src_file.to_string(), with_context).expect("should be ok");
		assert!(result.size > 0);
		assert!(!result.display.is_empty());
	}
}
