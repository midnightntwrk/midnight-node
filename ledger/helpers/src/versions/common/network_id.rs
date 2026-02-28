use super::{DefaultDB, FinalizedTransaction, Transaction, deserialize};

/// Get NetworkId from transaction bytes
pub fn network_id_from_transaction_bytes(tx_bytes: &[u8]) -> Result<String, std::io::Error> {
	let tx: FinalizedTransaction<DefaultDB> = deserialize(tx_bytes)?;
	let network_id = match tx {
		Transaction::Standard(standard_transaction) => standard_transaction.network_id,
		Transaction::ClaimRewards(claim_rewards_transaction) => {
			claim_rewards_transaction.network_id
		},
	};
	Ok(network_id)
}
