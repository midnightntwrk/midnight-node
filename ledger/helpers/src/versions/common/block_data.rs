use super::{BlockContext, DB, HashOutput, ProofKind, SerdeTransaction, SignatureKind, Tagged};
use serde::{Deserialize, Serialize};

/// Block data - struct containing all Ledger-relevant data for each block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockData<S: SignatureKind<D> + Tagged, P: ProofKind<D>, D: DB> {
	pub hash: HashOutput,
	pub parent_hash: HashOutput,
	pub number: u64,
	#[serde(bound(
		deserialize = "Vec<SerdeTransaction<S, P, D>>: Deserialize<'de>",
		serialize = "Vec<SerdeTransaction<S, P, D>>: Serialize"
	))]
	pub transactions: Vec<SerdeTransaction<S, P, D>>,
	pub context: BlockContext,
	pub state_root: Option<Vec<u8>>,
}
