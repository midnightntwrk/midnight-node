//! Scaffolding for the unit tests of [`crate::PartnerChainsVerifier`] and
//! [`crate::PartnerChainsBlockImport`], and for the Aura/BABE contract canaries.

use crate::{InherentDigest, SlotExtractor};
use parity_scale_codec::{Decode, Encode};
use sc_consensus::block_import::BlockImportParams;
use sp_api::{ApiRef, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus::BlockOrigin;
use sp_consensus_slots::Slot;
use sp_inherents::{CheckInherentsResult, InherentData, InherentDataProvider, InherentIdentifier};
use sp_runtime::generic::Header;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};
use sp_runtime::{Digest, DigestItem, OpaqueExtrinsic};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) type Block = sp_runtime::generic::Block<Header<u32, BlakeTwo256>, OpaqueExtrinsic>;

/// Slot [`TestSlotExtractor`] extracts from every header.
pub(crate) const TEST_SLOT: u64 = 7;
/// Pre-runtime digest ID standing in for a Partner Chains inherent digest (e.g. `mcsh`).
const TEST_DIGEST_ID: [u8; 4] = *b"pcsh";
/// Value [`TestInherentDigest`] extracts from every header.
pub(crate) const TEST_DIGEST_VALUE: u32 = 42;

pub(crate) struct TestSlotExtractor;

impl SlotExtractor<Block> for TestSlotExtractor {
	fn extract_slot(_header: &<Block as BlockT>::Header) -> Result<Slot, String> {
		Ok(Slot::from(TEST_SLOT))
	}
}

/// Stand-in for a Partner Chains inherent digest (e.g. the mainchain reference hash).
pub(crate) struct TestInherentDigest;

impl InherentDigest for TestInherentDigest {
	type Value = u32;

	fn from_inherent_data(
		_inherent_data: &InherentData,
	) -> Result<Vec<DigestItem>, Box<dyn std::error::Error + Send + Sync>> {
		Ok(vec![DigestItem::PreRuntime(TEST_DIGEST_ID, TEST_DIGEST_VALUE.encode())])
	}

	fn value_from_digest(
		digests: &[DigestItem],
	) -> Result<Self::Value, Box<dyn std::error::Error + Send + Sync>> {
		digests
			.iter()
			.find_map(|item| match item {
				DigestItem::PreRuntime(id, data) if *id == TEST_DIGEST_ID => {
					u32::decode(&mut &data[..]).ok()
				},
				_ => None,
			})
			.ok_or_else(|| "no Partner Chains inherent digest in header".into())
	}
}

pub(crate) struct TestIDP;

#[async_trait::async_trait]
impl InherentDataProvider for TestIDP {
	async fn provide_inherent_data(
		&self,
		_inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
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

pub(crate) type TestCIDP =
	fn(
		<Block as BlockT>::Hash,
		(Slot, u32),
	) -> futures::future::Ready<Result<TestIDP, Box<dyn std::error::Error + Send + Sync>>>;

fn create_inherent_data_providers(
	_parent_hash: <Block as BlockT>::Hash,
	(slot, digest_value): (Slot, u32),
) -> futures::future::Ready<Result<TestIDP, Box<dyn std::error::Error + Send + Sync>>> {
	assert_eq!(
		slot,
		Slot::from(TEST_SLOT),
		"the Partner Chains inherent check must be parameterised with the slot extracted from the block header",
	);
	assert_eq!(
		digest_value, TEST_DIGEST_VALUE,
		"the Partner Chains inherent check must be parameterised with the digest value extracted from the block header",
	);
	futures::future::ready(Ok(TestIDP))
}

pub(crate) fn test_create_inherent_data_providers() -> TestCIDP {
	create_inherent_data_providers
}

#[derive(Clone)]
pub(crate) struct MockApi {
	check_inherents_called: Arc<AtomicBool>,
	fail_inherent_check: bool,
}

sp_api::mock_impl_runtime_apis! {
	impl BlockBuilderApi<Block> for MockApi {
		fn apply_extrinsic(&self, _: <Block as BlockT>::Extrinsic) -> sp_runtime::ApplyExtrinsicResult {
			unimplemented!()
		}

		fn finalize_block(&self) -> <Block as BlockT>::Header {
			unimplemented!()
		}

		fn inherent_extrinsics(&self, _: InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			unimplemented!()
		}

		fn check_inherents(&self, _: <Block as BlockT>::LazyBlock, _: InherentData) -> CheckInherentsResult {
			self.check_inherents_called.store(true, Ordering::SeqCst);
			let mut result = CheckInherentsResult::new();
			if self.fail_inherent_check {
				result
					.put_error(*b"testinh0", &sp_inherents::MakeFatalError::from(()))
					.expect("error can be put into a fresh result");
			}
			result
		}
	}
}

pub(crate) struct TestClient {
	api: MockApi,
}

impl ProvideRuntimeApi<Block> for TestClient {
	type Api = MockApi;

	fn runtime_api(&self) -> ApiRef<'_, Self::Api> {
		self.api.clone().into()
	}
}

/// A client whose runtime reports inherent check success or failure as configured,
/// together with a flag recording whether `check_inherents` was invoked.
pub(crate) fn test_client(fail_inherent_check: bool) -> (Arc<TestClient>, Arc<AtomicBool>) {
	let check_inherents_called = Arc::new(AtomicBool::new(false));
	let client = TestClient {
		api: MockApi {
			check_inherents_called: check_inherents_called.clone(),
			fail_inherent_check,
		},
	};
	(Arc::new(client), check_inherents_called)
}

pub(crate) fn block_import_params(
	body: Option<Vec<<Block as BlockT>::Extrinsic>>,
) -> BlockImportParams<Block> {
	let digest_logs = TestInherentDigest::from_inherent_data(&InherentData::new())
		.expect("Partner Chains digest can be created");
	let header = Header {
		parent_hash: Default::default(),
		number: 1,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Digest { logs: digest_logs },
	};
	let mut block = BlockImportParams::new(BlockOrigin::NetworkInitialSync, header);
	block.body = body;
	block
}
