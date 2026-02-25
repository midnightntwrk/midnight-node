use super::ledger_helpers_local::{self, DefaultDB, PureGeneratorPedersen, deserialize};

type Signature = ledger_helpers_local::Signature;
type ProofMarker = ledger_helpers_local::ProofMarker;
type Transaction =
	ledger_helpers_local::Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;
type TransactionWithContext =
	ledger_helpers_local::TransactionWithContext<Signature, ProofMarker, DefaultDB>;

pub fn show_transaction(
	bytes: &[u8],
	with_context: bool,
) -> Result<(String, usize), Box<dyn std::error::Error + Send + Sync>> {
	let size = bytes.len();
	let display = if with_context {
		let tx: TransactionWithContext = deserialize(bytes)?;
		format!("{tx:#?}")
	} else {
		let tx: Transaction = deserialize(bytes)?;
		format!("{tx:#?}")
	};
	Ok((display, size))
}
