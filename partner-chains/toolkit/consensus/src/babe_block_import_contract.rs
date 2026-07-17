//! Contract ("canary") tests for the upstream behaviour [`PartnerChainsBlockImport`]
//! relies on, run against the real `sc_consensus_babe` block import and
//! `substrate-test-runtime`.
//!
//! [`PartnerChainsBlockImport`] withholds the block body from the wrapped consensus
//! import so that its body-gated inherent check is skipped while its header-based
//! consensus logic (epoch handling, equivocation reporting) still runs, and
//! [`PartnerChainsBodyRestore`] puts the body back for the imports beneath it. Nothing
//! in the `BlockImport` trait promises this split — it is an implicit contract with the
//! wrapped implementation. These tests pin that contract for BABE, so a polkadot-sdk
//! upgrade that changes it fails loudly here instead of silently on a live network.
//!
//! Unlike the Aura verifier (where the runtime's no-op `check_inherents` hides whether
//! the inner check ran), BABE's inherent check is directly observable: it creates
//! inherent data from its providers only inside the body gate, so a probe provider
//! records whether the check ran. The composed chain under test is the documented
//! sandwich, with a probe import recording what BABE passes downwards:
//!
//! ```text
//! PartnerChainsBlockImport<BabeBlockImport<Probe<PartnerChainsBodyRestore<Client>>>>
//! ```

use crate::{
	InherentDigest, PartnerChainsBlockImport, PartnerChainsBodyRestore, SlotExtractor,
	test_support::{TEST_DIGEST_VALUE, TestInherentDigest},
};
use parity_scale_codec::Encode;
use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::BlockBackend;
use sc_consensus::block_import::{
	BlockCheckParams, BlockImport, BlockImportParams, ForkChoiceStrategy, ImportResult,
};
use sc_consensus_babe::{BabeIntermediate, BabeLink, INTERMEDIATE_KEY};
use sc_consensus_epochs::descendent_query;
use sc_transaction_pool_api::{OffchainTransactionPoolFactory, RejectAllTxPool};
use sp_blockchain::HeaderBackend;
use sp_consensus::{BlockOrigin, Error as ConsensusError};
use sp_consensus_babe::BABE_ENGINE_ID;
use sp_consensus_babe::digests::{PreDigest, SecondaryPlainPreDigest};
use sp_consensus_slots::{Slot, SlotDuration};
use sp_inherents::{
	CreateInherentDataProviders, InherentData, InherentDataProvider, InherentIdentifier,
};
use sp_runtime::traits::Block as BlockT;
use sp_runtime::{Digest, DigestItem};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use substrate_test_runtime_client::{
	Backend, DefaultTestClientBuilderExt, TestClient, TestClientBuilder, TestClientBuilderExt,
	runtime::Block,
};

/// [`SlotExtractor`] reading the slot from the BABE pre-runtime digest, as a BABE-based
/// node would define it.
struct BabeSlotExtractor;

impl SlotExtractor<Block> for BabeSlotExtractor {
	fn extract_slot(header: &<Block as BlockT>::Header) -> Result<Slot, String> {
		sc_consensus_babe::find_pre_digest::<Block>(header)
			.map(|pre_digest| pre_digest.slot())
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

/// Inherent data provider passed to `BabeBlockImport` that records whether BABE created
/// inherent data. BABE creates its providers on every import (it reads the current slot
/// from them), but calls `provide_inherent_data` only from its body-gated inherent
/// check — so this flag observes directly whether that check ran.
struct InherentDataCreationProbe(Arc<AtomicBool>);

#[async_trait::async_trait]
impl InherentDataProvider for InherentDataCreationProbe {
	async fn provide_inherent_data(
		&self,
		_inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		self.0.store(true, Ordering::SeqCst);
		Ok(())
	}

	async fn try_handle_error(
		&self,
		_identifier: &InherentIdentifier,
		_error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}

/// The inherent data providers `BabeBlockImport` is configured with: the BABE slot
/// provider it requires, plus the recording probe.
struct BabeCIDP {
	slot_duration: SlotDuration,
	inherent_data_created: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl CreateInherentDataProviders<Block, ()> for BabeCIDP {
	type InherentDataProviders =
		(sp_consensus_babe::inherents::InherentDataProvider, InherentDataCreationProbe);

	async fn create_inherent_data_providers(
		&self,
		_parent: <Block as BlockT>::Hash,
		(): (),
	) -> Result<Self::InherentDataProviders, Box<dyn std::error::Error + Send + Sync>> {
		let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
		let slot =
			sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
				*timestamp,
				self.slot_duration,
			);
		Ok((slot, InherentDataCreationProbe(self.inherent_data_created.clone())))
	}
}

/// Records whether the block still had a body when BABE passed it downwards.
struct ProbeImport<Inner> {
	inner: Inner,
	body_seen_beneath_babe: Arc<Mutex<Option<bool>>>,
}

#[async_trait::async_trait]
impl<Inner: BlockImport<Block, Error = ConsensusError> + Send + Sync> BlockImport<Block>
	for ProbeImport<Inner>
{
	type Error = ConsensusError;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		block: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		*self.body_seen_beneath_babe.lock().unwrap() = Some(block.body.is_some());
		self.inner.import_block(block).await
	}
}

type BeneathBabe = ProbeImport<PartnerChainsBodyRestore<Arc<TestClient>, Block>>;
type BabeImport = sc_consensus_babe::BabeBlockImport<
	Block,
	TestClient,
	BeneathBabe,
	BabeCIDP,
	sc_consensus::LongestChain<Backend, Block>,
>;

struct TestBed {
	client: Arc<TestClient>,
	link: BabeLink<Block>,
	/// The real BABE import over `Probe<PartnerChainsBodyRestore<Client>>`.
	babe_import: Option<BabeImport>,
	babe_created_inherent_data: Arc<AtomicBool>,
	body_seen_beneath_babe: Arc<Mutex<Option<bool>>>,
	pc_recorded: Arc<Mutex<Vec<(Slot, u32)>>>,
}

impl TestBed {
	fn new() -> Self {
		let (client, select_chain) = TestClientBuilder::new().build_with_longest_chain();
		let client = Arc::new(client);
		let babe_created_inherent_data = Arc::new(AtomicBool::new(false));
		let body_seen_beneath_babe = Arc::new(Mutex::new(None));

		let beneath_babe = ProbeImport {
			inner: PartnerChainsBodyRestore::new(client.clone()),
			body_seen_beneath_babe: body_seen_beneath_babe.clone(),
		};
		let config =
			sc_consensus_babe::configuration(&*client).expect("BABE configuration is available");
		let (babe_import, link) = sc_consensus_babe::block_import(
			config.clone(),
			beneath_babe,
			client.clone(),
			BabeCIDP {
				slot_duration: config.slot_duration(),
				inherent_data_created: babe_created_inherent_data.clone(),
			},
			select_chain,
			OffchainTransactionPoolFactory::new(RejectAllTxPool::default()),
		)
		.expect("BABE block import can be created");

		Self {
			client,
			link,
			babe_import: Some(babe_import),
			babe_created_inherent_data,
			body_seen_beneath_babe,
			pc_recorded: Arc::new(Mutex::new(Vec::new())),
		}
	}

	/// The documented sandwich: [`PartnerChainsBlockImport`] wrapping the real BABE
	/// import, composed exactly as a BABE-based Partner Chains node would in its
	/// `new_partial`. Takes the BABE import out of the test bed; the observer handles
	/// remain usable.
	fn partner_chains_import(&mut self) -> PcBabeImport {
		PartnerChainsBlockImport::new(
			self.babe_import.take().expect("this should be called only once in test setup"),
			self.client.clone(),
			RecordingCIDP { recorded: self.pc_recorded.clone() },
		)
	}
}

type PcBabeImport = PartnerChainsBlockImport<
	BabeImport,
	TestClient,
	RecordingCIDP,
	Block,
	BabeSlotExtractor,
	TestInherentDigest,
>;

/// Builds an empty block on top of genesis carrying the BABE secondary-plain pre-digest
/// for `slot` (plus the Partner Chains digest unless disabled), with the epoch-descriptor
/// intermediate the BABE verifier would have attached.
fn importable_block(
	client: &TestClient,
	link: &BabeLink<Block>,
	slot: Slot,
	include_pc_digest: bool,
) -> BlockImportParams<Block> {
	let pre_digest =
		PreDigest::SecondaryPlain(SecondaryPlainPreDigest { authority_index: 0, slot });
	let mut logs = vec![DigestItem::PreRuntime(BABE_ENGINE_ID, pre_digest.encode())];
	if include_pc_digest {
		logs.extend(
			TestInherentDigest::from_inherent_data(&InherentData::new())
				.expect("Partner Chains digest can be created"),
		);
	}

	let genesis_hash = client.info().genesis_hash;
	let block = BlockBuilderBuilder::new(client)
		.on_parent_block(genesis_hash)
		.with_parent_block_number(0)
		.with_inherent_digests(Digest { logs })
		.build()
		.expect("block builder can be created")
		.build()
		.expect("empty block can be built")
		.block;

	let epoch_descriptor = link
		.epoch_changes()
		.shared_data()
		.epoch_descriptor_for_child_of(descendent_query(client), &genesis_hash, 0, slot)
		.expect("epoch descriptor lookup works")
		.expect("an epoch descriptor exists for a child of genesis");

	let (header, extrinsics) = block.deconstruct();
	let mut params = BlockImportParams::new(BlockOrigin::NetworkBroadcast, header);
	params.body = Some(extrinsics);
	params.insert_intermediate(INTERMEDIATE_KEY, BabeIntermediate::<Block> { epoch_descriptor });
	params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
	params
}

#[tokio::test]
async fn wrapped_babe_import_checks_pc_inherents_and_skips_babes_inherent_check() {
	let mut bed = TestBed::new();
	let block = importable_block(&bed.client, &bed.link, Slot::from(1), true);
	let block_hash = block.post_hash();
	let import = bed.partner_chains_import();

	let result = import.import_block(block).await.expect("import succeeds");
	assert!(matches!(result, ImportResult::Imported(_)));

	// The Partner Chains inherent check ran exactly once, parameterised by the slot and
	// digest value from the header.
	assert_eq!(*bed.pc_recorded.lock().unwrap(), vec![(Slot::from(1), TEST_DIGEST_VALUE)]);
	// Canary: BABE's body-gated inherent check did not run. If this fails after a
	// polkadot-sdk upgrade, BABE now checks inherents without the body gate and the
	// body-withholding approach no longer suppresses its check.
	assert!(
		!bed.babe_created_inherent_data.load(Ordering::SeqCst),
		"BABE must not create inherent data for a body-withheld block"
	);
	// BABE passed the block downwards without the body; the restore stage put it back
	// and the client stored the complete block.
	assert_eq!(*bed.body_seen_beneath_babe.lock().unwrap(), Some(false));
	let stored_body = bed.client.block_body(block_hash).expect("block body query works");
	assert_eq!(stored_body, Some(vec![]), "the client must store the restored (empty) body");
}

#[tokio::test]
async fn unwrapped_babe_import_runs_its_body_gated_inherent_check() {
	// Control for the canary above: importing through the real BABE import *without*
	// the Partner Chains wrapper (body present) must run its inherent check, proving
	// the probe actually observes it.
	let bed = TestBed::new();
	let block = importable_block(&bed.client, &bed.link, Slot::from(1), true);

	bed.babe_import
		.as_ref()
		.expect("the BABE import has not been taken")
		.import_block(block)
		.await
		.expect("import succeeds");

	assert!(
		bed.babe_created_inherent_data.load(Ordering::SeqCst),
		"BABE must run its inherent check when the body is present"
	);
	assert_eq!(*bed.body_seen_beneath_babe.lock().unwrap(), Some(true));
}

#[tokio::test]
async fn rejects_block_without_partner_chains_digest() {
	let mut bed = TestBed::new();
	let block = importable_block(&bed.client, &bed.link, Slot::from(1), false);
	let import = bed.partner_chains_import();

	let error = match import.import_block(block).await {
		Err(error) => error,
		Ok(_) => panic!("a consensus-valid block without the digest must still be rejected"),
	};

	assert!(
		matches!(
			&error,
			ConsensusError::ClientImport(message)
				if message.contains("Failed to retrieve inherent digest")
		),
		"unexpected error: {error}"
	);
	// The block was rejected before reaching BABE and the imports beneath it.
	assert!(bed.body_seen_beneath_babe.lock().unwrap().is_none());
}
