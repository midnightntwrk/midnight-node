//! Contract ("canary") tests for the upstream behaviour [`PartnerChainsVerifier`] relies
//! on, run against the standard `sc_consensus_aura` verifier and `substrate-test-runtime`.
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
//! The body gate on Aura's inherent check is pinned by control tests that use the standard
//! Aura verifier (without [`PartnerChainsVerifier`]) around a client recording
//! `BlockBuilderApi::check_inherents` calls.

use crate::{
	InherentDigest, PartnerChainsVerifier, SlotExtractor,
	test_support::{TEST_DIGEST_VALUE, TestInherentDigest},
};
use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::backend::AuxStore;
use sc_consensus::block_import::{BlockImportParams, ForkChoiceStrategy, StateAction};
use sc_consensus::import_queue::Verifier;
use sp_api::{ApiRef, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus::BlockOrigin;
use sp_consensus_aura::AuraApi;
use sp_consensus_aura::sr25519::{AuthorityId, AuthorityPair, AuthoritySignature};
use sp_consensus_slots::Slot;
use sp_inherents::{CheckInherentsResult, CreateInherentDataProviders, InherentData};
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::traits::{Block as BlockT, Header as _, NumberFor};
use sp_runtime::{Digest, ExtrinsicInclusionMode, KeyTypeId};
use sp_version::RuntimeVersion;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use substrate_test_runtime_client::{TestClient, runtime::Block};

/// Client wrapper recording whether the runtime's `check_inherents` API was invoked.
///
/// Delegates everything else to the inner [`TestClient`] so the standard Aura verifier can run
/// unchanged. Only `BlockBuilderApi::check_inherents` sets the flag.
struct RecordingClient {
	inner: Arc<TestClient>,
	check_inherents_called: Arc<AtomicBool>,
}

impl RecordingClient {
	fn new(inner: Arc<TestClient>, check_inherents_called: Arc<AtomicBool>) -> Self {
		Self { inner, check_inherents_called }
	}
}

struct RecordingRuntimeApi {
	inner: Arc<TestClient>,
	check_inherents_called: Arc<AtomicBool>,
}

impl BlockBuilderApi<Block> for RecordingRuntimeApi {
	fn __runtime_api_internal_call_api_at(
		&self,
		at: <Block as BlockT>::Hash,
		params: Vec<u8>,
		fn_name: &dyn Fn(RuntimeVersion) -> &'static str,
	) -> Result<Vec<u8>, sp_api::ApiError> {
		BlockBuilderApi::<Block>::__runtime_api_internal_call_api_at(
			&*self.inner.runtime_api(),
			at,
			params,
			fn_name,
		)
	}

	fn apply_extrinsic(
		&self,
		at: <Block as BlockT>::Hash,
		extrinsic: <Block as BlockT>::Extrinsic,
	) -> Result<sp_runtime::ApplyExtrinsicResult, sp_api::ApiError> {
		self.inner.runtime_api().apply_extrinsic(at, extrinsic)
	}

	fn finalize_block(
		&self,
		at: <Block as BlockT>::Hash,
	) -> Result<<Block as BlockT>::Header, sp_api::ApiError> {
		self.inner.runtime_api().finalize_block(at)
	}

	fn inherent_extrinsics(
		&self,
		at: <Block as BlockT>::Hash,
		inherent: InherentData,
	) -> Result<Vec<<Block as BlockT>::Extrinsic>, sp_api::ApiError> {
		self.inner.runtime_api().inherent_extrinsics(at, inherent)
	}

	fn check_inherents(
		&self,
		at: <Block as BlockT>::Hash,
		block: <Block as BlockT>::LazyBlock,
		data: InherentData,
	) -> Result<CheckInherentsResult, sp_api::ApiError> {
		self.check_inherents_called.store(true, Ordering::SeqCst);
		self.inner.runtime_api().check_inherents(at, block, data)
	}
}

impl AuraApi<Block, AuthorityId> for RecordingRuntimeApi {
	fn __runtime_api_internal_call_api_at(
		&self,
		at: <Block as BlockT>::Hash,
		params: Vec<u8>,
		fn_name: &dyn Fn(RuntimeVersion) -> &'static str,
	) -> Result<Vec<u8>, sp_api::ApiError> {
		AuraApi::<Block, AuthorityId>::__runtime_api_internal_call_api_at(
			&*self.inner.runtime_api(),
			at,
			params,
			fn_name,
		)
	}

	fn authorities(
		&self,
		at: <Block as BlockT>::Hash,
	) -> Result<Vec<AuthorityId>, sp_api::ApiError> {
		self.inner.runtime_api().authorities(at)
	}

	fn slot_duration(
		&self,
		at: <Block as BlockT>::Hash,
	) -> Result<sp_consensus_aura::SlotDuration, sp_api::ApiError> {
		self.inner.runtime_api().slot_duration(at)
	}
}

impl sp_api::Core<Block> for RecordingRuntimeApi {
	fn __runtime_api_internal_call_api_at(
		&self,
		at: <Block as BlockT>::Hash,
		params: Vec<u8>,
		fn_name: &dyn Fn(RuntimeVersion) -> &'static str,
	) -> Result<Vec<u8>, sp_api::ApiError> {
		sp_api::Core::<Block>::__runtime_api_internal_call_api_at(
			&*self.inner.runtime_api(),
			at,
			params,
			fn_name,
		)
	}

	fn version(&self, at: <Block as BlockT>::Hash) -> Result<RuntimeVersion, sp_api::ApiError> {
		self.inner.runtime_api().version(at)
	}

	fn execute_block(
		&self,
		at: <Block as BlockT>::Hash,
		block: <Block as BlockT>::LazyBlock,
	) -> Result<(), sp_api::ApiError> {
		self.inner.runtime_api().execute_block(at, block)
	}

	fn initialize_block(
		&self,
		at: <Block as BlockT>::Hash,
		header: &<Block as BlockT>::Header,
	) -> Result<ExtrinsicInclusionMode, sp_api::ApiError> {
		self.inner.runtime_api().initialize_block(at, header)
	}
}

impl sp_api::ApiExt<Block> for RecordingRuntimeApi {
	fn execute_in_transaction<F: FnOnce(&Self) -> sp_api::TransactionOutcome<R>, R>(
		&self,
		call: F,
	) -> R {
		call(self).into_inner()
	}

	fn has_api<A: sp_api::RuntimeApiInfo + ?Sized>(
		&self,
		at_hash: <Block as BlockT>::Hash,
	) -> Result<bool, sp_api::ApiError> {
		self.inner.runtime_api().has_api::<A>(at_hash)
	}

	fn has_api_with<A: sp_api::RuntimeApiInfo + ?Sized, P: Fn(u32) -> bool>(
		&self,
		at_hash: <Block as BlockT>::Hash,
		pred: P,
	) -> Result<bool, sp_api::ApiError> {
		self.inner.runtime_api().has_api_with::<A, P>(at_hash, pred)
	}

	fn api_version<A: sp_api::RuntimeApiInfo + ?Sized>(
		&self,
		at_hash: <Block as BlockT>::Hash,
	) -> Result<Option<u32>, sp_api::ApiError> {
		self.inner.runtime_api().api_version::<A>(at_hash)
	}

	fn record_proof(&mut self) {
		self.inner.runtime_api().record_proof();
	}

	fn record_proof_with_recorder(&mut self, recorder: sp_api::ProofRecorder<Block>) {
		self.inner.runtime_api().record_proof_with_recorder(recorder);
	}

	fn extract_proof(&mut self) -> Option<sp_api::StorageProof> {
		self.inner.runtime_api().extract_proof()
	}

	fn proof_recorder(&self) -> Option<sp_api::ProofRecorder<Block>> {
		self.inner.runtime_api().proof_recorder()
	}

	fn into_storage_changes<B: sp_state_machine::Backend<sp_runtime::traits::HashingFor<Block>>>(
		&self,
		backend: &B,
		parent_hash: <Block as BlockT>::Hash,
	) -> Result<sp_api::StorageChanges<Block>, String> {
		self.inner.runtime_api().into_storage_changes(backend, parent_hash)
	}

	fn set_call_context(&mut self, call_context: sp_api::CallContext) {
		self.inner.runtime_api().set_call_context(call_context);
	}

	fn register_extension<E: sp_externalities::Extension>(&mut self, extension: E) {
		self.inner.runtime_api().register_extension(extension);
	}

	fn set_overlayed_changes(
		&mut self,
		changes: sp_state_machine::OverlayedChanges<sp_runtime::traits::HashingFor<Block>>,
	) {
		self.inner.runtime_api().set_overlayed_changes(changes);
	}
}

impl ProvideRuntimeApi<Block> for RecordingClient {
	type Api = RecordingRuntimeApi;

	fn runtime_api(&self) -> ApiRef<'_, Self::Api> {
		RecordingRuntimeApi {
			inner: self.inner.clone(),
			check_inherents_called: self.check_inherents_called.clone(),
		}
		.into()
	}
}

impl HeaderBackend<Block> for RecordingClient {
	fn header(
		&self,
		hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<Option<<Block as BlockT>::Header>> {
		self.inner.header(hash)
	}

	fn info(&self) -> sp_blockchain::Info<Block> {
		self.inner.info()
	}

	fn status(
		&self,
		hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
		self.inner.status(hash)
	}

	fn number(
		&self,
		hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<Option<NumberFor<Block>>> {
		self.inner.number(hash)
	}

	fn hash(
		&self,
		number: NumberFor<Block>,
	) -> sp_blockchain::Result<Option<<Block as BlockT>::Hash>> {
		self.inner.hash(number)
	}
}

impl HeaderMetadata<Block> for RecordingClient {
	type Error = <TestClient as HeaderMetadata<Block>>::Error;

	fn header_metadata(
		&self,
		hash: <Block as BlockT>::Hash,
	) -> Result<sp_blockchain::CachedHeaderMetadata<Block>, Self::Error> {
		self.inner.header_metadata(hash)
	}

	fn insert_header_metadata(
		&self,
		hash: <Block as BlockT>::Hash,
		header_metadata: sp_blockchain::CachedHeaderMetadata<Block>,
	) {
		self.inner.insert_header_metadata(hash, header_metadata)
	}

	fn remove_header_metadata(&self, hash: <Block as BlockT>::Hash) {
		self.inner.remove_header_metadata(hash)
	}
}

impl AuxStore for RecordingClient {
	fn insert_aux<
		'a,
		'b: 'a,
		'c: 'a,
		I: IntoIterator<Item = &'a (&'c [u8], &'c [u8])>,
		D: IntoIterator<Item = &'a &'b [u8]>,
	>(
		&self,
		insert: I,
		delete: D,
	) -> sp_blockchain::Result<()> {
		self.inner.insert_aux(insert, delete)
	}

	fn get_aux(&self, key: &[u8]) -> sp_blockchain::Result<Option<Vec<u8>>> {
		self.inner.get_aux(key)
	}
}

const AURA_KEY_TYPE: KeyTypeId = KeyTypeId(*b"aura");

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

/// The standard Aura verifier wrapped in [`PartnerChainsVerifier`], composed exactly as in
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

	PartnerChainsVerifier::<_, _, _, _, AuraSlotExtractor, TestInherentDigest>::new(
		aura_verifier,
		client,
		RecordingCIDP { recorded },
	)
}

/// The standard Aura verifier alone, for control tests that observe its body-gated inherent check.
fn aura_verifier(client: Arc<RecordingClient>) -> impl Verifier<Block> {
	let slot_duration =
		sc_consensus_aura::slot_duration(&*client.inner).expect("slot duration is available");

	sc_consensus_aura::build_verifier::<AuthorityPair, _, _, _>(
		sc_consensus_aura::BuildVerifierParams {
			client,
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
			TestInherentDigest::from_inherent_data(&InherentData::new())
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

	// The standard Aura verifier ran on the withheld-body block: the seal was moved from
	// the header into the post-digests and the fork choice was set.
	assert_eq!(verified.post_digests.len(), 1);
	assert_eq!(verified.header.digest().logs().len(), 2);
	assert_eq!(verified.fork_choice, Some(ForkChoiceStrategy::LongestChain));
	// The body was restored for the import pipeline.
	assert!(verified.body.is_some());
	// The Partner Chains inherent check ran exactly once, parameterised by the slot and
	// digest value from the header.
	assert_eq!(*recorded.lock().unwrap(), vec![(slot, TEST_DIGEST_VALUE)]);
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

#[tokio::test]
async fn standard_aura_verifier_runs_its_body_gated_inherent_check() {
	// Control for the canary above: verifying through the standard Aura verifier, without
	// the Partner Chains wrapper (body present) must call `check_inherents`, proving the
	// recording client actually observes the body gate.
	let inner = Arc::new(substrate_test_runtime_client::new());
	let keystore: KeystorePtr = Arc::new(sp_keystore::testing::MemoryKeystore::new());
	let alice = generate_key(&keystore, "//Alice");
	let slot = latest_slot_of(&inner, &alice);
	let check_inherents_called = Arc::new(AtomicBool::new(false));
	let client = Arc::new(RecordingClient::new(inner.clone(), check_inherents_called.clone()));
	let verifier = aura_verifier(client);

	let block = sealed_block(&inner, &keystore, slot, false, &alice);
	verifier.verify(block).await.expect("verification succeeds");

	assert!(
		check_inherents_called.load(Ordering::SeqCst),
		"Aura must call check_inherents when the body is present"
	);
}

#[tokio::test]
async fn standard_aura_verifier_skips_inherent_check_without_body() {
	let inner = Arc::new(substrate_test_runtime_client::new());
	let keystore: KeystorePtr = Arc::new(sp_keystore::testing::MemoryKeystore::new());
	let alice = generate_key(&keystore, "//Alice");
	let slot = latest_slot_of(&inner, &alice);
	let check_inherents_called = Arc::new(AtomicBool::new(false));
	let client = Arc::new(RecordingClient::new(inner.clone(), check_inherents_called.clone()));
	let verifier = aura_verifier(client);

	let mut block = sealed_block(&inner, &keystore, slot, false, &alice);
	block.body = None;
	verifier.verify(block).await.expect("verification succeeds");

	assert!(
		!check_inherents_called.load(Ordering::SeqCst),
		"Aura must not call check_inherents for a header-only block"
	);
}

#[tokio::test]
async fn skip_execution_checks_bypasses_partner_chains_check_and_delegates_to_aura() {
	// Pins the warp-sync / gap-sync short-circuit in PartnerChainsVerifier: when the
	// import skips execution, the wrapper must hand the block (body included) to the
	// inner Aura verifier and must not run the Partner Chains inherent check. Aura
	// itself short-circuits the same way; if either side starts requiring a full
	// inherent check here, warp sync for new nodes wedges.
	let client = Arc::new(substrate_test_runtime_client::new());
	let keystore: KeystorePtr = Arc::new(sp_keystore::testing::MemoryKeystore::new());
	let alice = generate_key(&keystore, "//Alice");
	let slot = latest_slot_of(&client, &alice);
	let recorded = Arc::new(Mutex::new(Vec::new()));
	let verifier = partner_chains_aura_verifier(client.clone(), recorded.clone());

	let mut block = sealed_block(&client, &keystore, slot, true, &alice);
	block.state_action = StateAction::Skip;
	let verified = verifier.verify(block).await.expect("skip-execution verification succeeds");

	assert!(verified.body.is_some(), "body must be preserved through the short-circuit");
	assert!(
		recorded.lock().unwrap().is_empty(),
		"the Partner Chains check must not run when execution checks are skipped"
	);
	// Aura's short-circuit sets fork choice from with_state() (false for StateAction::Skip).
	assert_eq!(verified.fork_choice, Some(ForkChoiceStrategy::Custom(false)));
}
