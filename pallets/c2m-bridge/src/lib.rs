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

//! cNight Cardano-to-Midnight bridge transfer handling.
//!
//! This pallet implements the [`pallet_partner_chains_bridge::TransferHandler`] trait
//! with Midnight-specific logic.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
mod runtime_api;
#[cfg(test)]
mod tests;
pub mod weights;

use frame_support::pallet_prelude::*;
pub use pallet::*;
pub use runtime_api::*;
pub use weights::WeightInfo;

/// Maximum number of approved mainchain transaction hashes that can be added in a single batch.
pub const MAX_APPROVALS_PER_BATCH: u32 = 32;

/// Hash of a Midnight ledger transaction, returned by the system transaction executor.
pub type MidnightTxHash = [u8; 32];

#[derive(Debug, Decode, Encode, Default, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
pub struct SubminimalTransfersState {
	count: u32,
	sum: u64,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use alloc::vec::Vec;
	use frame_system::pallet_prelude::*;
	use midnight_node_ledger::types::{
		active_ledger_bridge as LedgerApi, active_version::LedgerApiError,
	};
	use midnight_primitives::{BridgeRecipient, MidnightSystemTransactionExecutor};
	use sidechain_domain::McTxHash;
	use sp_core::hexdisplay::HexDisplay;
	use sp_partner_chains_bridge::{
		BridgeTransferV1, SubminimalTransfersConfig, TransferRecipient,
	};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Provides access to the Midnight system transaction executor.
		type MidnightSystemTransactionExecutor: MidnightSystemTransactionExecutor;

		/// Provides access to the ledger's `c_to_m_bridge_min_amount` parameter.
		type MinBridgeAmountProvider: MinBridgeAmountProvider;

		/// Origin for governance extrinsic calls.
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for this pallet's extrinsics.
		type WeightInfo: crate::weights::WeightInfo;
	}

	/// Provides access to the minimum bridge transfer amount from the Midnight ledger.
	pub trait MinBridgeAmountProvider {
		/// Returns the minimum bridge transfer amount, in STARS, from the ledger parameters.
		fn get_c_to_m_bridge_min_amount() -> Result<u128, LedgerApiError>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Emitted for each handled bridge transfer.
		UserTransfer {
			/// Main chain transaction hash for correlation of PC with MC.
			mc_tx_hash: McTxHash,
			/// Amount of tokens that were transferred.
			amount: u64,
			/// Beneficiary of the transfer.
			recipient: BridgeRecipient,
			/// Hash of the Midnight system transaction produced by the handler.
			midnight_tx_hash: MidnightTxHash,
		},
		ReserveTransfer {
			/// Main chain transaction hash for correlation of PC with MC.
			mc_tx_hash: McTxHash,
			/// Amount of tokens that were transferred.
			amount: u64,
			/// Hash of the Midnight system transaction produced by the handler.
			midnight_tx_hash: MidnightTxHash,
		},
		InvalidTransfer {
			/// Main chain transaction hash for correlation of PC with MC.
			mc_tx_hash: McTxHash,
			/// Amount of tokens that were transferred.
			amount: u64,
			/// Hash of the Midnight system transaction produced by the handler.
			midnight_tx_hash: MidnightTxHash,
		},
		UnapprovedTransfer {
			/// Main chain transaction hash for correlation of PC with MC.
			mc_tx_hash: McTxHash,
			/// Amount of tokens that were transferred.
			amount: u64,
			/// Beneficiary of the transfer.
			recipient: BridgeRecipient,
			/// Hash of the Midnight system transaction produced by the handler.
			midnight_tx_hash: MidnightTxHash,
		},
		SubminimalFlushTransfer {
			/// Amount of tokens that were transferred.
			amount: u64,
			/// Number of subminimal transfer that contributed to this flush.
			count: u32,
			/// Hash of the Midnight system transaction produced by the handler.
			midnight_tx_hash: MidnightTxHash,
		},
	}

	#[pallet::storage]
	pub type SubminimalTransfersConfiguration<T: Config> =
		StorageValue<_, SubminimalTransfersConfig, ValueQuery>;

	#[pallet::storage]
	pub type SubminimalTransfers<T: Config> = StorageValue<_, SubminimalTransfersState, ValueQuery>;

	/// Block-scoped counter used for deterministic nonce generation per transfer.
	/// Because on_finalize kill call, it doesn't cost any storage operations.
	#[pallet::storage]
	pub type TransferCounter<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Set of mainchain transaction hashes pre-approved by governance for crediting
	/// mNIGHT to the recipient. Modeled as a map-with-unit-value: presence of a key
	/// denotes membership; absence denotes non-membership.
	#[pallet::storage]
	pub type ApprovedMcTxHashes<T: Config> =
		StorageMap<_, Blake2_128Concat, McTxHash, (), OptionQuery>;

	/// Genesis configuration of the pallet.
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// Initial subminimal transfers configuration.
		pub subminimal_transfers_config: SubminimalTransfersConfig,
		/// Mainchain transaction hashes to pre-approve at genesis.
		#[serde(default)]
		pub approved_txs: Vec<McTxHash>,
		#[allow(missing_docs)]
		pub _marker: PhantomData<T>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				subminimal_transfers_config: SubminimalTransfersConfig::default(),
				approved_txs: Vec::new(),
				_marker: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let GenesisConfig { subminimal_transfers_config, approved_txs, _marker } = self;
			SubminimalTransfersConfiguration::<T>::put(subminimal_transfers_config.clone());
			for hash in approved_txs {
				ApprovedMcTxHashes::<T>::insert(*hash, ());
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_n: BlockNumberFor<T>) {
			TransferCounter::<T>::kill();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Update the subminimal transfers configuration.
		///
		/// Must be called via governance (e.g. `sudo` or council).
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_subminimal_transfers_config())]
		pub fn set_subminimal_transfers_config(
			origin: OriginFor<T>,
			config: SubminimalTransfersConfig,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			SubminimalTransfersConfiguration::<T>::put(config);
			Ok(())
		}

		/// Add a batch of mainchain transaction hashes to the approval set.
		///
		/// Must be called via governance.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::add_approved_mc_tx_hashes(hashes.len() as u32))]
		pub fn add_approved_mc_tx_hashes(
			origin: OriginFor<T>,
			hashes: BoundedVec<McTxHash, ConstU32<MAX_APPROVALS_PER_BATCH>>,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			for hash in hashes {
				ApprovedMcTxHashes::<T>::insert(hash, ());
			}
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Returns the current subminimal transfers configuration.
		pub fn get_subminimal_transfers_config() -> SubminimalTransfersConfig {
			SubminimalTransfersConfiguration::<T>::get()
		}

		/// Returns the full set of mainchain transaction hashes pre-approved by governance.
		pub fn get_approved_mc_tx_hashes() -> Vec<McTxHash> {
			ApprovedMcTxHashes::<T>::iter_keys().collect()
		}

		/// Generate a deterministic unique nonce for a bridge transfer.
		///
		/// Uses the parent hash (unique per block) combined with an
		/// increasing counter (unique within a block) to guarantee uniqueness.
		fn generate_nonce() -> [u8; 32] {
			let counter = TransferCounter::<T>::get();
			TransferCounter::<T>::put(counter + 1);
			let parent_hash = frame_system::Pallet::<T>::parent_hash();
			let mut data = Vec::new();
			data.extend(b"midnight:bridge-transfer-nonce:");
			data.extend(parent_hash.as_ref());
			data.extend(&counter.to_le_bytes());
			sp_crypto_hashing::blake2_256(&data)
		}

		fn execute_serialized_tx<F>(
			result: Result<Vec<u8>, LedgerApiError>,
			make_event: F,
			description: &str,
		) where
			F: FnOnce([u8; 32]) -> Event<T>,
		{
			match result {
				Ok(serialized_tx) => {
					log::debug!("Serialized transaction for {}", description);
					match T::MidnightSystemTransactionExecutor::execute_system_transaction(
						serialized_tx,
					) {
						Ok(tx_hash) => {
							log::debug!("Executed system transaction for {}", description);
							let event = make_event(tx_hash);
							Self::deposit_event(event);
						},
						Err(e) => {
							log::error!(
								"Failed to execute system transaction for {}: {e:?}",
								description
							);
						},
					}
				},
				Err(e) => {
					log::error!("Failed to serialize transaction for {}: {e:?}", description);
				},
			}
		}

		fn handle_subminimal_transfer(transfer: BridgeTransferV1<BridgeRecipient>) {
			let SubminimalTransfersState { count, sum } = SubminimalTransfers::<T>::get();
			let config = SubminimalTransfersConfiguration::<T>::get();

			// Safe, because all existing cNight fits in u64.
			let sum = sum.saturating_add(transfer.amount);
			let count = count.saturating_add(1);
			if sum > config.subminimal_transfers_flush_threshold {
				Self::execute_serialized_tx(
					LedgerApi::construct_unlock_to_treasury_system_tx(sum.into()),
					|midnight_tx_hash| Event::SubminimalFlushTransfer {
						amount: sum,
						count,
						midnight_tx_hash,
					},
					&alloc::format!("subminimal transfers flush of total {}", sum),
				);
				SubminimalTransfers::<T>::kill();
			} else {
				SubminimalTransfers::<T>::put(SubminimalTransfersState { count, sum });
			}
		}

		fn handle_regular_transfer(transfer: BridgeTransferV1<BridgeRecipient>) {
			let amount = transfer.amount;
			let mc_tx_hash = transfer.mc_tx_hash;
			match transfer.recipient {
				TransferRecipient::Invalid => Self::handle_invalid_transfer(mc_tx_hash, amount),
				TransferRecipient::Reserve => Self::handle_reserve_transfer(mc_tx_hash, amount),
				TransferRecipient::Address { recipient } => {
					Self::handle_user_transfer(mc_tx_hash, amount, recipient)
				},
			}
		}

		fn handle_invalid_transfer(mc_tx_hash: McTxHash, amount: u64) {
			Self::execute_serialized_tx(
				LedgerApi::construct_unlock_to_treasury_system_tx(amount.into()),
				|midnight_tx_hash| Event::InvalidTransfer { mc_tx_hash, amount, midnight_tx_hash },
				&alloc::format!("'Invalid' transfer of {} from Cardano Tx: {}", amount, mc_tx_hash),
			);
		}

		fn handle_reserve_transfer(mc_tx_hash: McTxHash, amount: u64) {
			Self::execute_serialized_tx(
				LedgerApi::construct_distribute_reserve_system_tx(amount.into()),
				|midnight_tx_hash| Event::ReserveTransfer { mc_tx_hash, amount, midnight_tx_hash },
				&alloc::format!("'Reserve' transfer of {} from Cardano Tx: {}", amount, mc_tx_hash),
			);
		}

		fn handle_user_transfer(mc_tx_hash: McTxHash, amount: u64, recipient: BridgeRecipient) {
			// Approval is single-use: remove before executing so a failed ledger call
			// cannot be replayed against the same approval.
			match ApprovedMcTxHashes::<T>::take(mc_tx_hash) {
				None => {
					// Not pre-approved by governance — redirect funds to the Treasury.
					Self::execute_serialized_tx(
						LedgerApi::construct_unlock_to_treasury_system_tx(amount.into()),
						|midnight_tx_hash| Event::UnapprovedTransfer {
							mc_tx_hash,
							amount,
							recipient: recipient.clone(),
							midnight_tx_hash,
						},
						&alloc::format!(
							"Unapproved 'User' transfer of {} NIGHT to {} from Cardano Tx: {}",
							amount,
							HexDisplay::from(&recipient.as_ref()),
							mc_tx_hash
						),
					);
				},
				Some(_) => {
					let nonce = Self::generate_nonce();
					Self::execute_serialized_tx(
						LedgerApi::construct_distribute_night_cardano_bridge_system_tx(
							amount.into(),
							recipient.as_bytes(),
							nonce,
						),
						|midnight_tx_hash| Event::UserTransfer {
							mc_tx_hash,
							amount,
							recipient: recipient.clone(),
							midnight_tx_hash,
						},
						&alloc::format!(
							"'User' transfer of {} NIGHT to {} from Cardano Tx: {}",
							amount,
							HexDisplay::from(&recipient.as_ref()),
							mc_tx_hash
						),
					);
				},
			}
		}
	}

	impl<T: Config> pallet_partner_chains_bridge::TransferHandler<BridgeRecipient> for Pallet<T> {
		fn handle_incoming_transfer(transfer: BridgeTransferV1<BridgeRecipient>) {
			match T::MinBridgeAmountProvider::get_c_to_m_bridge_min_amount() {
				Ok(min_amount) => {
					if u128::from(transfer.amount) < min_amount {
						Self::handle_subminimal_transfer(transfer);
					} else {
						Self::handle_regular_transfer(transfer);
					}
				},
				Err(e) => {
					// If ledger read fails, then subminimal transfers functionality is bypassed.
					// Most likely, if ledger reads fail, the code will never succeed making a transaction.
					log::error!("Failed to read c_to_m_bridge_min_amount from ledger: {e:?}");
					Self::handle_regular_transfer(transfer);
				},
			};
		}
	}
}
