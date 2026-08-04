// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::ledger_helpers_local::{
	self, InitialNonce, QualifiedDustOutput, QualifiedInfo, Utxo, persistent_commit,
	serialize_untagged,
};
use crate::serde_def::{DustGenerationInfoSer, QualifiedDustOutputSer, QualifiedInfoSer, UtxoSer};
use hex::ToHex as _;

pub fn utxo_to_ser(utxo: Utxo) -> UtxoSer {
	let intent_hash = utxo.intent_hash.0.0.encode_hex();
	let output_number = utxo.output_no;
	let id = format!("{intent_hash}#{output_number}");
	let initial_nonce = InitialNonce(persistent_commit(&utxo.output_no, utxo.intent_hash.0))
		.0
		.0
		.encode_hex();
	UtxoSer {
		id,
		initial_nonce,
		value: utxo.value,
		user_address: utxo.owner.0.0.encode_hex(),
		token_type: utxo.type_.0.0.encode_hex(),
		intent_hash,
		output_number,
	}
}

pub fn qualified_info_to_ser(info: QualifiedInfo) -> QualifiedInfoSer {
	QualifiedInfoSer {
		nonce: serialize_untagged(&info.nonce).unwrap().encode_hex(),
		token_type: serialize_untagged(&info.type_).unwrap().encode_hex(),
		value: info.value,
		mt_index: info.mt_index,
	}
}

pub fn qualified_dust_output_to_ser(output: QualifiedDustOutput) -> QualifiedDustOutputSer {
	QualifiedDustOutputSer {
		initial_value: output.initial_value,
		dust_public: serialize_untagged(&output.owner).unwrap().encode_hex(),
		nonce: serialize_untagged(&output.nonce).unwrap().encode_hex(),
		seq: output.seq,
		ctime: midnight_node_ledger_helpers::Timestamp::from_secs(output.ctime.to_secs()),
		backing_night: serialize_untagged(&output.backing_night).unwrap().encode_hex(),
		mt_index: output.mt_index,
	}
}

pub fn dust_generation_info_to_ser(
	info: ledger_helpers_local::DustGenerationInfo,
) -> DustGenerationInfoSer {
	DustGenerationInfoSer {
		value: info.value,
		owner_dust_public_key: serialize_untagged(&info.owner).unwrap().encode_hex(),
		nonce: serialize_untagged(&info.nonce).unwrap().encode_hex(),
		dtime: midnight_node_ledger_helpers::Timestamp::from_secs(info.dtime.to_secs()),
	}
}
