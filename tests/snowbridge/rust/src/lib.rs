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

#[cfg(test)]
mod tests {
	use base_crypto::{hash::HashOutput, time::Timestamp};
	use coin_structure::coin::{NIGHT, UnshieldedTokenType, UserAddress};
	use ledger::{
		error::SystemTransactionError,
		structure::{IntentHash, LedgerState, SystemTransaction},
	};
	use serialize::{tagged_deserialize, tagged_serialize};
	use storage::{arena::Sp, db::InMemoryDB};

	#[test]
	fn write_fresh_local_node_genesis() {
		let state = LedgerState::<InMemoryDB>::new("snowbridge-fixture");
		assert_eq!(state.ethereum_deposits.size(), 0);
		assert_eq!(state.utxo.utxos.size(), 0);
		let mut bytes = vec![];
		tagged_serialize(&state, &mut bytes).unwrap();
		let decoded: LedgerState<InMemoryDB> = tagged_deserialize(&bytes[..]).unwrap();
		assert_eq!(state.state_hash(), decoded.state_hash());
		let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("../../../artifacts/snowbridge/local-node");
		std::fs::create_dir_all(&directory).unwrap();
		std::fs::write(directory.join("genesis-state.bin"), bytes).unwrap();
		std::fs::write(directory.join("genesis-block.json"), br#"{"batches":[]}"#).unwrap();
	}

	fn credit(id: u8) -> SystemTransaction {
		SystemTransaction::CreditEthereumAsset {
			deposit_id: HashOutput([id; 32]),
			output_id: IntentHash(HashOutput([id; 32])),
			token_type: UnshieldedTokenType(HashOutput([42; 32])),
			amount: 100,
			recipient: UserAddress(HashOutput([7; 32])),
		}
	}

	#[test]
	fn serialized_issuance_replays_and_cannot_reissue_even_after_spend_or_ttl() {
		let genesis = LedgerState::<InMemoryDB>::new("snowbridge-fixture");
		let tx = credit(1);
		let mut bytes = vec![];
		tagged_serialize(&tx, &mut bytes).unwrap();
		let decoded: SystemTransaction = tagged_deserialize(&bytes[..]).unwrap();
		assert_eq!(tx, decoded);
		let (issued, _) = genesis.apply_system_tx(&tx, Timestamp::from_secs(1)).unwrap();
		let (replayed, _) = genesis.apply_system_tx(&decoded, Timestamp::from_secs(1)).unwrap();
		assert_eq!(issued.state_hash(), replayed.state_hash());
		assert_eq!(issued.ethereum_deposits.size(), 1);
		assert_eq!(issued.utxo.utxos.size(), 1);
		let mut duplicate_output = credit(2);
		if let SystemTransaction::CreditEthereumAsset { output_id, .. } = &mut duplicate_output {
			*output_id = IntentHash(HashOutput([1; 32]));
		}
		assert!(matches!(
			issued.apply_system_tx(&duplicate_output, Timestamp::from_secs(2)),
			Err(SystemTransactionError::EthereumOutputAlreadyPresent)
		));
		let mut conflicting_deposit = credit(1);
		if let SystemTransaction::CreditEthereumAsset { amount, .. } = &mut conflicting_deposit {
			*amount = 99;
		}
		assert!(matches!(
			issued.apply_system_tx(&conflicting_deposit, Timestamp::from_secs(2)),
			Err(SystemTransactionError::EthereumDepositAlreadyPresent)
		));
		let entry = issued.utxo.utxos.iter().next().unwrap();
		let utxo = &entry.0;
		assert_eq!(utxo.value, 100);
		assert_ne!(utxo.type_, NIGHT);
		// Simulate removal only to test the permanent deposit marker independently of a UTXO's lifetime.
		let mut spent = issued.clone();
		spent.utxo = Sp::new(spent.utxo.remove(&utxo));
		assert!(matches!(
			spent.apply_system_tx(&decoded, Timestamp::from_secs(u64::MAX / 2)),
			Err(SystemTransactionError::EthereumDepositAlreadyPresent)
		));
		assert!(matches!(
			issued.apply_system_tx(&decoded, Timestamp::from_secs(1)),
			Err(SystemTransactionError::EthereumDepositAlreadyPresent)
		));
		assert_eq!(genesis.ethereum_deposits.size(), 0);
		genesis.check_night_balance_invariant().unwrap();
		issued.check_night_balance_invariant().unwrap();
	}

	#[test]
	fn invalid_amount_night_and_lifetime_capacity_are_rejected() {
		let mut state = LedgerState::<InMemoryDB>::new("snowbridge-fixture");
		for amount in [0, 1_000_000_000_001] {
			let mut tx = credit(1);
			if let SystemTransaction::CreditEthereumAsset { amount: a, .. } = &mut tx {
				*a = amount;
			}
			assert!(matches!(
				state.apply_system_tx(&tx, Timestamp::from_secs(1)),
				Err(SystemTransactionError::InvalidEthereumCredit)
			));
		}
		let mut tx = credit(1);
		if let SystemTransaction::CreditEthereumAsset { token_type, .. } = &mut tx {
			*token_type = NIGHT;
		}
		assert!(matches!(
			state.apply_system_tx(&tx, Timestamp::from_secs(1)),
			Err(SystemTransactionError::InvalidEthereumCredit)
		));
		for i in 0..64 {
			state = state.apply_system_tx(&credit(i), Timestamp::from_secs(1)).unwrap().0;
		}
		assert!(matches!(
			state.apply_system_tx(&credit(64), Timestamp::from_secs(1)),
			Err(SystemTransactionError::EthereumDepositCapacityExceeded)
		));
	}
	#[test]
	fn issued_asset_supports_a_signed_balanced_transfer_and_strict_replay() {
		use base_crypto::cost_model::FixedPoint;
		use ledger::{
			semantics::{TransactionContext, TransactionResult},
			structure::{
				Intent, Signature, SigningKey, Transaction, UnshieldedOffer, UtxoOutput, UtxoSpend,
			},
			verify::WellFormedStrictness,
		};
		use onchain_runtime::context::BlockContext;
		use rand::{SeedableRng, rngs::StdRng};
		let mut rng = StdRng::seed_from_u64(42);
		let sk = SigningKey::Schnorr(base_crypto::schnorr::SigningKey::sample(&mut rng));
		let owner = UserAddress::from(sk.verifying_key());
		let mut state = LedgerState::<InMemoryDB>::new("snowbridge-fixture");
		// A zero fee schedule isolates native token balancing from DUST provisioning.
		let mut params = (*state.parameters).clone();
		params.fee_prices.overall_price = FixedPoint::ZERO;
		state.parameters = Sp::new(params);
		let mut issue = credit(1);
		if let SystemTransaction::CreditEthereumAsset { recipient, .. } = &mut issue {
			*recipient = owner;
		}
		state = state.apply_system_tx(&issue, Timestamp::from_secs(1)).unwrap().0;
		let original = state.utxo.utxos.iter().next().unwrap().0.clone();
		let mut intent =
			Intent::<Signature, _, _, InMemoryDB>::empty(&mut rng, Timestamp::from_secs(100));
		intent.guaranteed_unshielded_offer = Some(Sp::new(UnshieldedOffer {
			inputs: vec![UtxoSpend {
				intent_hash: original.intent_hash,
				output_no: 0,
				owner: sk.verifying_key(),
				type_: original.type_,
				value: 100,
			}]
			.into(),
			outputs: vec![
				UtxoOutput {
					owner: UserAddress(HashOutput([8; 32])),
					type_: original.type_,
					value: 40,
				},
				UtxoOutput { owner, type_: original.type_, value: 60 },
			]
			.into(),
			signatures: vec![].into(),
		}));
		let intent = intent.sign(&mut rng, 1, &[sk], &[], &[]).unwrap();
		let tx =
			Transaction::from_intents("snowbridge-fixture", [(1, intent)].into_iter().collect())
				.seal(rng);
		let verified = tx
			.well_formed(&state, WellFormedStrictness::default(), Timestamp::from_secs(2))
			.unwrap();
		let context = TransactionContext {
			ref_state: state.clone(),
			block_context: BlockContext {
				tblock: Timestamp::from_secs(2),
				..BlockContext::default()
			},
			whitelist: None,
		};
		let (after, result) = state.apply(&verified, &context);
		assert!(matches!(result, TransactionResult::Success(_)), "{result:?}");
		assert_eq!(after.utxo.utxos.size(), 2);
		assert_eq!(after.utxo.utxos.iter().map(|entry| entry.0.value).sum::<u128>(), 100);
		assert!(!after.utxo.utxos.contains_key(&original));
		assert_eq!(state.apply(&verified, &context).0.state_hash(), after.state_hash());
		let (repeated, result) =
			after.apply(&verified, &TransactionContext { ref_state: after.clone(), ..context });
		assert!(matches!(result, TransactionResult::Failure(_)));
		assert_eq!(repeated.state_hash(), after.state_hash());
		assert!(matches!(
			after.apply_system_tx(&issue, Timestamp::from_secs(10000)),
			Err(SystemTransactionError::EthereumDepositAlreadyPresent)
		));
	}

	#[test]
	fn solidity_registration_and_deposit_decode_with_the_pinned_sdk() {
		use snowbridge_inbound_queue_primitives::{
			Log,
			v2::{EthereumAsset, Message, Payload},
		};
		use sp_core::{H160, H256};
		let fixture = std::env::var("SNOWBRIDGE_DEPOSIT_LOGS")
			.map(|path| std::fs::read_to_string(path).unwrap())
			.unwrap_or_else(|_| include_str!("../../fixtures/deposit-logs.json").to_owned());
		let fixtures: serde_json::Value = serde_json::from_str(&fixture).unwrap();
		let decode_hex = |s: &str| hex::decode(s.trim_start_matches("0x")).unwrap();
		let messages: Vec<_> = fixtures
			.as_array()
			.unwrap()
			.iter()
			.map(|item| {
				let log = Log {
					address: H160::from_slice(&decode_hex(item["address"].as_str().unwrap())),
					topics: item["topics"]
						.as_array()
						.unwrap()
						.iter()
						.map(|v| H256::from_slice(&decode_hex(v.as_str().unwrap())))
						.collect(),
					data: decode_hex(item["data"].as_str().unwrap()),
					tx_index: u64::from_str_radix(
						item["transactionIndex"].as_str().unwrap().trim_start_matches("0x"),
						16,
					)
					.unwrap(),
				};
				Message::try_from(&log).unwrap()
			})
			.collect();
		assert_eq!(messages.len(), 2);
		assert!(matches!(&messages[0].payload, Payload::CreateAsset { .. }));
		assert_eq!(messages[0].assets.len(), 0);
		assert_eq!(messages[1].nonce, messages[0].nonce + 1);
		let Payload::Raw(envelope) = &messages[1].payload else { panic!("not a raw envelope") };
		assert_eq!(envelope.len(), 96);
		assert_eq!(&envelope[..31], &[0; 31]);
		assert_eq!(envelope[31], 1);
		assert_eq!(&envelope[64..95], &[0; 31]);
		assert_eq!(envelope[95], 7);
		assert_eq!(messages[1].assets.len(), 1);
		assert!(matches!(
			messages[1].assets[0],
			EthereumAsset::NativeTokenERC20 { value: 100, .. }
		));
		assert_eq!(messages[1].value, 0);
		assert_eq!(messages[1].execution_fee, 0);
		assert_eq!(messages[1].relayer_fee, 0);
		assert!(messages[1].claimer.is_none());
	}

	#[test]
	fn compact_color_matches_native_ledger_derivation() {
		let color = coin_structure::contract::ContractAddress(HashOutput([4; 32]))
			.custom_unshielded_token_type(HashOutput([3; 32]));
		assert_eq!(
			hex::encode(color.0.0),
			"1faaecfc787ac86a98332ef1e8f5905a540ff62d0ffb881899e0aa001191ce77"
		);
	}
}

#[cfg(all(test, feature = "compact-ledger"))]
mod compact_ledger;

#[cfg(all(test, feature = "compact-ledger"))]
mod live_ledger;
