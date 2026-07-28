use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::{self, Settings};
use midnight_node_e2e::e2e_test;

mod observation;

use crate::global_faucet_manager;

// -------- TESTS --------

#[e2e_test]
async fn alice_cannot_deregister_bob() {
    let settings = Settings::default();

    // Create Alice and Bob wallets
    let alice =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let bob = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let bob_bech32 = bob.address_as_bech32();
    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);

    // Fund Alice and Bob wallets
    let faucet = global_faucet_manager().await;
    let alice_collateral = faucet
        .request_tokens(&alice.address_as_bech32(), 5_000_000)
        .await;
    let deregister_tx_in = faucet
        .request_tokens(&alice.address_as_bech32(), 10_000_000)
        .await;
    let bob_collateral = faucet.request_tokens(&bob_bech32, 5_000_000).await;
    let register_tx_in = faucet.request_tokens(&bob_bech32, 10_000_000).await;

    // Bob registers his DUST address
    tracing::info!(
        "Registering Bob wallet {} with DUST address {}",
        bob_bech32,
        dust_hex
    );
    let register_tx_id = bob
        .register(&dust_hex, &register_tx_in, &bob_collateral)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    // Find Bob's registration UTXO
    let validator_address = config::mapping_validator_address();
    let register_tx = bob
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

    // Alice attempts to deregister Bob
    let deregister_tx = alice
        .deregister(&deregister_tx_in, &register_tx, &alice_collateral)
        .await;
    assert!(
        deregister_tx.is_err(),
        "Alice should not be able to deregister Bob"
    );

    // Check if Bob's registration still exists in mapping validator UTXOs
    let still_unspent = bob
        .is_utxo_unspent_for_3_blocks(&validator_address, &hex::encode(register_tx_id))
        .await;
    assert!(
        still_unspent,
        "Bob's registration UTXO should still be unspent"
    );
}
