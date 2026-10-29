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

use base_crypto::time::{Duration, Timestamp};
use ledger::{
	dust::{DustActions, DustOutput},
	semantics::{TransactionContext, TransactionResult},
	structure::{ContractAction, Intent, LedgerState, ProofPreimageMarker, Signature, Transaction},
	test_utilities::{TestState, test_resolver, tx_prove_bind},
	verify::WellFormedStrictness,
};
use onchain_runtime::context::BlockContext;
use rand::{SeedableRng, rngs::StdRng};
use storage::{arena::Sp, db::InMemoryDB};
use transient_crypto::commitment::PedersenRandomness;

#[tokio::test]
#[ignore = "requires fresh live escrow intent and SNOWBRIDGE_LIVE_LEDGER_DIR"]
async fn prepare_live_escrow_deployment_and_fee_genesis() {
	let dir = std::path::PathBuf::from(std::env::var("SNOWBRIDGE_LIVE_LEDGER_DIR").unwrap());
	let now = Timestamp::from_secs(
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_secs(),
	);
	let mut rng = StdRng::seed_from_u64(45);
	// Genesis-only development fee payer. No Ethereum asset or contract is seeded.
	let mut wallet = TestState::<InMemoryDB>::new(&mut rng);
	wallet.time = Timestamp::from_secs(
		now.to_secs()
			- u64::try_from(wallet.ledger.parameters.dust.time_to_cap().as_seconds()).unwrap()
			- 100,
	);
	let fee_outputs: usize = std::env::var("SNOWBRIDGE_FEE_OUTPUTS")
		.unwrap_or_else(|_| "1".into())
		.parse()
		.unwrap();
	assert!((1..=6).contains(&fee_outputs));
	wallet.give_fee_token(&mut rng, fee_outputs).await;
	wallet.fast_forward(Duration::from_secs(i128::from(now.to_secs() - wallet.time.to_secs())));
	let genesis = wallet.ledger.clone();
	assert_eq!(genesis.utxo.utxos.size(), fee_outputs);
	assert_eq!(genesis.ethereum_deposits.size(), 0);
	assert!(wallet.dust.wallet_balance(now) > 0);
	let mut bytes = vec![];
	serialize::tagged_serialize(&genesis, &mut bytes).unwrap();
	std::fs::write(dir.join("genesis-state.bin"), bytes).unwrap();
	std::fs::write(dir.join("genesis-block.json"), br#"{"batches":[]}"#).unwrap();
	// Reserve a separate genesis DUST output for each live transaction. All
	// spends are proved normally and cannot fund more than one transaction.
	let old_dust = wallet.dust.clone();
	let mut fees = vec![];
	for qdo in old_dust.utxos() {
		let generation = old_dust.generation_info(&qdo).unwrap();
		let value =
			DustOutput::from(qdo.clone()).updated_value(&generation, now, &genesis.parameters.dust);
		assert!(value > 0);
		let (next, spend) = wallet.dust.spend(&wallet.dust_key, &qdo, value, now).unwrap();
		wallet.dust = next;
		let mut fee_intent = Intent::empty(&mut rng, now + Duration::from_secs(3600));
		fee_intent.dust_actions = Some(Sp::new(DustActions {
			spends: vec![spend].into(),
			registrations: vec![].into(),
			ctime: now,
		}));
		let index = fees.len();
		let mut bytes = vec![];
		serialize::tagged_serialize(&fee_intent, &mut bytes).unwrap();
		std::fs::write(dir.join(format!("fee-{index}.bin")), bytes).unwrap();
		fees.push((fee_intent, value));
	}
	assert_eq!(fees.len(), fee_outputs);

	std::fs::write(
		dir.join("fee-values.json"),
		serde_json::to_vec(&fees.iter().map(|(_, v)| v.to_string()).collect::<Vec<_>>()).unwrap(),
	)
	.unwrap();
	if std::env::var("SNOWBRIDGE_GENESIS_ONLY").as_deref() == Ok("1") {
		return;
	}
	let (fee_intent, paid) = fees.remove(0);
	prove_escrow(&dir, genesis, fee_intent, paid, rng).await;
}

#[tokio::test]
#[ignore = "requires fresh fee genesis and escrow intent"]
async fn prove_live_escrow_from_existing_genesis() {
	let dir = std::path::PathBuf::from(std::env::var("SNOWBRIDGE_LIVE_LEDGER_DIR").unwrap());
	let bytes = std::fs::read(dir.join("genesis-state.bin")).unwrap();
	let genesis = serialize::tagged_deserialize(&bytes[..]).unwrap();
	let bytes = std::fs::read(dir.join("fee-0.bin")).unwrap();
	let fee = serialize::tagged_deserialize(&bytes[..]).unwrap();
	let values: Vec<String> =
		serde_json::from_slice(&std::fs::read(dir.join("fee-values.json")).unwrap()).unwrap();
	prove_escrow(&dir, genesis, fee, values[0].parse().unwrap(), StdRng::seed_from_u64(45)).await;
}
async fn prove_escrow(
	dir: &std::path::Path,
	genesis: LedgerState<InMemoryDB>,
	fee_intent: Intent<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>,
	paid: u128,
	rng: StdRng,
) {
	let bytes = std::fs::read(dir.join("escrow-deploy.bin")).unwrap();
	let intent: Intent<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
		serialize::tagged_deserialize(&bytes[..]).unwrap();
	let ContractAction::Deploy(deploy) = intent.actions.get(0).unwrap() else {
		panic!("expected deployment")
	};
	let address = deploy.address();
	let operation = deploy.initial_state.operations.get(&b"retire".to_vec().into()).unwrap();
	let operation_hash: [u8; 32] = operation.hash().0.into();
	let mut config: serde_json::Value =
		serde_json::from_slice(&std::fs::read(dir.join("registry.json")).unwrap()).unwrap();
	config["escrow"]["retireOperation"] = format!("0x{}", hex::encode(operation_hash)).into();
	assert_eq!(config["escrow"]["escrow"], format!("0x{}", hex::encode(address.0.0)));
	std::fs::write(dir.join("registry.json"), serde_json::to_vec_pretty(&config).unwrap()).unwrap();
	let tx = Transaction::from_intents("local-test", [(1, intent)].into_iter().collect());

	let fee_tx =
		Transaction::from_intents("local-test", [(0xfeed, fee_intent)].into_iter().collect());
	let tx = tx.merge(&fee_tx).unwrap();
	let proven = tx_prove_bind(rng, &tx, &test_resolver("escrow")).await.unwrap();
	let now = Timestamp::from_secs(
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_secs(),
	);
	assert!(proven.fees(&genesis.parameters, false).unwrap() <= paid);
	let verified = proven.well_formed(&genesis, WellFormedStrictness::default(), now).unwrap();
	let (state, result) = genesis.apply(
		&verified,
		&TransactionContext {
			ref_state: genesis.clone(),
			block_context: BlockContext { tblock: now, ..Default::default() },
			whitelist: None,
		},
	);
	assert!(matches!(result, TransactionResult::Success(_)), "{result:?}");
	assert!(state.index(address).is_some());
	let mut bytes = vec![];
	serialize::tagged_serialize(&proven, &mut bytes).unwrap();
	std::fs::write(dir.join("escrow-proven.bin"), bytes).unwrap();
}

#[test]
#[ignore = "requires finalized live issuance event"]
fn replay_live_native_issuance_event() {
	use ledger::structure::SystemTransaction;
	let dir = std::path::PathBuf::from(std::env::var("SNOWBRIDGE_LIVE_LEDGER_DIR").unwrap());
	let bytes = std::fs::read(dir.join("live-issuance.bin")).unwrap();
	let issue: SystemTransaction = serialize::tagged_deserialize(&bytes[..]).unwrap();
	let SystemTransaction::CreditEthereumAsset {
		deposit_id,
		output_id,
		token_type,
		amount,
		recipient,
	} = &issue
	else {
		panic!("wrong issuance variant")
	};
	let config: serde_json::Value =
		serde_json::from_slice(&std::fs::read(dir.join("registry.json")).unwrap()).unwrap();
	let evidence: serde_json::Value = serde_json::from_slice(
		&std::fs::read(dir.parent().unwrap().join("midnight-beacon-evidence.json")).unwrap(),
	)
	.unwrap();
	assert_eq!(*amount, 100);
	assert_eq!(format!("0x{}", hex::encode(deposit_id.0)), evidence["depositId"]);
	assert_eq!(format!("0x{}", hex::encode(token_type.0.0)), config["escrow"]["color"]);
	let bytes = std::fs::read(dir.join("genesis-state.bin")).unwrap();
	let genesis: LedgerState<InMemoryDB> = serialize::tagged_deserialize(&bytes[..]).unwrap();
	let issued_at = Timestamp::from_secs(
		evidence["issuanceTimestampMs"].as_str().unwrap().parse::<u64>().unwrap() / 1000,
	);
	let (state, _) = genesis.apply_system_tx(&issue, issued_at).unwrap();
	assert_eq!(state.utxo.utxos.size(), genesis.utxo.utxos.size() + 1);
	let utxo = state
		.utxo
		.utxos
		.iter()
		.find(|entry| entry.0.type_ == *token_type)
		.unwrap()
		.0
		.clone();
	assert_eq!(utxo.owner, *recipient);
	assert_eq!(utxo.intent_hash, *output_id);
	assert_eq!(utxo.type_, *token_type);
	assert_eq!(utxo.value, *amount);
	assert!(state.apply_system_tx(&issue, issued_at).is_err());
}

#[tokio::test]
#[ignore = "requires live issuance and prepared Compact intents"]
async fn prove_live_compact_round_trip() {
	use base_crypto::hash::HashOutput;
	use coin_structure::{
		coin::{TokenType, UnshieldedTokenType, UserAddress},
		contract::ContractAddress,
	};
	use ledger::{
		structure::{SigningKey, SystemTransaction, UnshieldedOffer, UtxoOutput, UtxoSpend},
		test_utilities::TxBound,
	};
	let dir = std::path::PathBuf::from(std::env::var("SNOWBRIDGE_LIVE_LEDGER_DIR").unwrap());
	let manifest: serde_json::Value =
		serde_json::from_slice(&std::fs::read(dir.join("compact-manifest.json")).unwrap()).unwrap();
	let hash = |key: &str| {
		HashOutput(hex::decode(manifest[key].as_str().unwrap()).unwrap().try_into().unwrap())
	};
	let owner = UserAddress(hash("owner"));
	let color = UnshieldedTokenType(hash("color"));
	let escrow = ContractAddress(hash("escrow"));
	let consumer = ContractAddress(hash("consumer"));
	let sk = SigningKey::Schnorr(base_crypto::schnorr::SigningKey::from_bytes(&[1; 32]).unwrap());
	assert_eq!(owner, UserAddress::from(sk.verifying_key()));
	let now = Timestamp::from_secs(
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_secs(),
	);
	let apply = |state: &LedgerState<InMemoryDB>, tx: &TxBound<Signature, InMemoryDB>| {
		let verified = tx.well_formed(state, WellFormedStrictness::default(), now).unwrap();
		let (next, result) = state.apply(
			&verified,
			&TransactionContext {
				ref_state: state.clone(),
				block_context: BlockContext { tblock: now, ..Default::default() },
				whitelist: None,
			},
		);
		assert!(matches!(result, TransactionResult::Success(_)), "{result:?}");
		next
	};
	let bytes = std::fs::read(dir.join("genesis-state.bin")).unwrap();
	let mut state: LedgerState<InMemoryDB> = serialize::tagged_deserialize(&bytes[..]).unwrap();
	let bytes = std::fs::read(dir.join("escrow-proven.bin")).unwrap();
	state = apply(&state, &serialize::tagged_deserialize(&bytes[..]).unwrap());
	let bytes = std::fs::read(dir.join("live-issuance.bin")).unwrap();
	let issue: SystemTransaction = serialize::tagged_deserialize(&bytes[..]).unwrap();
	state = state.apply_system_tx(&issue, now).unwrap().0;
	let mut rng = StdRng::seed_from_u64(46);
	for (index, (contract, operation)) in [
		("consumer", "deploy"),
		("consumer", "deposit"),
		("consumer", "balance"),
		("consumer", "withdraw"),
		("escrow", "retire"),
	]
	.into_iter()
	.enumerate()
	{
		let bytes = std::fs::read(dir.join(format!("{contract}-{operation}.bin"))).unwrap();
		let mut intent: Intent<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
			serialize::tagged_deserialize(&bytes[..]).unwrap();
		let mut inputs = vec![];
		let mut outputs = vec![];
		if operation == "deposit" || operation == "retire" {
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
		let bytes = std::fs::read(dir.join(format!("fee-{}.bin", index + 1))).unwrap();
		let fee: Intent<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
			serialize::tagged_deserialize(&bytes[..]).unwrap();
		let tx = Transaction::from_intents(
			"local-test",
			[(1, intent), (0xfeed, fee)].into_iter().collect(),
		);
		let proven = tx_prove_bind(rng.clone(), &tx, &test_resolver(contract)).await.unwrap();
		state = apply(&state, &proven);
		let mut bytes = vec![];
		serialize::tagged_serialize(&proven, &mut bytes).unwrap();
		std::fs::write(dir.join(format!("compact-proven-{index}.bin")), bytes).unwrap();
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
	assert_eq!(
		state
			.index(escrow)
			.unwrap()
			.balance
			.get(&TokenType::Unshielded(color))
			.map(|v| *v),
		Some(40)
	);
	assert!(!state.utxo.utxos.iter().any(|entry| entry.0.type_ == color));
	for (name, address) in [("consumer", consumer), ("escrow", escrow)] {
		let mut bytes = vec![];
		serialize::tagged_serialize(&state.index(address).unwrap(), &mut bytes).unwrap();
		std::fs::write(dir.join(format!("expected-{name}-state.bin")), bytes).unwrap();
	}
}

#[test]
#[ignore = "offline fee/proof preflight only; never evidence of authenticated issuance"]
fn write_synthetic_issuance_for_fee_preflight() {
	use base_crypto::hash::HashOutput;
	use coin_structure::coin::{UnshieldedTokenType, UserAddress};
	use ledger::structure::{IntentHash, SigningKey, SystemTransaction};
	let dir = std::path::PathBuf::from(std::env::var("SNOWBRIDGE_LIVE_LEDGER_DIR").unwrap());
	assert_eq!(dir.parent().unwrap().file_name().unwrap(), "compact-fee-preflight");
	let config: serde_json::Value =
		serde_json::from_slice(&std::fs::read(dir.join("registry.json")).unwrap()).unwrap();
	let color =
		hex::decode(config["escrow"]["color"].as_str().unwrap().trim_start_matches("0x")).unwrap();
	let sk = SigningKey::Schnorr(base_crypto::schnorr::SigningKey::from_bytes(&[1; 32]).unwrap());
	let issue = SystemTransaction::CreditEthereumAsset {
		deposit_id: HashOutput([42; 32]),
		output_id: IntentHash(HashOutput([42; 32])),
		token_type: UnshieldedTokenType(HashOutput(color.try_into().unwrap())),
		amount: 100,
		recipient: UserAddress::from(sk.verifying_key()),
	};
	let mut bytes = vec![];
	serialize::tagged_serialize(&issue, &mut bytes).unwrap();
	std::fs::write(dir.join("live-issuance.bin"), bytes).unwrap();
	std::fs::write(
		dir.join("OFFLINE-SYNTHETIC-ISSUANCE"),
		"Not a live issuance or Ethereum proof. Fee/proof preflight only.\n",
	)
	.unwrap();
}
