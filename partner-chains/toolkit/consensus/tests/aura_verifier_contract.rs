//! Contract ("canary") tests for the upstream behaviour [`PartnerChainsVerifier`] relies
//! on, run against the real `sc_consensus_aura` verifier and `substrate-test-runtime`.
//!
//! [`PartnerChainsVerifier`] withholds the block body from the wrapped verifier so that
//! its body-gated inherent check is skipped while all of its header-level consensus
//! checks still run. Nothing in the `Verifier` trait promises this split — it is an
//! implicit contract with the wrapped implementation. These tests pin that contract for
//! Aura, so a polkadot-sdk upgrade that changes it fails loudly here instead of silently
//! on a live network:
//!
//! * a correctly sealed block carrying a Partner Chains digest passes body-withheld
//!   verification, and the Partner Chains inherent check runs with the slot and digest
//!   value extracted from the header;
//! * a block sealed by the wrong authority is rejected, proving the seal check still
//!   runs even though the inner verifier never sees the body.
//!
//! Not covered (not observable with the runtime's no-op `check_inherents`): an upstream
//! change that moves the inner verifier's inherent check off the body gate.

use parity_scale_codec::{Decode, Encode};
use sc_block_builder::BlockBuilderBuilder;
use sc_consensus::block_import::{BlockImportParams, ForkChoiceStrategy};
use sc_consensus::import_queue::Verifier;
use sc_partner_chains_consensus::{InherentDigest, PartnerChainsVerifier, SlotExtractor};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_consensus_aura::AuraApi;
use sp_consensus_aura::sr25519::{AuthorityId, AuthorityPair, AuthoritySignature};
use sp_consensus_slots::Slot;
use sp_inherents::{CreateInherentDataProviders, InherentData};
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::traits::{Block as BlockT, Header as _};
use sp_runtime::{Digest, DigestItem, KeyTypeId};
use std::sync::{Arc, Mutex};
use substrate_test_runtime_client::{TestClient, runtime::Block};

const AURA_KEY_TYPE: KeyTypeId = KeyTypeId(*b"aura");

/// Pre-runtime digest standing in for a Partner Chains inherent digest (e.g. `mcsh`).
const PC_DIGEST_ID: [u8; 4] = *b"pcsh";
const PC_DIGEST_VALUE: u32 = 42;

struct PcDigest;

impl InherentDigest for PcDigest {
	type Value = u32;

	fn from_inherent_data(
		_inherent_data: &InherentData,
	) -> Result<Vec<DigestItem>, Box<dyn std::error::Error + Send + Sync>> {
		Ok(vec![DigestItem::PreRuntime(PC_DIGEST_ID, PC_DIGEST_VALUE.encode())])
	}

	fn value_from_digest(
		digests: &[DigestItem],
	) -> Result<Self::Value, Box<dyn std::error::Error + Send + Sync>> {
		digests
			.iter()
			.find_map(|item| match item {
				DigestItem::PreRuntime(id, data) if *id == PC_DIGEST_ID => {
					u32::decode(&mut &data[..]).ok()
				},
				_ => None,
			})
			.ok_or_else(|| "no Partner Chains inherent digest in header".into())
	}
}

/// [`SlotExtractor`] reading the slot from the Aura pre-runtime digest, as the nodes
/// define it in their `service.rs`.
struct AuraSlotExtractor;

impl SlotExtractor<Block> for AuraSlotExtractor {
	fn extract_slot(header: &<Block as BlockT>::Header) -> Result<Slot, String> {
		sc_consensus_aura::find_pre_digest::<Block, AuthoritySignature>(header)
			.map_err(|e| e.to_string())
	}
}

/// Records the `(slot, digest value)` pairs the Partner Chains inherent check recreates
/// inherent data with.
struct RecordingCIDP {
	recorded: Arc<Mutex<Vec<(Slot, u32)>>>,
}

#[async_trait::async_trait]
impl CreateInherentDataProviders<Block, (Slot, u32)> for RecordingCIDP {
	type InherentDataProviders = ();

	async fn create_inherent_data_providers(
		&self,
		_parent: <Block as BlockT>::Hash,
		(slot, digest_value): (Slot, u32),
	) -> Result<Self::InherentDataProviders, Box<dyn std::error::Error + Send + Sync>> {
		self.recorded.lock().unwrap().push((slot, digest_value));
		Ok(())
	}
}

/// The real Aura verifier wrapped in [`PartnerChainsVerifier`], composed exactly as in
/// the nodes' `new_partial`.
fn partner_chains_aura_verifier(
	client: Arc<TestClient>,
	recorded: Arc<Mutex<Vec<(Slot, u32)>>>,
) -> impl Verifier<Block> {
	let slot_duration =
		sc_consensus_aura::slot_duration(&*client).expect("slot duration is available");

	let aura_verifier = sc_consensus_aura::build_verifier::<AuthorityPair, _, _, _>(
		sc_consensus_aura::BuildVerifierParams {
			client: client.clone(),
			create_inherent_data_providers: move |_parent_hash, ()| async move {
				let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
				let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
					*timestamp,
					slot_duration,
				);
				Ok((slot, timestamp))
			},
			check_for_equivocation: Default::default(),
			telemetry: None,
			compatibility_mode: Default::default(),
		},
	);

	PartnerChainsVerifier::<_, _, _, _, AuraSlotExtractor, PcDigest>::new(
		aura_verifier,
		client,
		RecordingCIDP { recorded },
	)
}

/// The most recent slot (not in the future, so the verifier accepts it) whose expected
/// author is `author`.
fn latest_slot_of(client: &TestClient, author: &AuthorityId) -> Slot {
	let genesis_hash = client.info().genesis_hash;
	let authorities: Vec<AuthorityId> = client
		.runtime_api()
		.authorities(genesis_hash)
		.expect("AuraApi::authorities is callable");
	let slot_duration =
		sc_consensus_aura::slot_duration(client).expect("slot duration is available");
	let slot_now = Slot::from_timestamp(
		*sp_timestamp::InherentDataProvider::from_system_time(),
		slot_duration,
	);

	(u64::from(slot_now).saturating_sub(authorities.len() as u64 - 1)..=u64::from(slot_now))
		.map(Slot::from)
		.find(|slot| {
			sc_consensus_aura::standalone::slot_author::<AuthorityPair>(*slot, &authorities)
				== Some(author)
		})
		.expect("one of the last `authorities.len()` slots belongs to the author")
}

/// Builds an empty block on top of genesis carrying the Aura pre-digest for `slot`
/// (plus the Partner Chains digest unless disabled) and seals it with `seal_with`'s key.
fn sealed_block(
	client: &TestClient,
	keystore: &KeystorePtr,
	slot: Slot,
	include_pc_digest: bool,
	seal_with: &AuthorityId,
) -> BlockImportParams<Block> {
	let mut logs = vec![sc_consensus_aura::standalone::pre_digest::<AuthorityPair>(slot)];
	if include_pc_digest {
		logs.extend(
			PcDigest::from_inherent_data(&InherentData::new())
				.expect("Partner Chains digest can be created"),
		);
	}

	let block = BlockBuilderBuilder::new(client)
		.on_parent_block(client.info().genesis_hash)
		.with_parent_block_number(0)
		.with_inherent_digests(Digest { logs })
		.build()
		.expect("block builder can be created")
		.build()
		.expect("empty block can be built")
		.block;

	let (mut header, extrinsics) = block.deconstruct();
	let seal = sc_consensus_aura::standalone::seal::<_, AuthorityPair>(
		&header.hash(),
		seal_with,
		keystore,
	)
	.expect("keystore holds the sealing key");
	header.digest_mut().push(seal);

	let mut params = BlockImportParams::new(BlockOrigin::NetworkInitialSync, header);
	params.body = Some(extrinsics);
	params
}

fn generate_key(keystore: &KeystorePtr, seed: &str) -> AuthorityId {
	AuthorityId::from(
		keystore
			.sr25519_generate_new(AURA_KEY_TYPE, Some(seed))
			.expect("generating a key works"),
	)
}

#[tokio::test]
async fn accepts_sealed_block_and_runs_partner_chains_check_with_header_extracted_values() {
	let client = Arc::new(substrate_test_runtime_client::new());
	let keystore: KeystorePtr = Arc::new(sp_keystore::testing::MemoryKeystore::new());
	let alice = generate_key(&keystore, "//Alice");
	let slot = latest_slot_of(&client, &alice);
	let recorded = Arc::new(Mutex::new(Vec::new()));
	let verifier = partner_chains_aura_verifier(client.clone(), recorded.clone());

	let block = sealed_block(&client, &keystore, slot, true, &alice);
	let verified = verifier.verify(block).await.expect("verification succeeds");

	// The real Aura verifier ran on the withheld-body block: the seal was moved from
	// the header into the post-digests and the fork choice was set.
	assert_eq!(verified.post_digests.len(), 1);
	assert_eq!(verified.header.digest().logs().len(), 2);
	assert_eq!(verified.fork_choice, Some(ForkChoiceStrategy::LongestChain));
	// The body was restored for the import pipeline.
	assert!(verified.body.is_some());
	// The Partner Chains inherent check ran exactly once, parameterised by the slot and
	// digest value from the header.
	assert_eq!(*recorded.lock().unwrap(), vec![(slot, PC_DIGEST_VALUE)]);
}

#[tokio::test]
async fn rejects_block_sealed_by_the_wrong_authority_despite_withheld_body() {
	let client = Arc::new(substrate_test_runtime_client::new());
	let keystore: KeystorePtr = Arc::new(sp_keystore::testing::MemoryKeystore::new());
	let alice = generate_key(&keystore, "//Alice");
	let bob = generate_key(&keystore, "//Bob");
	let slot = latest_slot_of(&client, &alice);
	let recorded = Arc::new(Mutex::new(Vec::new()));
	let verifier = partner_chains_aura_verifier(client.clone(), recorded.clone());

	// Bob seals a block whose slot belongs to Alice: the wrapped Aura verifier must
	// reject it, proving its header-level checks run even without the body. If this
	// starts passing after a polkadot-sdk upgrade, body-withheld verification has
	// become a pass-through and PartnerChainsVerifier must not be used with it.
	let block = sealed_block(&client, &keystore, slot, true, &bob);
	let result = verifier.verify(block).await;

	assert!(result.is_err(), "a wrongly sealed block must be rejected");
	assert!(
		recorded.lock().unwrap().is_empty(),
		"the Partner Chains check must not run for a block failing consensus checks"
	);
}

#[tokio::test]
async fn rejects_block_without_partner_chains_digest() {
	let client = Arc::new(substrate_test_runtime_client::new());
	let keystore: KeystorePtr = Arc::new(sp_keystore::testing::MemoryKeystore::new());
	let alice = generate_key(&keystore, "//Alice");
	let slot = latest_slot_of(&client, &alice);
	let recorded = Arc::new(Mutex::new(Vec::new()));
	let verifier = partner_chains_aura_verifier(client.clone(), recorded.clone());

	let block = sealed_block(&client, &keystore, slot, false, &alice);
	let error = match verifier.verify(block).await {
		Err(error) => error,
		Ok(_) => panic!("a correctly sealed block without the digest must still be rejected"),
	};

	assert!(error.contains("Failed to retrieve inherent digest"), "unexpected error: {error}");
}
