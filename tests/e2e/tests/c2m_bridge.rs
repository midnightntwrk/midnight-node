use midnight_node_e2e::api::cardano::{
    BridgeTransferRecipient, CardanoClient, SignedBridgeTransaction,
};
use midnight_node_e2e::api::midnight::{C2MBridgePalletCalls, MidnightClient};
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;
use midnight_node_ledger_helpers::{
    ClaimKind, HashOutput, SystemTransaction, UserAddress, deserialize,
};
use midnight_node_metadata::midnight_metadata_latest as mn_meta;
use midnight_node_metadata::midnight_metadata_latest::runtime_types::sp_partner_chains_bridge::TransferRecipient;
use std::sync::LazyLock;
use std::time::Duration;
use subxt::dynamic::Value as DynValue;
use tokio::sync::{Mutex as AsyncMutex, MutexGuard};

use crate::global_faucet_manager;

// Tests in this module both mutate and read shared chain state,
// so can't run in parallel.
static C2M_BRIDGE_SERIAL: LazyLock<AsyncMutex<()>> = LazyLock::new(|| AsyncMutex::new(()));

async fn lock_c2m_bridge_serial() -> MutexGuard<'static, ()> {
    C2M_BRIDGE_SERIAL.lock().await
}

/// Arbitrary recipient address bytes used as the bridge target on Midnight.
const RECIPIENT_ADDRESS: [u8; 32] = [7u8; 32];

const BRIDGE_AMOUNT_STARS: u64 = 49_000_000;

/// Upper bound on how long the test waits for the bridge transfer to be observed
/// by midnight and produce the user-transfer system tx.
const BRIDGE_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(120);

#[e2e_test]
async fn bridge_transfer_cnight_to_midnight_address() {
    let _serial = lock_c2m_bridge_serial().await;

    let (cardano_client, midnight_client, prepared) = setup_and_prepare_bridge_transfer(
        BridgeTransferRecipient::Address(RECIPIENT_ADDRESS),
        BRIDGE_AMOUNT_STARS,
    )
    .await;
    let bridge_tx = prepared.tx_id;
    tracing::info!(bridge_tx = %hex::encode(&bridge_tx), "bridge transfer signed (not yet submitted)");

    approve_mc_tx_hash_via_governance(&midnight_client, bridge_tx)
        .await
        .expect("Failed to pre-approve bridge tx hash via governance");
    tracing::info!("bridge tx hash pre-approved on Midnight");

    let min_midnight_block = midnight_client
        .get_finalized_block_number()
        .await
        .expect("Failed to read finalized head before submitting Cardano tx");

    cardano_client
        .submit_tx(prepared.signed_tx_bytes)
        .await
        .expect("Failed to submit bridge transfer transaction to Cardano");
    tracing::info!(bridge_tx = %hex::encode(&bridge_tx), min_midnight_block, "bridge transfer submitted on Cardano");

    let c2m_bridge_calls = wait_for_bridge_calls(&midnight_client, min_midnight_block).await;

    // ----- BridgeTransferV1 argument -----
    let transfer = c2m_bridge_calls
        .transfers
        .first()
        .expect("Expected at least one BridgeTransferV1 in the handle_transfers call");

    assert_eq!(
        transfer.mc_tx_hash.0, bridge_tx,
        "BridgeTransferV1.mc_tx_hash should match the Cardano tx id"
    );
    assert_eq!(
        transfer.amount, BRIDGE_AMOUNT_STARS,
        "BridgeTransferV1.amount should equal the STAR amount transferred"
    );
    // BridgeRecipient = BoundedVec<u8, BridgeRecipientMaxLen>; the pallet keeps
    // the raw recipient bytes verbatim inside the `Address` variant.
    let recipient_bytes = match &transfer.recipient {
        TransferRecipient::Address { recipient } => recipient.0.0.as_slice(),
        other => {
            panic!("BridgeTransferV1.recipient should be TransferRecipient::Address; got {other:?}")
        }
    };
    assert_eq!(
        recipient_bytes, &RECIPIENT_ADDRESS,
        "BridgeTransferV1.recipient should carry the original 32-byte recipient"
    );

    // ----- DistributeNight system transaction -----
    let instr = c2m_bridge_calls.system_transactions_applied.iter().find_map(|sta| {
        let sys_tx: SystemTransaction =
            deserialize(sta.0.serialized_system_transaction.as_slice()).ok()?;
        match sys_tx {
            SystemTransaction::DistributeNight(ClaimKind::CardanoBridge, output_instructions) => {
                output_instructions.first().cloned()
            }
            _ => None,
        }
    }).unwrap_or_else(||panic!("Expected SystemTransaction::DistributeNight(CardanoBridge, [instr]) for an valid bridge transfer"));

    assert_eq!(
        instr.amount, BRIDGE_AMOUNT_STARS as u128,
        "DistributeNight amount should equal BRIDGE_AMOUNT_STARS"
    );
    assert_eq!(
        instr.target_address,
        UserAddress(HashOutput(RECIPIENT_ADDRESS)),
        "DistributeNight should target the bridge recipient address"
    );

    // ----- C2MBridge::Event::UserTransfer -----
    let user_transfer = c2m_bridge_calls
        .c2m_bridge_events
        .iter()
        .find_map(|ev| match ev {
            mn_meta::c2m_bridge::Event::UserTransfer {
                mc_tx_hash,
                amount,
                recipient,
                midnight_tx_hash,
            } => Some((mc_tx_hash, *amount, recipient, midnight_tx_hash)),
            _ => None,
        })
        .expect(
            "Expected a UserTransfer event in the block where the bridge transfer \
             was processed (its absence suggests the approval did not land in time, so the \
             pallet emitted UnapprovedTransfer instead)",
        );

    let (mc_tx_hash, amount, recipient, _midnight_tx_hash) = user_transfer;
    assert_eq!(
        mc_tx_hash.0, bridge_tx,
        "UserTransfer.mc_tx_hash should match the Cardano tx id"
    );
    assert_eq!(
        amount, BRIDGE_AMOUNT_STARS,
        "UserTransfer.amount should equal the cNight amount transferred"
    );
    assert_eq!(
        recipient.0.0.as_slice(),
        &RECIPIENT_ADDRESS,
        "UserTransfer.recipient should carry the original 32-byte recipient"
    );
}

/// Bridge transfer whose Cardano-side metadatum is not valid Midnight address bytes.
#[e2e_test]
async fn bridge_transfer_invalid_recipient_unlocks_to_treasury() {
    let _serial = lock_c2m_bridge_serial().await;

    let (cardano_client, midnight_client, prepared) =
        setup_and_prepare_bridge_transfer(BridgeTransferRecipient::Invalid, BRIDGE_AMOUNT_STARS)
            .await;
    let bridge_tx = prepared.tx_id;
    tracing::info!(bridge_tx = %hex::encode(&bridge_tx), "invalid bridge transfer signed (not yet submitted)");

    approve_mc_tx_hash_via_governance(&midnight_client, bridge_tx)
        .await
        .expect("Failed to pre-approve bridge tx hash via governance");
    tracing::info!("bridge tx hash pre-approved on Midnight");

    let min_midnight_block = midnight_client
        .get_finalized_block_number()
        .await
        .expect("Failed to read finalized head before submitting Cardano tx");
    cardano_client
        .submit_tx(prepared.signed_tx_bytes)
        .await
        .expect("Failed to submit invalid bridge transfer transaction to Cardano");
    tracing::info!(bridge_tx = %hex::encode(&bridge_tx), min_midnight_block, "invalid bridge transfer submitted on Cardano");

    let c2m_bridge_calls = wait_for_bridge_calls(&midnight_client, min_midnight_block).await;

    // ----- BridgeTransferV1 argument -----
    let transfer = c2m_bridge_calls
        .transfers
        .first()
        .expect("Expected at least one BridgeTransferV1 in the handle_transfers call");
    assert_eq!(
        transfer.mc_tx_hash.0, bridge_tx,
        "BridgeTransferV1.mc_tx_hash should match the Cardano tx id"
    );
    assert_eq!(
        transfer.amount, BRIDGE_AMOUNT_STARS,
        "BridgeTransferV1.amount should equal the STAR amount transferred"
    );
    assert!(
        matches!(transfer.recipient, TransferRecipient::Invalid),
        "BridgeTransferV1.recipient should be TransferRecipient::Invalid; got {:?}",
        transfer.recipient
    );

    // ----- UnlockToTreasury system transaction -----
    let _ = c2m_bridge_calls.system_transactions_applied.iter().find_map(|sta| {
        let sys_tx: SystemTransaction =
            deserialize(sta.0.serialized_system_transaction.as_slice()).ok()?;
        match sys_tx {
            SystemTransaction::UnlockToTreasury { amount } if amount == BRIDGE_AMOUNT_STARS as u128 => {
                Some(amount)
            }
            _ => None,
        }
    }).unwrap_or_else(||panic!("Expected SystemTransaction::UnlockToTreasury {{ amount }} for an invalid bridge transfer"));

    // ----- C2MBridge::Event::InvalidTransfer -----
    let invalid_transfer = c2m_bridge_calls
        .c2m_bridge_events
        .iter()
        .find_map(|ev| match ev {
            mn_meta::c2m_bridge::Event::InvalidTransfer {
                mc_tx_hash,
                amount,
                midnight_tx_hash,
            } => Some((mc_tx_hash, *amount, midnight_tx_hash)),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "Expected a C2MBridge::Event::InvalidTransfer in the block where the bridge \
                 transfer was processed. None was emitted, which means \
                 `execute_system_transaction` returned Err — most likely the ledger rejected \
                 `UnlockToTreasury {{ amount: {} }}` (check node logs for c2m-bridge errors). \
                 c2m_bridge_events observed: {:?}",
                BRIDGE_AMOUNT_STARS, c2m_bridge_calls.c2m_bridge_events
            )
        });

    let (mc_tx_hash, amount, _midnight_tx_hash) = invalid_transfer;
    assert_eq!(
        mc_tx_hash.0, bridge_tx,
        "InvalidTransfer.mc_tx_hash should match the Cardano tx id"
    );
    assert_eq!(
        amount, BRIDGE_AMOUNT_STARS,
        "InvalidTransfer.amount should equal the STAR amount transferred"
    );
}

/// Unapproved Cardano Tx is accounted as transfer to Midnight Trasury
#[e2e_test]
async fn unapproved_cardano_tx_makes_transfer_that_unlocks_to_treasury() {
    let _serial = lock_c2m_bridge_serial().await;

    let (cardano_client, midnight_client, prepared) = setup_and_prepare_bridge_transfer(
        BridgeTransferRecipient::Address(RECIPIENT_ADDRESS),
        BRIDGE_AMOUNT_STARS,
    )
    .await;
    let bridge_tx = prepared.tx_id;
    tracing::info!(bridge_tx = %hex::encode(&bridge_tx), "invalid bridge transfer signed (not yet submitted)");

    let min_midnight_block = midnight_client
        .get_finalized_block_number()
        .await
        .expect("Failed to read finalized head before submitting Cardano tx");
    cardano_client
        .submit_tx(prepared.signed_tx_bytes)
        .await
        .expect("Failed to submit invalid bridge transfer transaction to Cardano");
    tracing::info!(bridge_tx = %hex::encode(&bridge_tx), min_midnight_block, "invalid bridge transfer submitted on Cardano");

    let c2m_bridge_calls = wait_for_bridge_calls(&midnight_client, min_midnight_block).await;

    // ----- BridgeTransferV1 argument -----
    let transfer = c2m_bridge_calls
        .transfers
        .first()
        .expect("Expected at least one BridgeTransferV1 in the handle_transfers call");
    assert_eq!(
        transfer.mc_tx_hash.0, bridge_tx,
        "BridgeTransferV1.mc_tx_hash should match the Cardano tx id"
    );
    assert_eq!(
        transfer.amount, BRIDGE_AMOUNT_STARS,
        "BridgeTransferV1.amount should equal the STAR amount transferred"
    );
    let recipient_bytes = match &transfer.recipient {
        TransferRecipient::Address { recipient } => recipient.0.0.as_slice(),
        other => {
            panic!("BridgeTransferV1.recipient should be TransferRecipient::Address; got {other:?}")
        }
    };
    assert_eq!(
        recipient_bytes, &RECIPIENT_ADDRESS,
        "BridgeTransferV1.recipient should carry the original 32-byte recipient"
    );

    // ----- C2MBridge::Event::UnapprovedTransfer -----
    let invalid_transfer = c2m_bridge_calls
        .c2m_bridge_events
        .iter()
        .find_map(|ev| match ev {
            mn_meta::c2m_bridge::Event::UnapprovedTransfer {
                mc_tx_hash,
                amount,
                recipient,
                midnight_tx_hash,
            } => Some((mc_tx_hash, recipient, *amount, midnight_tx_hash)),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "Expected a C2MBridge::Event::InvalidTransfer in the block where the bridge \
                 transfer was processed. None was emitted, which means \
                 `execute_system_transaction` returned Err — most likely the ledger rejected \
                 `UnlockToTreasury {{ amount: {} }}` (check node logs for c2m-bridge errors). \
                 c2m_bridge_events observed: {:?}",
                BRIDGE_AMOUNT_STARS, c2m_bridge_calls.c2m_bridge_events
            )
        });

    // ----- UnlockToTreasury system transaction -----
    let _ = c2m_bridge_calls.system_transactions_applied.iter().find_map(|sta| {
        let sys_tx: SystemTransaction =
            deserialize(sta.0.serialized_system_transaction.as_slice()).ok()?;
        match sys_tx {
            SystemTransaction::UnlockToTreasury { amount } if amount == BRIDGE_AMOUNT_STARS as u128 => {
                Some(amount)
            }
            _ => None,
        }
    }).unwrap_or_else(||panic!("Expected SystemTransaction::UnlockToTreasury {{ amount }} for an invalid bridge transfer"));

    let (mc_tx_hash, recipient, amount, _midnight_tx_hash) = invalid_transfer;
    assert_eq!(
        mc_tx_hash.0, bridge_tx,
        "InvalidTransfer.mc_tx_hash should match the Cardano tx id"
    );
    assert_eq!(
        recipient.0.0.as_slice(),
        &RECIPIENT_ADDRESS,
        "UserTransfer.recipient should carry the original 32-byte recipient"
    );
    assert_eq!(
        amount, BRIDGE_AMOUNT_STARS,
        "InvalidTransfer.amount should equal the STAR amount transferred"
    );
}

/// Subminimal-transfer accumulation: three transfers of 999 STARS each, all
/// individually below `c_to_m_bridge_min_amount`. The first two accumulate in
/// `SubminimalTransfers` storage without producing a system tx; we sleep 10s
/// after the second to demonstrate the pallet does not flush spontaneously.
/// The third transfer pushes the running sum past the configured threshold
/// (2000 STARS), at which point the pallet emits:
///   * `Event::SubminimalFlushTransfer { amount: 2997, count: 3, midnight_tx_hash }`
///   * `SystemTransaction::UnlockToTreasury { amount: 2997 }`
/// and resets the accumulator.
#[e2e_test]
async fn subminimal_transfers_accumulate_and_flush_on_threshold_breach() {
    let _serial = lock_c2m_bridge_serial().await;

    /// Each individual transfer (< `c_to_m_bridge_min_amount=1000` STARS).
    const SUBMINIMAL_AMOUNT_STARS: u64 = 999;
    /// Flush threshold (STARS). Must satisfy
    /// `2 * SUBMINIMAL_AMOUNT_STARS < threshold < 3 * SUBMINIMAL_AMOUNT_STARS`
    /// so #1 and #2 accumulate but #3 flushes.
    const FLUSH_THRESHOLD_STARS: u64 = 2000;
    const EXPECTED_TOTAL_STARS: u128 = 3 * SUBMINIMAL_AMOUNT_STARS as u128;

    // Bring up a midnight client just to drive governance; subsequent
    // setup_and_prepare_bridge_transfer calls will mint fresh wallets per
    // transfer.
    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Update subminimal flush threshold to a small value, so it can be breached
    // with few subminimal transactions
    let current_threshold = read_subminimal_flush_threshold(&midnight_client)
        .await
        .expect("Failed to read SubminimalTransfersConfig from chain state");
    if current_threshold == FLUSH_THRESHOLD_STARS {
        tracing::info!("subminimal flush threshold already configured, skipping governance call");
    } else {
        tracing::info!("Updating subminimal flush threshold via governance");
        set_subminimal_threshold_via_governance(&midnight_client, FLUSH_THRESHOLD_STARS)
            .await
            .expect("Failed to set subminimal flush threshold via governance");
    }

    let mut bridge_tx_ids: Vec<[u8; 32]> = Vec::with_capacity(3);

    for i in 1..=3u8 {
        let (cardano_client, midnight_client, prepared) = setup_and_prepare_bridge_transfer(
            BridgeTransferRecipient::Address(RECIPIENT_ADDRESS),
            SUBMINIMAL_AMOUNT_STARS,
        )
        .await;

        bridge_tx_ids.push(prepared.tx_id);
        let min_midnight_block = midnight_client
            .get_finalized_block_number()
            .await
            .expect("Failed to read finalized head before submitting subminimal tx");
        cardano_client
            .submit_tx(prepared.signed_tx_bytes)
            .await
            .expect("Failed to submit subminimal bridge transfer to Cardano");

        let calls = wait_for_bridge_calls(&midnight_client, min_midnight_block).await;

        // The handle_transfers extrinsic for this Cardano tx must be observed.
        assert!(
            calls
                .transfers
                .iter()
                .any(|t| t.mc_tx_hash.0 == prepared.tx_id),
            "Expected to observe BridgeTransferV1 for subminimal #{} (mc_tx_hash {})",
            i,
            hex::encode(prepared.tx_id)
        );

        let flush_event = calls.c2m_bridge_events.iter().find_map(|ev| match ev {
            mn_meta::c2m_bridge::Event::SubminimalFlushTransfer {
                amount,
                count,
                midnight_tx_hash,
            } => Some((*amount, *count, midnight_tx_hash)),
            _ => None,
        });
        let unlock_amount = calls.system_transactions_applied.iter().find_map(|sta| {
            let sys_tx: SystemTransaction =
                deserialize(sta.0.serialized_system_transaction.as_slice()).ok()?;
            match sys_tx {
                SystemTransaction::UnlockToTreasury { amount } => Some(amount),
                _ => None,
            }
        });

        if i < 3 {
            assert!(
                flush_event.is_none(),
                "Subminimal transfer #{} (sum {} STARS) must NOT trigger a flush",
                i,
                i as u64 * SUBMINIMAL_AMOUNT_STARS,
            );
            assert!(
                unlock_amount.is_none(),
                "Subminimal transfer #{} must NOT produce an UnlockToTreasury system tx",
                i,
            );
        } else {
            // Final transfer: assert flush fires with the expected amounts.
            let (amount, count, _midnight_tx_hash) = flush_event
                .expect("Subminimal transfer #3 should trigger a SubminimalFlushTransfer event");
            assert_eq!(
                amount as u128, EXPECTED_TOTAL_STARS,
                "SubminimalFlushTransfer.amount should equal sum of all 3 subminimal transfers"
            );
            assert_eq!(
                count, 3,
                "SubminimalFlushTransfer.count should equal the number of transfers accumulated"
            );

            let unlock_amount = unlock_amount.expect(
                "Subminimal flush must produce an UnlockToTreasury system tx for the accumulated sum",
            );
            assert_eq!(
                unlock_amount, EXPECTED_TOTAL_STARS,
                "UnlockToTreasury amount should equal the total subminimal sum being flushed"
            );
        }
    }
}

/// Shared bootstrap for the c2m-bridge e2e tests
async fn setup_and_prepare_bridge_transfer(
    recipient: BridgeTransferRecipient,
    amount_stars: u64,
) -> (CardanoClient, MidnightClient, SignedBridgeTransaction) {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    let cardano_wallet_address = cardano_client.address_as_bech32();

    // Fund the wallet: one UTXO for collateral, one for minting and transfer
    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet
        .request_tokens(&cardano_wallet_address, 5_000_000)
        .await;

    let mint_amount: u64 = amount_stars * 2;
    let mint_tx_id = cardano_client
        .mint_tokens(mint_amount, &collateral_utxo)
        .await
        .expect("Failed to mint cNight tokens")
        .transaction
        .id;
    tracing::info!(amount_stars, "Minted test cNight to Cardano wallet");
    let cnight_utxo = cardano_client
        .find_utxo_by_tx_id(&cardano_wallet_address, hex::encode(mint_tx_id))
        .await
        .expect("No cNight UTXO found after minting");

    let payment_utxo = faucet
        .request_tokens(&cardano_wallet_address, 5_000_000)
        .await;

    let ics_address = midnight_client
        .ics_validator_address()
        .await
        .expect("Failed to read ICS validator address from Bridge pallet storage");

    let prepared = cardano_client
        .make_bridge_transfer(
            &cnight_utxo,
            &payment_utxo,
            &ics_address,
            amount_stars,
            recipient,
        )
        .await
        .expect("Failed to build bridge transfer transaction");

    (cardano_client, midnight_client, prepared)
}

/// Wait until midnight's cnight observer picks up a bridge transfer, ignoring
/// any block at or before `min_block_number`.
async fn wait_for_bridge_calls(
    midnight_client: &MidnightClient,
    min_block_number: u64,
) -> C2MBridgePalletCalls {
    midnight_client
        .subscribe_to_c2m_bridge_transfers(BRIDGE_OBSERVATION_TIMEOUT, min_block_number)
        .await
        .expect("Failed to observe bridge transfer handler calls")
}

const LOCAL_ENV_COUNCIL_KEYS: [&str; 3] = ["//Four", "//Five", "//Six"];

const LOCAL_ENV_TC_KEYS: [&str; 3] = ["//One", "//Two", "//Three"];

/// Run an arbitrary `RuntimeCall` through the local-env governance flow
/// (Council propose+vote+close → Technical Committee propose+vote+close →
/// FederatedAuthority motion close → Root call) so it executes with Root
/// origin. The call is built dynamically against the live metadata so callers
/// don't depend on a generated subxt builder existing for the target pallet.
async fn submit_via_governance(
    midnight: &MidnightClient,
    pallet: &str,
    call: &str,
    args: Vec<DynValue>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tx = subxt::dynamic::tx(pallet, call, args);
    let encoded_call = midnight.online_client().tx().await?.call_data(&tx)?;
    midnight_node_toolkit::commands::root_call::execute(
        midnight_node_toolkit::commands::root_call::RootCallArgs {
            rpc_url: midnight.base_url().to_string(),
            council_keys: LOCAL_ENV_COUNCIL_KEYS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            tc_keys: LOCAL_ENV_TC_KEYS.iter().map(|s| s.to_string()).collect(),
            encoded_call: Some(encoded_call),
            encoded_call_file: None,
        },
    )
    .await
}

/// Pre-approve a single Cardano tx hash via `C2MBridge.add_approved_mc_tx_hashes`.
async fn approve_mc_tx_hash_via_governance(
    midnight: &MidnightClient,
    mc_tx_hash: [u8; 32],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // BoundedVec<McTxHash, _> SCALE-encodes identically to Vec<McTxHash>;
    // McTxHash is a single-field tuple struct around `[u8; 32]`.
    let hashes_value = DynValue::unnamed_composite(vec![DynValue::unnamed_composite(vec![
        DynValue::from_bytes(mc_tx_hash.as_slice()),
    ])]);
    submit_via_governance(
        midnight,
        "C2MBridge",
        "add_approved_mc_tx_hashes",
        vec![hashes_value],
    )
    .await
}

/// Set the `SubminimalTransfersConfig.subminimal_transfers_flush_threshold` on
/// the c2m-bridge pallet. Reaches `set_subminimal_transfers_config` with Root
/// origin via governance.
async fn set_subminimal_threshold_via_governance(
    midnight: &MidnightClient,
    threshold: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // SubminimalTransfersConfig { subminimal_transfers_flush_threshold: u64 } is
    // a single-field struct, so the dynamic value is a named composite with the
    // one field.
    let config_value = DynValue::named_composite(vec![(
        "subminimal_transfers_flush_threshold",
        DynValue::u128(threshold as u128),
    )]);
    submit_via_governance(
        midnight,
        "C2MBridge",
        "set_subminimal_transfers_config",
        vec![config_value],
    )
    .await
}

/// Read the c2m-bridge `SubminimalTransfersConfiguration` storage value and
/// return its `subminimal_transfers_flush_threshold` field. Lets callers skip a
/// no-op governance round-trip when the chain already holds the desired value.
async fn read_subminimal_flush_threshold(
    midnight: &MidnightClient,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let storage_address = mn_meta::storage()
        .c2m_bridge()
        .subminimal_transfers_configuration();
    // ValueQuery storage: absent → use Default (= 0). Existing → decode.
    let threshold = match midnight
        .online_client()
        .at_current_block()
        .await?
        .storage()
        .try_fetch(&storage_address, ())
        .await?
    {
        Some(value) => value.decode()?.subminimal_transfers_flush_threshold,
        None => 0,
    };
    Ok(threshold)
}
