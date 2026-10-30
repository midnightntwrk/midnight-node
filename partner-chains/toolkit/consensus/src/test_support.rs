//! Scaffolding for the unit tests of [`crate::PartnerChainsVerifier`] and
//! [`crate::PartnerChainsBlockImport`], and for the Aura/BABE contract canaries.

use crate::{InherentDigest, SlotExtractor};
use parity_scale_codec::{Decode, Encode};
use sc_consensus::block_import::BlockImportParams;
use sp_api::{ApiError, ApiExt, ApiRef, CallContext, ProvideRuntimeApi, RuntimeApiInfo};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus::BlockOrigin;
use sp_consensus_slots::Slot;
use sp_inherents::{CheckInherentsResult, InherentData, InherentDataProvider, InherentIdentifier};
use sp_runtime::generic::Header;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};
use sp_runtime::{Digest, DigestItem, OpaqueExtrinsic};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

/// Stand-in for the runtime's `BlockBuilder` API: reports inherent check success or
/// failure as configured and records whether, and in which [`CallContext`],
/// `check_inherents` was invoked.
///
/// Hand-written rather than generated with `sp_api::mock_impl_runtime_apis!`, whose
/// `set_call_context` is `unimplemented!()`; the call context is what the Partner Chains
/// inherent check must get right (see `inherent_check::check_inherents_on_chain`).
#[derive(Clone)]
pub(crate) struct MockApi {
	check_inherents_called: Arc<AtomicBool>,
	check_inherents_call_context: Arc<Mutex<Option<CallContext>>>,
	call_context: CallContext,
	fail_inherent_check: bool,
}

impl sp_api::Core<Block> for MockApi {
	fn __runtime_api_internal_call_api_at(
		&self,
		_at: <Block as BlockT>::Hash,
		_params: Vec<u8>,
		_fn_name: &dyn Fn(sp_version::RuntimeVersion) -> &'static str,
	) -> Result<Vec<u8>, ApiError> {
		unimplemented!()
	}

	fn version(
		&self,
		_at: <Block as BlockT>::Hash,
	) -> Result<sp_version::RuntimeVersion, ApiError> {
		unimplemented!()
	}

	fn execute_block(
		&self,
		_at: <Block as BlockT>::Hash,
		_block: <Block as BlockT>::LazyBlock,
	) -> Result<(), ApiError> {
		unimplemented!()
	}

	fn initialize_block(
		&self,
		_at: <Block as BlockT>::Hash,
		_header: &<Block as BlockT>::Header,
	) -> Result<sp_runtime::ExtrinsicInclusionMode, ApiError> {
		unimplemented!()
	}
}

impl BlockBuilderApi<Block> for MockApi {
	fn __runtime_api_internal_call_api_at(
		&self,
		_at: <Block as BlockT>::Hash,
		_params: Vec<u8>,
		_fn_name: &dyn Fn(sp_version::RuntimeVersion) -> &'static str,
	) -> Result<Vec<u8>, ApiError> {
		unimplemented!()
	}

	fn apply_extrinsic(
		&self,
		_at: <Block as BlockT>::Hash,
		_extrinsic: <Block as BlockT>::Extrinsic,
	) -> Result<sp_runtime::ApplyExtrinsicResult, ApiError> {
		unimplemented!()
	}

	fn finalize_block(
		&self,
		_at: <Block as BlockT>::Hash,
	) -> Result<<Block as BlockT>::Header, ApiError> {
		unimplemented!()
	}

	fn inherent_extrinsics(
		&self,
		_at: <Block as BlockT>::Hash,
		_inherent: InherentData,
	) -> Result<Vec<<Block as BlockT>::Extrinsic>, ApiError> {
		unimplemented!()
	}

	fn check_inherents(
		&self,
		_at: <Block as BlockT>::Hash,
		_block: <Block as BlockT>::LazyBlock,
		_data: InherentData,
	) -> Result<CheckInherentsResult, ApiError> {
		Ok(self.record_check_inherents())
	}
}

impl MockApi {
	fn record_check_inherents(&self) -> CheckInherentsResult {
		self.check_inherents_called.store(true, Ordering::SeqCst);
		*self.check_inherents_call_context.lock().unwrap() = Some(self.call_context);
		let mut result = CheckInherentsResult::new();
		if self.fail_inherent_check {
			result
				.put_error(*b"testinh0", &sp_inherents::MakeFatalError::from(()))
				.expect("error can be put into a fresh result");
		}
		result
	}
}

impl ApiExt<Block> for MockApi {
	fn execute_in_transaction<F: FnOnce(&Self) -> sp_api::TransactionOutcome<R>, R>(
		&self,
		call: F,
	) -> R {
		call(self).into_inner()
	}

	fn has_api<A: RuntimeApiInfo + ?Sized>(
		&self,
		_at_hash: <Block as BlockT>::Hash,
	) -> Result<bool, ApiError> {
		Ok(true)
	}

	fn has_api_with<A: RuntimeApiInfo + ?Sized, P: Fn(u32) -> bool>(
		&self,
		_at_hash: <Block as BlockT>::Hash,
		pred: P,
	) -> Result<bool, ApiError> {
		Ok(pred(A::VERSION))
	}

	fn api_version<A: RuntimeApiInfo + ?Sized>(
		&self,
		_at_hash: <Block as BlockT>::Hash,
	) -> Result<Option<u32>, ApiError> {
		Ok(Some(A::VERSION))
	}

	fn record_proof(&mut self) {}

	fn record_proof_with_recorder(&mut self, _recorder: sp_api::ProofRecorder<Block>) {}

	fn extract_proof(&mut self) -> Option<sp_api::StorageProof> {
		None
	}

	fn proof_recorder(&self) -> Option<sp_api::ProofRecorder<Block>> {
		None
	}

	fn into_storage_changes<B: sp_state_machine::Backend<sp_runtime::traits::HashingFor<Block>>>(
		&self,
		_backend: &B,
		_parent_hash: <Block as BlockT>::Hash,
	) -> Result<sp_api::StorageChanges<Block>, String> {
		unimplemented!()
	}

	fn set_call_context(&mut self, call_context: CallContext) {
		self.call_context = call_context;
	}

	fn register_extension<E: sp_externalities::Extension>(&mut self, _extension: E) {}

	fn set_overlayed_changes(
		&mut self,
		_changes: sp_state_machine::OverlayedChanges<sp_runtime::traits::HashingFor<Block>>,
	) {
	}
}

pub(crate) struct TestClient {
	api: MockApi,
}

impl TestClient {
	/// The [`CallContext`] in effect when `check_inherents` was last invoked, if ever.
	pub(crate) fn check_inherents_call_context(&self) -> Option<CallContext> {
		*self.api.check_inherents_call_context.lock().unwrap()
	}
}

impl ProvideRuntimeApi<Block> for TestClient {
	type Api = MockApi;

	fn runtime_api(&self) -> ApiRef<'_, Self::Api> {
		// A fresh API handle starts in the default (off-chain) context, as the real one does.
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
			check_inherents_call_context: Arc::new(Mutex::new(None)),
			call_context: CallContext::Offchain,
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
