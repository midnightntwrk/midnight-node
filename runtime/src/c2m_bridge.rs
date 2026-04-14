use crate::{LedgerApi, MidnightSystem, Runtime};
use alloc::vec::Vec;
use midnight_primitives::{BridgeRecipient, MidnightSystemTransactionExecutor};
use sp_partner_chains_bridge::{BridgeTransferV1, TransferRecipient};

pub struct MidnightTokenTransferHandler;

impl MidnightTokenTransferHandler {
	/// Generate a deterministic unique nonce for a bridge transfer.
	///
	/// Uses the parent hash (unique per block) combined with an
	/// increasing counter (unique within a block) to guarantee uniqueness.
	fn generate_nonce(counter: u32) -> [u8; 32] {
		let parent_hash = frame_system::Pallet::<Runtime>::parent_hash();
		let mut data = Vec::new();
		data.extend(b"midnight:bridge-transfer-nonce:");
		data.extend(parent_hash.as_ref());
		data.extend(&counter.to_le_bytes());
		sp_core::hashing::blake2_256(&data)
	}
}

pub(crate) type MidnightTxHash = [u8; 32];

impl pallet_partner_chains_bridge::TransferHandler<BridgeRecipient, MidnightTxHash>
	for MidnightTokenTransferHandler
{
	fn handle_incoming_transfer(
		transfer_index: u32,
		transfer: BridgeTransferV1<BridgeRecipient>,
	) -> Option<MidnightTxHash> {
		let amount = transfer.amount;
		let serialized_tx = match transfer.recipient {
			TransferRecipient::Address { recipient } => {
				let nonce = Self::generate_nonce(transfer_index);

				match LedgerApi::construct_distribute_night_cardano_bridge_system_tx(
					amount.into(),
					recipient.as_bytes(),
					nonce,
				) {
					Ok(tx) => {
						log::debug!(
							"Will execute distribute {amount} of Night to {:?}",
							recipient.as_bytes()
						);
						tx
					},
					Err(e) => {
						log::error!("Failed to construct bridge user transfer system tx: {e:?}");
						return None;
					},
				}
			},
			TransferRecipient::Reserve => {
				match LedgerApi::construct_distribute_reserve_system_tx(amount.into()) {
					Ok(tx) => {
						log::debug!("Will execute distribute {amount} of Night to reserve");
						tx
					},
					Err(e) => {
						log::debug!("Failed to construct bridge reserve transfer system tx: {e:?}");
						return None;
					},
				}
			},
			TransferRecipient::Invalid => {
				match LedgerApi::construct_distribute_treasury_system_tx(amount.into()) {
					Ok(tx) => {
						log::debug!("Will execute distribute {amount} of Night to treasury");
						tx
					},
					Err(e) => {
						log::error!(
							"Failed to construct bridge treasury transfer system tx: {e:?}"
						);
						return None;
					},
				}
			},
		};
		match MidnightSystem::execute_system_transaction(serialized_tx.clone()) {
			Ok(hash) => Some(hash),
			Err(e) => {
				log::error!("Failed to execute system transaction {serialized_tx:?}: {e:?}");
				None
			},
		}
	}
}
