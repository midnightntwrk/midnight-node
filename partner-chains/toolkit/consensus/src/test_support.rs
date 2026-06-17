//! Scaffolding shared by the unit tests of [`crate::PartnerChainsVerifier`] and
//! [`crate::PartnerChainsBlockImport`].

use crate::{InherentDigest, SlotExtractor, VerificationContextSink};
use sc_consensus::block_import::BlockImportParams;
use sp_consensus::BlockOrigin;
use sp_consensus_slots::Slot;
use sp_inherents::InherentData;
use sp_runtime::generic::Header;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};
use sp_runtime::{DigestItem, OpaqueExtrinsic};
use std::sync::{Arc, Mutex};

pub(crate) type Block = sp_runtime::generic::Block<Header<u32, BlakeTwo256>, OpaqueExtrinsic>;

/// Slot [`TestSlotExtractor`] extracts from every header.
pub(crate) const TEST_SLOT: u64 = 7;
/// Value [`TestInherentDigest`] extracts from every header.
pub(crate) const TEST_DIGEST_VALUE: u32 = 42;

pub(crate) struct TestSlotExtractor;

impl SlotExtractor<Block> for TestSlotExtractor {
	fn extract_slot(_header: &<Block as BlockT>::Header) -> Result<Slot, String> {
		Ok(Slot::from(TEST_SLOT))
	}
}

pub(crate) struct TestInherentDigest;

impl InherentDigest for TestInherentDigest {
	type Value = u32;

	fn from_inherent_data(
		_inherent_data: &InherentData,
	) -> Result<Vec<DigestItem>, Box<dyn std::error::Error + Send + Sync>> {
		Ok(vec![])
	}

	fn value_from_digest(
		_digests: &[DigestItem],
	) -> Result<Self::Value, Box<dyn std::error::Error + Send + Sync>> {
		Ok(TEST_DIGEST_VALUE)
	}
}

/// Stand-in for the node's inherent-data-provider factory: records the verification context
/// the wrapper injects, in place of recreating inherent data (which the inner verifier/import
/// stub doesn't need). Cloneable so the test can hold one handle while the wrapper holds
/// another, both observing the same interior-mutable state — exactly the sharing the real
/// wiring relies on.
#[derive(Clone, Default)]
pub(crate) struct TestCIDP {
	context: Arc<Mutex<Option<(Slot, u32)>>>,
}

impl TestCIDP {
	/// The `(slot, inherent_digest)` last set on this CIDP, if any.
	pub(crate) fn taken_context(&self) -> Option<(Slot, u32)> {
		*self.context.lock().unwrap()
	}
}

impl VerificationContextSink<TestInherentDigest> for TestCIDP {
	fn set_verification_context(&self, slot: Slot, inherent_digest: u32) {
		*self.context.lock().unwrap() = Some((slot, inherent_digest));
	}
}

pub(crate) fn block_import_params(
	body: Option<Vec<<Block as BlockT>::Extrinsic>>,
) -> BlockImportParams<Block> {
	let header = Header {
		parent_hash: Default::default(),
		number: 1,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};
	let mut block = BlockImportParams::new(BlockOrigin::NetworkInitialSync, header);
	block.body = body;
	block
}
