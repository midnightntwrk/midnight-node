#![allow(deprecated)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use crate::{AccountId, Balances};
	use frame_support::pallet_prelude::{StorageMap, *};
	use frame_support::traits::Currency;
	use sidechain_domain::*;
	use sp_partner_chains_bridge::{BridgeTransferV1, TransferRecipient};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type ReserveAccount: Get<AccountId>;
	}

	#[pallet::storage]
	pub type TotalInvalidTransfers<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
	pub type TotalReserveTransfers<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
	pub type UserTransferTotals<T: Config> =
		StorageMap<_, Twox64Concat, AccountId, u64, ValueQuery>;

	impl<T: Config> sp_sidechain::OnNewEpoch for Pallet<T> {
		fn on_new_epoch(
			_old_epoch: ScEpochNumber,
			_new_epoch: ScEpochNumber,
		) -> sp_weights::Weight {
			crate::RuntimeDbWeight::get().reads_writes(0, 0)
		}
	}

	impl<T: Config> pallet_partner_chains_bridge::TransferHandler<AccountId> for Pallet<T> {
		fn handle_incoming_transfer(transfer: BridgeTransferV1<AccountId>) {
			let token_amount = transfer.amount;
			let mc_tx_hash = transfer.mc_tx_hash;
			match transfer.recipient {
				TransferRecipient::Invalid => {
					log::warn!(
						"⚠️ Recorded an invalid transfer of {token_amount} (tx {mc_tx_hash})"
					);
					TotalInvalidTransfers::<T>::mutate(|v| *v + token_amount);
				},
				TransferRecipient::Address { recipient } => {
					log::info!("💸 Registered a transfer of {token_amount} to {recipient:?}");
					let _ = Balances::deposit_creating(&recipient, token_amount.into());
					UserTransferTotals::<T>::mutate(recipient, |v| *v += token_amount);
				},
				TransferRecipient::Reserve => {
					log::info!("🏦 Registered a reserve transfer of {token_amount}.");
					let _ =
						Balances::deposit_creating(&T::ReserveAccount::get(), token_amount.into());
					TotalReserveTransfers::<T>::mutate(|v| *v += token_amount);
				},
			}
			()
		}
	}
}
