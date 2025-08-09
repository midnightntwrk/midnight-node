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
	unwrapped_fee_token,
};
use std::collections::HashMap;
use thiserror::Error;

use midnight_node_ledger_helpers::{Transaction as MNLedgerTransaction, *};

pub const MINT_AMOUNT: u128 = 5_000_000_000_000_000;
pub const GENESIS_NONCE_SEED: &str =
	"0000000000000000000000000000000000000000000000000000000000000037";

type IntentsMap = HashMapStorage<
	u16,
	Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>,
	DefaultDB,
>;
type ShieldedOffer = Offer<ProofPreimage, DefaultDB>;

type Transaction = MNLedgerTransaction<Signature, ProofMarker, PedersenRandomness, DefaultDB>;

#[derive(Debug, Error)]
pub enum GenesisGeneratorError<D: DB> {
	#[error("Error validating Genesis tx: {0:?}")]
	GenesisValidation(#[from] MalformedTransaction<D>),
	#[error("Partial success generating Genesis: {0}")]
	TxPartialSuccess(String),
	#[error("Failure generating Genesiss generating Genesis: {0:?}")]
	TxFailed(TransactionInvalid<D>),
}

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
	pub shielded_addresses: Vec<WalletAddress>,
	/// Mint amount per output
	#[arg(long, default_value_t = MINT_AMOUNT)]
	pub shielded_mint_amount: u128,
	/// Number of funding outputs
	#[arg(long, default_value = "5")]
	pub shielded_num_funding_outputs: usize,
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
	pub tx: Transaction,
}

impl GenesisGenerator {
	pub async fn new(
		seed: [u8; 32],
		network_id: NetworkId,
		proof_server: Option<String>,
		shielded: FundingArgsShielded,
		unshielded: FundingArgsUnshielded,
	) -> Result<Self, GenesisGeneratorError<DefaultDB>> {
		// Initial states
		let ref_state: LedgerState<DefaultDB> = LedgerState::new();
		let tx_context =
			TransactionContext { ref_state, block_context: Default::default(), whitelist: None };

		// Source of randomness
		let mut rng = StdRng::from_seed(seed);

		// Generate Shielded Offer
		let guaranteed_shielded_offer = Self::shielded_offer(&shielded, &mut rng);
		let fallible_coins = HashMap::new();

		let intents = Self::create_intents_map(&unshielded, &mut rng);

		let genesis_tx = Self::run_proof(
			network_id,
			proof_server,
			intents,
			guaranteed_shielded_offer,
			fallible_coins,
			rng,
		)
		.await;

		// Check the transaction is well-formed with balance enforcing disabled
		let mut lax = WellFormedStrictness::default();
		lax.enforce_balancing = false;

		let timestamp = Timestamp::from_secs(0);
		genesis_tx.well_formed(&tx_context.ref_state, lax, timestamp)?;

		let ledger_state = Self::get_ledger_state(timestamp, tx_context, &genesis_tx)?;

		Ok(Self { state: ledger_state, tx: genesis_tx })
	}

	fn get_ledger_state(
		timestamp: Timestamp,
		tx_context: TransactionContext<DefaultDB>,
		genesis_tx: &Transaction,
	) -> Result<LedgerState<DefaultDB>, GenesisGeneratorError<DefaultDB>> {
		let (mut ledger_state, result) = tx_context.ref_state.apply(&genesis_tx, &tx_context);

		ledger_state = ledger_state.post_block_update(timestamp);

		match result {
			TransactionResult::Success => Ok(ledger_state),
			TransactionResult::PartialSuccess(invalid) => {
				Err(GenesisGeneratorError::TxPartialSuccess(format!("{invalid:?}")))
			},
			TransactionResult::Failure(invalid) => Err(GenesisGeneratorError::TxFailed(invalid)),
		}
	}

	// returns a transaction that underwent proving.
	async fn run_proof(
		network_id: NetworkId,
		proof_server: Option<String>,
		intents: IntentsMap,
		guaranteed_shielded_offer: ShieldedOffer,
		fallible_coins: HashMap<u16, ShieldedOffer>,
		rng: StdRng,
	) -> Transaction {
		let spin = Spin::new("proving genesis transaction...");
		let unproven_tx =
			MNLedgerTransaction::new(intents, Some(guaranteed_shielded_offer), fallible_coins);

		let proof_server: Box<dyn ProofProvider<DefaultDB>> = if let Some(url) = proof_server {
			Box::new(RemoteProofServer::new(url, network_id))
		} else {
			Box::new(LocalProofServer::new())
		};

		let genesis_tx = proof_server.prove(unproven_tx, rng, &DEFAULT_RESOLVER).await;

		spin.finish("genesis transaction proved.");

		genesis_tx
	}

	fn create_intents_map(unshielded: &FundingArgsUnshielded, rng: &mut StdRng) -> IntentsMap {
		// Generate Unshielded Offer
		let guaranteed_unshielded_offer = Self::unshielded_offer(unshielded);

		let mut intent = Intent::<Signature, _, _, _>::empty(rng, Timestamp::from_secs(0));
		intent.guaranteed_unshielded_offer = Some(guaranteed_unshielded_offer);

		let mut intents = IntentsMap::new();
		intents = intents.insert(Segment::Fallible.into(), intent);

		intents
	}

	fn shielded_offer(shielded: &FundingArgsShielded, rng: &mut StdRng) -> ShieldedOffer {
		let FundingArgsShielded {
			shielded_addresses,
			shielded_num_funding_outputs,
			shielded_mint_amount,
			shielded_alt_token_types,
		} = shielded;

		let mut outputs = vec![];

		for address in shielded_addresses {
			let wallet: ShieldedWallet<DefaultDB> =
				address.try_into().expect("Failed to parse shielded address");

			for _i in 0..*shielded_num_funding_outputs {
				let coin = CoinInfo::new(rng, *shielded_mint_amount, unwrapped_fee_token());
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
			token_type: unwrapped_fee_token(),
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
			inputs: Array::new(),
			outputs: outputs.into(),
			transient: Array::new(),
			deltas: deltas.into(),
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
			let wallet: UnshieldedWallet =
				address.try_into().expect("Failed to parse unshielded address");

			for _i in 0..*unshielded_num_funding_outputs {
				let out = UtxoOutput {
					value: *unshielded_mint_amount,
					owner: wallet.user_address,
					type_: NIGHT,
				};

				outputs.push(out);
			}

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
				unshielded_num_funding_outputs + unshielded_alt_token_types.len(),
				address
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
}

pub fn network_as_str(id: NetworkId) -> &'static str {
	match id {
		NetworkId::MainNet => "mainnet",
		NetworkId::DevNet => "devnet",
		NetworkId::TestNet => "testnet",
		NetworkId::Undeployed => "undeployed",
		_ => panic!("unknown network id: {id:?}"),
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use std::{fs::File, io::Read, path::Path};

	#[test]
	fn test_network_as_str() {
		assert_eq!("mainnet", network_as_str(NetworkId::MainNet));
		assert_eq!("devnet", network_as_str(NetworkId::DevNet));
		assert_eq!("undeployed", network_as_str(NetworkId::Undeployed));
	}

	#[test]
	fn test_get_ledger_state() {
		let network = "undeployed";
		let expected_tx_path = format!("../../res/test-genesis-generator/genesis_tx_{network}.mn");
		let expected_state_path =
			format!("../../res/test-genesis-generator/genesis_state_{network}.mn");

		// Initial states
		let ref_state: LedgerState<DefaultDB> = LedgerState::new();
		let tx_context =
			TransactionContext { ref_state, block_context: Default::default(), whitelist: None };
		let timestamp = Timestamp::from_secs(0);

		let genesis_tx: Transaction = read_and_deserialize(expected_tx_path, NetworkId::Undeployed);

		let expected_state: LedgerState<DefaultDB> =
			read_and_deserialize(expected_state_path, NetworkId::Undeployed);

		let ledger_state = GenesisGenerator::get_ledger_state(timestamp, tx_context, &genesis_tx)
			.expect("failed to get ledger state");

		assert_eq!(
			expected_state.unminted_native_token_supply,
			ledger_state.unminted_native_token_supply
		);
		assert_eq!(expected_state.unclaimed_mints, ledger_state.unclaimed_mints);
		assert_eq!(expected_state.treasury, ledger_state.treasury);
		assert_eq!(expected_state.zswap, ledger_state.zswap);
		assert_eq!(expected_state.contract, ledger_state.contract);
	}

	fn read_and_deserialize<P: AsRef<Path>, T: Deserializable>(
		file_path: P,
		network: NetworkId,
	) -> T {
		let mut file = File::open(file_path).expect("file not found");

		let mut bytes = vec![];
		file.read_to_end(&mut bytes).expect("failed to read at end");

		deserialize(bytes.as_slice(), network).expect("failed to deserialize")
	}
}
