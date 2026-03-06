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

use crate::{
	SeedableRng, Spin, StdRng,
	cli_parsers::{self as cli},
	remote_prover::RemoteProofServer,
	t_token,
};
use midnight_node_ledger_helpers::fork::raw_block_data::{SerializedTx, SerializedTxBatches};
use midnight_node_ledger_helpers::{
	Transaction as MNLedgerTransaction, fork::raw_block_data::RawTransaction, *,
};

use thiserror::Error;

// Re-export ICS types from the primitives crate
pub use midnight_primitives_ics_observation::{IcsAsset, IcsConfig, IcsUtxo};
pub use midnight_primitives_reserve_observation::ReserveConfig;

pub const MINT_AMOUNT: u128 = 50_000_000_000_000;
pub const GENESIS_NONCE_SEED: &str =
	"0000000000000000000000000000000000000000000000000000000000000037";

type IntentsMap = HashMapStorage<
	u16,
	Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>,
	DefaultDB,
>;
type ShieldedOffer = Offer<ProofPreimage, DefaultDB>;

type Transaction = MNLedgerTransaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;

#[derive(Debug, Error)]
pub enum GenesisGeneratorError<D: DB> {
	#[error("Error validating System tx: {0:?}")]
	GenesisSystemValidation(#[from] SystemTransactionError),
	#[error("Error validating Genesis tx: {0:?}")]
	GenesisValidation(#[from] MalformedTransaction<D>),
	#[error("Partial success generating Genesis: {0}")]
	TxPartialSuccess(String),
	#[error("Failure generating Genesis: {0:?}")]
	TxFailed(TransactionInvalid<D>),
	#[error("Error calculating fees: {0:?}")]
	FeeCalculationError(#[from] FeeCalculationError),
	#[error("Failure applying block: {0:?}")]
	BlockLimitExceeded(#[from] BlockLimitExceeded),
	#[error("Error serializing transaction: {0}")]
	SerializationError(#[from] std::io::Error),
	#[error("Missing verifying key for wallet")]
	MissingVerifyingKey,
}

/// Common arguments for funding wallets (shielded, unshielded, dust)
#[derive(Debug, Clone, clap::Args)]
pub struct FundingArgs {
	/// Mint amount per output
	#[arg(long, default_value_t = MINT_AMOUNT)]
	shielded_mint_amount: u128,
	/// Number of funding outputs
	#[arg(long, default_value = "5")]
	shielded_num_funding_outputs: usize,
	/// Alternative token types
	#[arg(
        long,
        value_parser = cli::token_decode::<ShieldedTokenType>,
        default_values = [
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000002"
        ]
    )]
	shielded_alt_token_types: Vec<ShieldedTokenType>,
	/// Mint amount per output
	#[arg(long, default_value_t = MINT_AMOUNT)]
	unshielded_mint_amount: u128,
	/// Number of funding outputs
	#[arg(long, default_value = "5")]
	unshielded_num_funding_outputs: usize,
	/// Alternative token types
	#[arg(
        long,
        value_parser = cli::token_decode::<UnshieldedTokenType>,
		/*
        default_values = [
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000002"
        ]
		 */
    )]
	unshielded_alt_token_types: Vec<UnshieldedTokenType>,
}

pub struct GenesisGenerator {
	pub state: LedgerState<DefaultDB>,
	pub txs: SerializedTxBatches,
	fullness: SyntheticCost,
}

const GLACIER_DROP_START_UNIX_EPOC: u64 = 1754395200;
const BEGINNING: Timestamp = Timestamp::from_secs(GLACIER_DROP_START_UNIX_EPOC);

// Provisional hardcoded expected values until transfers from iterim ICS to new ICS happens
const EXPECTED_RESERVE_VALUE: u128 = 6000000000873988; // STARS
const EXPECTED_ICS_VALUE: u128 = 1200000000000000; // STARS

type Result<T, E = GenesisGeneratorError<DefaultDB>> = std::result::Result<T, E>;

impl GenesisGenerator {
	#[allow(clippy::too_many_arguments)]
	pub async fn new(
		seed: [u8; 32],
		network_id: &str,
		proof_server: Option<String>,
		funding: FundingArgs,
		seeds: Option<&[WalletSeed]>,
		cnight_system_tx: Option<SystemTransaction>,
		_ics_config: Option<IcsConfig>,
		_reserve_config: Option<ReserveConfig>,
		ledger_parameters: Option<LedgerParameters>,
	) -> Result<Self> {
		// TODO: Uncomment after transfers from iterim ICS to new ICS happens
		// let reserve_pool = reserve_config.as_ref().map(|c| c.total_amount).unwrap_or(0);
		// let treasury = ics_config.as_ref().map(|c| c.total_amount).unwrap_or(0);

		// Provisional hardcoded expected values until transfers from iterim ICS to new ICS happens
		let reserve_pool = EXPECTED_RESERVE_VALUE;
		let treasury = EXPECTED_ICS_VALUE;
		let locked_pool = MAX_SUPPLY - reserve_pool - treasury;

		// If custom ledger parameters are provided, apply them first
		let original_parameters =
			if let Some(params) = ledger_parameters { params } else { INITIAL_PARAMETERS };

		let state = LedgerState::with_genesis_settings(
			network_id,
			original_parameters.clone(),
			locked_pool,
			reserve_pool,
			treasury,
		)
		.map_err(SystemTransactionError::from)?;
		let mut me = Self {
			state,
			txs: SerializedTxBatches { batches: vec![vec![]] },
			fullness: SyntheticCost::ZERO,
		};
		me.init(
			seed,
			network_id,
			proof_server,
			&funding,
			seeds,
			cnight_system_tx,
			original_parameters,
		)
		.await?;
		Ok(me)
	}

	#[allow(clippy::too_many_arguments)]
	async fn init(
		&mut self,
		seed: [u8; 32],
		network_id: &str,
		proof_server: Option<String>,
		funding: &FundingArgs,
		seeds: Option<&[WalletSeed]>,
		cnight_system_tx: Option<SystemTransaction>,
		original_parameters: LedgerParameters,
	) -> Result<(), GenesisGeneratorError<DefaultDB>> {
		let wallets: Vec<Wallet<DefaultDB>> = seeds
			.map(|s| s.iter().cloned().map(|seed| Wallet::default(seed, &self.state)).collect())
			.unwrap_or_default();

		// Source of randomness
		let mut rng = StdRng::from_seed(seed);

		let genesis_block_context = BlockContext {
			tblock: BEGINNING,
			tblock_err: 30,
			parent_block_hash: HashOutput::default(),
			last_block_time: BEGINNING,
		};

		// Only fund faucet wallets if seeds were provided
		if !wallets.is_empty() {
			// Distribute NIGHT as rewards to all wallets
			self.distribute_night(&genesis_block_context, funding, &wallets, &mut rng)?;

			// Set fees to zero to simplify setup logic.
			// This lets us claim the full requested amount of NIGHT,
			// and register DUST addresses without waiting for DUST to accumulate.
			let no_fee_parameters = without_fees(&original_parameters);
			self.set_parameters(no_fee_parameters, &genesis_block_context)?;

			// Register DUST addresses for our wallets
			self.register_dust_addresses(
				&genesis_block_context,
				funding,
				wallets.clone(),
				&mut rng,
				network_id,
				proof_server,
			)
			.await?;

			// Make our wallets claim their rewards; now they have NIGHT
			self.claim_rewards(&genesis_block_context, funding, &wallets, &mut rng)?;

			// Restore fees now that we've finished.
			self.set_parameters(original_parameters, &genesis_block_context)?;
		}

		if let Some(system_tx) = cnight_system_tx {
			self.apply_system_tx(system_tx.clone(), &genesis_block_context)?;
			log::info!("cNight System Tx applied: {:?}", system_tx);
		}

		let block_limits = self.state.parameters.limits.block_limits;
		let normalized_fullness =
			clamp_and_normalize(&self.fullness, &block_limits, "genesis_generator");
		let overall_fullness = compute_overall_fullness(&normalized_fullness);
		self.state = self.state.post_block_update(
			genesis_block_context.tblock,
			normalized_fullness,
			overall_fullness,
		)?;
		Ok(())
	}

	fn distribute_night(
		&mut self,
		block_context: &BlockContext,
		funding: &FundingArgs,
		wallets: &[Wallet<DefaultDB>],
		rng: &mut StdRng,
	) -> Result<(), GenesisGeneratorError<DefaultDB>> {
		// In the initial ledger state, the reserve pool is full of NIGHT.
		// Move any that we want to distribute into the reward pool.
		let sys_tx_distribute = SystemTransaction::DistributeReserve(
			funding.unshielded_mint_amount
				* funding.unshielded_num_funding_outputs as u128
				* wallets.len() as u128,
		);
		self.apply_system_tx(sys_tx_distribute, block_context)?;

		// And now reward it to each wallet.
		let mut night_distribution_instructions = vec![];
		for wallet in wallets.iter() {
			let target_address = wallet
				.unshielded
				.verifying_key
				.clone()
				.ok_or(GenesisGeneratorError::MissingVerifyingKey)?
				.into();
			for _ in 0..funding.unshielded_num_funding_outputs {
				night_distribution_instructions.push(OutputInstructionUnshielded {
					amount: funding.unshielded_mint_amount,
					target_address,
					nonce: rng.r#gen(),
				});
			}
		}
		let sys_tx_rewards =
			SystemTransaction::DistributeNight(ClaimKind::Reward, night_distribution_instructions);
		self.apply_system_tx(sys_tx_rewards, block_context)?;
		Ok(())
	}

	fn set_parameters(
		&mut self,
		parameters: LedgerParameters,
		block_context: &BlockContext,
	) -> Result<()> {
		let sys_tx_params = SystemTransaction::OverwriteParameters(parameters);
		self.apply_system_tx(sys_tx_params, block_context)
	}

	fn claim_rewards(
		&mut self,
		block_context: &BlockContext,
		funding: &FundingArgs,
		wallets: &[Wallet<DefaultDB>],
		rng: &mut StdRng,
	) -> Result<()> {
		for wallet in wallets {
			for _ in 0..funding.unshielded_num_funding_outputs {
				let claim_tx =
					self.build_claim_rewards_tx(wallet, funding.unshielded_mint_amount, rng);
				self.apply_standard_tx(claim_tx, block_context)?;
			}
		}
		Ok(())
	}

	fn build_claim_rewards_tx(
		&self,
		wallet: &Wallet<DefaultDB>,
		rewards: u128,
		rng: &mut StdRng,
	) -> Transaction {
		let unsigned_claim: ClaimRewardsTransaction<(), DefaultDB> = ClaimRewardsTransaction {
			network_id: self.state.network_id.clone(),
			value: rewards,
			owner: wallet.unshielded.verifying_key.clone().unwrap(),
			nonce: rng.r#gen(),
			signature: (),
			kind: ClaimKind::Reward,
		};
		let signature = wallet.unshielded.signing_key().sign(rng, &unsigned_claim.data_to_sign());
		let signed_claim = ClaimRewardsTransaction {
			network_id: unsigned_claim.network_id,
			value: unsigned_claim.value,
			owner: unsigned_claim.owner,
			nonce: unsigned_claim.nonce,
			signature,
			kind: unsigned_claim.kind,
		};
		Transaction::ClaimRewards(signed_claim)
	}

	async fn register_dust_addresses(
		&mut self,
		block_context: &BlockContext,
		funding: &FundingArgs,
		wallets: Vec<Wallet<DefaultDB>>,
		rng: &mut StdRng,
		network: &str,
		proof_server: Option<String>,
	) -> Result<()> {
		// Generate Shielded Offer
		let guaranteed_shielded_offer = Self::shielded_offer(&wallets, network, &funding, rng);
		let fallible_coins = HashMapStorage::new();

		// Generate Unshielded Offer
		let guaranteed_unshielded_offer = Self::unshielded_offer(&wallets, network, funding);

		let mut intent = Intent::<Signature, _, _, _>::empty(rng, block_context.tblock);
		intent.guaranteed_unshielded_offer = Some(guaranteed_unshielded_offer);
		Self::add_dust_actions(
			&mut intent,
			wallets,
			Segment::Fallible.into(),
			rng,
			block_context.tblock,
		);

		let intents = IntentsMap::new().insert(Segment::Fallible.into(), intent);

		let genesis_tx = self
			.run_proof(
				network,
				proof_server,
				intents,
				guaranteed_shielded_offer,
				fallible_coins,
				rng.split(),
			)
			.await;
		self.apply_standard_tx(genesis_tx, &block_context)?;

		Ok(())
	}

	// returns a transaction that underwent proving.
	async fn run_proof(
		&self,
		network_id: &str,
		proof_server: Option<String>,
		intents: IntentsMap,
		guaranteed_shielded_offer: Option<ShieldedOffer>,
		fallible_coins: HashMapStorage<u16, ShieldedOffer, DefaultDB>,
		rng: StdRng,
	) -> Transaction {
		let spin = Spin::new("proving genesis transaction...");
		let unproven_tx = MNLedgerTransaction::new(
			network_id,
			intents,
			guaranteed_shielded_offer,
			fallible_coins,
		);

		let proof_server: Box<dyn ProofProvider<DefaultDB>> = if let Some(url) = proof_server {
			Box::new(RemoteProofServer::new(url))
		} else {
			Box::new(LocalProofServer::new())
		};

		let genesis_tx = proof_server
			.prove(
				unproven_tx,
				rng.clone(),
				&DEFAULT_RESOLVER,
				&self.state.parameters.cost_model.runtime_cost_model,
			)
			.await;

		let sealed_genesis_tx = genesis_tx.seal(rng);

		spin.finish("genesis transaction proved.");

		sealed_genesis_tx
	}

	fn shielded_offer(
		wallets: &[Wallet<DefaultDB>],
		network: &str,
		funding: &FundingArgs,
		rng: &mut StdRng,
	) -> Option<ShieldedOffer> {
		let FundingArgs {
			shielded_num_funding_outputs,
			shielded_mint_amount,
			shielded_alt_token_types,
			..
		} = funding;

		if *shielded_mint_amount == 0 {
			// not minting any shielded tokens
			return None;
		}

		let mut outputs = vec![];

		for wallet in wallets {
			let wallet = &wallet.shielded;
			//TODO: 0th shielded token type isn't special so why special case it.
			for _i in 0..*shielded_num_funding_outputs {
				let coin = CoinInfo::new(rng, *shielded_mint_amount, t_token());
				let out = Output::new::<_>(
					rng,
					&coin,
					Segment::Guaranteed.into(),
					&wallet.coin_public_key,
					Some(wallet.enc_public_key),
				)
				.unwrap_or_else(|err| panic!("Error creating Output in Genesis: {:?}", err));
				outputs.push(out);
			}

			// Test tokens
			for token_type in shielded_alt_token_types {
				let coin = CoinInfo::new(rng, *shielded_mint_amount, *token_type);
				let out = Output::new::<_>(
					rng,
					&coin,
					Segment::Guaranteed.into(),
					&wallet.coin_public_key,
					Some(wallet.enc_public_key),
				)
				.unwrap_or_else(|err| panic!("Error creating Output in Genesis: {:?}", err));
				outputs.push(out);
			}

			log::info!(
				"generated {} outputs for wallet {:?}",
				shielded_num_funding_outputs + shielded_alt_token_types.len(),
				wallet.address(network).to_bech32(),
			);
		}

		let mut deltas = vec![Delta {
			token_type: t_token(),
			value: -((shielded_mint_amount
				* *shielded_num_funding_outputs as u128
				* wallets.len() as u128) as i128),
		}];

		for token_type in shielded_alt_token_types {
			deltas.push(Delta {
				token_type: *token_type,
				value: -((shielded_mint_amount * wallets.len() as u128) as i128),
			});
		}

		// Create unbalanced offer - no inputs
		let mut guaranteed_offer = Offer {
			inputs: Array::new(),
			outputs: outputs.into(),
			transient: Array::new(),
			deltas: deltas.into(),
		};
		guaranteed_offer.normalize();

		Some(guaranteed_offer)
	}

	fn unshielded_offer(
		wallets: &[Wallet<DefaultDB>],
		network: &str,
		funding: &FundingArgs,
	) -> Sp<UnshieldedOffer<Signature, DefaultDB>, DefaultDB> {
		let FundingArgs { unshielded_mint_amount, unshielded_alt_token_types, .. } = funding;

		let inputs = vec![];
		let mut outputs = vec![];

		for wallet in wallets {
			let wallet = &wallet.unshielded;

			// Test tokens
			for token_type in unshielded_alt_token_types {
				let out = UtxoOutput {
					value: *unshielded_mint_amount,
					owner: wallet.user_address,
					type_: *token_type,
				};
				outputs.push(out);
			}

			log::info!(
				"generated {} outputs for wallet {:?}",
				unshielded_alt_token_types.len(),
				wallet.address(network).to_bech32(),
			);
		}

		outputs.sort();

		let offer = UnshieldedOffer {
			inputs: inputs.into(),
			outputs: outputs.into(),
			signatures: Array::new(),
		};

		Sp::new(offer)
	}

	fn add_dust_actions(
		intent: &mut Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>,
		wallets: impl IntoIterator<Item = Wallet<DefaultDB>>,
		segment_id: u16,
		rng: &mut StdRng,
		timestamp: Timestamp,
	) {
		let data_to_sign = intent.erase_proofs().erase_signatures().data_to_sign(segment_id);
		let mut registrations = vec![];
		for wallet in wallets {
			let signature = wallet.unshielded.signing_key().sign(rng, &data_to_sign);
			let night_key = wallet.unshielded.verifying_key.unwrap();
			let dust_address = wallet.dust.public_key;
			registrations.push(DustRegistration {
				night_key,
				dust_address: Some(Sp::new(dust_address)),
				allow_fee_payment: 0,
				signature: Some(Sp::new(signature)),
			});
		}
		if registrations.is_empty() {
			return;
		}
		let dust_actions = DustActions {
			spends: Array::new(),
			registrations: registrations.into(),
			ctime: timestamp,
		};
		intent.dust_actions = Some(Sp::new(dust_actions));
	}

	fn apply_standard_tx(&mut self, tx: Transaction, block_context: &BlockContext) -> Result<()> {
		let tx_context = TransactionContext {
			ref_state: self.state.clone(),
			block_context: block_context.clone(),
			whitelist: None,
		};

		let strictness: WellFormedStrictness =
			if block_context.parent_block_hash == Default::default() {
				let mut lax: WellFormedStrictness = Default::default();
				lax.enforce_balancing = false;
				lax
			} else {
				Default::default()
			};

		let valid_tx =
			tx.well_formed(&tx_context.ref_state, strictness, tx_context.block_context.tblock)?;
		self.fullness = self.fullness + tx.cost(&self.state.parameters, false)?;
		let (state, result) = self.state.apply(&valid_tx, &tx_context);
		match result {
			TransactionResult::Success(_) => {
				self.state = state;
				let tx_hash = tx.transaction_hash().0.0;
				let raw_tx = RawTransaction::Midnight(serialize(&tx)?);
				self.txs.batches[0].push(SerializedTx {
					tx: raw_tx,
					context: block_context.clone(),
					tx_hash,
				});
				Ok(())
			},
			TransactionResult::PartialSuccess(failures, _) => {
				Err(GenesisGeneratorError::TxPartialSuccess(format!("{failures:?}")))
			},
			TransactionResult::Failure(failures) => Err(GenesisGeneratorError::TxFailed(failures)),
		}
	}

	fn apply_system_tx(
		&mut self,
		tx: SystemTransaction,
		block_context: &BlockContext,
	) -> Result<()> {
		self.fullness = self.fullness + tx.cost(&self.state.parameters);
		let (state, _) = self.state.apply_system_tx(&tx, block_context.tblock)?;
		self.state = state;
		let tx_hash = tx.transaction_hash().0.0;
		let raw_tx = RawTransaction::System(serialize(&tx)?);
		self.txs.batches[0].push(SerializedTx {
			tx: raw_tx,
			context: block_context.clone(),
			tx_hash,
		});
		Ok(())
	}
}

fn without_fees(params: &LedgerParameters) -> LedgerParameters {
	LedgerParameters {
		fee_prices: FeePrices {
			overall_price: FixedPoint::ZERO,
			read_factor: FixedPoint::ONE,
			compute_factor: FixedPoint::ONE,
			block_usage_factor: FixedPoint::ONE,
			write_factor: FixedPoint::ONE,
		},
		..params.clone()
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use midnight_primitives_ics_observation::PolicyId;
	use std::{collections::HashMap, str::FromStr};

	#[tokio::test]
	async fn test_genesis_with_ics_config() {
		let funding = FundingArgs {
			shielded_mint_amount: 0,
			shielded_num_funding_outputs: 0,
			shielded_alt_token_types: vec![],
			unshielded_mint_amount: MINT_AMOUNT,
			unshielded_num_funding_outputs: 5,
			unshielded_alt_token_types: vec![],
		};

		let seed = hex::decode(GENESIS_NONCE_SEED).unwrap().try_into().unwrap();
		let network_id = "undeployed";
		let proof_server = None;
		let seeds = [
			"0000000000000000000000000000000000000000000000000000000000000001",
			"0000000000000000000000000000000000000000000000000000000000000002",
		]
		.map(|seed| WalletSeed::try_from_hex_str(seed).unwrap())
		.to_vec();

		// ICS config is currently ignored (hardcoded values used instead),
		// but we still pass it to exercise the code path.
		let ics_config = IcsConfig {
			illiquid_circulation_supply_validator_address:
				"addr_test1wqgdspp2cnethukgvrve6wnue8adjjzz5ty9x3z4t5s8c8cnck7xz".to_string(),
			asset: IcsAsset {
				policy_id: PolicyId::from_str(
					"d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1",
				)
				.expect("valid policy ID"),
				asset_name: "".to_string(),
			},
			utxos: vec![
				IcsUtxo { tx_hash: "abc123".to_string(), output_index: 0, amount: 600_000_000_000 },
				IcsUtxo { tx_hash: "def456".to_string(), output_index: 1, amount: 400_000_000_000 },
			],
			total_amount: 1_000_000_000_000,
		};

		ics_config.validate().expect("ICS config should be valid");

		let genesis = GenesisGenerator::new(
			seed,
			network_id,
			proof_server,
			funding,
			Some(&seeds),
			None,
			Some(ics_config),
			None, // no reserve config
			None, // no custom ledger parameters
		)
		.await
		.unwrap();

		// Treasury uses the hardcoded EXPECTED_ICS_VALUE (not the config value)
		let night_token_type = TokenType::Unshielded(NIGHT);
		let treasury_balance = genesis.state.treasury.get(&night_token_type).copied().unwrap_or(0);
		assert_eq!(
			treasury_balance, EXPECTED_ICS_VALUE,
			"Treasury should contain {} NIGHT, but has {}",
			EXPECTED_ICS_VALUE, treasury_balance
		);
	}

	#[tokio::test]
	async fn test_genesis_with_reserve_config() {
		let funding = FundingArgs {
			shielded_mint_amount: 0,
			shielded_num_funding_outputs: 0,
			shielded_alt_token_types: vec![],
			unshielded_mint_amount: 0,
			unshielded_num_funding_outputs: 0,
			unshielded_alt_token_types: vec![],
		};

		let seed = hex::decode(GENESIS_NONCE_SEED).unwrap().try_into().unwrap();
		let network_id = "undeployed";

		// Reserve config is currently ignored (hardcoded values used instead),
		// but we still pass it to exercise the code path.
		let reserve_config = ReserveConfig {
			reserve_validator_address: "addr_test1qz_reserve".to_string(),
			asset: midnight_primitives_reserve_observation::ReserveAsset {
				policy_id: midnight_primitives_reserve_observation::PolicyId([0u8; 28]),
				asset_name: "NIGHT".to_string(),
			},
			utxos: vec![
				midnight_primitives_reserve_observation::ReserveUtxo {
					tx_hash: "abc123".to_string(),
					output_index: 0,
					amount: 3_000_000_000_000,
				},
				midnight_primitives_reserve_observation::ReserveUtxo {
					tx_hash: "def456".to_string(),
					output_index: 1,
					amount: 2_000_000_000_000,
				},
			],
			total_amount: 5_000_000_000_000,
		};

		reserve_config.validate().expect("Reserve config should be valid");

		let genesis = GenesisGenerator::new(
			seed,
			network_id,
			None,
			funding,
			None, // no wallets — keeps pool accounting simple
			None,
			None,
			Some(reserve_config),
			None,
		)
		.await
		.unwrap();

		// Pools use hardcoded values, not the reserve config
		let expected_locked = MAX_SUPPLY - EXPECTED_RESERVE_VALUE - EXPECTED_ICS_VALUE;
		assert_eq!(
			genesis.state.locked_pool, expected_locked,
			"locked_pool should be MAX_SUPPLY minus reserve and ICS expected values"
		);
		assert_eq!(
			genesis.state.reserve_pool, EXPECTED_RESERVE_VALUE,
			"reserve_pool should equal EXPECTED_RESERVE_VALUE"
		);
	}

	#[tokio::test]
	async fn test_genesis_state() {
		let funding = FundingArgs {
			shielded_mint_amount: 0,
			shielded_num_funding_outputs: 0,
			shielded_alt_token_types: vec![],
			unshielded_mint_amount: MINT_AMOUNT,
			unshielded_num_funding_outputs: 5,
			unshielded_alt_token_types: vec![],
		};

		let seed = hex::decode(GENESIS_NONCE_SEED).unwrap().try_into().unwrap();
		let network_id = "undeployed";
		let proof_server = None;
		let seeds = [
			"0000000000000000000000000000000000000000000000000000000000000001",
			"0000000000000000000000000000000000000000000000000000000000000002",
			"0000000000000000000000000000000000000000000000000000000000000003",
			"0000000000000000000000000000000000000000000000000000000000000004",
		]
		.map(|seed| WalletSeed::try_from_hex_str(seed).unwrap())
		.to_vec();

		let genesis = GenesisGenerator::new(
			seed,
			network_id,
			proof_server,
			funding,
			Some(&seeds),
			None,
			None,
			None,
			None,
		)
		.await
		.unwrap();

		let wallets = seeds
			.iter()
			.map(|seed| Wallet::default(*seed, &genesis.state))
			.collect::<Vec<_>>();

		let state = genesis.state;
		let mut night_utxos: HashMap<UserAddress, Vec<u128>> = HashMap::new();
		for utxo in state.utxo.utxos.iter() {
			if utxo.0.type_ != NIGHT {
				continue;
			}
			night_utxos.entry(utxo.0.owner).or_default().push(utxo.0.value);
		}

		for wallet in wallets {
			let address = wallet.unshielded.user_address;
			let utxos = night_utxos.get(&address).expect("no UTXOs for wallet");
			assert_eq!(utxos, &vec![MINT_AMOUNT; 5]);
		}
	}
}
