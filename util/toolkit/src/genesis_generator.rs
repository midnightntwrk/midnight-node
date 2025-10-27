// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
	ProofType, SeedableRng, SignatureType, Spin, StdRng,
	cli_parsers::{self as cli},
	remote_prover::RemoteProofServer,
	t_token,
};
use midnight_node_ledger_helpers::{Transaction as MNLedgerTransaction, *};
use std::collections::HashMap;
use thiserror::Error;

pub const MINT_AMOUNT: u128 = 500_000_000_000_000;
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
}

/// Common arguments for funding wallets (shielded, unshielded, dust)
#[derive(clap::Args)]
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
	pub txs: Vec<TransactionWithContext<SignatureType, ProofType, DefaultDB>>,
	fullness: SyntheticCost,
}

const GLACIER_DROP_START_UNIX_EPOC: u64 = 1754395200;
const BEGINNING: Timestamp = Timestamp::from_secs(GLACIER_DROP_START_UNIX_EPOC);

type Result<T, E = GenesisGeneratorError<DefaultDB>> = std::result::Result<T, E>;

impl GenesisGenerator {
	pub async fn new(
		seed: [u8; 32],
		network_id: &str,
		proof_server: Option<String>,
		funding: FundingArgs,
		seeds: &[WalletSeed],
		cnight_system_tx: Option<SystemTransaction>,
	) -> Result<Self> {
		let state = LedgerState::new(network_id);
		let mut me = Self { state, txs: vec![], fullness: SyntheticCost::ZERO };
		me.init(seed, network_id, proof_server, &funding, seeds, cnight_system_tx)
			.await?;
		Ok(me)
	}

	async fn init(
		&mut self,
		seed: [u8; 32],
		network_id: &str,
		proof_server: Option<String>,
		funding: &FundingArgs,
		seeds: &[WalletSeed],
		cnight_system_tx: Option<SystemTransaction>,
	) -> Result<(), GenesisGeneratorError<DefaultDB>> {
		let wallets: Vec<Wallet<DefaultDB>> =
			seeds.iter().cloned().map(|seed| Wallet::default(seed, &self.state)).collect();

		// Source of randomness
		let mut rng = StdRng::from_seed(seed);

		let genesis_block_context = BlockContext {
			tblock: BEGINNING,
			tblock_err: 30,
			parent_block_hash: HashOutput::default(),
		};

		// Distribute NIGHT as rewards to all wallets
		self.distribute_night(&genesis_block_context, funding, &wallets, &mut rng)?;

		// Set fees to zero to simplify setup logic.
		// This lets us claim the full requested amount of NIGHT,
		// and register DUST addresses without waiting for DUST to accumulate.
		let original_parameters = (*self.state.parameters).clone();
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

		if let Some(system_tx) = cnight_system_tx {
			self.apply_system_tx(system_tx, &genesis_block_context)?;
		}

		self.state = self.state.post_block_update(genesis_block_context.tblock, self.fullness)?;
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
			let target_address = wallet.unshielded.verifying_key.clone().unwrap().into();
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
		let fallible_coins = HashMap::new();

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
		fallible_coins: HashMap<u16, ShieldedOffer>,
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

			println!(
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

			println!(
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
				self.txs.push(TransactionWithContext {
					tx: SerdeTransaction::Midnight(tx),
					block_context: tx_context.block_context,
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
		self.txs.push(TransactionWithContext {
			tx: SerdeTransaction::System(tx),
			block_context: block_context.clone(),
		});
		Ok(())
	}
}

fn without_fees(params: &LedgerParameters) -> LedgerParameters {
	LedgerParameters {
		fee_prices: FeePrices {
			read_price: FixedPoint::ZERO,
			compute_price: FixedPoint::ZERO,
			block_usage_price: FixedPoint::ZERO,
			write_price: FixedPoint::ZERO,
		},
		..params.clone()
	}
}

#[cfg(test)]
mod test {
	use super::*;

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

		let genesis = GenesisGenerator::new(seed, network_id, proof_server, funding, &seeds, None)
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
