use hex::ToHex as _;
use midnight_node_ledger_helpers::{
	DustGenerationInfo, InitialNonce, QualifiedDustOutput, QualifiedInfo, Timestamp, Utxo,
	persistent_commit, serialize_untagged,
};

#[derive(Debug, serde::Serialize)]
pub struct UtxoSer {
	pub id: String,
	pub initial_nonce: String,
	pub value: u128,
	pub user_address: String,
	pub token_type: String,
	pub intent_hash: String,
	pub output_number: u32,
}

impl From<Utxo> for UtxoSer {
	fn from(utxo: Utxo) -> Self {
		let intent_hash = utxo.intent_hash.0.0.encode_hex();
		let output_number = utxo.output_no;
		let id = format!("{intent_hash}#{output_number}");
		let initial_nonce = InitialNonce(persistent_commit(&utxo.output_no, utxo.intent_hash.0))
			.0
			.0
			.encode_hex();
		Self {
			id,
			initial_nonce,
			value: utxo.value,
			user_address: utxo.owner.0.0.encode_hex(),
			token_type: utxo.type_.0.0.encode_hex(),
			intent_hash,
			output_number,
		}
	}
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct QualifiedDustOutputSer {
	pub initial_value: u128,
	pub dust_public: String,
	pub nonce: String,
	pub seq: u32,
	pub ctime: Timestamp,
	pub backing_night: String,
	pub mt_index: u64,
}

impl From<QualifiedDustOutput> for QualifiedDustOutputSer {
	fn from(output: QualifiedDustOutput) -> Self {
		Self {
			initial_value: output.initial_value,
			dust_public: serialize_untagged(&output.owner).unwrap().encode_hex(),
			nonce: serialize_untagged(&output.nonce).unwrap().encode_hex(),
			seq: output.seq,
			ctime: output.ctime,
			backing_night: serialize_untagged(&output.backing_night).unwrap().encode_hex(),
			mt_index: output.mt_index,
		}
	}
}

#[derive(Debug, serde::Serialize)]
pub struct QualifiedInfoSer {
	pub nonce: String,
	pub token_type: String,
	pub value: u128,
	pub mt_index: u64,
}

impl From<QualifiedInfo> for QualifiedInfoSer {
	fn from(info: QualifiedInfo) -> Self {
		Self {
			nonce: serialize_untagged(&info.nonce).unwrap().encode_hex(),
			token_type: serialize_untagged(&info.type_).unwrap().encode_hex(),
			value: info.value,
			mt_index: info.mt_index,
		}
	}
}

#[derive(Debug, serde::Serialize)]
pub struct DustGenerationInfoSer {
	pub value: u128,
	pub owner_dust_public_key: String,
	pub nonce: String,
	pub dtime: Timestamp,
}

impl From<DustGenerationInfo> for DustGenerationInfoSer {
	fn from(value: DustGenerationInfo) -> Self {
		Self {
			value: value.value,
			owner_dust_public_key: serialize_untagged(&value.owner).unwrap().encode_hex(),
			nonce: serialize_untagged(&value.nonce).unwrap().encode_hex(),
			dtime: value.dtime,
		}
	}
}
