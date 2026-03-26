use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionDebug {
	pub tx_type: String,
	pub size_bytes: usize,
	#[serde(with = "hex")]
	pub hash: [u8; 32],
	pub debug_str: String,
}
