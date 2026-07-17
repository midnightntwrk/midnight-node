//! Network integration test for [`PartnerChainsVerifier`], using
//! `sc_network_test::TestNetFactory`.
//!
//! Blocks are generated with the inherent digest and a test slot pre-digest directly
//! in their headers (no block production gadget), and the verifier wraps the framework's
//! `PassThroughVerifier` instead of a consensus verifier.
//!
//! One peer generates blocks; the other peers sync them through import queues running
//! [`PartnerChainsVerifier`], which must extract the slot and the inherent digest value
//! from each header and recreate the inherent data providers parameterised by them.
//! The test asserts the digest value stored at block creation round-trips to every
//! syncing peer's verifier.

use parity_scale_codec::{Decode, Encode};
use sc_consensus::{BoxJustificationImport, ForkChoiceStrategy};
use sc_network_test::{
	Block as TestBlock, BlockImportAdapter, PassThroughVerifier, Peer, PeersClient,
	PeersFullClient, TestNetFactory,
};
use sc_partner_chains_consensus::{InherentDigest, PartnerChainsVerifier, SlotExtractor};
use sp_consensus::BlockOrigin;
use sp_consensus_slots::Slot;
use sp_inherents::{CreateInherentDataProviders, InherentData, InherentIdentifier};
use sp_runtime::generic::BlockId;
use sp_runtime::traits::Block as BlockT;
use sp_runtime::{Digest, DigestItem};
use std::sync::{Arc, Mutex};

const BLOCK_COUNT: u64 = 5;

/// Inherent data carrying the slot number, standing in for a consensus slot provider.
const SLOT_INHERENT_ID: InherentIdentifier = *b"testslot";
/// Pre-runtime digest written by the (simulated) block production gadget.
const SLOT_PRE_DIGEST_ID: [u8; 4] = *b"slot";
/// Pre-runtime digest carrying the Partner Chains inherent digest value.
const PC_DIGEST_ID: [u8; 4] = *b"pcsl";

/// Stand-in for a Partner Chains inherent digest (e.g. the mainchain reference hash):
/// stores the slot from the inherent data in the header, so the test can verify the
/// digest value round-trips from block creation to verification on other peers.
struct SlotInherentDigest;

impl InherentDigest for SlotInherentDigest {
	type Value = u64;

	fn from_inherent_data(
		inherent_data: &InherentData,
	) -> Result<Vec<DigestItem>, Box<dyn std::error::Error + Send + Sync>> {
		let slot = inherent_data
			.get_data::<u64>(&SLOT_INHERENT_ID)?
			.ok_or("no slot in inherent data")?;
		Ok(vec![DigestItem::PreRuntime(PC_DIGEST_ID, slot.encode())])
	}

	fn value_from_digest(
		digests: &[DigestItem],
	) -> Result<Self::Value, Box<dyn std::error::Error + Send + Sync>> {
		decode_pre_digest(digests, PC_DIGEST_ID)
			.ok_or_else(|| "no inherent digest in header".into())
	}
}

/// [`SlotExtractor`] reading the slot from the test pre-runtime digest.
struct TestSlotExtractor;

impl SlotExtractor<TestBlock> for TestSlotExtractor {
	fn extract_slot(header: &<TestBlock as BlockT>::Header) -> Result<Slot, String> {
		decode_pre_digest(header.digest.logs(), SLOT_PRE_DIGEST_ID)
			.map(Slot::from)
			.ok_or_else(|| "no slot pre-digest in header".to_string())
	}
}

fn decode_pre_digest(digests: &[DigestItem], id: [u8; 4]) -> Option<u64> {
	digests.iter().find_map(|item| match item {
		DigestItem::PreRuntime(item_id, data) if *item_id == id => u64::decode(&mut &data[..]).ok(),
		_ => None,
	})
}

/// Records the `(slot, digest value)` pairs the verifier recreates inherent data with,
/// standing in for the Partner Chains data source backed providers of a real node.
struct RecordingVerifierCIDP {
	recorded: Arc<Mutex<Vec<(Slot, u64)>>>,
}

#[async_trait::async_trait]
impl CreateInherentDataProviders<TestBlock, (Slot, u64)> for RecordingVerifierCIDP {
	type InherentDataProviders = ();

	async fn create_inherent_data_providers(
		&self,
		_parent: <TestBlock as BlockT>::Hash,
		(slot, digest_value): (Slot, u64),
	) -> Result<Self::InherentDataProviders, Box<dyn std::error::Error + Send + Sync>> {
		self.recorded.lock().unwrap().push((slot, digest_value));
		Ok(())
	}
}

type TestVerifier = PartnerChainsVerifier<
	PassThroughVerifier,
	PeersFullClient,
	RecordingVerifierCIDP,
	TestBlock,
	TestSlotExtractor,
	SlotInherentDigest,
>;
type PartnerChainsPeer = Peer<(), PeersClient>;

#[derive(Default)]
struct PartnerChainsTestNet {
	peers: Vec<PartnerChainsPeer>,
	recorded: Arc<Mutex<Vec<(Slot, u64)>>>,
}

impl TestNetFactory for PartnerChainsTestNet {
	type Verifier = TestVerifier;
	type PeerData = ();
	type BlockImport = PeersClient;

	fn make_verifier(&self, client: PeersClient, _peer_data: &()) -> Self::Verifier {
		PartnerChainsVerifier::new(
			PassThroughVerifier::new(false),
			client.as_client(),
			RecordingVerifierCIDP { recorded: self.recorded.clone() },
		)
	}

	fn make_block_import(
		&self,
		client: PeersClient,
	) -> (
		BlockImportAdapter<Self::BlockImport>,
		Option<BoxJustificationImport<TestBlock>>,
		Self::PeerData,
	) {
		(client.as_block_import(), None, ())
	}

	fn peer(&mut self, i: usize) -> &mut PartnerChainsPeer {
		&mut self.peers[i]
	}

	fn peers(&self) -> &Vec<PartnerChainsPeer> {
		&self.peers
	}

	fn peers_mut(&mut self) -> &mut Vec<PartnerChainsPeer> {
		&mut self.peers
	}

	fn mut_peers<F: FnOnce(&mut Vec<PartnerChainsPeer>)>(&mut self, closure: F) {
		closure(&mut self.peers);
	}
}

/// Header digests for the block at height `number`: the slot pre-digest a block
/// production gadget would add, plus the Partner Chains inherent digest created from
/// the inherent data, as `PartnerChainsProposer` does during block proposal.
fn block_digests(number: u64) -> Digest {
	let slot = number;
	let mut inherent_data = InherentData::new();
	inherent_data
		.put_data(SLOT_INHERENT_ID, &slot)
		.expect("fresh inherent data accepts slot");

	let mut logs = vec![DigestItem::PreRuntime(SLOT_PRE_DIGEST_ID, slot.encode())];
	logs.extend(
		SlotInherentDigest::from_inherent_data(&inherent_data)
			.expect("inherent digest can be created from the slot inherent data"),
	);
	Digest { logs }
}

#[tokio::test]
async fn syncing_peers_verify_blocks_against_inherent_digests() {
	sp_tracing::try_init_simple();
	let mut net = PartnerChainsTestNet::new(3);
	let recorded = net.recorded.clone();

	let hashes = net.peer(0).generate_blocks_at_with_inherent_digests(
		BlockId::Number(0),
		BLOCK_COUNT as usize,
		BlockOrigin::File,
		|builder| builder.build().unwrap().block,
		|i| block_digests(i as u64 + 1),
		false,
		true,
		true,
		ForkChoiceStrategy::LongestChain,
	);

	net.run_until_sync().await;

	let best_hash = *hashes.last().expect("blocks were generated");
	assert!(net.peer(1).has_block(best_hash), "peer 1 should sync all blocks");
	assert!(net.peer(2).has_block(best_hash), "peer 2 should sync all blocks");

	let recorded = recorded.lock().unwrap();
	// Each peer verifies every generated block: peer 0 while generating, peers 1 and 2
	// while importing through their PartnerChainsVerifier-backed import queues.
	assert!(
		recorded.len() >= 3 * BLOCK_COUNT as usize,
		"every peer should have verified every block, got {} verifications",
		recorded.len()
	);
	for (slot, digest_value) in recorded.iter() {
		assert_eq!(
			u64::from(*slot),
			*digest_value,
			"digest value stored at block creation should round-trip to verification"
		);
	}
	for number in 1..=BLOCK_COUNT {
		assert!(
			recorded.iter().any(|(_, value)| *value == number),
			"block at height {number} should have been verified"
		);
	}
}
