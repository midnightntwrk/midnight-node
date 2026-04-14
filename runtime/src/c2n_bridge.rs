use crate::{LedgerApi, MidnightSystem, Runtime};
use alloc::vec::Vec;
use midnight_primitives::{BridgeRecipient, MidnightSystemTransactionExecutor};
use sp_partner_chains_bridge::{BridgeTransferV1, TransferRecipient};

pub struct MidnightTokenTransferHandler;

/// Storage key for the bridge transfer nonce counter (transient, reset each block).
const BRIDGE_TRANSFER_NONCE_COUNTER_KEY: &[u8] = b":bridge_transfer_nonce_counter:";

impl MidnightTokenTransferHandler {
	/// Generate a deterministic unique nonce for a bridge transfer.
	///
	/// Uses the parent hash (unique per block) combined with a monotonically
	/// increasing counter (unique within a block) to guarantee uniqueness.
	fn generate_nonce() -> [u8; 32] {
		let counter: u32 =
			frame_support::storage::unhashed::get(BRIDGE_TRANSFER_NONCE_COUNTER_KEY).unwrap_or(0);
		frame_support::storage::unhashed::put(BRIDGE_TRANSFER_NONCE_COUNTER_KEY, &(counter + 1));

		let parent_hash = frame_system::Pallet::<Runtime>::parent_hash();
		let mut data = Vec::new();
		data.extend(b"midnight:bridge-transfer-nonce:");
		data.extend(parent_hash.as_ref());
		data.extend(&counter.to_le_bytes());
		sp_core::hashing::blake2_256(&data)
	}
}

pub(crate) type MaybeMidnightTxHash = Option<[u8; 32]>;

impl pallet_partner_chains_bridge::TransferHandler<BridgeRecipient, MaybeMidnightTxHash>
	for MidnightTokenTransferHandler
{
	fn handle_incoming_transfer(
		transfer: BridgeTransferV1<BridgeRecipient>,
	) -> MaybeMidnightTxHash {
		let amount = transfer.amount;
		let serialized_tx = match transfer.recipient {
			TransferRecipient::Address { recipient } => {
				let recipient_bytes = recipient.as_bytes().to_vec();
				let nonce = Self::generate_nonce();

				match LedgerApi::construct_distribute_night_cardano_bridge_system_tx(
					amount.into(),
					&recipient_bytes.clone(),
					nonce,
				) {
					Ok(tx) => {
						log::info!(
							"Will execute distribute {amount} of Night to {recipient_bytes:?}",
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
						log::info!("Will execute distribute {amount} of Night to reserve");
						tx
					},
					Err(e) => {
						log::error!("Failed to construct bridge reserve transfer system tx: {e:?}");
						return None;
					},
				}
			},
			TransferRecipient::Invalid => {
				match LedgerApi::construct_distribute_treasury_system_tx(amount.into()) {
					Ok(tx) => {
						log::info!("Will execute distribute {amount} of Night to treasury");
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
