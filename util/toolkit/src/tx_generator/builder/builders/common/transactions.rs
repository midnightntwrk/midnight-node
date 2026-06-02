use midnight_node_ledger_helpers::fork::raw_block_data::RawTransaction;

use super::ledger_helpers_local::*;

pub fn from_serde_tx<S, P>(tx: &SerdeTransaction<S, P, DefaultDB>) -> RawTransaction
where
	S: SignatureKind<DefaultDB>,
	P: ProofKind<DefaultDB> + Send + Sync + 'static,
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	match tx {
		SerdeTransaction::Midnight(transaction) => {
			RawTransaction::Midnight(serialize(transaction).expect("failed to serialize tx"))
		},
		SerdeTransaction::System(system_transaction) => {
			RawTransaction::System(serialize(system_transaction).expect("failed to serialize tx"))
		},
	}
}
