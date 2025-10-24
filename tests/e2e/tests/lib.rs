use midnight_node_e2e::api::cardano::*;
use midnight_node_e2e::api::midnight::*;
use midnight_node_metadata::midnight_metadata_latest::{
	federated_authority_observation, native_token_observation,
};
use ogmios_client::query_ledger_state::QueryLedgerState;
use whisky::Asset;

#[tokio::test]
async fn register_for_dust_production() {
	let cardano_wallet = create_wallet();
	let bech32_address = get_cardano_address_as_bech32(&cardano_wallet);
	println!("New Cardano wallet created: {:?}", bech32_address);

	let dust_hex = new_dust_hex(32);
	println!("Registering Cardano wallet {} with DUST address {}", bech32_address, dust_hex);

	let collateral_utxo = make_collateral(&bech32_address).await;
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = fund_wallet(&bech32_address, assets).await;

	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(&[bech32_address.clone().into()]).await.unwrap();
	assert_eq!(utxos.len(), 2, "New wallet should have exactly two UTXOs after funding");

	let register_tx_id =
		register(&bech32_address, &dust_hex, &cardano_wallet, &tx_in, &collateral_utxo).await;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	let cardano_address = get_cardano_address_as_bytes(&cardano_wallet);
	let dust_address = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	let registration_events = subscribe_to_cngd_registration_extrinsic(&register_tx_id)
		.await
		.expect("Failed to listen to cNgD registration event");

	let registration = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<native_token_observation::events::Registration>().ok().flatten()
		})
		.find(|reg| {
			reg.0.cardano_address.0 == cardano_address && reg.0.dust_address == dust_address
		});
	assert!(
		registration.is_some(),
		"Did not find registration event with expected cardano_address and dust_address"
	);
	if let Some(registration) = registration {
		println!("Matching Registration event found: {:?}", registration);
	}

	let mapping_added = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<native_token_observation::events::MappingAdded>().ok().flatten()
		})
		.find(|map| {
			map.0.cardano_address.0 == cardano_address
				&& map.0.dust_address == dust_hex
				&& map.0.utxo_id == hex::encode(register_tx_id)
		});
	assert!(
		mapping_added.is_some(),
		"Did not find MappingAdded event with expected cardano_address, dust_address, and utxo_id"
	);
	if let Some(mapping) = mapping_added {
		println!("Matching MappingAdded event found: {:?}", mapping);
	}
}

#[tokio::test]
// #[ignore] // Remove this when ready to run the test with a live environment
async fn deploy_governance_contracts_and_validate_membership_reset() {
	println!("=== Starting Governance Contracts E2E Test ===");

	// Example Sr25519 public keys for testing (Alice and Bob from Substrate)
	// In production, these would be the actual governance committee member keys
	const ALICE_SR25519: &str = "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
	const BOB_SR25519: &str = "8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48";

	// Use the funded_address from config as the deployer
	// The funded_address owns the one-shot UTxOs, so we use it for all inputs to simplify signing
	use midnight_node_e2e::cfg::*;
	let cfg = load_config();
	let funded_address = cfg.payment_addr.clone();
	println!("Using funded_address for deployment: {}", funded_address);

	// Get funded_address Cardano pubkey hash for the multisig mapping
	// The funded_address key hash is: e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b
	let funded_cardano_hash = "e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b";

	// Fund UTxOs for deployment (these will be owned by funded_address)
	let funding_assets = vec![Asset::new_from_str("lovelace", "500000000")]; // 500 ADA
	let tx_in_utxo = fund_wallet(&funded_address, funding_assets.clone()).await;
	println!("First funding UTXO created");

	// Create additional funding UTxO for second deployment
	let tx_in_utxo_2 = fund_wallet(&funded_address, funding_assets).await;
	println!("Second funding UTXO created");

	// Create collateral for script transactions
	let collateral_utxo = make_collateral(&funded_address).await;
	println!("Collateral UTXO created");

	// Load contract CBORs and calculate addresses and policy IDs
	let council_cbor = get_council_forever_cbor();
	let council_address = get_council_forever_address();
	let council_policy_id = get_council_forever_policy_id();

	let tech_auth_cbor = get_tech_auth_forever_cbor();
	let tech_auth_address = get_tech_auth_forever_address();
	let tech_auth_policy_id = get_tech_auth_forever_policy_id();

	println!("Council Forever:");
	println!("  Policy ID (calculated): {}", council_policy_id);
	println!("  Address: {}", council_address);

	println!("Technical Authority Forever:");
	println!("  Policy ID (calculated): {}", tech_auth_policy_id);
	println!("  Address: {}", tech_auth_address);

	// Get pre-created one-shot UTxOs from local-environment
	// These are created by the Cardano entrypoint.sh script during network setup
	let council_one_shot = get_one_shot_utxo("council").await;
	println!("✓ Council one-shot UTXO retrieved from local-environment");

	let tech_auth_one_shot = get_one_shot_utxo("techauth").await;
	println!("✓ Technical Authority one-shot UTXO retrieved from local-environment");

	// Deploy Council Forever contract
	println!("\n=== Deploying Council Forever Contract ===");
	let council_members = vec![
		(funded_cardano_hash.to_string(), ALICE_SR25519.to_string()),
		(funded_cardano_hash.to_string(), BOB_SR25519.to_string()),
	];

	let _council_tx_id = deploy_governance_contract(
		&tx_in_utxo,
		&collateral_utxo,
		&council_one_shot,
		&council_cbor,
		&council_address,
		&council_policy_id,
		council_members.clone(),
		2, // total_signers
	)
	.await;

	println!("✓ Council Forever contract deployed successfully");

	// Deploy Technical Authority Forever contract
	println!("\n=== Deploying Technical Authority Forever Contract ===");
	let tech_auth_members = vec![
		(funded_cardano_hash.to_string(), ALICE_SR25519.to_string()),
		(funded_cardano_hash.to_string(), BOB_SR25519.to_string()),
	];

	let _tech_auth_tx_id = deploy_governance_contract(
		&tx_in_utxo_2,
		&collateral_utxo,
		&tech_auth_one_shot,
		&tech_auth_cbor,
		&tech_auth_address,
		&tech_auth_policy_id,
		tech_auth_members.clone(),
		2, // total_signers
	)
	.await;

	println!("✓ Technical Authority Forever contract deployed successfully");

	// Subscribe to federated authority observation events
	println!("Subscribing to federated authority events...");
	let events_result = subscribe_to_federated_authority_events().await;

	match events_result {
		Ok(events) => {
			println!("Successfully received federated authority events");

			// Verify CouncilMembersReset event
			let council_reset = events
				.iter()
				.filter_map(|e| e.ok())
				.filter_map(|evt| {
					evt.as_event::<federated_authority_observation::events::CouncilMembersReset>()
						.ok()
						.flatten()
				})
				.next();

			assert!(council_reset.is_some(), "CouncilMembersReset event not found");

			if let Some(event) = council_reset {
				println!(
					"✓ CouncilMembersReset event found with {} members",
					event.members.0.len()
				);
				// TODO: Verify the members match the expected Sr25519 public keys
			}

			// Verify TechnicalCommitteeMembersReset event
			let tech_committee_reset = events
				.iter()
				.filter_map(|e| e.ok())
				.filter_map(|evt| {
					evt.as_event::<federated_authority_observation::events::TechnicalCommitteeMembersReset>()
						.ok()
						.flatten()
				})
				.next();

			assert!(
				tech_committee_reset.is_some(),
				"TechnicalCommitteeMembersReset event not found"
			);

			if let Some(event) = tech_committee_reset {
				println!(
					"✓ TechnicalCommitteeMembersReset event found with {} members",
					event.members.0.len()
				);
				// TODO: Verify the members match the expected Sr25519 public keys
			}

			println!("=== Governance Contracts E2E Test PASSED ===");
		},
		Err(e) => {
			panic!("Failed to receive federated authority events: {}", e);
		},
	}
}
