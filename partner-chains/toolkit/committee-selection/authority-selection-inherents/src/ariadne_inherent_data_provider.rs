//! Logic to provide committee selection inputs compatible with the Ariadne algorithm, based on Cardano observation.
#[cfg(feature = "std")]
use crate::authority_selection_inputs::AuthoritySelectionDataSource;
use crate::authority_selection_inputs::AuthoritySelectionInputs;
use parity_scale_codec::{Decode, Encode};
use sp_session_validator_management::MC_EPOCH_API_VERSION;
#[cfg(feature = "std")]
use {
	crate::authority_selection_inputs::AuthoritySelectionInputsCreationError,
	sidechain_domain::mainchain_epoch::MainchainEpochDerivation,
	sidechain_domain::*,
	sp_api::{ApiExt, CallContext, ProvideRuntimeApi},
	sp_consensus_slots::Slot,
	sp_inherents::{InherentData, InherentIdentifier},
	sp_runtime::traits::Block as BlockT,
	sp_session_validator_management::CommitteeMember as CommitteeMemberT,
	sp_session_validator_management::{
		INHERENT_IDENTIFIER, InherentError, MainChainScripts, SessionValidatorManagementApi,
	},
};

#[cfg(feature = "std")]
pub use {sidechain_domain::mainchain_epoch::MainchainEpochConfig, sidechain_slots::ScSlotConfig};

#[derive(Clone, Debug, Encode, Decode)]
/// Inherent data provider providing inputs for authority selection.
pub struct AriadneInherentDataProvider {
	/// Authority selection inputs.
	pub data: Option<AuthoritySelectionInputs>,
	/// Format of the data the runtime takes.
	pub format: InherentDataFormat,
}

/// Format of the [AriadneInherentDataProvider] data a runtime takes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum InherentDataFormat {
	/// Before [MC_EPOCH_API_VERSION]: the selection inputs alone.
	InputsOnly,
	/// From [MC_EPOCH_API_VERSION]: the inputs with the Cardano epoch they come from.
	WithMcEpoch(sidechain_domain::McEpochNumber),
}

impl InherentDataFormat {
	/// The format of a runtime whose [SessionValidatorManagementApi] is at `api_version`, for
	/// inputs from Cardano epoch `mc_epoch`.
	pub fn for_runtime(
		api_version: Option<u32>,
		mc_epoch: sidechain_domain::McEpochNumber,
	) -> Self {
		match api_version {
			Some(version) if version >= MC_EPOCH_API_VERSION => Self::WithMcEpoch(mc_epoch),
			_ => Self::InputsOnly,
		}
	}
}

#[cfg(feature = "std")]
impl AriadneInherentDataProvider {
	/// Creates a new [AriadneInherentDataProvider] for use in [sp_inherents::CreateInherentDataProviders].
	///
	/// Parameters:
	/// - `client`: runtime client capable of providing [SessionValidatorManagementApi] runtime API
	/// - `sc_slot_config`: partner chain slot configuration
	/// - `mc_epoch_config`: main chain epoch configuration
	/// - `parent_hash`: parent hash of the current block
	/// - `slot`: slot of the current block
	/// - `data_source`: data source implementing [AuthoritySelectionDataSource]
	/// - `mc_reference_epoch`: latest stable mainchain epoch
	pub async fn new<Block, CommitteeMember, T>(
		client: &T,
		sc_slot_config: &ScSlotConfig,
		mc_epoch_config: &MainchainEpochConfig,
		parent_hash: <Block as BlockT>::Hash,
		slot: Slot,
		data_source: &(dyn AuthoritySelectionDataSource + Send + Sync),
		mc_reference_epoch: McEpochNumber,
	) -> Result<Self, InherentProviderCreationError>
	where
		CommitteeMember: CommitteeMemberT + Decode + Encode,
		CommitteeMember::AuthorityKeys: Decode + Encode,
		CommitteeMember::AuthorityId: Decode + Encode,
		Block: BlockT,
		T: ProvideRuntimeApi<Block> + Send + Sync,
		T::Api: SessionValidatorManagementApi<
				Block,
				CommitteeMember,
				AuthoritySelectionInputs,
				ScEpochNumber,
			>,
	{
		// The block is built and imported in the on-chain context, which runs the code a runtime
		// upgrade in the parent block staged in `:pending_code`. The default context would report
		// the replaced runtime's version for the first block of the new one.
		let api_version = {
			let mut api = client.runtime_api();
			api.set_call_context(CallContext::Onchain { import: false });
			api.api_version::<dyn SessionValidatorManagementApi<
					Block,
					CommitteeMember,
					AuthoritySelectionInputs,
					ScEpochNumber,
				>>(parent_hash)?
		};
		Self::new_for_api_version(
			client,
			sc_slot_config,
			mc_epoch_config,
			parent_hash,
			slot,
			data_source,
			mc_reference_epoch,
			api_version,
		)
		.await
	}

	/// [Self::new] for a runtime whose [SessionValidatorManagementApi] is at `api_version`
	#[allow(clippy::too_many_arguments)]
	async fn new_for_api_version<Block, CommitteeMember, T>(
		client: &T,
		sc_slot_config: &ScSlotConfig,
		mc_epoch_config: &MainchainEpochConfig,
		parent_hash: <Block as BlockT>::Hash,
		slot: Slot,
		data_source: &(dyn AuthoritySelectionDataSource + Send + Sync),
		mc_reference_epoch: McEpochNumber,
		api_version: Option<u32>,
	) -> Result<Self, InherentProviderCreationError>
	where
		CommitteeMember: CommitteeMemberT + Decode + Encode,
		CommitteeMember::AuthorityKeys: Decode + Encode,
		CommitteeMember::AuthorityId: Decode + Encode,
		Block: BlockT,
		T: ProvideRuntimeApi<Block> + Send + Sync,
		T::Api: SessionValidatorManagementApi<
				Block,
				CommitteeMember,
				AuthoritySelectionInputs,
				ScEpochNumber,
			>,
	{
		let for_mc_epoch = mc_epoch_for_next_ariadne_cidp(
			client,
			sc_slot_config,
			mc_epoch_config,
			parent_hash,
			slot,
		)?;

		let format = InherentDataFormat::for_runtime(api_version, for_mc_epoch);

		let data_epoch = data_source.data_epoch(for_mc_epoch).await?;
		// We could accept mc_reference at last slot of data_epoch, but calculations are much easier like that.
		// Additionally, in current implementation, the inequality below is always true, thus there is no need to make it more accurate.
		let scripts = client.runtime_api().get_main_chain_scripts(parent_hash)?;
		if data_epoch < mc_reference_epoch {
			Ok(AriadneInherentDataProvider::from_mc_data(
				data_source,
				for_mc_epoch,
				scripts,
				format,
			)
			.await?)
		} else {
			Ok(AriadneInherentDataProvider { data: None, format })
		}
	}

	async fn from_mc_data(
		candidate_data_source: &(dyn AuthoritySelectionDataSource + Send + Sync),
		for_epoch: McEpochNumber,
		scripts: MainChainScripts,
		format: InherentDataFormat,
	) -> Result<Self, InherentProviderCreationError> {
		Ok(Self {
			data: Some(
				AuthoritySelectionInputs::from_mc_data(candidate_data_source, for_epoch, scripts)
					.await?,
			),
			format,
		})
	}
}

#[cfg(feature = "std")]
#[derive(Debug, thiserror::Error)]
/// Error type returned when creation of [AriadneInherentDataProvider] fails.
pub enum InherentProviderCreationError {
	/// Slot represents a timestamp bigger than of [u64::MAX].
	#[error("Slot represents a timestamp bigger than of u64::MAX")]
	SlotTooBig,
	/// Couldn't convert timestamp to main chain epoch.
	#[error("Couldn't convert timestamp to main chain epoch: {0}")]
	McEpochDerivationError(#[from] sidechain_domain::mainchain_epoch::EpochDerivationError),
	/// Runtime API call failed.
	#[error("Runtime API call failed: {0}")]
	ApiError(#[from] sp_api::ApiError),
	/// Failed to create authority selection inputs.
	#[error("Failed to create authority selection inputs: {0}")]
	InputsCreationError(#[from] AuthoritySelectionInputsCreationError),
	/// Data source call failed.
	#[error("Data source call failed: {0}")]
	DataSourceError(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(feature = "std")]
fn mc_epoch_for_next_ariadne_cidp<Block, CommitteeMember, T>(
	client: &T,
	sc_slot_config: &ScSlotConfig,
	epoch_config: &MainchainEpochConfig,
	parent_hash: <Block as BlockT>::Hash,
	slot: Slot,
) -> Result<McEpochNumber, InherentProviderCreationError>
where
	Block: BlockT,
	CommitteeMember: Decode + Encode + CommitteeMemberT,
	CommitteeMember::AuthorityKeys: Decode + Encode,
	CommitteeMember::AuthorityId: Decode + Encode,
	T: ProvideRuntimeApi<Block> + Send + Sync,
	T::Api: SessionValidatorManagementApi<
			Block,
			CommitteeMember,
			AuthoritySelectionInputs,
			ScEpochNumber,
		>,
{
	let next_unset_epoch = client.runtime_api().get_next_unset_epoch_number(parent_hash)?;

	let for_sc_epoch_number = {
		// A special case for the genesis committee (current epoch 0, next unset epoch 1).
		// The genesis committee epoch is initialized with 0, so in the very first block we need to provide
		// the epoch number based on the current slot number
		if next_unset_epoch == ScEpochNumber(1) {
			sc_slot_config.epoch_number(slot)
		} else {
			next_unset_epoch
		}
	};

	sc_epoch_to_mc_epoch(for_sc_epoch_number, sc_slot_config, epoch_config)
}

#[cfg(feature = "std")]
fn sc_epoch_to_mc_epoch(
	sc_epoch: ScEpochNumber,
	sc_slot_config: &ScSlotConfig,
	epoch_config: &MainchainEpochConfig,
) -> Result<McEpochNumber, InherentProviderCreationError> {
	let timestamp = sc_slot_config
		.epoch_start_time(sc_epoch)
		.ok_or(InherentProviderCreationError::SlotTooBig)?;

	epoch_config
		.timestamp_to_mainchain_epoch(timestamp)
		.map_err(InherentProviderCreationError::McEpochDerivationError)
}

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for AriadneInherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		match (&self.data, self.format) {
			(None, _) => Ok(()),
			(Some(inputs), InherentDataFormat::InputsOnly) => {
				inherent_data.put_data(INHERENT_IDENTIFIER, inputs)
			},
			// Encodes as `SelectionInherentData` without cloning the inputs.
			(Some(inputs), InherentDataFormat::WithMcEpoch(mc_epoch)) => {
				inherent_data.put_data(INHERENT_IDENTIFIER, &(inputs, mc_epoch))
			},
		}
	}

	async fn try_handle_error(
		&self,
		identifier: &InherentIdentifier,
		error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		// Don't process inherents from other modules
		if *identifier != INHERENT_IDENTIFIER {
			return None;
		}

		let mut error = error;
		Some(Err(sp_inherents::Error::Application(Box::from(
			<InherentError as parity_scale_codec::Decode>::decode(&mut error).ok()?,
		))))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ariadne_inherent_data_provider::AriadneInherentDataProvider;
	use crate::mock::MockAuthoritySelectionDataSource;
	use crate::runtime_api_mock::*;
	use SlotDuration;
	use sidechain_domain::mainchain_epoch::*;
	use sidechain_slots::*;
	use sp_core::H256;
	use sp_core::offchain::Timestamp;
	use sp_inherents::InherentDataProvider as _;
	use sp_session_validator_management::SelectionInherentData;

	#[tokio::test]
	async fn return_empty_ariadne_cidp_if_runtime_requests_too_new_epoch() {
		// This is the epoch number that is too new
		let next_unset_epoch_number = ScEpochNumber(42);
		let mc_reference_epoch = McEpochNumber(1);
		let empty_ariadne_idp = AriadneInherentDataProvider::new_for_api_version(
			&client(next_unset_epoch_number),
			&sc_slot_config(),
			&epoch_config(),
			H256::zero(),
			// This is the slot that will be used to calculate current_epoch_number
			Slot::from(400u64),
			&MockAuthoritySelectionDataSource::default(),
			mc_reference_epoch,
			Some(MC_EPOCH_API_VERSION),
		)
		.await;

		assert!(empty_ariadne_idp.is_ok());
		assert!(empty_ariadne_idp.unwrap().data.is_none());
	}

	#[tokio::test]
	async fn error_if_num_permissioned_candidates_non_zero_and_no_permissioned_candidate_list() {
		use crate::ariadne_inherent_data_provider::InherentProviderCreationError::InputsCreationError;
		use crate::authority_selection_inputs::AuthoritySelectionInputsCreationError::AriadneParametersQuery;

		let next_unset_epoch_number = ScEpochNumber(42);
		let mc_reference_epoch = McEpochNumber(5);
		let ariadne_idp = AriadneInherentDataProvider::new_for_api_version(
			&client(next_unset_epoch_number),
			&sc_slot_config(),
			&epoch_config(),
			H256::zero(),
			// This is the slot that will be used to calculate current_epoch_number
			Slot::from(400u64),
			&MockAuthoritySelectionDataSource::default()
				.with_permissioned_candidates(vec![None, None, None, None, None])
				.with_num_permissioned_candidates(3),
			mc_reference_epoch,
			Some(MC_EPOCH_API_VERSION),
		)
		.await;

		assert!(ariadne_idp.is_err());
		assert!(matches!(
			ariadne_idp.unwrap_err(),
			InputsCreationError(AriadneParametersQuery(_, _, _, _))
		));
	}

	#[tokio::test]
	async fn error_if_epoch_nonce_not_found() {
		use crate::ariadne_inherent_data_provider::InherentProviderCreationError::InputsCreationError;
		use crate::authority_selection_inputs::AuthoritySelectionInputsCreationError::EpochNonceNotFound;

		let next_unset_epoch_number = ScEpochNumber(42);
		let mc_reference_epoch = McEpochNumber(5);
		let ariadne_idp = AriadneInherentDataProvider::new_for_api_version(
			&client(next_unset_epoch_number),
			&sc_slot_config(),
			&epoch_config(),
			H256::zero(),
			// This is the slot that will be used to calculate current_epoch_number
			Slot::from(400u64),
			&MockAuthoritySelectionDataSource::default()
				.with_permissioned_candidates(vec![Some(vec![]); 5])
				.with_epoch_nonce(None),
			mc_reference_epoch,
			Some(MC_EPOCH_API_VERSION),
		)
		.await;

		assert!(matches!(ariadne_idp, Err(InputsCreationError(EpochNonceNotFound(_)))));
	}

	#[tokio::test]
	async fn ok_if_num_permissioned_candidates_zero_and_no_permissioned_candidate_list() {
		let next_unset_epoch_number = ScEpochNumber(42);
		let mc_reference_epoch = McEpochNumber(5);
		let ariadne_idp = AriadneInherentDataProvider::new_for_api_version(
			&client(next_unset_epoch_number),
			&sc_slot_config(),
			&epoch_config(),
			H256::zero(),
			// This is the slot that will be used to calculate current_epoch_number
			Slot::from(400u64),
			&MockAuthoritySelectionDataSource::default()
				.with_permissioned_candidates(vec![None, None, None, None, None])
				.with_num_permissioned_candidates(0),
			mc_reference_epoch,
			Some(MC_EPOCH_API_VERSION),
		)
		.await;

		assert!(ariadne_idp.is_ok());
		assert!(ariadne_idp.unwrap().data.is_some());
	}

	#[test]
	fn the_format_follows_the_runtime_api_version() {
		let epoch = McEpochNumber(4);
		let format = |version| InherentDataFormat::for_runtime(version, epoch);
		assert_eq!(format(Some(MC_EPOCH_API_VERSION)), InherentDataFormat::WithMcEpoch(epoch));
		assert_eq!(format(Some(MC_EPOCH_API_VERSION + 1)), InherentDataFormat::WithMcEpoch(epoch));
		assert_eq!(format(Some(MC_EPOCH_API_VERSION - 1)), InherentDataFormat::InputsOnly);
		assert_eq!(format(None), InherentDataFormat::InputsOnly);
	}

	async fn provider_with_inputs(api_version: Option<u32>) -> AriadneInherentDataProvider {
		// Partner Chain epoch 42 starts at 420 s, in Cardano epoch 4 (100 s epochs).
		AriadneInherentDataProvider::new_for_api_version(
			&client(ScEpochNumber(42)),
			&sc_slot_config(),
			&epoch_config(),
			H256::zero(),
			Slot::from(400u64),
			&MockAuthoritySelectionDataSource::default()
				.with_permissioned_candidates(vec![None, None, None, None, None])
				.with_num_permissioned_candidates(0),
			McEpochNumber(5),
			api_version,
		)
		.await
		.unwrap()
	}

	async fn provided(provider: &AriadneInherentDataProvider) -> InherentData {
		let mut inherent_data = InherentData::new();
		provider.provide_inherent_data(&mut inherent_data).await.unwrap();
		inherent_data
	}

	#[tokio::test]
	async fn a_current_runtime_gets_the_inputs_with_their_cardano_epoch() {
		let provider = provider_with_inputs(Some(MC_EPOCH_API_VERSION)).await;
		assert_eq!(provider.format, InherentDataFormat::WithMcEpoch(McEpochNumber(4)));
		let data = provided(&provider)
			.await
			.get_data::<SelectionInherentData<AuthoritySelectionInputs>>(&INHERENT_IDENTIFIER)
			.unwrap()
			.unwrap();
		assert_eq!(Some(data.inputs.encode()), provider.data.map(|inputs| inputs.encode()));
		assert_eq!(data.mc_epoch, McEpochNumber(4));
	}

	#[tokio::test]
	async fn an_older_runtime_gets_the_inputs_alone() {
		let provider = provider_with_inputs(Some(MC_EPOCH_API_VERSION - 1)).await;
		let data = provided(&provider)
			.await
			.get_data::<AuthoritySelectionInputs>(&INHERENT_IDENTIFIER)
			.unwrap()
			.unwrap();
		assert_eq!(Some(data.encode()), provider.data.map(|inputs| inputs.encode()));
	}

	fn sc_slot_config() -> ScSlotConfig {
		ScSlotConfig {
			slots_per_epoch: SlotsPerEpoch(10),
			slot_duration: SlotDuration::from_millis(1000),
		}
	}

	fn client(next_unset_epoch_number: ScEpochNumber) -> TestApi {
		TestApi { next_unset_epoch_number }
	}

	fn epoch_config() -> MainchainEpochConfig {
		MainchainEpochConfig {
			first_epoch_timestamp_millis: Timestamp::from_unix_millis(0),
			first_epoch_number: 0,
			epoch_duration_millis: Duration::from_millis(
				u64::from(sc_slot_config().slots_per_epoch.0)
					* sc_slot_config().slot_duration.as_millis()
					* 10,
			),
			first_slot_number: 0,
			slot_duration_millis: Duration::from_millis(1000),
		}
	}
}
