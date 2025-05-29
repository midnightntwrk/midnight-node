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
	SeedableRng, Spin, StdRng,
	cli_parsers::{self as cli},
	remote_prover::RemoteProofServer,
};

use midnight_node_ledger_helpers::*;

pub const MINT_AMOUNT: u128 = 5_000_000_000_000_000;

/// Common arguments for funding wallets (shielded or unshielded)
#[derive(clap::Args)]
pub struct FundingArgsShielded {
	/// Funding wallet addresses
	#[arg(
        long,
        num_args = 1..,
        value_parser = cli::wallet_address,
        value_delimiter = ' '
    )]
	shielded_addresses: Vec<WalletAddress>,
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
}

#[derive(clap::Args)]
pub struct FundingArgsUnshielded {
	/// Funding wallet addresses
	#[arg(
        long,
        num_args = 1..,
        value_parser = cli::wallet_address,
        value_delimiter = ' '
    )]
	unshielded_addresses: Vec<WalletAddress>,
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
        default_values = [
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000002"
        ]
    )]
	unshielded_alt_token_types: Vec<UnshieldedTokenType>,
}

pub struct GenesisGenerator {
	pub state: LedgerState<DefaultDB>,
	pub tx: Transaction<Signature, ProofMarker, PedersenRandomness, DefaultDB>,
}

impl GenesisGenerator {
	pub async fn new(
		seed: [u8; 32],
		network_id: NetworkId,
		proof_server: Option<String>,
		shielded: FundingArgsShielded,
		unshielded: FundingArgsUnshielded,
	) -> Self {
		// Initial states
		let ref_state: LedgerState<DefaultDB> = LedgerState::new();
		let tx_context =
			TransactionContext { ref_state, block_context: Default::default(), whitelist: None };

		// Source of randomness
		let mut rng = StdRng::from_seed(seed);

		// Generate Shielded Offer
		let guaranteed_shielded_offer = Self::shielded_offer(&shielded, &mut rng);
		let fallible_coins = std::collections::HashMap::new();

		// Generate Unshielded Offer
		let guaranteed_unshielded_offer = Self::unshielded_offer(&unshielded);

		let mut intents = HashMapStorage::<
			u16,
			Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>,
			DefaultDB,
		>::new();

		let mut intent = Intent::<Signature, _, _, _>::empty(&mut rng, Timestamp::from_secs(0));
		intent.guaranteed_unshielded_offer = Some(guaranteed_unshielded_offer);

		intents = intents.insert(Segment::Fallible.into(), intent);

		let spin = Spin::new("proving genesis transaction...");
		let unproven_tx =
			Transaction::new(intents, Some(guaranteed_shielded_offer), fallible_coins);

		let proof_server: Box<dyn ProofProvider<DefaultDB>> = if let Some(url) = proof_server {
			Box::new(RemoteProofServer::new(url, network_id))
		} else {
			Box::new(LocalProofServer::new())
		};

		let genesis_tx = proof_server.prove(unproven_tx, rng, &DEFAULT_RESOLVER).await;

		spin.finish("genesis transaction proved.");

		// Check the transaction is well-formed with balance enforcing disabled
		let mut lax = WellFormedStrictness::default();
		lax.enforce_balancing = false;
		lax.enforce_limits = true;
		lax.verify_contract_proofs = true;
		lax.verify_native_proofs = true;
		genesis_tx
			.well_formed(&tx_context.ref_state, lax, Timestamp::from_secs(0))
			.unwrap_or_else(|err| panic!("Error validating Genesis tx: {:?}", err));

		let (mut ledger_state, result) = tx_context.ref_state.apply(&genesis_tx, &tx_context);

		ledger_state = ledger_state.post_block_update(Timestamp::from_secs(0));

		match result {
			TransactionResult::Success => Self { state: ledger_state, tx: genesis_tx },
			TransactionResult::PartialSuccess(invalid) => {
				panic!("Partial Success generating Genesis: {:?}", invalid)
			},
			TransactionResult::Failure(invalid) => {
				panic!("Failure generating Genesis: {:?}", invalid)
			},
		}
	}

	fn shielded_offer(
		shielded: &FundingArgsShielded,
		rng: &mut StdRng,
	) -> Offer<ProofPreimage, DefaultDB> {
		let FundingArgsShielded {
			shielded_addresses,
			shielded_num_funding_outputs,
			shielded_mint_amount,
			shielded_alt_token_types,
		} = shielded;

		let mut outputs = vec![];

		for address in shielded_addresses {
			let wallet: ShieldedWallet<DefaultDB> = address.clone().into();

			for _i in 0..*shielded_num_funding_outputs {
				let coin = CoinInfo::new(rng, *shielded_mint_amount, FEE_TOKEN);
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
				address
			);
		}

		let mut deltas = vec![Delta {
			token_type: FEE_TOKEN,
			value: -((shielded_mint_amount
				* *shielded_num_funding_outputs as u128
				* shielded_addresses.len() as u128) as i128),
		}];

		for token_type in shielded_alt_token_types {
			deltas.push(Delta {
				token_type: *token_type,
				value: -((shielded_mint_amount * shielded_addresses.len() as u128) as i128),
			});
		}

		// Create unbalanced offer - no inputs
		let mut guaranteed_offer = Offer {
			inputs: VecStorage::new(),
			outputs: VecStorage::from_std_vec(outputs),
			transient: VecStorage::new(),
			deltas: VecStorage::from_std_vec(deltas),
		};
		guaranteed_offer.normalize();

		guaranteed_offer
	}

	fn unshielded_offer(
		unshielded: &FundingArgsUnshielded,
	) -> Sp<UnshieldedOffer<Signature, DefaultDB>, DefaultDB> {
		let FundingArgsUnshielded {
			unshielded_addresses,
			unshielded_num_funding_outputs,
			unshielded_mint_amount,
			unshielded_alt_token_types,
		} = unshielded;

		let inputs = vec![];

		let mut outputs = Vec::new();

		for address in unshielded_addresses {
			let wallet: UnshieldedWallet = address.clone().into();

			for _i in 0..*unshielded_num_funding_outputs {
				let out = UtxoOutput {
					value: *unshielded_mint_amount,
					owner: wallet.verifying_key.clone().into(),
					type_: NIGHT,
				};

				outputs.push(out);
			}

			// Test tokens
			for token_type in unshielded_alt_token_types {
				let out = UtxoOutput {
					value: *unshielded_mint_amount,
					owner: wallet.verifying_key.clone().into(),
					type_: *token_type,
				};
				outputs.push(out);
			}

			println!(
				"generated {} outputs for wallet {:?}",
				unshielded_num_funding_outputs + unshielded_alt_token_types.len(),
				address
			);
		}

		outputs.sort();

		let offer = UnshieldedOffer {
			inputs: VecStorage::from_std_vec(inputs),
			outputs: VecStorage::from_std_vec(outputs),
			signatures: VecStorage::new(),
		};

		Sp::new(offer)
	}
}
