//! Scaffolding shared by the unit tests of [`crate::PartnerChainsVerifier`] and
//! [`crate::PartnerChainsBlockImport`].

use crate::{InherentDigest, SlotExtractor};
use sc_consensus::block_import::BlockImportParams;
use sp_api::{ApiRef, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus::BlockOrigin;
use sp_consensus_slots::Slot;
use sp_inherents::{CheckInherentsResult, InherentData, InherentDataProvider, InherentIdentifier};
use sp_runtime::generic::Header;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};
use sp_runtime::{DigestItem, OpaqueExtrinsic};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
	// The Partner Chains inherent check must be parameterised with the values
	// extracted from the block header.
	assert_eq!(slot, Slot::from(TEST_SLOT));
	assert_eq!(digest_value, TEST_DIGEST_VALUE);
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
