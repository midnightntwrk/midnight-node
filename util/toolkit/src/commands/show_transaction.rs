use std::fmt;

use crate::{
	DefaultDB, NetworkId, ProofType, SignatureType, Transaction, TransactionWithContext,
	deserialize,
};
use clap::Args;
use midnight_node_ledger_helpers::PureGeneratorPedersen;
use midnight_node_toolkit::cli_parsers::{self as cli};

type InnerReturnType = Result<ShowTransactionResult, Box<dyn std::error::Error + Send + Sync>>;

pub enum TransactionInfo {
	Transaction(Transaction<SignatureType, ProofType, PureGeneratorPedersen, DefaultDB>),
	TransactionWithContext(TransactionWithContext<SignatureType, ProofType, DefaultDB>),
}
pub struct ShowTransactionResult {
	transaction: TransactionInfo,
	size: usize,
}

impl fmt::Display for ShowTransactionResult {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		writeln!(f)?;
		writeln!(f, "Tx {}", self.transaction)?;
		writeln!(f)?;
		write!(f, "Size {:?}", self.size)
	}
}

impl fmt::Display for TransactionInfo {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			TransactionInfo::Transaction(tx) => write!(f, "{:#?}", tx),
			TransactionInfo::TransactionWithContext(tx_ctx) => write!(f, "{:#?}", tx_ctx),
		}
	}
}

#[derive(Args)]
pub struct ShowTransactionArgs {
	#[arg(long, value_parser = cli::network_id_decode)]
	network: NetworkId,
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
	if !args.from_bytes {
		tx_from_bytes(args.src_file, args.with_context, args.network)
	} else {
		tx_from_file(args.src_file, args.with_context, args.network)
	}
}

fn tx_from_bytes(src_file: String, with_context: bool, _network: NetworkId) -> InnerReturnType {
	let file_content = std::fs::read(&src_file)?;
	// Some IDEs auto-add an extra empty line at the end of the file
	let tx_hex: String = String::from_utf8_lossy(&file_content)
		.chars()
		.filter(|c| c.is_ascii_hexdigit())
		.collect();

	let tx_bytes = hex::decode(&tx_hex)?;
	let tx_bytes = tx_bytes.as_slice();
	Ok(ShowTransactionResult {
		transaction: if with_context {
			TransactionInfo::TransactionWithContext(deserialize(tx_bytes)?)
		} else {
			TransactionInfo::Transaction(deserialize(tx_bytes)?)
		},
		size: tx_bytes.len(),
	})
}

fn tx_from_file(src_file: String, with_context: bool, _network: NetworkId) -> InnerReturnType {
	let bytes = std::fs::read(&src_file)?;
	Ok(ShowTransactionResult {
		transaction: if with_context {
			TransactionInfo::TransactionWithContext(deserialize(bytes.as_slice())?)
		} else {
			TransactionInfo::Transaction(deserialize(bytes.as_slice())?)
		},
		size: bytes.len(),
	})
}

#[cfg(test)]
mod test {
	use super::{InnerReturnType, NetworkId, TransactionInfo, tx_from_file};
	use test_case::test_case;

	#[test_case(
		"../../res/test-tx-deserialize/serialized_tx_no_context.mn",
		false,
		tx_from_file;
		"transaction no context"
	)]
	#[test_case(
		"../../res/test-tx-deserialize/serialized_tx_with_context.mn",
		true,
		tx_from_file;
		"transaction with context"
	)]
	fn test_show_transaction_funcs<F>(src_file: &str, with_context: bool, func: F)
	where
		F: Fn(String, bool, NetworkId) -> InnerReturnType,
	{
		let result =
			func(src_file.to_string(), with_context, NetworkId::Undeployed).expect("should be ok");

		match result.transaction {
			TransactionInfo::Transaction(_) if with_context => assert!(false),
			TransactionInfo::Transaction(_) if !with_context => assert!(true),
			TransactionInfo::TransactionWithContext(_) if with_context => assert!(true),
			TransactionInfo::TransactionWithContext(_) if !with_context => assert!(false),
			_ => assert!(false),
		}
	}
}
