use std::fmt;

use clap::Args;

use crate::{
	commands::show_block::{ShowBlockJson, ShowBlockTransaction},
	tx_generator::source::GetTxsFromFile,
};
use midnight_node_ledger_helpers::fork::raw_block_data::RawBlockData;

pub struct ShowTransactionResult {
	txs: Vec<ShowBlockTransaction>,
}

impl fmt::Display for ShowTransactionResult {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		for tx in &self.txs {
			writeln!(f, "{}", tx)?;
		}
		Ok(())
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
	let batches = GetTxsFromFile::load_single_or_multiple(&args.src_file)?;
	let blocks: Vec<RawBlockData> = (&batches).try_into()?;
	let mut txs = Vec::new();
	for block in blocks {
		let show_block = ShowBlockJson::new(&block)?;
		let start_index = txs.len();
		for (i, mut tx) in show_block.transactions.into_iter().enumerate() {
			tx.index = i + start_index;
			txs.push(tx);
		}
	}
	Ok(ShowTransactionResult { txs })
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
		assert!(!result.txs.is_empty());
	}
}
