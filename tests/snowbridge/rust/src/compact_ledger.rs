// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use base_crypto::{cost_model::FixedPoint, hash::HashOutput, time::Timestamp};
use coin_structure::{
	coin::{TokenType, UnshieldedTokenType, UserAddress},
	contract::ContractAddress,
};
use ledger::{
	semantics::{TransactionContext, TransactionResult},
	structure::{
		Intent, IntentHash, LedgerState, ProofPreimageMarker, Signature, SigningKey,
		SystemTransaction, Transaction, UnshieldedOffer, UtxoOutput, UtxoSpend,
	},
	test_utilities::{TxBound, test_resolver, tx_prove_bind},
	verify::WellFormedStrictness,
};
use onchain_runtime::context::BlockContext;
use rand::{SeedableRng, rngs::StdRng};
use std::path::PathBuf;
use storage::{arena::Sp, db::InMemoryDB};
use transient_crypto::commitment::PedersenRandomness;

fn fixture_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../artifacts/snowbridge/compact-ledger")
}

fn hash(value: &serde_json::Value) -> HashOutput {
	HashOutput(hex::decode(value.as_str().unwrap()).unwrap().try_into().unwrap())
}

fn apply(
	state: &LedgerState<InMemoryDB>,
	tx: &TxBound<Signature, InMemoryDB>,
) -> LedgerState<InMemoryDB> {
	let verified = tx
		.well_formed(state, WellFormedStrictness::default(), Timestamp::from_secs(2))
		.unwrap();
	let context = TransactionContext {
		ref_state: state.clone(),
		block_context: BlockContext { tblock: Timestamp::from_secs(2), ..BlockContext::default() },
		whitelist: None,
	};
	let (next, result) = state.apply(&verified, &context);
	assert!(matches!(result, TransactionResult::Success(_)), "{result:?}");
	next
}

#[tokio::test]
async fn funded_compact_round_trip_with_real_proofs_and_genesis_replay() {
	let manifest: serde_json::Value =
		serde_json::from_slice(&std::fs::read(fixture_dir().join("manifest.json")).unwrap())
			.unwrap();
	let owner = UserAddress(hash(&manifest["owner"]));
	let color = UnshieldedTokenType(hash(&manifest["color"]));
	let escrow = ContractAddress(hash(&manifest["escrow"]));
	let consumer = ContractAddress(hash(&manifest["consumer"]));
	let sk = SigningKey::Schnorr(base_crypto::schnorr::SigningKey::from_bytes(&[1; 32]).unwrap());
	assert_eq!(owner, UserAddress::from(sk.verifying_key()));
	let mut genesis = LedgerState::<InMemoryDB>::new("snowbridge-fixture");
	let mut params = (*genesis.parameters).clone();
	params.fee_prices.overall_price = FixedPoint::ZERO;
	genesis.parameters = Sp::new(params);
	let issue = SystemTransaction::CreditEthereumAsset {
		deposit_id: HashOutput([1; 32]),
		output_id: IntentHash(HashOutput([1; 32])),
		token_type: color,
		amount: 100,
		recipient: owner,
	};
	let mut state = genesis.apply_system_tx(&issue, Timestamp::from_secs(1)).unwrap().0;
	let mut rng = StdRng::seed_from_u64(43);
	let mut journal = Vec::new();
	for (contract, operation) in [
		("escrow", "deploy"),
		("consumer", "deploy"),
		("consumer", "deposit"),
		("consumer", "balance"),
		("consumer", "withdraw"),
		("escrow", "retire"),
	] {
		let bytes =
			std::fs::read(fixture_dir().join(format!("{contract}-{operation}.bin"))).unwrap();
		let mut intent: Intent<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
			serialize::tagged_deserialize(&bytes[..]).unwrap();
		let mut inputs = vec![];
		let mut outputs = vec![];
		if operation == "deposit" || operation == "retire" {
			// The circuit transcript alone cannot create the received balance.
			let unfunded = Transaction::from_intents(
				"snowbridge-fixture",
				[(1, intent.clone())].into_iter().collect(),
			);
			if let Ok(proven) =
				tx_prove_bind(rng.clone(), &unfunded, &test_resolver(contract)).await
			{
				if let Ok(verified) = proven.well_formed(
					&state,
					WellFormedStrictness::default(),
					Timestamp::from_secs(2),
				) {
					let (unchanged, result) = state.apply(
						&verified,
						&TransactionContext {
							ref_state: state.clone(),
							block_context: BlockContext {
								tblock: Timestamp::from_secs(2),
								..BlockContext::default()
							},
							whitelist: None,
						},
					);
					assert!(
						matches!(result, TransactionResult::Failure(_)),
						"unfunded {operation} succeeded"
					);
					assert_eq!(unchanged.state_hash(), state.state_hash());
				}
			}
			let utxo = state
				.utxo
				.utxos
				.iter()
				.find(|entry| entry.0.owner == owner && entry.0.type_ == color)
				.unwrap()
				.0
				.clone();
			let expected = if operation == "deposit" { 100 } else { 40 };
			assert_eq!(utxo.value, expected);
			inputs.push(UtxoSpend {
				intent_hash: utxo.intent_hash,
				output_no: utxo.output_no,
				owner: sk.verifying_key(),
				type_: color,
				value: expected,
			});
		}
		if operation == "withdraw" {
			outputs.push(UtxoOutput { owner, type_: color, value: 40 });
		}
		if !inputs.is_empty() || !outputs.is_empty() {
			intent.guaranteed_unshielded_offer = Some(Sp::new(UnshieldedOffer {
				inputs: inputs.into(),
				outputs: outputs.into(),
				signatures: vec![].into(),
			}));
			intent = intent.sign(&mut rng, 1, &[sk.clone()], &[], &[]).unwrap();
		}
		let tx =
			Transaction::from_intents("snowbridge-fixture", [(1, intent)].into_iter().collect());
		let resolver = test_resolver(contract);
		let proven = tx_prove_bind(rng.clone(), &tx, &resolver).await.unwrap();
		// Neither transcript gathering nor an extrinsic success substitutes for ledger application.
		state = apply(&state, &proven);
		let mut serialized = vec![];
		serialize::tagged_serialize(&proven, &mut serialized).unwrap();
		journal.push(serialized);
	}
	assert_eq!(
		state
			.index(consumer)
			.unwrap()
			.balance
			.get(&TokenType::Unshielded(color))
			.map(|v| *v),
		Some(60)
	);
	let retired = state.index(escrow).unwrap();
	assert_eq!(retired.balance.get(&TokenType::Unshielded(color)).map(|v| *v), Some(40));
	assert_eq!(state.utxo.utxos.size(), 0);
	let mut replayed = genesis.apply_system_tx(&issue, Timestamp::from_secs(1)).unwrap().0;
	for bytes in &journal {
		let tx: TxBound<Signature, InMemoryDB> = serialize::tagged_deserialize(&bytes[..]).unwrap();
		replayed = apply(&replayed, &tx);
	}
	assert_eq!(replayed.state_hash(), state.state_hash());
	assert!(retired.maintenance_authority.committee.is_empty());
	assert_eq!(retired.maintenance_authority.threshold, 1);
	assert_eq!(retired.maintenance_authority.counter, 0);
	assert_eq!(retired.operations.size(), 1);

	// Persist real ledger results so the independent JS decoder can check compiled fields.
	let mut escrow_bytes = Vec::new();
	serialize::tagged_serialize(&retired, &mut escrow_bytes).unwrap();
	std::fs::write(fixture_dir().join("escrow-state.bin"), escrow_bytes).unwrap();
	let mut issue_bytes = Vec::new();
	serialize::tagged_serialize(&issue, &mut issue_bytes).unwrap();
	std::fs::write(fixture_dir().join("issuance.bin"), issue_bytes).unwrap();
	for (index, bytes) in journal.iter().enumerate() {
		std::fs::write(fixture_dir().join(format!("proven-{index}.bin")), bytes).unwrap();
	}
	let evidence = serde_json::json!({
		"profile": "funded-ledger-real-compact-proofs",
		"liveMidnightConsensus": false,
		"ethereumDepositProofVerified": false,
		"feePrice": 0,
		"issued": 100, "consumerBalance": 60, "escrowBalance": 40, "walletBalance": 0,
		"unfundedCallsRejected": ["deposit", "retire"],
		"provenTransactionsReplayed": journal.len(),
		"stateRoot": format!("{:?}", state.state_hash()),
		"replayedStateRoot": format!("{:?}", replayed.state_hash()),
		"manifest": manifest,
	});
	std::fs::write(
		fixture_dir().join("evidence.json"),
		serde_json::to_vec_pretty(&evidence).unwrap(),
	)
	.unwrap();
}
