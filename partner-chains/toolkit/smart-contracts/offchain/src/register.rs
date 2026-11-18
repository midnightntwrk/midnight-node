use crate::cardano_keys::CardanoPaymentSigningKey;
use crate::csl::TransactionOutputAmountBuilderExt;
use crate::csl::{
	CostStore, Costs, InputsBuilderExt, OgmiosUtxoExt, TransactionBuilderExt, TransactionContext,
	unit_plutus_data,
};
use crate::{await_tx::AwaitTx, plutus_script::PlutusScript};
use anyhow::anyhow;
use cardano_serialization_lib::{
	Ed25519Signature, PlutusData, PublicKey, Transaction, TransactionBuilder,
	TransactionOutputBuilder, TxInputsBuilder,
};
use ogmios_client::{
	query_ledger_state::{QueryLedgerState, QueryUtxoByUtxoId},
	query_network::QueryNetwork,
	transactions::Transactions,
	types::OgmiosUtxo,
};
use partner_chains_plutus_data::registered_candidates::{
	RegisterValidatorDatum, candidate_registration_to_plutus_data,
};
use plutus::to_datum_cbor_bytes;
use plutus_datum_derive::ToDatum;
use sidechain_domain::*;

/// Maximum number of registration UTxOs consumed by a single registration or deregistration
/// transaction.
const MAX_REGISTRATION_UTXOS_PER_TX: usize = 10;

/// Message signed by the SPO's stake pool (cold) key when registering.
///
/// Its datum encoding has to match `authority_selection_inherents::RegisterValidatorSignedMessage`,
/// which the Partner Chain runtime validates registrations against.
#[derive(Debug, ToDatum)]
struct RegisterValidatorSignedMessage {
	genesis_utxo: UtxoId,
	sidechain_pub_key: SidechainPublicKey,
	registration_utxo: UtxoId,
}

/// Submits a transaction to register a registered candidate.
/// Arguments:
///  - `genesis_utxo`: UTxO identifying the Partner Chain.
///  - `candidate_registration`: [CandidateRegistration] registration data.
///  - `ogmios_client`: Ogmios client.
///  - `await_tx`: [AwaitTx] strategy.
pub async fn run_register<
	C: QueryLedgerState + QueryNetwork + QueryUtxoByUtxoId + Transactions,
	A: AwaitTx,
>(
	genesis_utxo: UtxoId,
	candidate_registration: &CandidateRegistration,
	payment_signing_key: &CardanoPaymentSigningKey,
	client: &C,
	await_tx: A,
) -> anyhow::Result<Option<McTxHash>> {
	let ctx = TransactionContext::for_payment_key(payment_signing_key, client).await?;
	let validator = crate::scripts_data::registered_candidates_scripts(genesis_utxo)?;
	let validator_address = validator.address_bech32(ctx.network)?;
	let registration_utxo = ctx
		.payment_key_utxos
		.iter()
		.find(|u| u.utxo_id() == candidate_registration.registration_utxo)
		.ok_or(anyhow!("registration utxo not found at payment address"))?;
	let all_registration_utxos = client.query_utxos(&[validator_address]).await?;
	let own_registrations = get_own_registrations(
		candidate_registration.own_pkh,
		candidate_registration.stake_ownership.pub_key.clone(),
		&all_registration_utxos,
	);

	if own_registrations.iter().any(|(_, existing_registration)| {
		candidate_registration.matches_keys(existing_registration)
	}) {
		log::info!("✅ Candidate already registered with same keys.");
		return Ok(None);
	}
	let own_registration_utxos =
		select_registration_utxos_to_spend(genesis_utxo, &own_registrations);

	let tx = Costs::calculate_costs(
		|costs| {
			register_tx(
				&validator,
				candidate_registration,
				registration_utxo,
				&own_registration_utxos,
				costs,
				&ctx,
			)
		},
		client,
	)
	.await?;

	let signed_tx = ctx.sign(&tx).to_bytes();
	let result = client.submit_transaction(&signed_tx).await.map_err(|e| {
		anyhow!(
			"Submit candidate registration transaction request failed: {}, bytes: {}",
			e,
			hex::encode(tx.to_bytes())
		)
	})?;
	let tx_id = result.transaction.id;
	log::info!("✅ Transaction submitted. ID: {}", hex::encode(result.transaction.id));
	await_tx.await_tx_output(client, McTxHash(tx_id)).await?;

	Ok(Some(McTxHash(result.transaction.id)))
}

/// Submits a transaction to register a registered candidate.
/// Arguments:
///  - `genesis_utxo`: UTxO identifying the Partner Chain.
///  - `stake_ownership_pub_key`: Stake pub key of the registered candidate to be deregistered.
///  - `await_tx`: [AwaitTx] strategy.
pub async fn run_deregister<
	C: QueryLedgerState + QueryNetwork + QueryUtxoByUtxoId + Transactions,
	A: AwaitTx,
>(
	genesis_utxo: UtxoId,
	payment_signing_key: &CardanoPaymentSigningKey,
	stake_ownership_pub_key: StakePoolPublicKey,
	client: &C,
	await_tx: A,
) -> anyhow::Result<Option<McTxHash>> {
	let ctx = TransactionContext::for_payment_key(payment_signing_key, client).await?;
	let validator = crate::scripts_data::registered_candidates_scripts(genesis_utxo)?;
	let validator_address = validator.address_bech32(ctx.network)?;
	let all_registration_utxos = client.query_utxos(&[validator_address]).await?;
	let own_registrations = get_own_registrations(
		payment_signing_key.to_pub_key_hash(),
		stake_ownership_pub_key.clone(),
		&all_registration_utxos,
	);

	if own_registrations.is_empty() {
		log::info!("✅ Candidate is not registered.");
		return Ok(None);
	}

	let own_registration_utxos =
		select_registration_utxos_to_spend(genesis_utxo, &own_registrations);

	let tx = Costs::calculate_costs(
		|costs| deregister_tx(&validator, &own_registration_utxos, costs, &ctx),
		client,
	)
	.await?;

	let signed_tx = ctx.sign(&tx).to_bytes();
	let result = client.submit_transaction(&signed_tx).await.map_err(|e| {
		anyhow!(
			"Submit candidate deregistration transaction request failed: {}, bytes: {}",
			e,
			hex::encode(tx.to_bytes())
		)
	})?;
	let tx_id = result.transaction.id;
	log::info!("✅ Transaction submitted. ID: {}", hex::encode(result.transaction.id));
	await_tx.await_tx_output(client, McTxHash(tx_id)).await?;

	Ok(Some(McTxHash(result.transaction.id)))
}

fn get_own_registrations(
	own_pkh: MainchainKeyHash,
	spo_pub_key: StakePoolPublicKey,
	validator_utxos: &[OgmiosUtxo],
) -> Vec<(OgmiosUtxo, CandidateRegistration)> {
	let mut own_registrations = Vec::new();
	for validator_utxo in validator_utxos {
		match get_candidate_registration(validator_utxo.clone()) {
			Ok(candidate_registration) => {
				if candidate_registration.stake_ownership.pub_key == spo_pub_key
					&& candidate_registration.own_pkh == own_pkh
				{
					own_registrations.push((validator_utxo.clone(), candidate_registration.clone()))
				}
			},
			Err(e) => log::debug!("Found invalid UTXO at validator address: {}", e),
		}
	}
	own_registrations
}

/// Verifies the stake pool (Cardano cold key) signature of a registration datum.
///
/// The fields [get_own_registrations] matches on are public data, and anyone can pay to the
/// validator address with an arbitrary datum, so an output matching them proves nothing: a third
/// party can plant outputs that look like registrations of any SPO. Only the stake pool signature
/// tells a genuine registration apart from a planted one - it is also what the Partner Chain
/// runtime validates registrations against.
fn has_valid_stake_pool_signature(
	genesis_utxo: UtxoId,
	registration: &CandidateRegistration,
) -> bool {
	let message = to_datum_cbor_bytes(RegisterValidatorSignedMessage {
		genesis_utxo,
		sidechain_pub_key: registration.partner_chain_pub_key.clone(),
		registration_utxo: registration.registration_utxo,
	});
	let Ok(pub_key) = PublicKey::from_bytes(&registration.stake_ownership.pub_key.0) else {
		return false;
	};
	let Ok(signature) =
		Ed25519Signature::from_bytes(registration.stake_ownership.signature.0.to_vec())
	else {
		return false;
	};
	pub_key.verify(&message, &signature)
}

/// Picks the registration UTxOs to be spent by a single transaction, bounded by
/// [MAX_REGISTRATION_UTXOS_PER_TX].
///
/// Planted outputs are spent like any other: the datum names this SPO as their owner, so it can
/// consume them, clearing them from the validator address and recovering the ADA they hold. They
/// are only put last, so that a bounded batch always takes the SPO's genuine registrations first
/// and the operation takes effect in a single transaction no matter how many outputs were
/// planted. The rest is cleaned up by re-running the command.
fn select_registration_utxos_to_spend(
	genesis_utxo: UtxoId,
	own_registrations: &[(OgmiosUtxo, CandidateRegistration)],
) -> Vec<OgmiosUtxo> {
	let mut utxos: Vec<(bool, OgmiosUtxo)> = own_registrations
		.iter()
		.map(|(utxo, registration)| {
			(!has_valid_stake_pool_signature(genesis_utxo, registration), utxo.clone())
		})
		.collect();
	utxos.sort_by_key(|(unsigned, utxo)| (*unsigned, utxo.transaction.id, utxo.index));
	let planted = utxos.iter().filter(|(unsigned, _)| *unsigned).count();
	if planted > 0 {
		log::info!(
			"Found {planted} UTXO(s) at the registration validator address that carry this SPO's keys but no valid stake pool signature. They will be spent to clear them from the validator address."
		);
	}
	if utxos.len() > MAX_REGISTRATION_UTXOS_PER_TX {
		log::warn!(
			"Found {} registration UTXOs for this SPO, consuming {MAX_REGISTRATION_UTXOS_PER_TX} of them in this transaction. Re-run the command to consume the rest.",
			utxos.len()
		);
		utxos.truncate(MAX_REGISTRATION_UTXOS_PER_TX);
	}
	utxos.into_iter().map(|(_, utxo)| utxo).collect()
}

fn get_candidate_registration(validator_utxo: OgmiosUtxo) -> anyhow::Result<CandidateRegistration> {
	let datum = validator_utxo.datum.ok_or_else(|| anyhow!("UTXO does not have a datum"))?;
	let datum_plutus_data = PlutusData::from_bytes(datum.bytes)
		.map_err(|e| anyhow!("Could not decode datum of validator script: {}", e))?;
	let register_validator_datum = RegisterValidatorDatum::try_from(datum_plutus_data)
		.map_err(|e| anyhow!("Could not decode datum of validator script: {}", e))?;
	Ok(register_validator_datum.into())
}

fn register_tx(
	validator: &PlutusScript,
	candidate_registration: &CandidateRegistration,
	registration_utxo: &OgmiosUtxo,
	own_registration_utxos: &[OgmiosUtxo],
	costs: Costs,
	ctx: &TransactionContext,
) -> anyhow::Result<Transaction> {
	let config = crate::csl::get_builder_config(ctx)?;
	let mut tx_builder = TransactionBuilder::new(&config);

	{
		let mut inputs = TxInputsBuilder::new();
		for own_registration_utxo in own_registration_utxos {
			let spend_cost = costs.get_spend_for_input(&own_registration_utxo.to_csl_tx_input())?;
			inputs.add_script_utxo_input(
				own_registration_utxo,
				validator,
				&register_redeemer_data(),
				&spend_cost,
			)?;
		}
		inputs.add_regular_inputs(&[registration_utxo.clone()])?;
		tx_builder.set_inputs(&inputs);
	}

	{
		let datum = candidate_registration_to_plutus_data(candidate_registration);
		let amount_builder = TransactionOutputBuilder::new()
			.with_address(&validator.address(ctx.network))
			.with_plutus_data(&datum)
			.next()?;
		let output = amount_builder.with_minimum_ada(ctx)?.build()?;
		tx_builder.add_output(&output)?;
	}

	Ok(tx_builder.balance_update_and_build(ctx)?)
}

fn deregister_tx(
	validator: &PlutusScript,
	own_registration_utxos: &[OgmiosUtxo],
	costs: Costs,
	ctx: &TransactionContext,
) -> anyhow::Result<Transaction> {
	let config = crate::csl::get_builder_config(ctx)?;
	let mut tx_builder = TransactionBuilder::new(&config);

	{
		let mut inputs = TxInputsBuilder::new();
		for own_registration_utxo in own_registration_utxos {
			let spend_cost = costs.get_spend_for_input(&own_registration_utxo.to_csl_tx_input())?;
			inputs.add_script_utxo_input(
				own_registration_utxo,
				validator,
				&register_redeemer_data(),
				&spend_cost,
			)?;
		}
		tx_builder.set_inputs(&inputs);
	}

	Ok(tx_builder.balance_update_and_build(ctx)?)
}

fn register_redeemer_data() -> PlutusData {
	unit_plutus_data()
}

#[cfg(test)]
mod tests {
	use super::{
		MAX_REGISTRATION_UTXOS_PER_TX, RegisterValidatorSignedMessage, deregister_tx, register_tx,
		select_registration_utxos_to_spend,
	};
	use crate::csl::{Costs, OgmiosUtxoExt, TransactionContext};
	use crate::test_values::{self, *};
	use cardano_serialization_lib::{
		Address, ExUnits, NetworkIdKind, PrivateKey, Transaction, TransactionInput,
		TransactionInputs,
	};
	use ogmios_client::types::OgmiosValue;
	use ogmios_client::types::{OgmiosTx, OgmiosUtxo};
	use partner_chains_plutus_data::registered_candidates::candidate_registration_to_plutus_data;
	use plutus::to_datum_cbor_bytes;
	use proptest::{
		array::uniform32,
		collection::{hash_set, vec},
		prelude::*,
	};
	use std::collections::HashMap;

	use sidechain_domain::{
		AdaBasedStaking, CandidateKeys, CandidateRegistration, MainchainKeyHash,
		MainchainSignature, McTxHash, SidechainPublicKey, SidechainSignature, StakePoolPublicKey,
		UtxoId, UtxoIndex,
	};

	fn sum_lovelace(utxos: &[OgmiosUtxo]) -> u64 {
		utxos.iter().map(|utxo| utxo.value.lovelace).sum()
	}

	const MIN_UTXO_LOVELACE: u64 = 1000000;
	const FIVE_ADA: u64 = 5000000;

	fn own_pkh() -> MainchainKeyHash {
		MainchainKeyHash([0; 28])
	}
	fn candidate_registration(registration_utxo: UtxoId) -> CandidateRegistration {
		CandidateRegistration {
			stake_ownership: AdaBasedStaking {
				pub_key: test_values::stake_pool_pub_key(),
				signature: MainchainSignature([0u8; 64]),
			},
			partner_chain_pub_key: SidechainPublicKey(Vec::new()),
			partner_chain_signature: SidechainSignature(Vec::new()),
			registration_utxo,
			own_pkh: own_pkh(),
			keys: CandidateKeys(vec![]),
		}
	}

	fn lesser_payment_utxo() -> OgmiosUtxo {
		make_utxo(1u8, 0, 1200000, &payment_addr())
	}

	fn greater_payment_utxo() -> OgmiosUtxo {
		make_utxo(4u8, 1, 1200001, &payment_addr())
	}

	fn registration_utxo() -> OgmiosUtxo {
		make_utxo(11u8, 0, 1000000, &payment_addr())
	}

	fn validator_addr() -> Address {
		Address::from_bech32("addr_test1wpha4546lvfcau5jsrwpht9h6350m3au86fev6nwmuqz9gqer2ung")
			.unwrap()
	}

	fn genesis_utxo() -> UtxoId {
		UtxoId { tx_hash: McTxHash([42u8; 32]), index: UtxoIndex(0) }
	}

	fn spo_signing_key() -> PrivateKey {
		PrivateKey::from_normal_bytes(&[7u8; 32]).unwrap()
	}

	/// Builds a registration signed with `spo_signing_key`, as a genuine registration would be.
	fn signed_candidate_registration(registration_utxo: UtxoId) -> CandidateRegistration {
		let key = spo_signing_key();
		let pub_key =
			StakePoolPublicKey(key.to_public().as_bytes().try_into().expect("ed25519 key is 32B"));
		let partner_chain_pub_key = SidechainPublicKey(vec![1u8; 33]);
		let message = to_datum_cbor_bytes(RegisterValidatorSignedMessage {
			genesis_utxo: genesis_utxo(),
			sidechain_pub_key: partner_chain_pub_key.clone(),
			registration_utxo,
		});
		let signature = MainchainSignature(
			key.sign(&message).to_bytes().try_into().expect("ed25519 signature is 64B"),
		);
		CandidateRegistration {
			stake_ownership: AdaBasedStaking { pub_key, signature },
			partner_chain_pub_key,
			partner_chain_signature: SidechainSignature(vec![2u8; 64]),
			registration_utxo,
			own_pkh: own_pkh(),
			keys: CandidateKeys(vec![]),
		}
	}

	fn utxo_with_registration(
		id_byte: u8,
		index: u16,
		registration: &CandidateRegistration,
	) -> OgmiosUtxo {
		let mut utxo = make_utxo(id_byte, index, 1_500_000, &validator_addr());
		utxo.datum = Some(candidate_registration_to_plutus_data(registration).to_bytes().into());
		utxo
	}

	fn ctx_with_payment_utxos() -> TransactionContext {
		TransactionContext {
			payment_key: payment_key(),
			payment_key_utxos: vec![
				lesser_payment_utxo(),
				greater_payment_utxo(),
				registration_utxo(),
			],
			network: NetworkIdKind::Testnet,
			protocol_parameters: protocol_parameters(),
			change_address: payment_addr(),
		}
	}

	fn ex_units(mem: u64, cpu: u64) -> ExUnits {
		ExUnits::new(&mem.into(), &cpu.into())
	}

	fn costs_for(spend_inputs: &[(TransactionInput, ExUnits)]) -> Costs {
		// `spends` is intentionally populated with plain 0..n indices: it is what the previous
		// implementation keyed on, and it does not match the redeemer indices of these inputs.
		let spends = spend_inputs
			.iter()
			.enumerate()
			.map(|(ix, (_, cost))| (ix as u32, cost.clone()))
			.collect::<HashMap<_, _>>();
		Costs::new_with_spend_inputs(HashMap::new(), spends, spend_inputs.iter().cloned().collect())
	}

	/// Asserts that every spend redeemer in `tx` carries the budget that was given for the input
	/// it points at, after CSL sorted the inputs and assigned the redeemer indices. Returns those
	/// redeemer indices, sorted.
	fn assert_budget_per_spend_redeemer(
		tx: &Transaction,
		expected: &HashMap<TransactionInput, ExUnits>,
	) -> Vec<u64> {
		let redeemers = tx.witness_set().redeemers().expect("script inputs produce redeemers");
		let inputs = tx.body().inputs();
		assert_eq!(redeemers.len(), expected.len());
		let mut indices = Vec::new();
		for i in 0..redeemers.len() {
			let redeemer = redeemers.get(i);
			let index: u64 = redeemer.index().into();
			let input = inputs.get(index as usize);
			let expected_budget = expected
				.get(&input)
				.unwrap_or_else(|| panic!("redeemer {index} points at a non-script input"));
			assert_eq!(&redeemer.ex_units(), expected_budget);
			indices.push(index);
		}
		indices.sort();
		indices
	}

	/// The script inputs must not sit at 0..n, otherwise a test would pass even for an
	/// implementation that assigns budgets by the order the inputs were added.
	fn assert_script_inputs_are_not_first(indices: &[u64]) {
		assert_ne!(
			indices,
			(0..indices.len() as u64).collect::<Vec<_>>(),
			"regular inputs should sort in between the script inputs"
		);
	}

	#[test]
	fn register_tx_regression_test() {
		let payment_key_utxos =
			vec![lesser_payment_utxo(), greater_payment_utxo(), registration_utxo()];
		let ctx = TransactionContext {
			payment_key: payment_key(),
			payment_key_utxos: payment_key_utxos.clone(),
			network: NetworkIdKind::Testnet,
			protocol_parameters: protocol_parameters(),
			change_address: payment_addr(),
		};
		let own_registration_utxos = vec![payment_key_utxos.get(1).unwrap().clone()];
		let registration_utxo = payment_key_utxos.first().unwrap();
		let candidate_registration = candidate_registration(registration_utxo.utxo_id());
		let tx = register_tx(
			&test_values::test_validator(),
			&candidate_registration,
			registration_utxo,
			&own_registration_utxos,
			Costs::ZeroCosts,
			&ctx,
		)
		.unwrap();

		let body = tx.body();
		let inputs = body.inputs();
		// Both inputs are used to cover transaction
		assert_eq!(
			inputs.get(0).to_string(),
			"0101010101010101010101010101010101010101010101010101010101010101#0"
		);
		assert_eq!(
			inputs.get(1).to_string(),
			"0404040404040404040404040404040404040404040404040404040404040404#1"
		);
		let outputs = body.outputs();

		let script_output = outputs.into_iter().find(|o| o.address() == validator_addr()).unwrap();
		let coins_sum = script_output.amount().coin().checked_add(&body.fee()).unwrap();
		assert_eq!(
			coins_sum,
			(greater_payment_utxo().value.lovelace + lesser_payment_utxo().value.lovelace).into()
		);
		assert_eq!(
			script_output.plutus_data().unwrap(),
			candidate_registration_to_plutus_data(&candidate_registration)
		);
	}

	fn register_transaction_balancing_test(payment_utxos: Vec<OgmiosUtxo>) {
		let payment_key_utxos = payment_utxos.clone();
		let ctx = TransactionContext {
			payment_key: payment_key(),
			payment_key_utxos: payment_key_utxos.clone(),
			network: NetworkIdKind::Testnet,
			protocol_parameters: protocol_parameters(),
			change_address: payment_addr(),
		};
		let registration_utxo = payment_key_utxos.first().unwrap();
		let candidate_registration = candidate_registration(registration_utxo.utxo_id());
		let own_registration_utxos = if payment_utxos.len() >= 2 {
			vec![payment_utxos.get(1).unwrap().clone()]
		} else {
			Vec::new()
		};
		let tx = register_tx(
			&test_values::test_validator(),
			&candidate_registration,
			registration_utxo,
			&own_registration_utxos,
			Costs::ZeroCosts,
			&ctx,
		)
		.unwrap();

		let validator_address = &test_values::test_validator().address(ctx.network);

		used_inputs_lovelace_equals_outputs_and_fee(&tx, &payment_key_utxos.clone());
		fee_is_less_than_one_and_half_ada(&tx);
		output_at_validator_has_register_candidate_datum(
			&tx,
			&candidate_registration,
			validator_address,
		);
		spends_own_registration_utxos(&tx, &own_registration_utxos);
	}

	fn match_inputs(inputs: &TransactionInputs, payment_utxos: &[OgmiosUtxo]) -> Vec<OgmiosUtxo> {
		inputs
			.into_iter()
			.map(|input| {
				payment_utxos
					.iter()
					.find(|utxo| utxo.to_csl_tx_input() == *input)
					.unwrap()
					.clone()
			})
			.collect()
	}

	fn used_inputs_lovelace_equals_outputs_and_fee(tx: &Transaction, payment_utxos: &[OgmiosUtxo]) {
		let used_inputs: Vec<OgmiosUtxo> = match_inputs(&tx.body().inputs(), payment_utxos);
		let used_inputs_value: u64 = sum_lovelace(&used_inputs);
		let outputs_lovelace_sum: u64 = tx
			.body()
			.outputs()
			.into_iter()
			.map(|output| {
				let value: u64 = output.amount().coin().into();
				value
			})
			.sum();
		let fee: u64 = tx.body().fee().into();
		// Used inputs are qual to the sum of the outputs plus the fee
		assert_eq!(used_inputs_value, outputs_lovelace_sum + fee);
	}

	// Exact fee depends on inputs and outputs, but it definately is less than 1.5 ADA
	fn fee_is_less_than_one_and_half_ada(tx: &Transaction) {
		assert!(tx.body().fee() <= 1500000u64.into());
	}

	fn output_at_validator_has_register_candidate_datum(
		tx: &Transaction,
		candidate_registration: &CandidateRegistration,
		validator_address: &Address,
	) {
		let outputs = tx.body().outputs();
		let validator_output =
			outputs.into_iter().find(|o| o.address() == *validator_address).unwrap();
		assert_eq!(
			validator_output.plutus_data().unwrap(),
			candidate_registration_to_plutus_data(candidate_registration)
		);
	}

	fn spends_own_registration_utxos(tx: &Transaction, own_registration_utxos: &[OgmiosUtxo]) {
		let inputs = tx.body().inputs();
		assert!(
			own_registration_utxos
				.iter()
				.all(|p| inputs.into_iter().any(|i| *i == p.to_csl_tx_input()))
		);
	}

	#[test]
	fn register_tx_assigns_the_budget_of_each_script_input() {
		let ctx = ctx_with_payment_utxos();
		let registration_utxo = lesser_payment_utxo();
		// Tx hashes 0x02 and 0x09 sort around the payment inputs (0x01, 0x04, 0x0b), so the two
		// script inputs do not end up at redeemer indices 0 and 1.
		let own_registration_utxos = vec![
			make_utxo(2u8, 0, 1_500_000, &validator_addr()),
			make_utxo(9u8, 3, 1_500_000, &validator_addr()),
		];
		let spend_inputs = vec![
			(own_registration_utxos[0].to_csl_tx_input(), ex_units(1000, 2000)),
			(own_registration_utxos[1].to_csl_tx_input(), ex_units(3000, 4000)),
		];
		let tx = register_tx(
			&test_values::test_validator(),
			&candidate_registration(registration_utxo.utxo_id()),
			&registration_utxo,
			&own_registration_utxos,
			costs_for(&spend_inputs),
			&ctx,
		)
		.expect("transaction with two script inputs is built");

		let indices = assert_budget_per_spend_redeemer(&tx, &spend_inputs.into_iter().collect());
		assert_script_inputs_are_not_first(&indices);
		spends_own_registration_utxos(&tx, &own_registration_utxos);
	}

	#[test]
	fn deregister_tx_assigns_the_budget_of_each_script_input() {
		let ctx = ctx_with_payment_utxos();
		// Small values, so that balancing has to pull in payment UTXOs which sort in between the
		// script inputs and shift their redeemer indices.
		let own_registration_utxos = vec![
			make_utxo(2u8, 0, 50_000, &validator_addr()),
			make_utxo(9u8, 3, 50_000, &validator_addr()),
			make_utxo(13u8, 1, 50_000, &validator_addr()),
		];
		let spend_inputs = vec![
			(own_registration_utxos[0].to_csl_tx_input(), ex_units(1000, 2000)),
			(own_registration_utxos[1].to_csl_tx_input(), ex_units(3000, 4000)),
			(own_registration_utxos[2].to_csl_tx_input(), ex_units(5000, 6000)),
		];
		let tx = deregister_tx(
			&test_values::test_validator(),
			&own_registration_utxos,
			costs_for(&spend_inputs),
			&ctx,
		)
		.expect("transaction with three script inputs is built");

		let indices = assert_budget_per_spend_redeemer(&tx, &spend_inputs.into_iter().collect());
		assert_script_inputs_are_not_first(&indices);
		spends_own_registration_utxos(&tx, &own_registration_utxos);
	}

	#[test]
	fn register_tx_errors_instead_of_panicking_on_missing_budget() {
		let ctx = ctx_with_payment_utxos();
		let registration_utxo = lesser_payment_utxo();
		let own_registration_utxos = vec![
			make_utxo(2u8, 0, 1_500_000, &validator_addr()),
			make_utxo(9u8, 3, 1_500_000, &validator_addr()),
		];
		// Evaluation result covers only one of the two script inputs.
		let spend_inputs =
			vec![(own_registration_utxos[0].to_csl_tx_input(), ex_units(1000, 2000))];

		let err = register_tx(
			&test_values::test_validator(),
			&candidate_registration(registration_utxo.utxo_id()),
			&registration_utxo,
			&own_registration_utxos,
			costs_for(&spend_inputs),
			&ctx,
		)
		.expect_err("missing budget is reported as an error");
		assert!(err.to_string().contains("no execution budget"), "unexpected error: {err}");
	}

	#[test]
	fn deregister_tx_errors_instead_of_panicking_on_missing_budget() {
		let ctx = ctx_with_payment_utxos();
		let own_registration_utxos = vec![
			make_utxo(2u8, 0, 1_500_000, &validator_addr()),
			make_utxo(9u8, 3, 1_500_000, &validator_addr()),
		];
		let spend_inputs =
			vec![(own_registration_utxos[1].to_csl_tx_input(), ex_units(1000, 2000))];

		let err = deregister_tx(
			&test_values::test_validator(),
			&own_registration_utxos,
			costs_for(&spend_inputs),
			&ctx,
		)
		.expect_err("missing budget is reported as an error");
		assert!(err.to_string().contains("no execution budget"), "unexpected error: {err}");
	}

	#[test]
	fn planted_outputs_are_spent_after_the_genuine_registration() {
		let genuine = signed_candidate_registration(registration_utxo().utxo_id());
		// Same public keys, but no valid stake pool signature - anyone can create this output.
		let mut planted = genuine.clone();
		planted.stake_ownership.signature = MainchainSignature([0u8; 64]);
		// The genuine registration sorts last by UTXO id, so only the signature can put it first.
		let own_registrations = vec![
			(utxo_with_registration(3u8, 0, &planted), planted.clone()),
			(utxo_with_registration(5u8, 0, &planted), planted.clone()),
			(utxo_with_registration(9u8, 0, &genuine), genuine),
		];

		let selected = select_registration_utxos_to_spend(genesis_utxo(), &own_registrations);

		// Everything is spent: planted outputs are cleared from the validator address and the ADA
		// they hold is recovered. They are only kept from crowding out the genuine registration.
		let selected_ids: Vec<UtxoId> = selected.iter().map(|u| u.utxo_id()).collect();
		assert_eq!(
			selected_ids,
			vec![
				own_registrations[2].0.utxo_id(),
				own_registrations[0].0.utxo_id(),
				own_registrations[1].0.utxo_id(),
			]
		);
	}

	#[test]
	fn selected_registration_utxos_are_bounded_and_keep_the_genuine_registration() {
		let genuine = signed_candidate_registration(registration_utxo().utxo_id());
		let mut planted = genuine.clone();
		planted.stake_ownership.signature = MainchainSignature([0u8; 64]);
		let mut own_registrations: Vec<(OgmiosUtxo, CandidateRegistration)> = (1..30u8)
			.map(|i| (utxo_with_registration(i, 0, &planted), planted.clone()))
			.collect();
		// Sorts last by UTXO id, so a bound applied to the plain id order would drop it.
		own_registrations.push((utxo_with_registration(30u8, 0, &genuine), genuine));

		let selected = select_registration_utxos_to_spend(genesis_utxo(), &own_registrations);

		assert_eq!(selected.len(), MAX_REGISTRATION_UTXOS_PER_TX);
		assert_eq!(selected[0].utxo_id(), own_registrations[29].0.utxo_id());
		let planted_ids: Vec<[u8; 32]> = selected[1..].iter().map(|u| u.transaction.id).collect();
		let mut sorted = planted_ids.clone();
		sorted.sort();
		assert_eq!(planted_ids, sorted);
		assert_eq!(planted_ids[0], [1u8; 32]);
	}

	proptest! {
		#[test]
		fn spends_input_utxo_and_outputs_to_validator_address(payment_utxos in arb_payment_utxos(10)
			.prop_filter("Inputs total lovelace too low", |utxos| sum_lovelace(utxos) > 4000000)) {
			register_transaction_balancing_test(payment_utxos)
		}
	}

	prop_compose! {
		// Set is needed to be used, because we have to avoid UTXOs with the same id.
		fn arb_payment_utxos(n: usize)
			(utxo_ids in hash_set(arb_utxo_id(), 1..n))
			(utxo_ids in Just(utxo_ids.clone()), values in vec(arb_utxo_lovelace(), utxo_ids.len())
		) -> Vec<OgmiosUtxo> {
			utxo_ids.into_iter().zip(values.into_iter()).map(|(utxo_id, value)| OgmiosUtxo {
				transaction: OgmiosTx { id: utxo_id.tx_hash.0 },
				index: utxo_id.index.0,
				value,
				address: PAYMENT_ADDR.into(),
				..Default::default()
			}).collect()
		}
	}

	prop_compose! {
		fn arb_utxo_lovelace()(value in MIN_UTXO_LOVELACE..FIVE_ADA) -> OgmiosValue {
			OgmiosValue::new_lovelace(value)
		}
	}

	prop_compose! {
		fn arb_utxo_id()(tx_hash in uniform32(0u8..255u8), index in any::<u16>()) -> UtxoId {
			UtxoId {
				tx_hash: McTxHash(tx_hash),
				index: UtxoIndex(index),
			}
		}
	}
}
