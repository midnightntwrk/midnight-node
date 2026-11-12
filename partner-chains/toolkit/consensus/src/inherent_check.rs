use crate::{InherentDigest, SlotExtractor};
use log::warn;
use sp_api::{ApiExt, CallContext, ProvideRuntimeApi};
use sp_block_builder::{BlockBuilder as BlockBuilderApi, CheckInherentsError};
use sp_consensus_slots::Slot;
use sp_inherents::{CreateInherentDataProviders, InherentData, InherentDataProvider};
use sp_runtime::traits::{Block as BlockT, Header};
use std::sync::Arc;

const LOG_TARGET: &str = "partner-chains-consensus";

/// The complete Partner Chains inherent check, shared by
/// [`PartnerChainsVerifier`](crate::PartnerChainsVerifier) and
/// [`PartnerChainsBlockImport`](crate::PartnerChainsBlockImport).
///
/// Extracts the slot and the [`InherentDigest`] value from the block header, recreates
/// the inherent data providers parameterised by them, and checks the block's inherents
/// against the recreated inherent data. The check is skipped for blocks without a body
/// and for runtimes too old to support it; the header extraction is performed (and
/// validated) regardless.
pub(crate) async fn check_partner_chains_inherents<B, C, CIDP, SE, ID>(
	client: &Arc<C>,
	create_inherent_data_providers: &CIDP,
	header: &B::Header,
	body: Option<&Vec<B::Extrinsic>>,
	post_hash: B::Hash,
) -> Result<(), String>
where
	B: BlockT,
	C: ProvideRuntimeApi<B> + Send + Sync,
	C::Api: BlockBuilderApi<B> + ApiExt<B>,
	CIDP: CreateInherentDataProviders<B, (Slot, ID::Value)> + Send + Sync,
	SE: SlotExtractor<B>,
	ID: InherentDigest + Send + Sync + 'static,
{
	let parent_hash = *header.parent_hash();
	let slot = SE::extract_slot(header)?;
	let digest_value = ID::value_from_digest(header.digest().logs()).map_err(|e| {
		format!("Failed to retrieve inherent digest from header of block {post_hash:?}: {e}")
	})?;

	if let Some(inner_body) = body {
		// Skip the inherents check if the runtime API is too old to support it.
		if client
			.runtime_api()
			.has_api_with::<dyn BlockBuilderApi<B>, _>(parent_hash, |v| v >= 2)
			.map_err(|e| e.to_string())?
		{
			let inherent_data_providers = create_inherent_data_providers
				.create_inherent_data_providers(parent_hash, (slot, digest_value))
				.await
				.map_err(|e| format!("Failed to create inherent data providers: {e}"))?;

			let inherent_data = inherent_data_providers
				.create_inherent_data()
				.await
				.map_err(|e| format!("Failed to create inherent data: {e}"))?;

			let check_block = B::new(header.clone(), inner_body.clone());

			check_inherents_on_chain(
				client,
				parent_hash,
				check_block,
				&inherent_data_providers,
				inherent_data,
			)
			.await
			.map_err(|e| {
				warn!(
					target: LOG_TARGET,
					"Rejecting block {post_hash:?}: inherent check failed: {e:?}",
				);
				format!("Inherent check failed: {e:?}")
			})?;
		}
	}

	Ok(())
}

/// Checks the block's inherents through the runtime's `check_inherents` API in the
/// **on-chain** call context, the context the block builder that produced the block
/// used.
///
/// The context decides which runtime executes the call around a runtime upgrade. With
/// `system_version >= 3` an upgrade is staged in `:pending_code` and replaces `:code`
/// only at the end of the block *after* the upgrade block. On-chain calls (block
/// building and import) resolve the staged runtime; a runtime API call in the default,
/// off-chain context does not and runs the old one. The first post-upgrade block is
/// therefore built with the new runtime, and checking it with the old one fails to
/// decode any inherent whose encoding changed across the upgrade (the generated
/// `check_extrinsics` panics): every node other than the author rejects the block and
/// each authority ends up on a fork of its own.
///
/// Equivalent to `sp_block_builder::check_inherents_with_data` apart from the call
/// context.
async fn check_inherents_on_chain<B, C>(
	client: &Arc<C>,
	at_hash: B::Hash,
	block: B,
	inherent_data_providers: &impl InherentDataProvider,
	inherent_data: InherentData,
) -> Result<(), CheckInherentsError>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + ApiExt<B>,
{
	// Scoped so the (non-`Send`) API handle is dropped before the awaits below.
	let result = {
		let mut api = client.runtime_api();
		api.set_call_context(CallContext::Onchain { import: false });
		api.check_inherents(at_hash, block.into(), inherent_data)
			.map_err(CheckInherentsError::Client)?
	};

	if !result.ok() {
		for (identifier, error) in result.into_errors() {
			match inherent_data_providers.try_handle_error(&identifier, &error).await {
				Some(handled) => handled.map_err(CheckInherentsError::CheckInherents)?,
				None => return Err(CheckInherentsError::CheckInherentsUnknownError(identifier)),
			}
		}
	}

	Ok(())
}
