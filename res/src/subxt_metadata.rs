// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[allow(dead_code, unused_imports, non_camel_case_types, unreachable_patterns)]
#[allow(clippy::all)]
#[allow(rustdoc::broken_intra_doc_links)]
pub mod api {
	#[allow(unused_imports)]
	mod root_mod {
		pub use super::*;
	}
	pub static PALLETS: [&str; 23usize] = [
		"System",
		"Timestamp",
		"Aura",
		"Grandpa",
		"Sidechain",
		"Midnight",
		"Balances",
		"Sudo",
		"SessionCommitteeManagement",
		"RuntimeUpgrade",
		"NodeVersion",
		"NativeTokenManagement",
		"NativeTokenObservation",
		"Preimage",
		"MultiBlockMigrations",
		"PalletSession",
		"Scheduler",
		"TxPause",
		"Beefy",
		"Mmr",
		"BeefyMmrLeaf",
		"Session",
		"GovernedMap",
	];
	pub static RUNTIME_APIS: [&str; 23usize] = [
		"NativeTokenManagementApi",
		"GenesisBuilder",
		"Core",
		"MidnightRuntimeApi",
		"UpgradeApi",
		"Metadata",
		"BlockBuilder",
		"TaggedTransactionQueue",
		"OffchainWorkerApi",
		"AuraApi",
		"BeefyApi",
		"MmrApi",
		"BeefyMmrApi",
		"SessionKeys",
		"GrandpaApi",
		"AccountNonceApi",
		"GetSidechainStatus",
		"GetGenesisUtxo",
		"SlotApi",
		"SessionValidatorManagementApi",
		"CandidateValidationApi",
		"NativeTokenObservationApi",
		"GovernedMapIDPApi",
	];
	#[doc = r" The error type returned when there is a runtime issue."]
	pub type DispatchError = runtime_types::sp_runtime::DispatchError;
	#[doc = r" The outer event enum."]
	pub type Event = runtime_types::midnight_node_runtime::RuntimeEvent;
	#[doc = r" The outer extrinsic enum."]
	pub type Call = runtime_types::midnight_node_runtime::RuntimeCall;
	#[doc = r" The outer error enum representing the DispatchError's Module variant."]
	pub type Error = runtime_types::midnight_node_runtime::RuntimeError;
	pub fn constants() -> ConstantsApi {
		ConstantsApi
	}
	pub fn storage() -> StorageApi {
		StorageApi
	}
	pub fn tx() -> TransactionApi {
		TransactionApi
	}
	pub fn apis() -> runtime_apis::RuntimeApi {
		runtime_apis::RuntimeApi
	}
	pub mod runtime_apis {
		use super::root_mod;
		use super::runtime_types;
		use subxt::ext::subxt_core::ext::codec::Encode;
		pub struct RuntimeApi;
		impl RuntimeApi {
			pub fn native_token_management_api(
				&self,
			) -> native_token_management_api::NativeTokenManagementApi {
				native_token_management_api::NativeTokenManagementApi
			}
			pub fn genesis_builder(&self) -> genesis_builder::GenesisBuilder {
				genesis_builder::GenesisBuilder
			}
			pub fn core(&self) -> core::Core {
				core::Core
			}
			pub fn midnight_runtime_api(&self) -> midnight_runtime_api::MidnightRuntimeApi {
				midnight_runtime_api::MidnightRuntimeApi
			}
			pub fn upgrade_api(&self) -> upgrade_api::UpgradeApi {
				upgrade_api::UpgradeApi
			}
			pub fn metadata(&self) -> metadata::Metadata {
				metadata::Metadata
			}
			pub fn block_builder(&self) -> block_builder::BlockBuilder {
				block_builder::BlockBuilder
			}
			pub fn tagged_transaction_queue(
				&self,
			) -> tagged_transaction_queue::TaggedTransactionQueue {
				tagged_transaction_queue::TaggedTransactionQueue
			}
			pub fn offchain_worker_api(&self) -> offchain_worker_api::OffchainWorkerApi {
				offchain_worker_api::OffchainWorkerApi
			}
			pub fn aura_api(&self) -> aura_api::AuraApi {
				aura_api::AuraApi
			}
			pub fn beefy_api(&self) -> beefy_api::BeefyApi {
				beefy_api::BeefyApi
			}
			pub fn mmr_api(&self) -> mmr_api::MmrApi {
				mmr_api::MmrApi
			}
			pub fn beefy_mmr_api(&self) -> beefy_mmr_api::BeefyMmrApi {
				beefy_mmr_api::BeefyMmrApi
			}
			pub fn session_keys(&self) -> session_keys::SessionKeys {
				session_keys::SessionKeys
			}
			pub fn grandpa_api(&self) -> grandpa_api::GrandpaApi {
				grandpa_api::GrandpaApi
			}
			pub fn account_nonce_api(&self) -> account_nonce_api::AccountNonceApi {
				account_nonce_api::AccountNonceApi
			}
			pub fn get_sidechain_status(&self) -> get_sidechain_status::GetSidechainStatus {
				get_sidechain_status::GetSidechainStatus
			}
			pub fn get_genesis_utxo(&self) -> get_genesis_utxo::GetGenesisUtxo {
				get_genesis_utxo::GetGenesisUtxo
			}
			pub fn slot_api(&self) -> slot_api::SlotApi {
				slot_api::SlotApi
			}
			pub fn session_validator_management_api(
				&self,
			) -> session_validator_management_api::SessionValidatorManagementApi {
				session_validator_management_api::SessionValidatorManagementApi
			}
			pub fn candidate_validation_api(
				&self,
			) -> candidate_validation_api::CandidateValidationApi {
				candidate_validation_api::CandidateValidationApi
			}
			pub fn native_token_observation_api(
				&self,
			) -> native_token_observation_api::NativeTokenObservationApi {
				native_token_observation_api::NativeTokenObservationApi
			}
			pub fn governed_map_idp_api(&self) -> governed_map_idp_api::GovernedMapIDPApi {
				governed_map_idp_api::GovernedMapIDPApi
			}
		}
		pub mod native_token_management_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " Runtime API exposing configuration and initialization status of the Native Token Management pallet"]
			pub struct NativeTokenManagementApi;
			impl NativeTokenManagementApi {
				#[doc = " Returns the current main chain scripts configured in the pallet or [None] if they are not set."]
				pub fn get_main_chain_scripts(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetMainChainScripts,
					types::get_main_chain_scripts::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"NativeTokenManagementApi",
						"get_main_chain_scripts",
						types::GetMainChainScripts {},
						[
							254u8, 50u8, 21u8, 109u8, 15u8, 79u8, 79u8, 148u8, 219u8, 87u8, 198u8,
							166u8, 74u8, 3u8, 97u8, 252u8, 68u8, 146u8, 102u8, 60u8, 88u8, 59u8,
							104u8, 255u8, 207u8, 29u8, 20u8, 234u8, 239u8, 76u8, 1u8, 15u8,
						],
					)
				}
				#[doc = " Gets current initializaion status and set it to `true` afterwards. This check is used to"]
				#[doc = " determine whether historical data from the beginning of main chain should be queried."]
				pub fn initialized(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::Initialized,
					types::initialized::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"NativeTokenManagementApi",
						"initialized",
						types::Initialized {},
						[
							220u8, 169u8, 225u8, 201u8, 227u8, 43u8, 220u8, 167u8, 16u8, 200u8,
							162u8, 181u8, 140u8, 54u8, 119u8, 63u8, 242u8, 96u8, 245u8, 23u8,
							135u8, 109u8, 131u8, 138u8, 230u8, 47u8, 206u8, 73u8, 10u8, 82u8, 21u8,
							75u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod get_main_chain_scripts {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<
							runtime_types::sp_native_token_management::MainChainScripts,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetMainChainScripts {}
				pub mod initialized {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::primitive::bool;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Initialized {}
			}
		}
		pub mod genesis_builder {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " API to interact with `RuntimeGenesisConfig` for the runtime"]
			pub struct GenesisBuilder;
			impl GenesisBuilder {
				#[doc = " Build `RuntimeGenesisConfig` from a JSON blob not using any defaults and store it in the"]
				#[doc = " storage."]
				#[doc = ""]
				#[doc = " In the case of a FRAME-based runtime, this function deserializes the full"]
				#[doc = " `RuntimeGenesisConfig` from the given JSON blob and puts it into the storage. If the"]
				#[doc = " provided JSON blob is incorrect or incomplete or the deserialization fails, an error"]
				#[doc = " is returned."]
				#[doc = ""]
				#[doc = " Please note that provided JSON blob must contain all `RuntimeGenesisConfig` fields, no"]
				#[doc = " defaults will be used."]
				pub fn build_state(
					&self,
					json: types::build_state::Json,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::BuildState,
					types::build_state::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GenesisBuilder",
						"build_state",
						types::BuildState { json },
						[
							203u8, 233u8, 104u8, 116u8, 111u8, 131u8, 201u8, 235u8, 117u8, 116u8,
							140u8, 185u8, 93u8, 25u8, 155u8, 210u8, 56u8, 49u8, 23u8, 32u8, 253u8,
							92u8, 149u8, 241u8, 85u8, 245u8, 137u8, 45u8, 209u8, 189u8, 81u8, 2u8,
						],
					)
				}
				#[doc = " Returns a JSON blob representation of the built-in `RuntimeGenesisConfig` identified by"]
				#[doc = " `id`."]
				#[doc = ""]
				#[doc = " If `id` is `None` the function should return JSON blob representation of the default"]
				#[doc = " `RuntimeGenesisConfig` struct of the runtime. Implementation must provide default"]
				#[doc = " `RuntimeGenesisConfig`."]
				#[doc = ""]
				#[doc = " Otherwise function returns a JSON representation of the built-in, named"]
				#[doc = " `RuntimeGenesisConfig` preset identified by `id`, or `None` if such preset does not"]
				#[doc = " exist. Returned `Vec<u8>` contains bytes of JSON blob (patch) which comprises a list of"]
				#[doc = " (potentially nested) key-value pairs that are intended for customizing the default"]
				#[doc = " runtime genesis config. The patch shall be merged (rfc7386) with the JSON representation"]
				#[doc = " of the default `RuntimeGenesisConfig` to create a comprehensive genesis config that can"]
				#[doc = " be used in `build_state` method."]
				pub fn get_preset(
					&self,
					id: types::get_preset::Id,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetPreset,
					types::get_preset::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GenesisBuilder",
						"get_preset",
						types::GetPreset { id },
						[
							43u8, 153u8, 23u8, 52u8, 113u8, 161u8, 227u8, 122u8, 169u8, 135u8,
							119u8, 8u8, 128u8, 33u8, 143u8, 235u8, 13u8, 173u8, 58u8, 121u8, 178u8,
							223u8, 66u8, 217u8, 22u8, 244u8, 168u8, 113u8, 202u8, 186u8, 241u8,
							124u8,
						],
					)
				}
				#[doc = " Returns a list of identifiers for available builtin `RuntimeGenesisConfig` presets."]
				#[doc = ""]
				#[doc = " The presets from the list can be queried with [`GenesisBuilder::get_preset`] method. If"]
				#[doc = " no named presets are provided by the runtime the list is empty."]
				pub fn preset_names(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::PresetNames,
					types::preset_names::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GenesisBuilder",
						"preset_names",
						types::PresetNames {},
						[
							150u8, 117u8, 54u8, 129u8, 221u8, 130u8, 186u8, 71u8, 13u8, 140u8,
							77u8, 180u8, 141u8, 37u8, 22u8, 219u8, 149u8, 218u8, 186u8, 206u8,
							80u8, 42u8, 165u8, 41u8, 99u8, 184u8, 73u8, 37u8, 125u8, 188u8, 167u8,
							122u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod build_state {
					use super::runtime_types;
					pub type Json =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::result::Result<
							(),
							::subxt::ext::subxt_core::alloc::string::String,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BuildState {
					pub json: build_state::Json,
				}
				pub mod get_preset {
					use super::runtime_types;
					pub type Id =
						::core::option::Option<::subxt::ext::subxt_core::alloc::string::String>;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetPreset {
					pub id: get_preset::Id,
				}
				pub mod preset_names {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::subxt::ext::subxt_core::alloc::vec::Vec<
							::subxt::ext::subxt_core::alloc::string::String,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct PresetNames {}
			}
		}
		pub mod core {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " The `Core` runtime api that every Substrate runtime needs to implement."]
			pub struct Core;
			impl Core {
				#[doc = " Returns the version of the runtime."]
				pub fn version(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::Version,
					types::version::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"Core",
						"version",
						types::Version {},
						[
							79u8, 22u8, 137u8, 4u8, 40u8, 64u8, 30u8, 180u8, 49u8, 222u8, 114u8,
							125u8, 44u8, 25u8, 33u8, 152u8, 98u8, 42u8, 72u8, 178u8, 240u8, 103u8,
							34u8, 187u8, 81u8, 161u8, 183u8, 6u8, 120u8, 2u8, 146u8, 0u8,
						],
					)
				}
				#[doc = " Execute the given block."]
				pub fn execute_block(
					&self,
					block: types::execute_block::Block,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::ExecuteBlock,
					types::execute_block::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"Core",
						"execute_block",
						types::ExecuteBlock { block },
						[
							133u8, 135u8, 228u8, 65u8, 106u8, 27u8, 85u8, 158u8, 112u8, 254u8,
							93u8, 26u8, 102u8, 201u8, 118u8, 216u8, 249u8, 247u8, 91u8, 74u8, 56u8,
							208u8, 231u8, 115u8, 131u8, 29u8, 209u8, 6u8, 65u8, 57u8, 214u8, 125u8,
						],
					)
				}
				#[doc = " Initialize a block with the given header and return the runtime executive mode."]
				pub fn initialize_block(
					&self,
					header: types::initialize_block::Header,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::InitializeBlock,
					types::initialize_block::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"Core",
						"initialize_block",
						types::InitializeBlock { header },
						[
							132u8, 169u8, 113u8, 112u8, 80u8, 139u8, 113u8, 35u8, 41u8, 81u8, 36u8,
							35u8, 37u8, 202u8, 29u8, 207u8, 205u8, 229u8, 145u8, 7u8, 133u8, 94u8,
							25u8, 108u8, 233u8, 86u8, 234u8, 29u8, 236u8, 57u8, 56u8, 186u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod version {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_version::RuntimeVersion;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Version {}
				pub mod execute_block {
					use super::runtime_types;
					pub type Block = runtime_types :: sp_runtime :: generic :: block :: Block < runtime_types :: sp_runtime :: generic :: header :: Header < :: core :: primitive :: u32 > , :: subxt :: ext :: subxt_core :: utils :: UncheckedExtrinsic < :: subxt :: ext :: subxt_core :: utils :: MultiAddress < :: subxt :: ext :: subxt_core :: utils :: AccountId32 , () > , runtime_types :: midnight_node_runtime :: RuntimeCall , runtime_types :: sp_runtime :: MultiSignature , (runtime_types :: frame_system :: extensions :: check_non_zero_sender :: CheckNonZeroSender , runtime_types :: frame_system :: extensions :: check_spec_version :: CheckSpecVersion , runtime_types :: frame_system :: extensions :: check_tx_version :: CheckTxVersion , runtime_types :: frame_system :: extensions :: check_genesis :: CheckGenesis , runtime_types :: frame_system :: extensions :: check_mortality :: CheckMortality , runtime_types :: frame_system :: extensions :: check_nonce :: CheckNonce , runtime_types :: frame_system :: extensions :: check_weight :: CheckWeight ,) > > ;
					pub mod output {
						use super::runtime_types;
						pub type Output = ();
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ExecuteBlock {
					pub block: execute_block::Block,
				}
				pub mod initialize_block {
					use super::runtime_types;
					pub type Header =
						runtime_types::sp_runtime::generic::header::Header<::core::primitive::u32>;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_runtime::ExtrinsicInclusionMode;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct InitializeBlock {
					pub header: initialize_block::Header,
				}
			}
		}
		pub mod midnight_runtime_api {
			use super::root_mod;
			use super::runtime_types;
			pub struct MidnightRuntimeApi;
			impl MidnightRuntimeApi {
				pub fn get_contract_state(
					&self,
					contract_address: types::get_contract_state::ContractAddress,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetContractState,
					types::get_contract_state::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_contract_state",
						types::GetContractState { contract_address },
						[
							150u8, 188u8, 46u8, 158u8, 24u8, 241u8, 215u8, 69u8, 61u8, 215u8,
							242u8, 33u8, 247u8, 117u8, 248u8, 222u8, 166u8, 25u8, 183u8, 213u8,
							199u8, 202u8, 156u8, 74u8, 233u8, 63u8, 131u8, 55u8, 52u8, 172u8, 24u8,
							9u8,
						],
					)
				}
				pub fn get_decoded_transaction(
					&self,
					transaction_bytes: types::get_decoded_transaction::TransactionBytes,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetDecodedTransaction,
					types::get_decoded_transaction::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_decoded_transaction",
						types::GetDecodedTransaction { transaction_bytes },
						[
							253u8, 182u8, 41u8, 148u8, 99u8, 189u8, 163u8, 79u8, 230u8, 221u8,
							98u8, 157u8, 135u8, 103u8, 36u8, 150u8, 12u8, 34u8, 218u8, 91u8, 234u8,
							164u8, 229u8, 125u8, 233u8, 218u8, 115u8, 229u8, 176u8, 246u8, 227u8,
							68u8,
						],
					)
				}
				pub fn get_zswap_chain_state(
					&self,
					contract_address: types::get_zswap_chain_state::ContractAddress,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetZswapChainState,
					types::get_zswap_chain_state::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_zswap_chain_state",
						types::GetZswapChainState { contract_address },
						[
							173u8, 238u8, 28u8, 6u8, 199u8, 121u8, 180u8, 34u8, 227u8, 133u8,
							171u8, 107u8, 108u8, 176u8, 190u8, 195u8, 219u8, 8u8, 218u8, 117u8,
							152u8, 231u8, 201u8, 118u8, 111u8, 116u8, 107u8, 195u8, 125u8, 89u8,
							179u8, 237u8,
						],
					)
				}
				pub fn get_network_id(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetNetworkId,
					types::get_network_id::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_network_id",
						types::GetNetworkId {},
						[
							224u8, 25u8, 252u8, 217u8, 77u8, 248u8, 167u8, 240u8, 153u8, 37u8,
							151u8, 198u8, 139u8, 246u8, 158u8, 80u8, 183u8, 198u8, 144u8, 165u8,
							157u8, 163u8, 95u8, 215u8, 237u8, 195u8, 76u8, 34u8, 237u8, 159u8, 9u8,
							187u8,
						],
					)
				}
				pub fn get_ledger_version(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetLedgerVersion,
					types::get_ledger_version::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_ledger_version",
						types::GetLedgerVersion {},
						[
							200u8, 30u8, 152u8, 16u8, 133u8, 239u8, 85u8, 67u8, 48u8, 240u8, 36u8,
							210u8, 244u8, 66u8, 221u8, 235u8, 88u8, 112u8, 33u8, 226u8, 116u8,
							148u8, 124u8, 242u8, 26u8, 59u8, 231u8, 118u8, 106u8, 53u8, 192u8,
							59u8,
						],
					)
				}
				pub fn get_unclaimed_amount(
					&self,
					beneficiary: types::get_unclaimed_amount::Beneficiary,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetUnclaimedAmount,
					types::get_unclaimed_amount::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_unclaimed_amount",
						types::GetUnclaimedAmount { beneficiary },
						[
							47u8, 98u8, 216u8, 210u8, 21u8, 229u8, 246u8, 124u8, 194u8, 119u8,
							103u8, 243u8, 146u8, 151u8, 101u8, 58u8, 57u8, 198u8, 186u8, 194u8,
							115u8, 115u8, 32u8, 11u8, 9u8, 211u8, 220u8, 15u8, 125u8, 57u8, 139u8,
							45u8,
						],
					)
				}
				pub fn get_ledger_parameters(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetLedgerParameters,
					types::get_ledger_parameters::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_ledger_parameters",
						types::GetLedgerParameters {},
						[
							206u8, 221u8, 26u8, 104u8, 55u8, 182u8, 8u8, 29u8, 135u8, 150u8, 217u8,
							153u8, 22u8, 18u8, 227u8, 108u8, 204u8, 133u8, 68u8, 166u8, 255u8,
							47u8, 226u8, 37u8, 108u8, 248u8, 131u8, 152u8, 128u8, 194u8, 111u8,
							53u8,
						],
					)
				}
				pub fn get_transaction_cost(
					&self,
					transaction_bytes: types::get_transaction_cost::TransactionBytes,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetTransactionCost,
					types::get_transaction_cost::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_transaction_cost",
						types::GetTransactionCost { transaction_bytes },
						[
							116u8, 223u8, 252u8, 95u8, 115u8, 172u8, 16u8, 66u8, 4u8, 247u8, 106u8,
							88u8, 33u8, 191u8, 42u8, 59u8, 11u8, 165u8, 219u8, 155u8, 249u8, 150u8,
							43u8, 34u8, 21u8, 115u8, 119u8, 52u8, 31u8, 63u8, 80u8, 15u8,
						],
					)
				}
				pub fn get_zswap_state_root(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetZswapStateRoot,
					types::get_zswap_state_root::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MidnightRuntimeApi",
						"get_zswap_state_root",
						types::GetZswapStateRoot {},
						[
							133u8, 131u8, 64u8, 100u8, 239u8, 68u8, 25u8, 246u8, 83u8, 218u8, 63u8,
							138u8, 207u8, 206u8, 156u8, 71u8, 152u8, 240u8, 0u8, 211u8, 151u8,
							92u8, 44u8, 133u8, 224u8, 228u8, 134u8, 36u8, 8u8, 235u8, 113u8, 183u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod get_contract_state {
					use super::runtime_types;
					pub type ContractAddress =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < :: core :: primitive :: u8 > , runtime_types :: midnight_node_ledger :: latest :: common :: types :: LedgerApiError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetContractState {
					pub contract_address: get_contract_state::ContractAddress,
				}
				pub mod get_decoded_transaction {
					use super::runtime_types;
					pub type TransactionBytes =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < runtime_types :: midnight_node_ledger :: common :: types :: Tx , runtime_types :: midnight_node_ledger :: latest :: common :: types :: LedgerApiError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetDecodedTransaction {
					pub transaction_bytes: get_decoded_transaction::TransactionBytes,
				}
				pub mod get_zswap_chain_state {
					use super::runtime_types;
					pub type ContractAddress =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < :: core :: primitive :: u8 > , runtime_types :: midnight_node_ledger :: latest :: common :: types :: LedgerApiError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetZswapChainState {
					pub contract_address: get_zswap_chain_state::ContractAddress,
				}
				pub mod get_network_id {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetNetworkId {}
				pub mod get_ledger_version {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetLedgerVersion {}
				pub mod get_unclaimed_amount {
					use super::runtime_types;
					pub type Beneficiary =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < :: core :: primitive :: u128 , runtime_types :: midnight_node_ledger :: latest :: common :: types :: LedgerApiError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetUnclaimedAmount {
					pub beneficiary: get_unclaimed_amount::Beneficiary,
				}
				pub mod get_ledger_parameters {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < :: core :: primitive :: u8 > , runtime_types :: midnight_node_ledger :: latest :: common :: types :: LedgerApiError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetLedgerParameters {}
				pub mod get_transaction_cost {
					use super::runtime_types;
					pub type TransactionBytes =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < (:: core :: primitive :: u128 , :: core :: primitive :: u64 ,) , runtime_types :: midnight_node_ledger :: latest :: common :: types :: LedgerApiError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetTransactionCost {
					pub transaction_bytes: get_transaction_cost::TransactionBytes,
				}
				pub mod get_zswap_state_root {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < :: core :: primitive :: u8 > , runtime_types :: midnight_node_ledger :: latest :: common :: types :: LedgerApiError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetZswapStateRoot {}
			}
		}
		pub mod upgrade_api {
			use super::root_mod;
			use super::runtime_types;
			pub struct UpgradeApi;
			impl UpgradeApi {
				pub fn get_current_version_info(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetCurrentVersionInfo,
					types::get_current_version_info::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"UpgradeApi",
						"get_current_version_info",
						types::GetCurrentVersionInfo {},
						[
							132u8, 98u8, 45u8, 197u8, 245u8, 209u8, 7u8, 216u8, 10u8, 24u8, 114u8,
							90u8, 250u8, 54u8, 39u8, 66u8, 53u8, 189u8, 135u8, 159u8, 66u8, 22u8,
							49u8, 126u8, 11u8, 213u8, 38u8, 223u8, 50u8, 128u8, 68u8, 11u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod get_current_version_info {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							(::core::primitive::u32, ::subxt::ext::subxt_core::utils::H256);
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetCurrentVersionInfo {}
			}
		}
		pub mod metadata {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " The `Metadata` api trait that returns metadata for the runtime."]
			pub struct Metadata;
			impl Metadata {
				#[doc = " Returns the metadata of a runtime."]
				pub fn metadata(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::Metadata,
					types::metadata::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"Metadata",
						"metadata",
						types::Metadata {},
						[
							231u8, 24u8, 67u8, 152u8, 23u8, 26u8, 188u8, 82u8, 229u8, 6u8, 185u8,
							27u8, 175u8, 68u8, 83u8, 122u8, 69u8, 89u8, 185u8, 74u8, 248u8, 87u8,
							217u8, 124u8, 193u8, 252u8, 199u8, 186u8, 196u8, 179u8, 179u8, 96u8,
						],
					)
				}
				#[doc = " Returns the metadata at a given version."]
				#[doc = ""]
				#[doc = " If the given `version` isn't supported, this will return `None`."]
				#[doc = " Use [`Self::metadata_versions`] to find out about supported metadata version of the runtime."]
				pub fn metadata_at_version(
					&self,
					version: types::metadata_at_version::Version,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::MetadataAtVersion,
					types::metadata_at_version::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"Metadata",
						"metadata_at_version",
						types::MetadataAtVersion { version },
						[
							131u8, 53u8, 212u8, 234u8, 16u8, 25u8, 120u8, 252u8, 153u8, 153u8,
							216u8, 28u8, 54u8, 113u8, 52u8, 236u8, 146u8, 68u8, 142u8, 8u8, 10u8,
							169u8, 131u8, 142u8, 204u8, 38u8, 48u8, 108u8, 134u8, 86u8, 226u8,
							61u8,
						],
					)
				}
				#[doc = " Returns the supported metadata versions."]
				#[doc = ""]
				#[doc = " This can be used to call `metadata_at_version`."]
				pub fn metadata_versions(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::MetadataVersions,
					types::metadata_versions::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"Metadata",
						"metadata_versions",
						types::MetadataVersions {},
						[
							23u8, 144u8, 137u8, 91u8, 188u8, 39u8, 231u8, 208u8, 252u8, 218u8,
							224u8, 176u8, 77u8, 32u8, 130u8, 212u8, 223u8, 76u8, 100u8, 190u8,
							82u8, 94u8, 190u8, 8u8, 82u8, 244u8, 225u8, 179u8, 85u8, 176u8, 56u8,
							16u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod metadata {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_core::OpaqueMetadata;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Metadata {}
				pub mod metadata_at_version {
					use super::runtime_types;
					pub type Version = ::core::primitive::u32;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::core::option::Option<runtime_types::sp_core::OpaqueMetadata>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct MetadataAtVersion {
					pub version: metadata_at_version::Version,
				}
				pub mod metadata_versions {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u32>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct MetadataVersions {}
			}
		}
		pub mod block_builder {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " The `BlockBuilder` api trait that provides the required functionality for building a block."]
			pub struct BlockBuilder;
			impl BlockBuilder {
				#[doc = " Apply the given extrinsic."]
				#[doc = ""]
				#[doc = " Returns an inclusion outcome which specifies if this extrinsic is included in"]
				#[doc = " this block or not."]
				pub fn apply_extrinsic(
					&self,
					extrinsic: types::apply_extrinsic::Extrinsic,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::ApplyExtrinsic,
					types::apply_extrinsic::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BlockBuilder",
						"apply_extrinsic",
						types::ApplyExtrinsic { extrinsic },
						[
							192u8, 184u8, 199u8, 4u8, 85u8, 136u8, 214u8, 205u8, 29u8, 29u8, 98u8,
							145u8, 172u8, 92u8, 168u8, 161u8, 150u8, 133u8, 100u8, 243u8, 100u8,
							100u8, 118u8, 28u8, 104u8, 82u8, 93u8, 63u8, 79u8, 36u8, 149u8, 144u8,
						],
					)
				}
				#[doc = " Finish the current block."]
				pub fn finalize_block(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::FinalizeBlock,
					types::finalize_block::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BlockBuilder",
						"finalize_block",
						types::FinalizeBlock {},
						[
							244u8, 207u8, 24u8, 33u8, 13u8, 69u8, 9u8, 249u8, 145u8, 143u8, 122u8,
							96u8, 197u8, 55u8, 64u8, 111u8, 238u8, 224u8, 34u8, 201u8, 27u8, 146u8,
							232u8, 99u8, 191u8, 30u8, 114u8, 16u8, 32u8, 220u8, 58u8, 62u8,
						],
					)
				}
				#[doc = " Generate inherent extrinsics. The inherent data will vary from chain to chain."]
				pub fn inherent_extrinsics(
					&self,
					inherent: types::inherent_extrinsics::Inherent,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::InherentExtrinsics,
					types::inherent_extrinsics::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BlockBuilder",
						"inherent_extrinsics",
						types::InherentExtrinsics { inherent },
						[
							254u8, 110u8, 245u8, 201u8, 250u8, 192u8, 27u8, 228u8, 151u8, 213u8,
							166u8, 89u8, 94u8, 81u8, 189u8, 234u8, 64u8, 18u8, 245u8, 80u8, 29u8,
							18u8, 140u8, 129u8, 113u8, 236u8, 135u8, 55u8, 79u8, 159u8, 175u8,
							183u8,
						],
					)
				}
				#[doc = " Check that the inherents are valid. The inherent data will vary from chain to chain."]
				pub fn check_inherents(
					&self,
					block: types::check_inherents::Block,
					data: types::check_inherents::Data,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::CheckInherents,
					types::check_inherents::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BlockBuilder",
						"check_inherents",
						types::CheckInherents { block, data },
						[
							153u8, 134u8, 1u8, 215u8, 139u8, 11u8, 53u8, 51u8, 210u8, 175u8, 197u8,
							28u8, 38u8, 209u8, 175u8, 247u8, 142u8, 157u8, 50u8, 151u8, 164u8,
							191u8, 181u8, 118u8, 80u8, 97u8, 160u8, 248u8, 110u8, 217u8, 181u8,
							234u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod apply_extrinsic {
					use super::runtime_types;
					pub type Extrinsic = :: subxt :: ext :: subxt_core :: utils :: UncheckedExtrinsic < :: subxt :: ext :: subxt_core :: utils :: MultiAddress < :: subxt :: ext :: subxt_core :: utils :: AccountId32 , () > , runtime_types :: midnight_node_runtime :: RuntimeCall , runtime_types :: sp_runtime :: MultiSignature , (runtime_types :: frame_system :: extensions :: check_non_zero_sender :: CheckNonZeroSender , runtime_types :: frame_system :: extensions :: check_spec_version :: CheckSpecVersion , runtime_types :: frame_system :: extensions :: check_tx_version :: CheckTxVersion , runtime_types :: frame_system :: extensions :: check_genesis :: CheckGenesis , runtime_types :: frame_system :: extensions :: check_mortality :: CheckMortality , runtime_types :: frame_system :: extensions :: check_nonce :: CheckNonce , runtime_types :: frame_system :: extensions :: check_weight :: CheckWeight ,) > ;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < :: core :: result :: Result < () , runtime_types :: sp_runtime :: DispatchError > , runtime_types :: sp_runtime :: transaction_validity :: TransactionValidityError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ApplyExtrinsic {
					pub extrinsic: apply_extrinsic::Extrinsic,
				}
				pub mod finalize_block {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_runtime::generic::header::Header<
							::core::primitive::u32,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct FinalizeBlock {}
				pub mod inherent_extrinsics {
					use super::runtime_types;
					pub type Inherent = runtime_types::sp_inherents::InherentData;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < :: subxt :: ext :: subxt_core :: utils :: UncheckedExtrinsic < :: subxt :: ext :: subxt_core :: utils :: MultiAddress < :: subxt :: ext :: subxt_core :: utils :: AccountId32 , () > , runtime_types :: midnight_node_runtime :: RuntimeCall , runtime_types :: sp_runtime :: MultiSignature , (runtime_types :: frame_system :: extensions :: check_non_zero_sender :: CheckNonZeroSender , runtime_types :: frame_system :: extensions :: check_spec_version :: CheckSpecVersion , runtime_types :: frame_system :: extensions :: check_tx_version :: CheckTxVersion , runtime_types :: frame_system :: extensions :: check_genesis :: CheckGenesis , runtime_types :: frame_system :: extensions :: check_mortality :: CheckMortality , runtime_types :: frame_system :: extensions :: check_nonce :: CheckNonce , runtime_types :: frame_system :: extensions :: check_weight :: CheckWeight ,) > > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct InherentExtrinsics {
					pub inherent: inherent_extrinsics::Inherent,
				}
				pub mod check_inherents {
					use super::runtime_types;
					pub type Block = runtime_types :: sp_runtime :: generic :: block :: Block < runtime_types :: sp_runtime :: generic :: header :: Header < :: core :: primitive :: u32 > , :: subxt :: ext :: subxt_core :: utils :: UncheckedExtrinsic < :: subxt :: ext :: subxt_core :: utils :: MultiAddress < :: subxt :: ext :: subxt_core :: utils :: AccountId32 , () > , runtime_types :: midnight_node_runtime :: RuntimeCall , runtime_types :: sp_runtime :: MultiSignature , (runtime_types :: frame_system :: extensions :: check_non_zero_sender :: CheckNonZeroSender , runtime_types :: frame_system :: extensions :: check_spec_version :: CheckSpecVersion , runtime_types :: frame_system :: extensions :: check_tx_version :: CheckTxVersion , runtime_types :: frame_system :: extensions :: check_genesis :: CheckGenesis , runtime_types :: frame_system :: extensions :: check_mortality :: CheckMortality , runtime_types :: frame_system :: extensions :: check_nonce :: CheckNonce , runtime_types :: frame_system :: extensions :: check_weight :: CheckWeight ,) > > ;
					pub type Data = runtime_types::sp_inherents::InherentData;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_inherents::CheckInherentsResult;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct CheckInherents {
					pub block: check_inherents::Block,
					pub data: check_inherents::Data,
				}
			}
		}
		pub mod tagged_transaction_queue {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " The `TaggedTransactionQueue` api trait for interfering with the transaction queue."]
			pub struct TaggedTransactionQueue;
			impl TaggedTransactionQueue {
				#[doc = " Validate the transaction."]
				#[doc = ""]
				#[doc = " This method is invoked by the transaction pool to learn details about given transaction."]
				#[doc = " The implementation should make sure to verify the correctness of the transaction"]
				#[doc = " against current state. The given `block_hash` corresponds to the hash of the block"]
				#[doc = " that is used as current state."]
				#[doc = ""]
				#[doc = " Note that this call may be performed by the pool multiple times and transactions"]
				#[doc = " might be verified in any possible order."]
				pub fn validate_transaction(
					&self,
					source: types::validate_transaction::Source,
					tx: types::validate_transaction::Tx,
					block_hash: types::validate_transaction::BlockHash,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::ValidateTransaction,
					types::validate_transaction::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"TaggedTransactionQueue",
						"validate_transaction",
						types::ValidateTransaction { source, tx, block_hash },
						[
							19u8, 53u8, 170u8, 115u8, 75u8, 121u8, 231u8, 50u8, 199u8, 181u8,
							243u8, 170u8, 163u8, 224u8, 213u8, 134u8, 206u8, 207u8, 88u8, 242u8,
							80u8, 139u8, 233u8, 87u8, 175u8, 249u8, 178u8, 169u8, 255u8, 171u8,
							4u8, 125u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod validate_transaction {
					use super::runtime_types;
					pub type Source =
						runtime_types::sp_runtime::transaction_validity::TransactionSource;
					pub type Tx = :: subxt :: ext :: subxt_core :: utils :: UncheckedExtrinsic < :: subxt :: ext :: subxt_core :: utils :: MultiAddress < :: subxt :: ext :: subxt_core :: utils :: AccountId32 , () > , runtime_types :: midnight_node_runtime :: RuntimeCall , runtime_types :: sp_runtime :: MultiSignature , (runtime_types :: frame_system :: extensions :: check_non_zero_sender :: CheckNonZeroSender , runtime_types :: frame_system :: extensions :: check_spec_version :: CheckSpecVersion , runtime_types :: frame_system :: extensions :: check_tx_version :: CheckTxVersion , runtime_types :: frame_system :: extensions :: check_genesis :: CheckGenesis , runtime_types :: frame_system :: extensions :: check_mortality :: CheckMortality , runtime_types :: frame_system :: extensions :: check_nonce :: CheckNonce , runtime_types :: frame_system :: extensions :: check_weight :: CheckWeight ,) > ;
					pub type BlockHash = ::subxt::ext::subxt_core::utils::H256;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: result :: Result < runtime_types :: sp_runtime :: transaction_validity :: ValidTransaction , runtime_types :: sp_runtime :: transaction_validity :: TransactionValidityError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ValidateTransaction {
					pub source: validate_transaction::Source,
					pub tx: validate_transaction::Tx,
					pub block_hash: validate_transaction::BlockHash,
				}
			}
		}
		pub mod offchain_worker_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " The offchain worker api."]
			pub struct OffchainWorkerApi;
			impl OffchainWorkerApi {
				#[doc = " Starts the off-chain task for given block header."]
				pub fn offchain_worker(
					&self,
					header: types::offchain_worker::Header,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::OffchainWorker,
					types::offchain_worker::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"OffchainWorkerApi",
						"offchain_worker",
						types::OffchainWorker { header },
						[
							10u8, 135u8, 19u8, 153u8, 33u8, 216u8, 18u8, 242u8, 33u8, 140u8, 4u8,
							223u8, 200u8, 130u8, 103u8, 118u8, 137u8, 24u8, 19u8, 127u8, 161u8,
							29u8, 184u8, 111u8, 222u8, 111u8, 253u8, 73u8, 45u8, 31u8, 79u8, 60u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod offchain_worker {
					use super::runtime_types;
					pub type Header =
						runtime_types::sp_runtime::generic::header::Header<::core::primitive::u32>;
					pub mod output {
						use super::runtime_types;
						pub type Output = ();
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct OffchainWorker {
					pub header: offchain_worker::Header,
				}
			}
		}
		pub mod aura_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " API necessary for block authorship with aura."]
			pub struct AuraApi;
			impl AuraApi {
				#[doc = " Returns the slot duration for Aura."]
				#[doc = ""]
				#[doc = " Currently, only the value provided by this type at genesis will be used."]
				pub fn slot_duration(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::SlotDuration,
					types::slot_duration::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"AuraApi",
						"slot_duration",
						types::SlotDuration {},
						[
							233u8, 210u8, 132u8, 172u8, 100u8, 125u8, 239u8, 92u8, 114u8, 82u8,
							7u8, 110u8, 179u8, 196u8, 10u8, 19u8, 211u8, 15u8, 174u8, 2u8, 91u8,
							73u8, 133u8, 100u8, 205u8, 201u8, 191u8, 60u8, 163u8, 122u8, 215u8,
							10u8,
						],
					)
				}
				#[doc = " Return the current set of authorities."]
				pub fn authorities(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::Authorities,
					types::authorities::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"AuraApi",
						"authorities",
						types::Authorities {},
						[
							35u8, 244u8, 24u8, 155u8, 95u8, 1u8, 221u8, 159u8, 33u8, 144u8, 213u8,
							26u8, 13u8, 21u8, 136u8, 72u8, 45u8, 47u8, 15u8, 51u8, 235u8, 10u8,
							6u8, 219u8, 9u8, 246u8, 50u8, 252u8, 49u8, 77u8, 64u8, 182u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod slot_duration {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_consensus_slots::SlotDuration;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SlotDuration {}
				pub mod authorities {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::subxt::ext::subxt_core::alloc::vec::Vec<
							runtime_types::sp_consensus_aura::sr25519::app_sr25519::Public,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Authorities {}
			}
		}
		pub mod beefy_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " API necessary for BEEFY voters."]
			pub struct BeefyApi;
			impl BeefyApi {
				#[doc = " Return the block number where BEEFY consensus is enabled/started"]
				pub fn beefy_genesis(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::BeefyGenesis,
					types::beefy_genesis::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyApi",
						"beefy_genesis",
						types::BeefyGenesis {},
						[
							246u8, 129u8, 31u8, 77u8, 24u8, 47u8, 5u8, 156u8, 64u8, 222u8, 180u8,
							78u8, 110u8, 77u8, 218u8, 149u8, 210u8, 151u8, 164u8, 220u8, 165u8,
							119u8, 116u8, 220u8, 20u8, 122u8, 37u8, 176u8, 75u8, 218u8, 194u8,
							244u8,
						],
					)
				}
				#[doc = " Return the current active BEEFY validator set"]
				pub fn validator_set(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::ValidatorSet,
					types::validator_set::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyApi",
						"validator_set",
						types::ValidatorSet {},
						[
							89u8, 55u8, 143u8, 190u8, 77u8, 133u8, 228u8, 92u8, 245u8, 253u8, 70u8,
							232u8, 213u8, 119u8, 88u8, 47u8, 122u8, 60u8, 179u8, 61u8, 199u8,
							251u8, 45u8, 230u8, 76u8, 202u8, 49u8, 178u8, 204u8, 143u8, 196u8,
							135u8,
						],
					)
				}
				#[doc = " Submits an unsigned extrinsic to report a double voting equivocation. The caller"]
				#[doc = " must provide the double voting proof and a key ownership proof"]
				#[doc = " (should be obtained using `generate_key_ownership_proof`). The"]
				#[doc = " extrinsic will be unsigned and should only be accepted for local"]
				#[doc = " authorship (not to be broadcast to the network). This method returns"]
				#[doc = " `None` when creation of the extrinsic fails, e.g. if equivocation"]
				#[doc = " reporting is disabled for the given runtime (i.e. this method is"]
				#[doc = " hardcoded to return `None`). Only useful in an offchain context."]
				pub fn submit_report_double_voting_unsigned_extrinsic(
					&self,
					equivocation_proof : types :: submit_report_double_voting_unsigned_extrinsic :: EquivocationProof,
					key_owner_proof : types :: submit_report_double_voting_unsigned_extrinsic :: KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::SubmitReportDoubleVotingUnsignedExtrinsic,
					types::submit_report_double_voting_unsigned_extrinsic::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyApi",
						"submit_report_double_voting_unsigned_extrinsic",
						types::SubmitReportDoubleVotingUnsignedExtrinsic {
							equivocation_proof,
							key_owner_proof,
						},
						[
							150u8, 77u8, 238u8, 96u8, 27u8, 152u8, 189u8, 202u8, 142u8, 30u8,
							252u8, 43u8, 134u8, 87u8, 86u8, 188u8, 184u8, 126u8, 229u8, 230u8,
							106u8, 6u8, 0u8, 89u8, 47u8, 219u8, 138u8, 136u8, 6u8, 204u8, 189u8,
							213u8,
						],
					)
				}
				#[doc = " Submits an unsigned extrinsic to report a fork voting equivocation. The caller"]
				#[doc = " must provide the fork voting proof (the ancestry proof should be obtained using"]
				#[doc = " `generate_ancestry_proof`) and a key ownership proof (should be obtained using"]
				#[doc = " `generate_key_ownership_proof`). The extrinsic will be unsigned and should only"]
				#[doc = " be accepted for local authorship (not to be broadcast to the network). This method"]
				#[doc = " returns `None` when creation of the extrinsic fails, e.g. if equivocation"]
				#[doc = " reporting is disabled for the given runtime (i.e. this method is"]
				#[doc = " hardcoded to return `None`). Only useful in an offchain context."]
				pub fn submit_report_fork_voting_unsigned_extrinsic(
					&self,
					equivocation_proof : types :: submit_report_fork_voting_unsigned_extrinsic :: EquivocationProof,
					key_owner_proof : types :: submit_report_fork_voting_unsigned_extrinsic :: KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::SubmitReportForkVotingUnsignedExtrinsic,
					types::submit_report_fork_voting_unsigned_extrinsic::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyApi",
						"submit_report_fork_voting_unsigned_extrinsic",
						types::SubmitReportForkVotingUnsignedExtrinsic {
							equivocation_proof,
							key_owner_proof,
						},
						[
							148u8, 230u8, 78u8, 95u8, 219u8, 17u8, 5u8, 8u8, 254u8, 198u8, 252u8,
							211u8, 136u8, 20u8, 103u8, 153u8, 248u8, 144u8, 128u8, 58u8, 96u8,
							140u8, 245u8, 97u8, 13u8, 17u8, 19u8, 35u8, 123u8, 30u8, 24u8, 38u8,
						],
					)
				}
				#[doc = " Submits an unsigned extrinsic to report a future block voting equivocation. The caller"]
				#[doc = " must provide the future block voting proof and a key ownership proof"]
				#[doc = " (should be obtained using `generate_key_ownership_proof`)."]
				#[doc = " The extrinsic will be unsigned and should only be accepted for local"]
				#[doc = " authorship (not to be broadcast to the network). This method returns"]
				#[doc = " `None` when creation of the extrinsic fails, e.g. if equivocation"]
				#[doc = " reporting is disabled for the given runtime (i.e. this method is"]
				#[doc = " hardcoded to return `None`). Only useful in an offchain context."]
				pub fn submit_report_future_block_voting_unsigned_extrinsic(
					&self,
					equivocation_proof : types :: submit_report_future_block_voting_unsigned_extrinsic :: EquivocationProof,
					key_owner_proof : types :: submit_report_future_block_voting_unsigned_extrinsic :: KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::SubmitReportFutureBlockVotingUnsignedExtrinsic,
					types::submit_report_future_block_voting_unsigned_extrinsic::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyApi",
						"submit_report_future_block_voting_unsigned_extrinsic",
						types::SubmitReportFutureBlockVotingUnsignedExtrinsic {
							equivocation_proof,
							key_owner_proof,
						},
						[
							111u8, 49u8, 126u8, 212u8, 123u8, 116u8, 91u8, 235u8, 190u8, 88u8,
							70u8, 175u8, 50u8, 229u8, 231u8, 241u8, 10u8, 208u8, 107u8, 140u8,
							236u8, 225u8, 78u8, 205u8, 43u8, 218u8, 51u8, 68u8, 236u8, 247u8, 74u8,
							181u8,
						],
					)
				}
				#[doc = " Generates a proof of key ownership for the given authority in the"]
				#[doc = " given set. An example usage of this module is coupled with the"]
				#[doc = " session historical module to prove that a given authority key is"]
				#[doc = " tied to a given staking identity during a specific session. Proofs"]
				#[doc = " of key ownership are necessary for submitting equivocation reports."]
				#[doc = " NOTE: even though the API takes a `set_id` as parameter the current"]
				#[doc = " implementations ignores this parameter and instead relies on this"]
				#[doc = " method being called at the correct block height, i.e. any point at"]
				#[doc = " which the given set id is live on-chain. Future implementations will"]
				#[doc = " instead use indexed data through an offchain worker, not requiring"]
				#[doc = " older states to be available."]
				pub fn generate_key_ownership_proof(
					&self,
					set_id: types::generate_key_ownership_proof::SetId,
					authority_id: types::generate_key_ownership_proof::AuthorityId,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GenerateKeyOwnershipProof,
					types::generate_key_ownership_proof::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyApi",
						"generate_key_ownership_proof",
						types::GenerateKeyOwnershipProof { set_id, authority_id },
						[
							119u8, 183u8, 178u8, 109u8, 57u8, 250u8, 176u8, 255u8, 11u8, 190u8,
							35u8, 142u8, 102u8, 112u8, 4u8, 219u8, 204u8, 81u8, 163u8, 87u8, 129u8,
							134u8, 34u8, 227u8, 213u8, 163u8, 50u8, 90u8, 54u8, 61u8, 84u8, 215u8,
						],
					)
				}
				#[doc = " Generates a proof that the `prev_block_number` is part of the canonical chain at"]
				#[doc = " `best_known_block_number`."]
				pub fn generate_ancestry_proof(
					&self,
					prev_block_number: types::generate_ancestry_proof::PrevBlockNumber,
					best_known_block_number: types::generate_ancestry_proof::BestKnownBlockNumber,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GenerateAncestryProof,
					types::generate_ancestry_proof::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyApi",
						"generate_ancestry_proof",
						types::GenerateAncestryProof { prev_block_number, best_known_block_number },
						[
							2u8, 159u8, 64u8, 196u8, 142u8, 10u8, 120u8, 189u8, 137u8, 49u8, 163u8,
							103u8, 5u8, 137u8, 190u8, 89u8, 232u8, 17u8, 216u8, 145u8, 99u8, 131u8,
							85u8, 67u8, 75u8, 234u8, 62u8, 61u8, 226u8, 61u8, 187u8, 120u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod beefy_genesis {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<::core::primitive::u32>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BeefyGenesis {}
				pub mod validator_set {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<
							runtime_types::sp_consensus_beefy::ValidatorSet<
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
							>,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ValidatorSet {}
				pub mod submit_report_double_voting_unsigned_extrinsic {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_beefy::DoubleVotingProof<
							::core::primitive::u32,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Signature,
						>;
					pub type KeyOwnerProof = runtime_types::sp_runtime::OpaqueValue;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<()>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SubmitReportDoubleVotingUnsignedExtrinsic {
					pub equivocation_proof:
						submit_report_double_voting_unsigned_extrinsic::EquivocationProof,
					pub key_owner_proof:
						submit_report_double_voting_unsigned_extrinsic::KeyOwnerProof,
				}
				pub mod submit_report_fork_voting_unsigned_extrinsic {
					use super::runtime_types;
					pub type EquivocationProof = runtime_types::sp_consensus_beefy::ForkVotingProof<
						runtime_types::sp_runtime::generic::header::Header<::core::primitive::u32>,
						runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
						runtime_types::sp_runtime::OpaqueValue,
					>;
					pub type KeyOwnerProof = runtime_types::sp_runtime::OpaqueValue;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<()>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SubmitReportForkVotingUnsignedExtrinsic {
					pub equivocation_proof:
						submit_report_fork_voting_unsigned_extrinsic::EquivocationProof,
					pub key_owner_proof:
						submit_report_fork_voting_unsigned_extrinsic::KeyOwnerProof,
				}
				pub mod submit_report_future_block_voting_unsigned_extrinsic {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_beefy::FutureBlockVotingProof<
							::core::primitive::u32,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
						>;
					pub type KeyOwnerProof = runtime_types::sp_runtime::OpaqueValue;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<()>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SubmitReportFutureBlockVotingUnsignedExtrinsic {
					pub equivocation_proof:
						submit_report_future_block_voting_unsigned_extrinsic::EquivocationProof,
					pub key_owner_proof:
						submit_report_future_block_voting_unsigned_extrinsic::KeyOwnerProof,
				}
				pub mod generate_key_ownership_proof {
					use super::runtime_types;
					pub type SetId = ::core::primitive::u64;
					pub type AuthorityId = runtime_types::sp_consensus_beefy::ecdsa_crypto::Public;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::core::option::Option<runtime_types::sp_runtime::OpaqueValue>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GenerateKeyOwnershipProof {
					pub set_id: generate_key_ownership_proof::SetId,
					pub authority_id: generate_key_ownership_proof::AuthorityId,
				}
				pub mod generate_ancestry_proof {
					use super::runtime_types;
					pub type PrevBlockNumber = ::core::primitive::u32;
					pub type BestKnownBlockNumber = ::core::option::Option<::core::primitive::u32>;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::core::option::Option<runtime_types::sp_runtime::OpaqueValue>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GenerateAncestryProof {
					pub prev_block_number: generate_ancestry_proof::PrevBlockNumber,
					pub best_known_block_number: generate_ancestry_proof::BestKnownBlockNumber,
				}
			}
		}
		pub mod mmr_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " API to interact with MMR pallet."]
			pub struct MmrApi;
			impl MmrApi {
				#[doc = " Return the on-chain MMR root hash."]
				pub fn mmr_root(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::MmrRoot,
					types::mmr_root::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MmrApi",
						"mmr_root",
						types::MmrRoot {},
						[
							148u8, 252u8, 77u8, 233u8, 236u8, 8u8, 119u8, 105u8, 207u8, 161u8,
							109u8, 158u8, 211u8, 64u8, 67u8, 216u8, 242u8, 52u8, 122u8, 4u8, 83u8,
							113u8, 54u8, 77u8, 165u8, 89u8, 61u8, 159u8, 98u8, 51u8, 45u8, 90u8,
						],
					)
				}
				#[doc = " Return the number of MMR blocks in the chain."]
				pub fn mmr_leaf_count(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::MmrLeafCount,
					types::mmr_leaf_count::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MmrApi",
						"mmr_leaf_count",
						types::MmrLeafCount {},
						[
							165u8, 141u8, 127u8, 184u8, 27u8, 185u8, 251u8, 25u8, 44u8, 93u8,
							239u8, 158u8, 104u8, 91u8, 22u8, 87u8, 101u8, 166u8, 90u8, 90u8, 45u8,
							105u8, 254u8, 136u8, 233u8, 121u8, 9u8, 216u8, 179u8, 55u8, 126u8,
							158u8,
						],
					)
				}
				#[doc = " Generate MMR proof for a series of block numbers. If `best_known_block_number = Some(n)`,"]
				#[doc = " use historical MMR state at given block height `n`. Else, use current MMR state."]
				pub fn generate_proof(
					&self,
					block_numbers: types::generate_proof::BlockNumbers,
					best_known_block_number: types::generate_proof::BestKnownBlockNumber,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GenerateProof,
					types::generate_proof::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MmrApi",
						"generate_proof",
						types::GenerateProof { block_numbers, best_known_block_number },
						[
							187u8, 175u8, 153u8, 82u8, 245u8, 180u8, 126u8, 156u8, 67u8, 89u8,
							253u8, 29u8, 54u8, 168u8, 196u8, 144u8, 24u8, 123u8, 154u8, 69u8,
							245u8, 90u8, 110u8, 239u8, 15u8, 125u8, 204u8, 148u8, 71u8, 209u8,
							58u8, 32u8,
						],
					)
				}
				#[doc = " Verify MMR proof against on-chain MMR for a batch of leaves."]
				#[doc = ""]
				#[doc = " Note this function will use on-chain MMR root hash and check if the proof matches the hash."]
				#[doc = " Note, the leaves should be sorted such that corresponding leaves and leaf indices have the"]
				#[doc = " same position in both the `leaves` vector and the `leaf_indices` vector contained in the [LeafProof]"]
				pub fn verify_proof(
					&self,
					leaves: types::verify_proof::Leaves,
					proof: types::verify_proof::Proof,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::VerifyProof,
					types::verify_proof::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MmrApi",
						"verify_proof",
						types::VerifyProof { leaves, proof },
						[
							236u8, 54u8, 135u8, 196u8, 161u8, 247u8, 183u8, 78u8, 153u8, 69u8,
							59u8, 78u8, 62u8, 20u8, 187u8, 47u8, 77u8, 209u8, 209u8, 224u8, 127u8,
							85u8, 122u8, 33u8, 123u8, 128u8, 92u8, 251u8, 110u8, 233u8, 50u8,
							160u8,
						],
					)
				}
				#[doc = " Verify MMR proof against given root hash for a batch of leaves."]
				#[doc = ""]
				#[doc = " Note this function does not require any on-chain storage - the"]
				#[doc = " proof is verified against given MMR root hash."]
				#[doc = ""]
				#[doc = " Note, the leaves should be sorted such that corresponding leaves and leaf indices have the"]
				#[doc = " same position in both the `leaves` vector and the `leaf_indices` vector contained in the [LeafProof]"]
				pub fn verify_proof_stateless(
					&self,
					root: types::verify_proof_stateless::Root,
					leaves: types::verify_proof_stateless::Leaves,
					proof: types::verify_proof_stateless::Proof,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::VerifyProofStateless,
					types::verify_proof_stateless::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"MmrApi",
						"verify_proof_stateless",
						types::VerifyProofStateless { root, leaves, proof },
						[
							163u8, 232u8, 190u8, 65u8, 135u8, 136u8, 50u8, 60u8, 137u8, 37u8,
							192u8, 24u8, 137u8, 144u8, 165u8, 131u8, 49u8, 88u8, 15u8, 139u8, 83u8,
							152u8, 162u8, 148u8, 22u8, 74u8, 82u8, 25u8, 183u8, 83u8, 212u8, 56u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod mmr_root {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::result::Result<
							::subxt::ext::subxt_core::utils::H256,
							runtime_types::sp_mmr_primitives::Error,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct MmrRoot {}
				pub mod mmr_leaf_count {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::result::Result<
							::core::primitive::u64,
							runtime_types::sp_mmr_primitives::Error,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct MmrLeafCount {}
				pub mod generate_proof {
					use super::runtime_types;
					pub type BlockNumbers =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u32>;
					pub type BestKnownBlockNumber = ::core::option::Option<::core::primitive::u32>;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::result::Result<
							(
								::subxt::ext::subxt_core::alloc::vec::Vec<
									runtime_types::sp_mmr_primitives::EncodableOpaqueLeaf,
								>,
								runtime_types::sp_mmr_primitives::LeafProof<
									::subxt::ext::subxt_core::utils::H256,
								>,
							),
							runtime_types::sp_mmr_primitives::Error,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GenerateProof {
					pub block_numbers: generate_proof::BlockNumbers,
					pub best_known_block_number: generate_proof::BestKnownBlockNumber,
				}
				pub mod verify_proof {
					use super::runtime_types;
					pub type Leaves = ::subxt::ext::subxt_core::alloc::vec::Vec<
						runtime_types::sp_mmr_primitives::EncodableOpaqueLeaf,
					>;
					pub type Proof = runtime_types::sp_mmr_primitives::LeafProof<
						::subxt::ext::subxt_core::utils::H256,
					>;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::core::result::Result<(), runtime_types::sp_mmr_primitives::Error>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct VerifyProof {
					pub leaves: verify_proof::Leaves,
					pub proof: verify_proof::Proof,
				}
				pub mod verify_proof_stateless {
					use super::runtime_types;
					pub type Root = ::subxt::ext::subxt_core::utils::H256;
					pub type Leaves = ::subxt::ext::subxt_core::alloc::vec::Vec<
						runtime_types::sp_mmr_primitives::EncodableOpaqueLeaf,
					>;
					pub type Proof = runtime_types::sp_mmr_primitives::LeafProof<
						::subxt::ext::subxt_core::utils::H256,
					>;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::core::result::Result<(), runtime_types::sp_mmr_primitives::Error>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct VerifyProofStateless {
					pub root: verify_proof_stateless::Root,
					pub leaves: verify_proof_stateless::Leaves,
					pub proof: verify_proof_stateless::Proof,
				}
			}
		}
		pub mod beefy_mmr_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " API useful for BEEFY light clients."]
			pub struct BeefyMmrApi;
			impl BeefyMmrApi {
				#[doc = " Return the currently active BEEFY authority set proof."]
				pub fn authority_set_proof(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::AuthoritySetProof,
					types::authority_set_proof::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyMmrApi",
						"authority_set_proof",
						types::AuthoritySetProof {},
						[
							199u8, 220u8, 251u8, 219u8, 216u8, 5u8, 181u8, 172u8, 191u8, 209u8,
							123u8, 25u8, 151u8, 129u8, 166u8, 21u8, 107u8, 22u8, 74u8, 144u8,
							202u8, 6u8, 254u8, 197u8, 148u8, 227u8, 131u8, 244u8, 254u8, 193u8,
							212u8, 97u8,
						],
					)
				}
				#[doc = " Return the next/queued BEEFY authority set proof."]
				pub fn next_authority_set_proof(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::NextAuthoritySetProof,
					types::next_authority_set_proof::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"BeefyMmrApi",
						"next_authority_set_proof",
						types::NextAuthoritySetProof {},
						[
							66u8, 217u8, 48u8, 108u8, 211u8, 187u8, 61u8, 85u8, 210u8, 59u8, 128u8,
							159u8, 34u8, 151u8, 154u8, 140u8, 13u8, 244u8, 31u8, 216u8, 67u8, 67u8,
							171u8, 112u8, 51u8, 145u8, 4u8, 22u8, 252u8, 242u8, 192u8, 130u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod authority_set_proof {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_consensus_beefy::mmr::BeefyAuthoritySet<
							::subxt::ext::subxt_core::utils::H256,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct AuthoritySetProof {}
				pub mod next_authority_set_proof {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_consensus_beefy::mmr::BeefyAuthoritySet<
							::subxt::ext::subxt_core::utils::H256,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct NextAuthoritySetProof {}
			}
		}
		pub mod session_keys {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " Session keys runtime api."]
			pub struct SessionKeys;
			impl SessionKeys {
				#[doc = " Generate a set of session keys with optionally using the given seed."]
				#[doc = " The keys should be stored within the keystore exposed via runtime"]
				#[doc = " externalities."]
				#[doc = ""]
				#[doc = " The seed needs to be a valid `utf8` string."]
				#[doc = ""]
				#[doc = " Returns the concatenated SCALE encoded public keys."]
				pub fn generate_session_keys(
					&self,
					seed: types::generate_session_keys::Seed,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GenerateSessionKeys,
					types::generate_session_keys::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"SessionKeys",
						"generate_session_keys",
						types::GenerateSessionKeys { seed },
						[
							96u8, 171u8, 164u8, 166u8, 175u8, 102u8, 101u8, 47u8, 133u8, 95u8,
							102u8, 202u8, 83u8, 26u8, 238u8, 47u8, 126u8, 132u8, 22u8, 11u8, 33u8,
							190u8, 175u8, 94u8, 58u8, 245u8, 46u8, 80u8, 195u8, 184u8, 107u8, 65u8,
						],
					)
				}
				#[doc = " Decode the given public session keys."]
				#[doc = ""]
				#[doc = " Returns the list of public raw public keys + key type."]
				pub fn decode_session_keys(
					&self,
					encoded: types::decode_session_keys::Encoded,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::DecodeSessionKeys,
					types::decode_session_keys::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"SessionKeys",
						"decode_session_keys",
						types::DecodeSessionKeys { encoded },
						[
							57u8, 242u8, 18u8, 51u8, 132u8, 110u8, 238u8, 255u8, 39u8, 194u8, 8u8,
							54u8, 198u8, 178u8, 75u8, 151u8, 148u8, 176u8, 144u8, 197u8, 87u8,
							29u8, 179u8, 235u8, 176u8, 78u8, 252u8, 103u8, 72u8, 203u8, 151u8,
							248u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod generate_session_keys {
					use super::runtime_types;
					pub type Seed = ::core::option::Option<
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					>;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GenerateSessionKeys {
					pub seed: generate_session_keys::Seed,
				}
				pub mod decode_session_keys {
					use super::runtime_types;
					pub type Encoded =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<
							::subxt::ext::subxt_core::alloc::vec::Vec<(
								::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
								runtime_types::sp_core::crypto::KeyTypeId,
							)>,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct DecodeSessionKeys {
					pub encoded: decode_session_keys::Encoded,
				}
			}
		}
		pub mod grandpa_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " APIs for integrating the GRANDPA finality gadget into runtimes."]
			#[doc = " This should be implemented on the runtime side."]
			#[doc = ""]
			#[doc = " This is primarily used for negotiating authority-set changes for the"]
			#[doc = " gadget. GRANDPA uses a signaling model of changing authority sets:"]
			#[doc = " changes should be signaled with a delay of N blocks, and then automatically"]
			#[doc = " applied in the runtime after those N blocks have passed."]
			#[doc = ""]
			#[doc = " The consensus protocol will coordinate the handoff externally."]
			pub struct GrandpaApi;
			impl GrandpaApi {
				#[doc = " Get the current GRANDPA authorities and weights. This should not change except"]
				#[doc = " for when changes are scheduled and the corresponding delay has passed."]
				#[doc = ""]
				#[doc = " When called at block B, it will return the set of authorities that should be"]
				#[doc = " used to finalize descendants of this block (B+1, B+2, ...). The block B itself"]
				#[doc = " is finalized by the authorities from block B-1."]
				pub fn grandpa_authorities(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GrandpaAuthorities,
					types::grandpa_authorities::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GrandpaApi",
						"grandpa_authorities",
						types::GrandpaAuthorities {},
						[
							8u8, 1u8, 99u8, 227u8, 52u8, 95u8, 230u8, 139u8, 198u8, 90u8, 159u8,
							146u8, 193u8, 81u8, 37u8, 27u8, 216u8, 227u8, 108u8, 126u8, 12u8, 94u8,
							125u8, 183u8, 143u8, 231u8, 87u8, 101u8, 114u8, 190u8, 193u8, 180u8,
						],
					)
				}
				#[doc = " Submits an unsigned extrinsic to report an equivocation. The caller"]
				#[doc = " must provide the equivocation proof and a key ownership proof"]
				#[doc = " (should be obtained using `generate_key_ownership_proof`). The"]
				#[doc = " extrinsic will be unsigned and should only be accepted for local"]
				#[doc = " authorship (not to be broadcast to the network). This method returns"]
				#[doc = " `None` when creation of the extrinsic fails, e.g. if equivocation"]
				#[doc = " reporting is disabled for the given runtime (i.e. this method is"]
				#[doc = " hardcoded to return `None`). Only useful in an offchain context."]
				pub fn submit_report_equivocation_unsigned_extrinsic(
					&self,
					equivocation_proof : types :: submit_report_equivocation_unsigned_extrinsic :: EquivocationProof,
					key_owner_proof : types :: submit_report_equivocation_unsigned_extrinsic :: KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::SubmitReportEquivocationUnsignedExtrinsic,
					types::submit_report_equivocation_unsigned_extrinsic::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GrandpaApi",
						"submit_report_equivocation_unsigned_extrinsic",
						types::SubmitReportEquivocationUnsignedExtrinsic {
							equivocation_proof,
							key_owner_proof,
						},
						[
							27u8, 32u8, 16u8, 79u8, 172u8, 124u8, 44u8, 13u8, 176u8, 89u8, 69u8,
							60u8, 45u8, 176u8, 72u8, 151u8, 252u8, 5u8, 243u8, 82u8, 170u8, 51u8,
							179u8, 197u8, 117u8, 177u8, 110u8, 111u8, 97u8, 15u8, 109u8, 169u8,
						],
					)
				}
				#[doc = " Generates a proof of key ownership for the given authority in the"]
				#[doc = " given set. An example usage of this module is coupled with the"]
				#[doc = " session historical module to prove that a given authority key is"]
				#[doc = " tied to a given staking identity during a specific session. Proofs"]
				#[doc = " of key ownership are necessary for submitting equivocation reports."]
				#[doc = " NOTE: even though the API takes a `set_id` as parameter the current"]
				#[doc = " implementations ignore this parameter and instead rely on this"]
				#[doc = " method being called at the correct block height, i.e. any point at"]
				#[doc = " which the given set id is live on-chain. Future implementations will"]
				#[doc = " instead use indexed data through an offchain worker, not requiring"]
				#[doc = " older states to be available."]
				pub fn generate_key_ownership_proof(
					&self,
					set_id: types::generate_key_ownership_proof::SetId,
					authority_id: types::generate_key_ownership_proof::AuthorityId,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GenerateKeyOwnershipProof,
					types::generate_key_ownership_proof::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GrandpaApi",
						"generate_key_ownership_proof",
						types::GenerateKeyOwnershipProof { set_id, authority_id },
						[
							13u8, 144u8, 66u8, 235u8, 24u8, 190u8, 39u8, 75u8, 29u8, 157u8, 215u8,
							181u8, 173u8, 145u8, 224u8, 244u8, 189u8, 79u8, 6u8, 116u8, 139u8,
							196u8, 54u8, 16u8, 89u8, 190u8, 121u8, 43u8, 137u8, 150u8, 117u8, 68u8,
						],
					)
				}
				#[doc = " Get current GRANDPA authority set id."]
				pub fn current_set_id(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::CurrentSetId,
					types::current_set_id::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GrandpaApi",
						"current_set_id",
						types::CurrentSetId {},
						[
							42u8, 230u8, 120u8, 211u8, 156u8, 245u8, 109u8, 86u8, 100u8, 146u8,
							234u8, 205u8, 41u8, 183u8, 109u8, 42u8, 17u8, 33u8, 156u8, 25u8, 139u8,
							84u8, 101u8, 75u8, 232u8, 198u8, 87u8, 136u8, 218u8, 233u8, 103u8,
							156u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod grandpa_authorities {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::subxt::ext::subxt_core::alloc::vec::Vec<(
							runtime_types::sp_consensus_grandpa::app::Public,
							::core::primitive::u64,
						)>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GrandpaAuthorities {}
				pub mod submit_report_equivocation_unsigned_extrinsic {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_grandpa::EquivocationProof<
							::subxt::ext::subxt_core::utils::H256,
							::core::primitive::u32,
						>;
					pub type KeyOwnerProof = runtime_types::sp_runtime::OpaqueValue;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<()>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SubmitReportEquivocationUnsignedExtrinsic {
					pub equivocation_proof:
						submit_report_equivocation_unsigned_extrinsic::EquivocationProof,
					pub key_owner_proof:
						submit_report_equivocation_unsigned_extrinsic::KeyOwnerProof,
				}
				pub mod generate_key_ownership_proof {
					use super::runtime_types;
					pub type SetId = ::core::primitive::u64;
					pub type AuthorityId = runtime_types::sp_consensus_grandpa::app::Public;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::core::option::Option<runtime_types::sp_runtime::OpaqueValue>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GenerateKeyOwnershipProof {
					pub set_id: generate_key_ownership_proof::SetId,
					pub authority_id: generate_key_ownership_proof::AuthorityId,
				}
				pub mod current_set_id {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::primitive::u64;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct CurrentSetId {}
			}
		}
		pub mod account_nonce_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " The API to query account nonce."]
			pub struct AccountNonceApi;
			impl AccountNonceApi {
				#[doc = " Get current account nonce of given `AccountId`."]
				pub fn account_nonce(
					&self,
					account: types::account_nonce::Account,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::AccountNonce,
					types::account_nonce::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"AccountNonceApi",
						"account_nonce",
						types::AccountNonce { account },
						[
							231u8, 82u8, 7u8, 227u8, 131u8, 2u8, 215u8, 252u8, 173u8, 82u8, 11u8,
							103u8, 200u8, 25u8, 114u8, 116u8, 79u8, 229u8, 152u8, 150u8, 236u8,
							37u8, 101u8, 26u8, 220u8, 146u8, 182u8, 101u8, 73u8, 55u8, 191u8,
							171u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod account_nonce {
					use super::runtime_types;
					pub type Account = ::subxt::ext::subxt_core::utils::AccountId32;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::primitive::u32;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct AccountNonce {
					pub account: account_nonce::Account,
				}
			}
		}
		pub mod get_sidechain_status {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " Runtime API for getting information about current Partner Chain slot and epoch"]
			pub struct GetSidechainStatus;
			impl GetSidechainStatus {
				#[doc = " Returns current Partner Chain slot and epoch"]
				pub fn get_sidechain_status(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetSidechainStatus,
					types::get_sidechain_status::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GetSidechainStatus",
						"get_sidechain_status",
						types::GetSidechainStatus {},
						[
							88u8, 40u8, 13u8, 225u8, 14u8, 152u8, 49u8, 242u8, 158u8, 136u8, 176u8,
							47u8, 143u8, 112u8, 254u8, 187u8, 187u8, 181u8, 240u8, 18u8, 139u8,
							56u8, 147u8, 210u8, 41u8, 47u8, 236u8, 197u8, 60u8, 70u8, 146u8, 167u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod get_sidechain_status {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sp_sidechain::SidechainStatus;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetSidechainStatus {}
			}
		}
		pub mod get_genesis_utxo {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " Runtime API for retrieving the Partner Chain's genesis UTXO"]
			pub struct GetGenesisUtxo;
			impl GetGenesisUtxo {
				#[doc = " Returns the Partner Chain's genesis UTXO"]
				pub fn genesis_utxo(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GenesisUtxo,
					types::genesis_utxo::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GetGenesisUtxo",
						"genesis_utxo",
						types::GenesisUtxo {},
						[
							247u8, 8u8, 28u8, 181u8, 94u8, 69u8, 40u8, 110u8, 252u8, 89u8, 25u8,
							196u8, 189u8, 2u8, 54u8, 124u8, 153u8, 60u8, 9u8, 164u8, 2u8, 106u8,
							144u8, 89u8, 47u8, 160u8, 77u8, 237u8, 71u8, 155u8, 188u8, 152u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod genesis_utxo {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sidechain_domain::UtxoId;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GenesisUtxo {}
			}
		}
		pub mod slot_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " Runtime API serving slot configuration"]
			pub struct SlotApi;
			impl SlotApi {
				#[doc = " Returns the current slot configuration"]
				pub fn slot_config(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::SlotConfig,
					types::slot_config::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"SlotApi",
						"slot_config",
						types::SlotConfig {},
						[
							110u8, 28u8, 227u8, 158u8, 191u8, 127u8, 173u8, 20u8, 234u8, 20u8,
							65u8, 158u8, 203u8, 203u8, 172u8, 39u8, 184u8, 211u8, 95u8, 240u8,
							109u8, 86u8, 204u8, 38u8, 216u8, 129u8, 130u8, 8u8, 25u8, 247u8, 173u8,
							7u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod slot_config {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sidechain_slots::ScSlotConfig;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SlotConfig {}
			}
		}
		pub mod session_validator_management_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " Runtime API declaration for Session Validator Management"]
			pub struct SessionValidatorManagementApi;
			impl SessionValidatorManagementApi {
				#[doc = " Returns main chain scripts"]
				pub fn get_main_chain_scripts(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetMainChainScripts,
					types::get_main_chain_scripts::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"SessionValidatorManagementApi",
						"get_main_chain_scripts",
						types::GetMainChainScripts {},
						[
							127u8, 31u8, 86u8, 28u8, 12u8, 116u8, 51u8, 118u8, 250u8, 184u8, 111u8,
							173u8, 84u8, 125u8, 240u8, 184u8, 191u8, 95u8, 106u8, 249u8, 27u8,
							129u8, 146u8, 102u8, 103u8, 22u8, 88u8, 24u8, 31u8, 151u8, 97u8, 214u8,
						],
					)
				}
				#[doc = " Returns next unset [ScEpochNumber]"]
				pub fn get_next_unset_epoch_number(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetNextUnsetEpochNumber,
					types::get_next_unset_epoch_number::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"SessionValidatorManagementApi",
						"get_next_unset_epoch_number",
						types::GetNextUnsetEpochNumber {},
						[
							18u8, 52u8, 69u8, 147u8, 243u8, 90u8, 109u8, 159u8, 1u8, 154u8, 55u8,
							75u8, 186u8, 240u8, 83u8, 115u8, 221u8, 93u8, 165u8, 144u8, 163u8,
							150u8, 145u8, 252u8, 148u8, 125u8, 164u8, 206u8, 252u8, 140u8, 20u8,
							203u8,
						],
					)
				}
				#[doc = " Returns current committee"]
				pub fn get_current_committee(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetCurrentCommittee,
					types::get_current_committee::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"SessionValidatorManagementApi",
						"get_current_committee",
						types::GetCurrentCommittee {},
						[
							158u8, 192u8, 130u8, 95u8, 112u8, 37u8, 49u8, 155u8, 140u8, 44u8,
							178u8, 3u8, 80u8, 34u8, 250u8, 158u8, 53u8, 247u8, 55u8, 235u8, 8u8,
							66u8, 92u8, 225u8, 2u8, 120u8, 83u8, 193u8, 136u8, 153u8, 209u8, 0u8,
						],
					)
				}
				#[doc = " Returns next committee"]
				pub fn get_next_committee(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetNextCommittee,
					types::get_next_committee::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"SessionValidatorManagementApi",
						"get_next_committee",
						types::GetNextCommittee {},
						[
							249u8, 246u8, 19u8, 212u8, 83u8, 151u8, 114u8, 117u8, 36u8, 85u8,
							152u8, 23u8, 17u8, 234u8, 120u8, 57u8, 92u8, 182u8, 207u8, 205u8, 43u8,
							137u8, 200u8, 171u8, 230u8, 178u8, 231u8, 10u8, 228u8, 52u8, 80u8,
							46u8,
						],
					)
				}
				#[doc = " Calculates committee"]
				pub fn calculate_committee(
					&self,
					authority_selection_inputs : types :: calculate_committee :: AuthoritySelectionInputs,
					sidechain_epoch: types::calculate_committee::SidechainEpoch,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::CalculateCommittee,
					types::calculate_committee::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"SessionValidatorManagementApi",
						"calculate_committee",
						types::CalculateCommittee { authority_selection_inputs, sidechain_epoch },
						[
							25u8, 98u8, 204u8, 185u8, 216u8, 72u8, 28u8, 36u8, 72u8, 38u8, 246u8,
							42u8, 2u8, 133u8, 193u8, 220u8, 250u8, 205u8, 183u8, 232u8, 129u8, 8u8,
							112u8, 164u8, 249u8, 29u8, 160u8, 123u8, 66u8, 60u8, 19u8, 70u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod get_main_chain_scripts {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							runtime_types::sp_session_validator_management::MainChainScripts;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetMainChainScripts {}
				pub mod get_next_unset_epoch_number {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types::sidechain_domain::ScEpochNumber;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetNextUnsetEpochNumber {}
				pub mod get_current_committee {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = (runtime_types :: sidechain_domain :: ScEpochNumber , :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < runtime_types :: authority_selection_inherents :: CommitteeMember < runtime_types :: midnight_node_runtime :: opaque :: cross_chain_app :: Public , runtime_types :: midnight_node_runtime :: opaque :: SessionKeys > > ,) ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetCurrentCommittee {}
				pub mod get_next_committee {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: option :: Option < (runtime_types :: sidechain_domain :: ScEpochNumber , :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < runtime_types :: authority_selection_inherents :: CommitteeMember < runtime_types :: midnight_node_runtime :: opaque :: cross_chain_app :: Public , runtime_types :: midnight_node_runtime :: opaque :: SessionKeys > > ,) > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetNextCommittee {}
				pub mod calculate_committee {
					use super::runtime_types;
					pub type AuthoritySelectionInputs = runtime_types :: authority_selection_inherents :: authority_selection_inputs :: AuthoritySelectionInputs ;
					pub type SidechainEpoch = runtime_types::sidechain_domain::ScEpochNumber;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: option :: Option < :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < runtime_types :: authority_selection_inherents :: CommitteeMember < runtime_types :: midnight_node_runtime :: opaque :: cross_chain_app :: Public , runtime_types :: midnight_node_runtime :: opaque :: SessionKeys > > > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct CalculateCommittee {
					pub authority_selection_inputs: calculate_committee::AuthoritySelectionInputs,
					pub sidechain_epoch: calculate_committee::SidechainEpoch,
				}
			}
		}
		pub mod candidate_validation_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " Runtime API trait for candidate validation"]
			#[doc = ""]
			#[doc = " When implementing, make sure that the same validation is used here and in the committee selection logic!"]
			pub struct CandidateValidationApi;
			impl CandidateValidationApi {
				#[doc = " Should validate data provided by registered candidate,"]
				#[doc = " and return [RegistrationDataError] in case of validation failure."]
				#[doc = ""]
				#[doc = " Should validate:"]
				#[doc = " * Aura, GRANDPA, and Partner Chain public keys of the candidate"]
				#[doc = " * stake pool signature"]
				#[doc = " * sidechain signature"]
				#[doc = " * transaction inputs contain correct registration utxo"]
				pub fn validate_registered_candidate_data(
					&self,
					mainchain_pub_key: types::validate_registered_candidate_data::MainchainPubKey,
					registration_data: types::validate_registered_candidate_data::RegistrationData,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::ValidateRegisteredCandidateData,
					types::validate_registered_candidate_data::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"CandidateValidationApi",
						"validate_registered_candidate_data",
						types::ValidateRegisteredCandidateData {
							mainchain_pub_key,
							registration_data,
						},
						[
							13u8, 153u8, 183u8, 140u8, 125u8, 136u8, 216u8, 116u8, 69u8, 165u8,
							22u8, 50u8, 110u8, 55u8, 179u8, 245u8, 251u8, 199u8, 149u8, 88u8,
							103u8, 11u8, 186u8, 33u8, 30u8, 102u8, 102u8, 242u8, 150u8, 252u8,
							59u8, 156u8,
						],
					)
				}
				#[doc = " Should validate candidate stake and return [StakeError] in case of validation failure."]
				#[doc = " Should validate stake exists and is positive."]
				pub fn validate_stake(
					&self,
					stake: types::validate_stake::Stake,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::ValidateStake,
					types::validate_stake::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"CandidateValidationApi",
						"validate_stake",
						types::ValidateStake { stake },
						[
							92u8, 152u8, 106u8, 212u8, 13u8, 28u8, 240u8, 215u8, 243u8, 241u8,
							166u8, 32u8, 221u8, 55u8, 38u8, 4u8, 157u8, 36u8, 31u8, 145u8, 101u8,
							37u8, 142u8, 129u8, 160u8, 211u8, 22u8, 205u8, 144u8, 108u8, 186u8,
							66u8,
						],
					)
				}
				#[doc = " Should validate data provided by permissioned candidate,"]
				#[doc = " and return [PermissionedCandidateDataError] in case of validation failure."]
				#[doc = ""]
				#[doc = " Should validate:"]
				#[doc = " * Aura, GRANDPA, and Partner Chain public keys of the candidate"]
				pub fn validate_permissioned_candidate_data(
					&self,
					candidate: types::validate_permissioned_candidate_data::Candidate,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::ValidatePermissionedCandidateData,
					types::validate_permissioned_candidate_data::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"CandidateValidationApi",
						"validate_permissioned_candidate_data",
						types::ValidatePermissionedCandidateData { candidate },
						[
							28u8, 135u8, 45u8, 160u8, 0u8, 72u8, 46u8, 106u8, 138u8, 135u8, 53u8,
							155u8, 27u8, 228u8, 160u8, 51u8, 20u8, 226u8, 15u8, 18u8, 39u8, 94u8,
							7u8, 145u8, 170u8, 173u8, 131u8, 116u8, 54u8, 159u8, 65u8, 149u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod validate_registered_candidate_data {
					use super::runtime_types;
					pub type MainchainPubKey = runtime_types::sidechain_domain::StakePoolPublicKey;
					pub type RegistrationData = runtime_types::sidechain_domain::RegistrationData;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: option :: Option < runtime_types :: authority_selection_inherents :: filter_invalid_candidates :: RegistrationDataError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ValidateRegisteredCandidateData {
					pub mainchain_pub_key: validate_registered_candidate_data::MainchainPubKey,
					pub registration_data: validate_registered_candidate_data::RegistrationData,
				}
				pub mod validate_stake {
					use super::runtime_types;
					pub type Stake =
						::core::option::Option<runtime_types::sidechain_domain::StakeDelegation>;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: option :: Option < runtime_types :: authority_selection_inherents :: filter_invalid_candidates :: StakeError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ValidateStake {
					pub stake: validate_stake::Stake,
				}
				pub mod validate_permissioned_candidate_data {
					use super::runtime_types;
					pub type Candidate = runtime_types::sidechain_domain::PermissionedCandidateData;
					pub mod output {
						use super::runtime_types;
						pub type Output = :: core :: option :: Option < runtime_types :: authority_selection_inherents :: filter_invalid_candidates :: PermissionedCandidateDataError > ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ValidatePermissionedCandidateData {
					pub candidate: validate_permissioned_candidate_data::Candidate,
				}
			}
		}
		pub mod native_token_observation_api {
			use super::root_mod;
			use super::runtime_types;
			pub struct NativeTokenObservationApi;
			impl NativeTokenObservationApi {
				#[doc = " Get the contract address on Cardano which emits registration mappings in utxo datums"]
				pub fn get_mapping_validator_address(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetMappingValidatorAddress,
					types::get_mapping_validator_address::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"NativeTokenObservationApi",
						"get_mapping_validator_address",
						types::GetMappingValidatorAddress {},
						[
							179u8, 186u8, 180u8, 41u8, 246u8, 126u8, 37u8, 81u8, 82u8, 70u8, 238u8,
							155u8, 193u8, 144u8, 177u8, 207u8, 62u8, 163u8, 71u8, 200u8, 114u8,
							135u8, 62u8, 57u8, 220u8, 236u8, 157u8, 28u8, 50u8, 172u8, 188u8, 54u8,
						],
					)
				}
				#[doc = " Get the Cardano native token identifier for the chosen asset"]
				pub fn get_native_token_identifier(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetNativeTokenIdentifier,
					types::get_native_token_identifier::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"NativeTokenObservationApi",
						"get_native_token_identifier",
						types::GetNativeTokenIdentifier {},
						[
							83u8, 89u8, 223u8, 10u8, 118u8, 21u8, 37u8, 86u8, 224u8, 155u8, 6u8,
							198u8, 154u8, 136u8, 100u8, 16u8, 63u8, 208u8, 126u8, 84u8, 53u8,
							176u8, 186u8, 222u8, 129u8, 251u8, 83u8, 58u8, 226u8, 205u8, 89u8,
							112u8,
						],
					)
				}
				pub fn get_next_cardano_position(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetNextCardanoPosition,
					types::get_next_cardano_position::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"NativeTokenObservationApi",
						"get_next_cardano_position",
						types::GetNextCardanoPosition {},
						[
							192u8, 236u8, 85u8, 239u8, 165u8, 144u8, 33u8, 201u8, 39u8, 247u8,
							194u8, 166u8, 104u8, 243u8, 245u8, 109u8, 157u8, 127u8, 204u8, 148u8,
							12u8, 248u8, 239u8, 123u8, 109u8, 146u8, 157u8, 37u8, 184u8, 0u8, 60u8,
							101u8,
						],
					)
				}
				pub fn get_cardano_block_window_size(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetCardanoBlockWindowSize,
					types::get_cardano_block_window_size::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"NativeTokenObservationApi",
						"get_cardano_block_window_size",
						types::GetCardanoBlockWindowSize {},
						[
							138u8, 54u8, 195u8, 40u8, 30u8, 179u8, 21u8, 161u8, 131u8, 171u8,
							239u8, 209u8, 172u8, 203u8, 199u8, 62u8, 132u8, 86u8, 127u8, 38u8,
							15u8, 241u8, 136u8, 190u8, 111u8, 164u8, 233u8, 254u8, 233u8, 125u8,
							197u8, 34u8,
						],
					)
				}
				pub fn get_utxo_capacity_per_block(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetUtxoCapacityPerBlock,
					types::get_utxo_capacity_per_block::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"NativeTokenObservationApi",
						"get_utxo_capacity_per_block",
						types::GetUtxoCapacityPerBlock {},
						[
							22u8, 59u8, 83u8, 1u8, 102u8, 242u8, 104u8, 219u8, 100u8, 163u8, 201u8,
							197u8, 132u8, 122u8, 166u8, 31u8, 169u8, 245u8, 219u8, 47u8, 35u8,
							203u8, 253u8, 48u8, 113u8, 71u8, 173u8, 154u8, 236u8, 57u8, 230u8,
							189u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod get_mapping_validator_address {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output =
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetMappingValidatorAddress {}
				pub mod get_native_token_identifier {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = (
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						);
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetNativeTokenIdentifier {}
				pub mod get_next_cardano_position {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = runtime_types :: midnight_primitives_native_token_observation :: CardanoPosition ;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetNextCardanoPosition {}
				pub mod get_cardano_block_window_size {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::primitive::u32;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetCardanoBlockWindowSize {}
				pub mod get_utxo_capacity_per_block {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::primitive::u32;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetUtxoCapacityPerBlock {}
			}
		}
		pub mod governed_map_idp_api {
			use super::root_mod;
			use super::runtime_types;
			#[doc = " Runtime API exposing data required for the [GovernedMapInherentDataProvider] to operate."]
			pub struct GovernedMapIDPApi;
			impl GovernedMapIDPApi {
				#[doc = " Returns initialization state of the pallet"]
				pub fn is_initialized(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::IsInitialized,
					types::is_initialized::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GovernedMapIDPApi",
						"is_initialized",
						types::IsInitialized {},
						[
							44u8, 142u8, 63u8, 31u8, 98u8, 144u8, 175u8, 218u8, 155u8, 223u8,
							121u8, 127u8, 189u8, 95u8, 92u8, 76u8, 4u8, 155u8, 47u8, 194u8, 112u8,
							1u8, 103u8, 103u8, 191u8, 246u8, 165u8, 62u8, 184u8, 135u8, 23u8,
							203u8,
						],
					)
				}
				#[doc = " Returns all mappings currently stored in the pallet"]
				pub fn get_current_state(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetCurrentState,
					types::get_current_state::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GovernedMapIDPApi",
						"get_current_state",
						types::GetCurrentState {},
						[
							184u8, 68u8, 78u8, 228u8, 199u8, 97u8, 83u8, 38u8, 31u8, 102u8, 108u8,
							185u8, 248u8, 93u8, 48u8, 22u8, 25u8, 221u8, 163u8, 216u8, 242u8,
							109u8, 116u8, 229u8, 188u8, 90u8, 181u8, 123u8, 184u8, 133u8, 152u8,
							126u8,
						],
					)
				}
				#[doc = " Returns the main chain scripts currently set in the pallet or [None] otherwise"]
				pub fn get_main_chain_scripts(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetMainChainScripts,
					types::get_main_chain_scripts::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GovernedMapIDPApi",
						"get_main_chain_scripts",
						types::GetMainChainScripts {},
						[
							42u8, 73u8, 16u8, 79u8, 230u8, 88u8, 77u8, 138u8, 104u8, 136u8, 102u8,
							120u8, 178u8, 30u8, 141u8, 22u8, 225u8, 221u8, 244u8, 17u8, 23u8,
							201u8, 73u8, 23u8, 177u8, 28u8, 102u8, 87u8, 46u8, 174u8, 43u8, 99u8,
						],
					)
				}
				#[doc = " Returns the current version of the pallet, 1-based."]
				pub fn get_pallet_version(
					&self,
				) -> ::subxt::ext::subxt_core::runtime_api::payload::StaticPayload<
					types::GetPalletVersion,
					types::get_pallet_version::output::Output,
				> {
					::subxt::ext::subxt_core::runtime_api::payload::StaticPayload::new_static(
						"GovernedMapIDPApi",
						"get_pallet_version",
						types::GetPalletVersion {},
						[
							77u8, 171u8, 171u8, 178u8, 128u8, 58u8, 55u8, 234u8, 168u8, 88u8, 78u8,
							186u8, 143u8, 211u8, 110u8, 162u8, 186u8, 152u8, 137u8, 170u8, 228u8,
							51u8, 172u8, 241u8, 137u8, 80u8, 92u8, 228u8, 218u8, 64u8, 139u8, 78u8,
						],
					)
				}
			}
			pub mod types {
				use super::runtime_types;
				pub mod is_initialized {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::primitive::bool;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct IsInitialized {}
				pub mod get_current_state {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::subxt::ext::subxt_core::utils::KeyedVec<
							::subxt::ext::subxt_core::alloc::string::String,
							runtime_types::sidechain_domain::byte_string::ByteString,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetCurrentState {}
				pub mod get_main_chain_scripts {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::option::Option<
							runtime_types::sp_governed_map::MainChainScriptsV1,
						>;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetMainChainScripts {}
				pub mod get_pallet_version {
					use super::runtime_types;
					pub mod output {
						use super::runtime_types;
						pub type Output = ::core::primitive::u32;
					}
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct GetPalletVersion {}
			}
		}
	}
	pub fn custom() -> CustomValuesApi {
		CustomValuesApi
	}
	pub struct CustomValuesApi;
	impl CustomValuesApi {}
	pub struct ConstantsApi;
	impl ConstantsApi {
		pub fn system(&self) -> system::constants::ConstantsApi {
			system::constants::ConstantsApi
		}
		pub fn timestamp(&self) -> timestamp::constants::ConstantsApi {
			timestamp::constants::ConstantsApi
		}
		pub fn aura(&self) -> aura::constants::ConstantsApi {
			aura::constants::ConstantsApi
		}
		pub fn grandpa(&self) -> grandpa::constants::ConstantsApi {
			grandpa::constants::ConstantsApi
		}
		pub fn balances(&self) -> balances::constants::ConstantsApi {
			balances::constants::ConstantsApi
		}
		pub fn session_committee_management(
			&self,
		) -> session_committee_management::constants::ConstantsApi {
			session_committee_management::constants::ConstantsApi
		}
		pub fn runtime_upgrade(&self) -> runtime_upgrade::constants::ConstantsApi {
			runtime_upgrade::constants::ConstantsApi
		}
		pub fn native_token_observation(
			&self,
		) -> native_token_observation::constants::ConstantsApi {
			native_token_observation::constants::ConstantsApi
		}
		pub fn multi_block_migrations(&self) -> multi_block_migrations::constants::ConstantsApi {
			multi_block_migrations::constants::ConstantsApi
		}
		pub fn scheduler(&self) -> scheduler::constants::ConstantsApi {
			scheduler::constants::ConstantsApi
		}
		pub fn tx_pause(&self) -> tx_pause::constants::ConstantsApi {
			tx_pause::constants::ConstantsApi
		}
		pub fn beefy(&self) -> beefy::constants::ConstantsApi {
			beefy::constants::ConstantsApi
		}
		pub fn governed_map(&self) -> governed_map::constants::ConstantsApi {
			governed_map::constants::ConstantsApi
		}
	}
	pub struct StorageApi;
	impl StorageApi {
		pub fn system(&self) -> system::storage::StorageApi {
			system::storage::StorageApi
		}
		pub fn timestamp(&self) -> timestamp::storage::StorageApi {
			timestamp::storage::StorageApi
		}
		pub fn aura(&self) -> aura::storage::StorageApi {
			aura::storage::StorageApi
		}
		pub fn grandpa(&self) -> grandpa::storage::StorageApi {
			grandpa::storage::StorageApi
		}
		pub fn sidechain(&self) -> sidechain::storage::StorageApi {
			sidechain::storage::StorageApi
		}
		pub fn midnight(&self) -> midnight::storage::StorageApi {
			midnight::storage::StorageApi
		}
		pub fn balances(&self) -> balances::storage::StorageApi {
			balances::storage::StorageApi
		}
		pub fn sudo(&self) -> sudo::storage::StorageApi {
			sudo::storage::StorageApi
		}
		pub fn session_committee_management(
			&self,
		) -> session_committee_management::storage::StorageApi {
			session_committee_management::storage::StorageApi
		}
		pub fn runtime_upgrade(&self) -> runtime_upgrade::storage::StorageApi {
			runtime_upgrade::storage::StorageApi
		}
		pub fn native_token_management(&self) -> native_token_management::storage::StorageApi {
			native_token_management::storage::StorageApi
		}
		pub fn native_token_observation(&self) -> native_token_observation::storage::StorageApi {
			native_token_observation::storage::StorageApi
		}
		pub fn preimage(&self) -> preimage::storage::StorageApi {
			preimage::storage::StorageApi
		}
		pub fn multi_block_migrations(&self) -> multi_block_migrations::storage::StorageApi {
			multi_block_migrations::storage::StorageApi
		}
		pub fn pallet_session(&self) -> pallet_session::storage::StorageApi {
			pallet_session::storage::StorageApi
		}
		pub fn scheduler(&self) -> scheduler::storage::StorageApi {
			scheduler::storage::StorageApi
		}
		pub fn tx_pause(&self) -> tx_pause::storage::StorageApi {
			tx_pause::storage::StorageApi
		}
		pub fn beefy(&self) -> beefy::storage::StorageApi {
			beefy::storage::StorageApi
		}
		pub fn mmr(&self) -> mmr::storage::StorageApi {
			mmr::storage::StorageApi
		}
		pub fn beefy_mmr_leaf(&self) -> beefy_mmr_leaf::storage::StorageApi {
			beefy_mmr_leaf::storage::StorageApi
		}
		pub fn session(&self) -> session::storage::StorageApi {
			session::storage::StorageApi
		}
		pub fn governed_map(&self) -> governed_map::storage::StorageApi {
			governed_map::storage::StorageApi
		}
	}
	pub struct TransactionApi;
	impl TransactionApi {
		pub fn system(&self) -> system::calls::TransactionApi {
			system::calls::TransactionApi
		}
		pub fn timestamp(&self) -> timestamp::calls::TransactionApi {
			timestamp::calls::TransactionApi
		}
		pub fn grandpa(&self) -> grandpa::calls::TransactionApi {
			grandpa::calls::TransactionApi
		}
		pub fn midnight(&self) -> midnight::calls::TransactionApi {
			midnight::calls::TransactionApi
		}
		pub fn balances(&self) -> balances::calls::TransactionApi {
			balances::calls::TransactionApi
		}
		pub fn sudo(&self) -> sudo::calls::TransactionApi {
			sudo::calls::TransactionApi
		}
		pub fn session_committee_management(
			&self,
		) -> session_committee_management::calls::TransactionApi {
			session_committee_management::calls::TransactionApi
		}
		pub fn runtime_upgrade(&self) -> runtime_upgrade::calls::TransactionApi {
			runtime_upgrade::calls::TransactionApi
		}
		pub fn native_token_management(&self) -> native_token_management::calls::TransactionApi {
			native_token_management::calls::TransactionApi
		}
		pub fn native_token_observation(&self) -> native_token_observation::calls::TransactionApi {
			native_token_observation::calls::TransactionApi
		}
		pub fn preimage(&self) -> preimage::calls::TransactionApi {
			preimage::calls::TransactionApi
		}
		pub fn multi_block_migrations(&self) -> multi_block_migrations::calls::TransactionApi {
			multi_block_migrations::calls::TransactionApi
		}
		pub fn pallet_session(&self) -> pallet_session::calls::TransactionApi {
			pallet_session::calls::TransactionApi
		}
		pub fn scheduler(&self) -> scheduler::calls::TransactionApi {
			scheduler::calls::TransactionApi
		}
		pub fn tx_pause(&self) -> tx_pause::calls::TransactionApi {
			tx_pause::calls::TransactionApi
		}
		pub fn beefy(&self) -> beefy::calls::TransactionApi {
			beefy::calls::TransactionApi
		}
		pub fn governed_map(&self) -> governed_map::calls::TransactionApi {
			governed_map::calls::TransactionApi
		}
	}
	#[doc = r" check whether the metadata provided is aligned with this statically generated code."]
	pub fn is_codegen_valid_for(metadata: &::subxt::ext::subxt_core::Metadata) -> bool {
		let runtime_metadata_hash = metadata
			.hasher()
			.only_these_pallets(&PALLETS)
			.only_these_runtime_apis(&RUNTIME_APIS)
			.hash();
		runtime_metadata_hash
			== [
				213u8, 43u8, 217u8, 237u8, 57u8, 149u8, 133u8, 93u8, 214u8, 74u8, 13u8, 186u8,
				147u8, 191u8, 214u8, 43u8, 61u8, 129u8, 87u8, 207u8, 189u8, 144u8, 245u8, 170u8,
				72u8, 47u8, 196u8, 55u8, 126u8, 98u8, 227u8, 8u8,
			]
	}
	pub mod system {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "Error for the System pallet"]
		pub type Error = runtime_types::frame_system::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::frame_system::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Make some on-chain remark."]
				#[doc = ""]
				#[doc = "Can be executed by every `origin`."]
				pub struct Remark {
					pub remark: remark::Remark,
				}
				pub mod remark {
					use super::runtime_types;
					pub type Remark =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Remark {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "remark";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Set the number of pages in the WebAssembly environment's heap."]
				pub struct SetHeapPages {
					pub pages: set_heap_pages::Pages,
				}
				pub mod set_heap_pages {
					use super::runtime_types;
					pub type Pages = ::core::primitive::u64;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetHeapPages {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "set_heap_pages";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Set the new runtime code."]
				pub struct SetCode {
					pub code: set_code::Code,
				}
				pub mod set_code {
					use super::runtime_types;
					pub type Code =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetCode {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "set_code";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Set the new runtime code without doing any checks of the given `code`."]
				#[doc = ""]
				#[doc = "Note that runtime upgrades will not run if this is called with a not-increasing spec"]
				#[doc = "version!"]
				pub struct SetCodeWithoutChecks {
					pub code: set_code_without_checks::Code,
				}
				pub mod set_code_without_checks {
					use super::runtime_types;
					pub type Code =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetCodeWithoutChecks {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "set_code_without_checks";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Set some items of storage."]
				pub struct SetStorage {
					pub items: set_storage::Items,
				}
				pub mod set_storage {
					use super::runtime_types;
					pub type Items = ::subxt::ext::subxt_core::alloc::vec::Vec<(
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					)>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetStorage {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "set_storage";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Kill some items from storage."]
				pub struct KillStorage {
					pub keys: kill_storage::Keys,
				}
				pub mod kill_storage {
					use super::runtime_types;
					pub type Keys = ::subxt::ext::subxt_core::alloc::vec::Vec<
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for KillStorage {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "kill_storage";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Kill all storage items with a key that starts with the given prefix."]
				#[doc = ""]
				#[doc = "**NOTE:** We rely on the Root origin to provide us the number of subkeys under"]
				#[doc = "the prefix we are removing to accurately calculate the weight of this function."]
				pub struct KillPrefix {
					pub prefix: kill_prefix::Prefix,
					pub subkeys: kill_prefix::Subkeys,
				}
				pub mod kill_prefix {
					use super::runtime_types;
					pub type Prefix =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub type Subkeys = ::core::primitive::u32;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for KillPrefix {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "kill_prefix";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Make some on-chain remark and emit event."]
				pub struct RemarkWithEvent {
					pub remark: remark_with_event::Remark,
				}
				pub mod remark_with_event {
					use super::runtime_types;
					pub type Remark =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for RemarkWithEvent {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "remark_with_event";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied"]
				#[doc = "later."]
				#[doc = ""]
				#[doc = "This call requires Root origin."]
				pub struct AuthorizeUpgrade {
					pub code_hash: authorize_upgrade::CodeHash,
				}
				pub mod authorize_upgrade {
					use super::runtime_types;
					pub type CodeHash = ::subxt::ext::subxt_core::utils::H256;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for AuthorizeUpgrade {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "authorize_upgrade";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied"]
				#[doc = "later."]
				#[doc = ""]
				#[doc = "WARNING: This authorizes an upgrade that will take place without any safety checks, for"]
				#[doc = "example that the spec name remains the same and that the version number increases. Not"]
				#[doc = "recommended for normal use. Use `authorize_upgrade` instead."]
				#[doc = ""]
				#[doc = "This call requires Root origin."]
				pub struct AuthorizeUpgradeWithoutChecks {
					pub code_hash: authorize_upgrade_without_checks::CodeHash,
				}
				pub mod authorize_upgrade_without_checks {
					use super::runtime_types;
					pub type CodeHash = ::subxt::ext::subxt_core::utils::H256;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for AuthorizeUpgradeWithoutChecks {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "authorize_upgrade_without_checks";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Provide the preimage (runtime binary) `code` for an upgrade that has been authorized."]
				#[doc = ""]
				#[doc = "If the authorization required a version check, this call will ensure the spec name"]
				#[doc = "remains unchanged and that the spec version has increased."]
				#[doc = ""]
				#[doc = "Depending on the runtime's `OnSetCode` configuration, this function may directly apply"]
				#[doc = "the new `code` in the same block or attempt to schedule the upgrade."]
				#[doc = ""]
				#[doc = "All origins are allowed."]
				pub struct ApplyAuthorizedUpgrade {
					pub code: apply_authorized_upgrade::Code,
				}
				pub mod apply_authorized_upgrade {
					use super::runtime_types;
					pub type Code =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ApplyAuthorizedUpgrade {
					const PALLET: &'static str = "System";
					const CALL: &'static str = "apply_authorized_upgrade";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Make some on-chain remark."]
				#[doc = ""]
				#[doc = "Can be executed by every `origin`."]
				pub fn remark(
					&self,
					remark: types::remark::Remark,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Remark> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"remark",
						types::Remark { remark },
						[
							43u8, 126u8, 180u8, 174u8, 141u8, 48u8, 52u8, 125u8, 166u8, 212u8,
							216u8, 98u8, 100u8, 24u8, 132u8, 71u8, 101u8, 64u8, 246u8, 169u8, 33u8,
							250u8, 147u8, 208u8, 2u8, 40u8, 129u8, 209u8, 232u8, 207u8, 207u8,
							13u8,
						],
					)
				}
				#[doc = "Set the number of pages in the WebAssembly environment's heap."]
				pub fn set_heap_pages(
					&self,
					pages: types::set_heap_pages::Pages,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetHeapPages> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"set_heap_pages",
						types::SetHeapPages { pages },
						[
							188u8, 191u8, 99u8, 216u8, 219u8, 109u8, 141u8, 50u8, 78u8, 235u8,
							215u8, 242u8, 195u8, 24u8, 111u8, 76u8, 229u8, 64u8, 99u8, 225u8,
							134u8, 121u8, 81u8, 209u8, 127u8, 223u8, 98u8, 215u8, 150u8, 70u8,
							57u8, 147u8,
						],
					)
				}
				#[doc = "Set the new runtime code."]
				pub fn set_code(
					&self,
					code: types::set_code::Code,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetCode> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"set_code",
						types::SetCode { code },
						[
							233u8, 248u8, 88u8, 245u8, 28u8, 65u8, 25u8, 169u8, 35u8, 237u8, 19u8,
							203u8, 136u8, 160u8, 18u8, 3u8, 20u8, 197u8, 81u8, 169u8, 244u8, 188u8,
							27u8, 147u8, 147u8, 236u8, 65u8, 25u8, 3u8, 143u8, 182u8, 22u8,
						],
					)
				}
				#[doc = "Set the new runtime code without doing any checks of the given `code`."]
				#[doc = ""]
				#[doc = "Note that runtime upgrades will not run if this is called with a not-increasing spec"]
				#[doc = "version!"]
				pub fn set_code_without_checks(
					&self,
					code: types::set_code_without_checks::Code,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetCodeWithoutChecks>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"set_code_without_checks",
						types::SetCodeWithoutChecks { code },
						[
							82u8, 212u8, 157u8, 44u8, 70u8, 0u8, 143u8, 15u8, 109u8, 109u8, 107u8,
							157u8, 141u8, 42u8, 169u8, 11u8, 15u8, 186u8, 252u8, 138u8, 10u8,
							147u8, 15u8, 178u8, 247u8, 229u8, 213u8, 98u8, 207u8, 231u8, 119u8,
							115u8,
						],
					)
				}
				#[doc = "Set some items of storage."]
				pub fn set_storage(
					&self,
					items: types::set_storage::Items,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetStorage> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"set_storage",
						types::SetStorage { items },
						[
							141u8, 216u8, 52u8, 222u8, 223u8, 136u8, 123u8, 181u8, 19u8, 75u8,
							163u8, 102u8, 229u8, 189u8, 158u8, 142u8, 95u8, 235u8, 240u8, 49u8,
							150u8, 76u8, 78u8, 137u8, 126u8, 88u8, 183u8, 88u8, 231u8, 146u8,
							234u8, 43u8,
						],
					)
				}
				#[doc = "Kill some items from storage."]
				pub fn kill_storage(
					&self,
					keys: types::kill_storage::Keys,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::KillStorage> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"kill_storage",
						types::KillStorage { keys },
						[
							73u8, 63u8, 196u8, 36u8, 144u8, 114u8, 34u8, 213u8, 108u8, 93u8, 209u8,
							234u8, 153u8, 185u8, 33u8, 91u8, 187u8, 195u8, 223u8, 130u8, 58u8,
							156u8, 63u8, 47u8, 228u8, 249u8, 216u8, 139u8, 143u8, 177u8, 41u8,
							35u8,
						],
					)
				}
				#[doc = "Kill all storage items with a key that starts with the given prefix."]
				#[doc = ""]
				#[doc = "**NOTE:** We rely on the Root origin to provide us the number of subkeys under"]
				#[doc = "the prefix we are removing to accurately calculate the weight of this function."]
				pub fn kill_prefix(
					&self,
					prefix: types::kill_prefix::Prefix,
					subkeys: types::kill_prefix::Subkeys,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::KillPrefix> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"kill_prefix",
						types::KillPrefix { prefix, subkeys },
						[
							184u8, 57u8, 139u8, 24u8, 208u8, 87u8, 108u8, 215u8, 198u8, 189u8,
							175u8, 242u8, 167u8, 215u8, 97u8, 63u8, 110u8, 166u8, 238u8, 98u8,
							67u8, 236u8, 111u8, 110u8, 234u8, 81u8, 102u8, 5u8, 182u8, 5u8, 214u8,
							85u8,
						],
					)
				}
				#[doc = "Make some on-chain remark and emit event."]
				pub fn remark_with_event(
					&self,
					remark: types::remark_with_event::Remark,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::RemarkWithEvent>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"remark_with_event",
						types::RemarkWithEvent { remark },
						[
							120u8, 120u8, 153u8, 92u8, 184u8, 85u8, 34u8, 2u8, 174u8, 206u8, 105u8,
							228u8, 233u8, 130u8, 80u8, 246u8, 228u8, 59u8, 234u8, 240u8, 4u8, 49u8,
							147u8, 170u8, 115u8, 91u8, 149u8, 200u8, 228u8, 181u8, 8u8, 154u8,
						],
					)
				}
				#[doc = "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied"]
				#[doc = "later."]
				#[doc = ""]
				#[doc = "This call requires Root origin."]
				pub fn authorize_upgrade(
					&self,
					code_hash: types::authorize_upgrade::CodeHash,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::AuthorizeUpgrade>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"authorize_upgrade",
						types::AuthorizeUpgrade { code_hash },
						[
							4u8, 14u8, 76u8, 107u8, 209u8, 129u8, 9u8, 39u8, 193u8, 17u8, 84u8,
							254u8, 170u8, 214u8, 24u8, 155u8, 29u8, 184u8, 249u8, 241u8, 109u8,
							58u8, 145u8, 131u8, 109u8, 63u8, 38u8, 165u8, 107u8, 215u8, 217u8,
							172u8,
						],
					)
				}
				#[doc = "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied"]
				#[doc = "later."]
				#[doc = ""]
				#[doc = "WARNING: This authorizes an upgrade that will take place without any safety checks, for"]
				#[doc = "example that the spec name remains the same and that the version number increases. Not"]
				#[doc = "recommended for normal use. Use `authorize_upgrade` instead."]
				#[doc = ""]
				#[doc = "This call requires Root origin."]
				pub fn authorize_upgrade_without_checks(
					&self,
					code_hash: types::authorize_upgrade_without_checks::CodeHash,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::AuthorizeUpgradeWithoutChecks,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"authorize_upgrade_without_checks",
						types::AuthorizeUpgradeWithoutChecks { code_hash },
						[
							126u8, 126u8, 55u8, 26u8, 47u8, 55u8, 66u8, 8u8, 167u8, 18u8, 29u8,
							136u8, 146u8, 14u8, 189u8, 117u8, 16u8, 227u8, 162u8, 61u8, 149u8,
							197u8, 104u8, 184u8, 185u8, 161u8, 99u8, 154u8, 80u8, 125u8, 181u8,
							233u8,
						],
					)
				}
				#[doc = "Provide the preimage (runtime binary) `code` for an upgrade that has been authorized."]
				#[doc = ""]
				#[doc = "If the authorization required a version check, this call will ensure the spec name"]
				#[doc = "remains unchanged and that the spec version has increased."]
				#[doc = ""]
				#[doc = "Depending on the runtime's `OnSetCode` configuration, this function may directly apply"]
				#[doc = "the new `code` in the same block or attempt to schedule the upgrade."]
				#[doc = ""]
				#[doc = "All origins are allowed."]
				pub fn apply_authorized_upgrade(
					&self,
					code: types::apply_authorized_upgrade::Code,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::ApplyAuthorizedUpgrade,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"System",
						"apply_authorized_upgrade",
						types::ApplyAuthorizedUpgrade { code },
						[
							232u8, 107u8, 127u8, 38u8, 230u8, 29u8, 97u8, 4u8, 160u8, 191u8, 222u8,
							156u8, 245u8, 102u8, 196u8, 141u8, 44u8, 163u8, 98u8, 68u8, 125u8,
							32u8, 124u8, 101u8, 108u8, 93u8, 211u8, 52u8, 0u8, 231u8, 33u8, 227u8,
						],
					)
				}
			}
		}
		#[doc = "Event for the System pallet."]
		pub type Event = runtime_types::frame_system::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An extrinsic completed successfully."]
			pub struct ExtrinsicSuccess {
				pub dispatch_info: extrinsic_success::DispatchInfo,
			}
			pub mod extrinsic_success {
				use super::runtime_types;
				pub type DispatchInfo = runtime_types::frame_system::DispatchEventInfo;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ExtrinsicSuccess {
				const PALLET: &'static str = "System";
				const EVENT: &'static str = "ExtrinsicSuccess";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An extrinsic failed."]
			pub struct ExtrinsicFailed {
				pub dispatch_error: extrinsic_failed::DispatchError,
				pub dispatch_info: extrinsic_failed::DispatchInfo,
			}
			pub mod extrinsic_failed {
				use super::runtime_types;
				pub type DispatchError = runtime_types::sp_runtime::DispatchError;
				pub type DispatchInfo = runtime_types::frame_system::DispatchEventInfo;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ExtrinsicFailed {
				const PALLET: &'static str = "System";
				const EVENT: &'static str = "ExtrinsicFailed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "`:code` was updated."]
			pub struct CodeUpdated;
			impl ::subxt::ext::subxt_core::events::StaticEvent for CodeUpdated {
				const PALLET: &'static str = "System";
				const EVENT: &'static str = "CodeUpdated";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A new account was created."]
			pub struct NewAccount {
				pub account: new_account::Account,
			}
			pub mod new_account {
				use super::runtime_types;
				pub type Account = ::subxt::ext::subxt_core::utils::AccountId32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for NewAccount {
				const PALLET: &'static str = "System";
				const EVENT: &'static str = "NewAccount";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An account was reaped."]
			pub struct KilledAccount {
				pub account: killed_account::Account,
			}
			pub mod killed_account {
				use super::runtime_types;
				pub type Account = ::subxt::ext::subxt_core::utils::AccountId32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for KilledAccount {
				const PALLET: &'static str = "System";
				const EVENT: &'static str = "KilledAccount";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "On on-chain remark happened."]
			pub struct Remarked {
				pub sender: remarked::Sender,
				pub hash: remarked::Hash,
			}
			pub mod remarked {
				use super::runtime_types;
				pub type Sender = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Hash = ::subxt::ext::subxt_core::utils::H256;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Remarked {
				const PALLET: &'static str = "System";
				const EVENT: &'static str = "Remarked";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An upgrade was authorized."]
			pub struct UpgradeAuthorized {
				pub code_hash: upgrade_authorized::CodeHash,
				pub check_version: upgrade_authorized::CheckVersion,
			}
			pub mod upgrade_authorized {
				use super::runtime_types;
				pub type CodeHash = ::subxt::ext::subxt_core::utils::H256;
				pub type CheckVersion = ::core::primitive::bool;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for UpgradeAuthorized {
				const PALLET: &'static str = "System";
				const EVENT: &'static str = "UpgradeAuthorized";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An invalid authorized upgrade was rejected while trying to apply it."]
			pub struct RejectedInvalidAuthorizedUpgrade {
				pub code_hash: rejected_invalid_authorized_upgrade::CodeHash,
				pub error: rejected_invalid_authorized_upgrade::Error,
			}
			pub mod rejected_invalid_authorized_upgrade {
				use super::runtime_types;
				pub type CodeHash = ::subxt::ext::subxt_core::utils::H256;
				pub type Error = runtime_types::sp_runtime::DispatchError;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for RejectedInvalidAuthorizedUpgrade {
				const PALLET: &'static str = "System";
				const EVENT: &'static str = "RejectedInvalidAuthorizedUpgrade";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod account {
					use super::runtime_types;
					pub type Account = runtime_types::frame_system::AccountInfo<
						::core::primitive::u32,
						runtime_types::pallet_balances::types::AccountData<::core::primitive::u128>,
					>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::AccountId32;
				}
				pub mod extrinsic_count {
					use super::runtime_types;
					pub type ExtrinsicCount = ::core::primitive::u32;
				}
				pub mod inherents_applied {
					use super::runtime_types;
					pub type InherentsApplied = ::core::primitive::bool;
				}
				pub mod block_weight {
					use super::runtime_types;
					pub type BlockWeight = runtime_types::frame_support::dispatch::PerDispatchClass<
						runtime_types::sp_weights::weight_v2::Weight,
					>;
				}
				pub mod all_extrinsics_len {
					use super::runtime_types;
					pub type AllExtrinsicsLen = ::core::primitive::u32;
				}
				pub mod block_hash {
					use super::runtime_types;
					pub type BlockHash = ::subxt::ext::subxt_core::utils::H256;
					pub type Param0 = ::core::primitive::u32;
				}
				pub mod extrinsic_data {
					use super::runtime_types;
					pub type ExtrinsicData =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
					pub type Param0 = ::core::primitive::u32;
				}
				pub mod number {
					use super::runtime_types;
					pub type Number = ::core::primitive::u32;
				}
				pub mod parent_hash {
					use super::runtime_types;
					pub type ParentHash = ::subxt::ext::subxt_core::utils::H256;
				}
				pub mod digest {
					use super::runtime_types;
					pub type Digest = runtime_types::sp_runtime::generic::digest::Digest;
				}
				pub mod events {
					use super::runtime_types;
					pub type Events = ::subxt::ext::subxt_core::alloc::vec::Vec<
						runtime_types::frame_system::EventRecord<
							runtime_types::midnight_node_runtime::RuntimeEvent,
							::subxt::ext::subxt_core::utils::H256,
						>,
					>;
				}
				pub mod event_count {
					use super::runtime_types;
					pub type EventCount = ::core::primitive::u32;
				}
				pub mod event_topics {
					use super::runtime_types;
					pub type EventTopics = ::subxt::ext::subxt_core::alloc::vec::Vec<(
						::core::primitive::u32,
						::core::primitive::u32,
					)>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::H256;
				}
				pub mod last_runtime_upgrade {
					use super::runtime_types;
					pub type LastRuntimeUpgrade =
						runtime_types::frame_system::LastRuntimeUpgradeInfo;
				}
				pub mod upgraded_to_u32_ref_count {
					use super::runtime_types;
					pub type UpgradedToU32RefCount = ::core::primitive::bool;
				}
				pub mod upgraded_to_triple_ref_count {
					use super::runtime_types;
					pub type UpgradedToTripleRefCount = ::core::primitive::bool;
				}
				pub mod execution_phase {
					use super::runtime_types;
					pub type ExecutionPhase = runtime_types::frame_system::Phase;
				}
				pub mod authorized_upgrade {
					use super::runtime_types;
					pub type AuthorizedUpgrade =
						runtime_types::frame_system::CodeUpgradeAuthorization;
				}
				pub mod extrinsic_weight_reclaimed {
					use super::runtime_types;
					pub type ExtrinsicWeightReclaimed =
						runtime_types::sp_weights::weight_v2::Weight;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The full account information for a particular account ID."]
				pub fn account_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::account::Account,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"Account",
						(),
						[
							14u8, 233u8, 115u8, 214u8, 0u8, 109u8, 222u8, 121u8, 162u8, 65u8, 60u8,
							175u8, 209u8, 79u8, 222u8, 124u8, 22u8, 235u8, 138u8, 176u8, 133u8,
							124u8, 90u8, 158u8, 85u8, 45u8, 37u8, 174u8, 47u8, 79u8, 47u8, 166u8,
						],
					)
				}
				#[doc = " The full account information for a particular account ID."]
				pub fn account(
					&self,
					_0: impl ::core::borrow::Borrow<types::account::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::account::Param0,
					>,
					types::account::Account,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"Account",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							14u8, 233u8, 115u8, 214u8, 0u8, 109u8, 222u8, 121u8, 162u8, 65u8, 60u8,
							175u8, 209u8, 79u8, 222u8, 124u8, 22u8, 235u8, 138u8, 176u8, 133u8,
							124u8, 90u8, 158u8, 85u8, 45u8, 37u8, 174u8, 47u8, 79u8, 47u8, 166u8,
						],
					)
				}
				#[doc = " Total extrinsics count for the current block."]
				pub fn extrinsic_count(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::extrinsic_count::ExtrinsicCount,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"ExtrinsicCount",
						(),
						[
							102u8, 76u8, 236u8, 42u8, 40u8, 231u8, 33u8, 222u8, 123u8, 147u8,
							153u8, 148u8, 234u8, 203u8, 181u8, 119u8, 6u8, 187u8, 177u8, 199u8,
							120u8, 47u8, 137u8, 254u8, 96u8, 100u8, 165u8, 182u8, 249u8, 230u8,
							159u8, 79u8,
						],
					)
				}
				#[doc = " Whether all inherents have been applied."]
				pub fn inherents_applied(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::inherents_applied::InherentsApplied,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"InherentsApplied",
						(),
						[
							132u8, 249u8, 142u8, 252u8, 8u8, 103u8, 80u8, 120u8, 50u8, 6u8, 188u8,
							223u8, 101u8, 55u8, 165u8, 189u8, 172u8, 249u8, 165u8, 230u8, 183u8,
							109u8, 34u8, 65u8, 185u8, 150u8, 29u8, 8u8, 186u8, 129u8, 135u8, 239u8,
						],
					)
				}
				#[doc = " The current weight for the block."]
				pub fn block_weight(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::block_weight::BlockWeight,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"BlockWeight",
						(),
						[
							158u8, 46u8, 228u8, 89u8, 210u8, 214u8, 84u8, 154u8, 50u8, 68u8, 63u8,
							62u8, 43u8, 42u8, 99u8, 27u8, 54u8, 42u8, 146u8, 44u8, 241u8, 216u8,
							229u8, 30u8, 216u8, 255u8, 165u8, 238u8, 181u8, 130u8, 36u8, 102u8,
						],
					)
				}
				#[doc = " Total length (in bytes) for all extrinsics put together, for the current block."]
				pub fn all_extrinsics_len(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::all_extrinsics_len::AllExtrinsicsLen,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"AllExtrinsicsLen",
						(),
						[
							117u8, 86u8, 61u8, 243u8, 41u8, 51u8, 102u8, 214u8, 137u8, 100u8,
							243u8, 185u8, 122u8, 174u8, 187u8, 117u8, 86u8, 189u8, 63u8, 135u8,
							101u8, 218u8, 203u8, 201u8, 237u8, 254u8, 128u8, 183u8, 169u8, 221u8,
							242u8, 65u8,
						],
					)
				}
				#[doc = " Map of block numbers to block hashes."]
				pub fn block_hash_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::block_hash::BlockHash,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"BlockHash",
						(),
						[
							217u8, 32u8, 215u8, 253u8, 24u8, 182u8, 207u8, 178u8, 157u8, 24u8,
							103u8, 100u8, 195u8, 165u8, 69u8, 152u8, 112u8, 181u8, 56u8, 192u8,
							164u8, 16u8, 20u8, 222u8, 28u8, 214u8, 144u8, 142u8, 146u8, 69u8,
							202u8, 118u8,
						],
					)
				}
				#[doc = " Map of block numbers to block hashes."]
				pub fn block_hash(
					&self,
					_0: impl ::core::borrow::Borrow<types::block_hash::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::block_hash::Param0,
					>,
					types::block_hash::BlockHash,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"BlockHash",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							217u8, 32u8, 215u8, 253u8, 24u8, 182u8, 207u8, 178u8, 157u8, 24u8,
							103u8, 100u8, 195u8, 165u8, 69u8, 152u8, 112u8, 181u8, 56u8, 192u8,
							164u8, 16u8, 20u8, 222u8, 28u8, 214u8, 144u8, 142u8, 146u8, 69u8,
							202u8, 118u8,
						],
					)
				}
				#[doc = " Extrinsics data for the current block (maps an extrinsic's index to its data)."]
				pub fn extrinsic_data_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::extrinsic_data::ExtrinsicData,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"ExtrinsicData",
						(),
						[
							160u8, 180u8, 122u8, 18u8, 196u8, 26u8, 2u8, 37u8, 115u8, 232u8, 133u8,
							220u8, 106u8, 245u8, 4u8, 129u8, 42u8, 84u8, 241u8, 45u8, 199u8, 179u8,
							128u8, 61u8, 170u8, 137u8, 231u8, 156u8, 247u8, 57u8, 47u8, 38u8,
						],
					)
				}
				#[doc = " Extrinsics data for the current block (maps an extrinsic's index to its data)."]
				pub fn extrinsic_data(
					&self,
					_0: impl ::core::borrow::Borrow<types::extrinsic_data::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::extrinsic_data::Param0,
					>,
					types::extrinsic_data::ExtrinsicData,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"ExtrinsicData",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							160u8, 180u8, 122u8, 18u8, 196u8, 26u8, 2u8, 37u8, 115u8, 232u8, 133u8,
							220u8, 106u8, 245u8, 4u8, 129u8, 42u8, 84u8, 241u8, 45u8, 199u8, 179u8,
							128u8, 61u8, 170u8, 137u8, 231u8, 156u8, 247u8, 57u8, 47u8, 38u8,
						],
					)
				}
				#[doc = " The current block number being processed. Set by `execute_block`."]
				pub fn number(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::number::Number,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"Number",
						(),
						[
							30u8, 194u8, 177u8, 90u8, 194u8, 232u8, 46u8, 180u8, 85u8, 129u8, 14u8,
							9u8, 8u8, 8u8, 23u8, 95u8, 230u8, 5u8, 13u8, 105u8, 125u8, 2u8, 22u8,
							200u8, 78u8, 93u8, 115u8, 28u8, 150u8, 113u8, 48u8, 53u8,
						],
					)
				}
				#[doc = " Hash of the previous block."]
				pub fn parent_hash(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::parent_hash::ParentHash,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"ParentHash",
						(),
						[
							26u8, 130u8, 11u8, 216u8, 155u8, 71u8, 128u8, 170u8, 30u8, 153u8, 21u8,
							192u8, 62u8, 93u8, 137u8, 80u8, 120u8, 81u8, 202u8, 94u8, 248u8, 125u8,
							71u8, 82u8, 141u8, 229u8, 32u8, 56u8, 73u8, 50u8, 101u8, 78u8,
						],
					)
				}
				#[doc = " Digest of the current block, also part of the block header."]
				pub fn digest(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::digest::Digest,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"Digest",
						(),
						[
							61u8, 64u8, 237u8, 91u8, 145u8, 232u8, 17u8, 254u8, 181u8, 16u8, 234u8,
							91u8, 51u8, 140u8, 254u8, 131u8, 98u8, 135u8, 21u8, 37u8, 251u8, 20u8,
							58u8, 92u8, 123u8, 141u8, 14u8, 227u8, 146u8, 46u8, 222u8, 117u8,
						],
					)
				}
				#[doc = " Events deposited for the current block."]
				#[doc = ""]
				#[doc = " NOTE: The item is unbound and should therefore never be read on chain."]
				#[doc = " It could otherwise inflate the PoV size of a block."]
				#[doc = ""]
				#[doc = " Events have a large in-memory size. Box the events to not go out-of-memory"]
				#[doc = " just in case someone still reads them from within the runtime."]
				pub fn events(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::events::Events,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"Events",
						(),
						[
							171u8, 161u8, 143u8, 18u8, 42u8, 179u8, 192u8, 12u8, 204u8, 226u8,
							149u8, 156u8, 41u8, 71u8, 30u8, 10u8, 222u8, 53u8, 12u8, 51u8, 197u8,
							86u8, 50u8, 83u8, 155u8, 45u8, 34u8, 7u8, 182u8, 175u8, 39u8, 17u8,
						],
					)
				}
				#[doc = " The number of events in the `Events<T>` list."]
				pub fn event_count(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::event_count::EventCount,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"EventCount",
						(),
						[
							175u8, 24u8, 252u8, 184u8, 210u8, 167u8, 146u8, 143u8, 164u8, 80u8,
							151u8, 205u8, 189u8, 189u8, 55u8, 220u8, 47u8, 101u8, 181u8, 33u8,
							254u8, 131u8, 13u8, 143u8, 3u8, 244u8, 245u8, 45u8, 2u8, 210u8, 79u8,
							133u8,
						],
					)
				}
				#[doc = " Mapping between a topic (represented by T::Hash) and a vector of indexes"]
				#[doc = " of events in the `<Events<T>>` list."]
				#[doc = ""]
				#[doc = " All topic vectors have deterministic storage locations depending on the topic. This"]
				#[doc = " allows light-clients to leverage the changes trie storage tracking mechanism and"]
				#[doc = " in case of changes fetch the list of events of interest."]
				#[doc = ""]
				#[doc = " The value has the type `(BlockNumberFor<T>, EventIndex)` because if we used only just"]
				#[doc = " the `EventIndex` then in case if the topic has the same contents on the next block"]
				#[doc = " no notification will be triggered thus the event might be lost."]
				pub fn event_topics_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::event_topics::EventTopics,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"EventTopics",
						(),
						[
							40u8, 225u8, 14u8, 75u8, 44u8, 176u8, 76u8, 34u8, 143u8, 107u8, 69u8,
							133u8, 114u8, 13u8, 172u8, 250u8, 141u8, 73u8, 12u8, 65u8, 217u8, 63u8,
							120u8, 241u8, 48u8, 106u8, 143u8, 161u8, 128u8, 100u8, 166u8, 59u8,
						],
					)
				}
				#[doc = " Mapping between a topic (represented by T::Hash) and a vector of indexes"]
				#[doc = " of events in the `<Events<T>>` list."]
				#[doc = ""]
				#[doc = " All topic vectors have deterministic storage locations depending on the topic. This"]
				#[doc = " allows light-clients to leverage the changes trie storage tracking mechanism and"]
				#[doc = " in case of changes fetch the list of events of interest."]
				#[doc = ""]
				#[doc = " The value has the type `(BlockNumberFor<T>, EventIndex)` because if we used only just"]
				#[doc = " the `EventIndex` then in case if the topic has the same contents on the next block"]
				#[doc = " no notification will be triggered thus the event might be lost."]
				pub fn event_topics(
					&self,
					_0: impl ::core::borrow::Borrow<types::event_topics::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::event_topics::Param0,
					>,
					types::event_topics::EventTopics,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"EventTopics",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							40u8, 225u8, 14u8, 75u8, 44u8, 176u8, 76u8, 34u8, 143u8, 107u8, 69u8,
							133u8, 114u8, 13u8, 172u8, 250u8, 141u8, 73u8, 12u8, 65u8, 217u8, 63u8,
							120u8, 241u8, 48u8, 106u8, 143u8, 161u8, 128u8, 100u8, 166u8, 59u8,
						],
					)
				}
				#[doc = " Stores the `spec_version` and `spec_name` of when the last runtime upgrade happened."]
				pub fn last_runtime_upgrade(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::last_runtime_upgrade::LastRuntimeUpgrade,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"LastRuntimeUpgrade",
						(),
						[
							197u8, 212u8, 249u8, 209u8, 79u8, 34u8, 55u8, 203u8, 31u8, 42u8, 199u8,
							242u8, 188u8, 74u8, 234u8, 250u8, 245u8, 44u8, 139u8, 162u8, 45u8,
							150u8, 230u8, 249u8, 135u8, 100u8, 158u8, 167u8, 118u8, 219u8, 28u8,
							98u8,
						],
					)
				}
				#[doc = " True if we have upgraded so that `type RefCount` is `u32`. False (default) if not."]
				pub fn upgraded_to_u32_ref_count(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::upgraded_to_u32_ref_count::UpgradedToU32RefCount,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"UpgradedToU32RefCount",
						(),
						[
							229u8, 73u8, 9u8, 132u8, 186u8, 116u8, 151u8, 171u8, 145u8, 29u8, 34u8,
							130u8, 52u8, 146u8, 124u8, 175u8, 79u8, 189u8, 147u8, 230u8, 234u8,
							107u8, 124u8, 31u8, 2u8, 22u8, 86u8, 190u8, 4u8, 147u8, 50u8, 245u8,
						],
					)
				}
				#[doc = " True if we have upgraded so that AccountInfo contains three types of `RefCount`. False"]
				#[doc = " (default) if not."]
				pub fn upgraded_to_triple_ref_count(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::upgraded_to_triple_ref_count::UpgradedToTripleRefCount,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"UpgradedToTripleRefCount",
						(),
						[
							97u8, 66u8, 124u8, 243u8, 27u8, 167u8, 147u8, 81u8, 254u8, 201u8,
							101u8, 24u8, 40u8, 231u8, 14u8, 179u8, 154u8, 163u8, 71u8, 81u8, 185u8,
							167u8, 82u8, 254u8, 189u8, 3u8, 101u8, 207u8, 206u8, 194u8, 155u8,
							151u8,
						],
					)
				}
				#[doc = " The execution phase of the block."]
				pub fn execution_phase(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::execution_phase::ExecutionPhase,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"ExecutionPhase",
						(),
						[
							191u8, 129u8, 100u8, 134u8, 126u8, 116u8, 154u8, 203u8, 220u8, 200u8,
							0u8, 26u8, 161u8, 250u8, 133u8, 205u8, 146u8, 24u8, 5u8, 156u8, 158u8,
							35u8, 36u8, 253u8, 52u8, 235u8, 86u8, 167u8, 35u8, 100u8, 119u8, 27u8,
						],
					)
				}
				#[doc = " `Some` if a code upgrade has been authorized."]
				pub fn authorized_upgrade(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::authorized_upgrade::AuthorizedUpgrade,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"AuthorizedUpgrade",
						(),
						[
							165u8, 97u8, 27u8, 138u8, 2u8, 28u8, 55u8, 92u8, 96u8, 96u8, 168u8,
							169u8, 55u8, 178u8, 44u8, 127u8, 58u8, 140u8, 206u8, 178u8, 1u8, 37u8,
							214u8, 213u8, 251u8, 123u8, 5u8, 111u8, 90u8, 148u8, 217u8, 135u8,
						],
					)
				}
				#[doc = " The weight reclaimed for the extrinsic."]
				#[doc = ""]
				#[doc = " This information is available until the end of the extrinsic execution."]
				#[doc = " More precisely this information is removed in `note_applied_extrinsic`."]
				#[doc = ""]
				#[doc = " Logic doing some post dispatch weight reduction must update this storage to avoid duplicate"]
				#[doc = " reduction."]
				pub fn extrinsic_weight_reclaimed(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::extrinsic_weight_reclaimed::ExtrinsicWeightReclaimed,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"System",
						"ExtrinsicWeightReclaimed",
						(),
						[
							195u8, 143u8, 164u8, 84u8, 225u8, 194u8, 227u8, 128u8, 196u8, 241u8,
							188u8, 159u8, 59u8, 197u8, 11u8, 12u8, 119u8, 164u8, 46u8, 229u8, 92u8,
							212u8, 236u8, 255u8, 238u8, 54u8, 105u8, 200u8, 229u8, 191u8, 221u8,
							202u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " Block & extrinsics weights: base values and limits."]
				pub fn block_weights(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					runtime_types::frame_system::limits::BlockWeights,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"System",
						"BlockWeights",
						[
							176u8, 124u8, 225u8, 136u8, 25u8, 73u8, 247u8, 33u8, 82u8, 206u8, 85u8,
							190u8, 127u8, 102u8, 71u8, 11u8, 185u8, 8u8, 58u8, 0u8, 94u8, 55u8,
							163u8, 177u8, 104u8, 59u8, 60u8, 136u8, 246u8, 116u8, 0u8, 239u8,
						],
					)
				}
				#[doc = " The maximum length of a block (in bytes)."]
				pub fn block_length(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					runtime_types::frame_system::limits::BlockLength,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"System",
						"BlockLength",
						[
							23u8, 242u8, 225u8, 39u8, 225u8, 67u8, 152u8, 41u8, 155u8, 104u8, 68u8,
							229u8, 185u8, 133u8, 10u8, 143u8, 184u8, 152u8, 234u8, 44u8, 140u8,
							96u8, 166u8, 235u8, 162u8, 160u8, 72u8, 7u8, 35u8, 194u8, 3u8, 37u8,
						],
					)
				}
				#[doc = " Maximum number of block number to block hash mappings to keep (oldest pruned first)."]
				pub fn block_hash_count(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"System",
						"BlockHashCount",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " The weight of runtime database operations the runtime can invoke."]
				pub fn db_weight(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					runtime_types::sp_weights::RuntimeDbWeight,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"System",
						"DbWeight",
						[
							42u8, 43u8, 178u8, 142u8, 243u8, 203u8, 60u8, 173u8, 118u8, 111u8,
							200u8, 170u8, 102u8, 70u8, 237u8, 187u8, 198u8, 120u8, 153u8, 232u8,
							183u8, 76u8, 74u8, 10u8, 70u8, 243u8, 14u8, 218u8, 213u8, 126u8, 29u8,
							177u8,
						],
					)
				}
				#[doc = " Get the chain's in-code version."]
				pub fn version(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					runtime_types::sp_version::RuntimeVersion,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"System",
						"Version",
						[
							214u8, 43u8, 96u8, 193u8, 96u8, 213u8, 63u8, 124u8, 22u8, 111u8, 41u8,
							78u8, 146u8, 77u8, 34u8, 163u8, 117u8, 100u8, 6u8, 216u8, 238u8, 54u8,
							80u8, 185u8, 219u8, 11u8, 192u8, 200u8, 129u8, 88u8, 161u8, 250u8,
						],
					)
				}
				#[doc = " The designated SS58 prefix of this chain."]
				#[doc = ""]
				#[doc = " This replaces the \"ss58Format\" property declared in the chain spec. Reason is"]
				#[doc = " that the runtime should know about the prefix in order to make use of it as"]
				#[doc = " an identifier of the chain."]
				pub fn ss58_prefix(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u16,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"System",
						"SS58Prefix",
						[
							116u8, 33u8, 2u8, 170u8, 181u8, 147u8, 171u8, 169u8, 167u8, 227u8,
							41u8, 144u8, 11u8, 236u8, 82u8, 100u8, 74u8, 60u8, 184u8, 72u8, 169u8,
							90u8, 208u8, 135u8, 15u8, 117u8, 10u8, 123u8, 128u8, 193u8, 29u8, 70u8,
						],
					)
				}
			}
		}
	}
	pub mod timestamp {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_timestamp::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Set the current time."]
				#[doc = ""]
				#[doc = "This call should be invoked exactly once per block. It will panic at the finalization"]
				#[doc = "phase, if this call hasn't been invoked by that time."]
				#[doc = ""]
				#[doc = "The timestamp should be greater than the previous one by the amount specified by"]
				#[doc = "[`Config::MinimumPeriod`]."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be _None_."]
				#[doc = ""]
				#[doc = "This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware"]
				#[doc = "that changing the complexity of this call could result exhausting the resources in a"]
				#[doc = "block to execute any other calls."]
				#[doc = ""]
				#[doc = "## Complexity"]
				#[doc = "- `O(1)` (Note that implementations of `OnTimestampSet` must also be `O(1)`)"]
				#[doc = "- 1 storage read and 1 storage mutation (codec `O(1)` because of `DidUpdate::take` in"]
				#[doc = "  `on_finalize`)"]
				#[doc = "- 1 event handler `on_timestamp_set`. Must be `O(1)`."]
				pub struct Set {
					#[codec(compact)]
					pub now: set::Now,
				}
				pub mod set {
					use super::runtime_types;
					pub type Now = ::core::primitive::u64;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Set {
					const PALLET: &'static str = "Timestamp";
					const CALL: &'static str = "set";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Set the current time."]
				#[doc = ""]
				#[doc = "This call should be invoked exactly once per block. It will panic at the finalization"]
				#[doc = "phase, if this call hasn't been invoked by that time."]
				#[doc = ""]
				#[doc = "The timestamp should be greater than the previous one by the amount specified by"]
				#[doc = "[`Config::MinimumPeriod`]."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be _None_."]
				#[doc = ""]
				#[doc = "This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware"]
				#[doc = "that changing the complexity of this call could result exhausting the resources in a"]
				#[doc = "block to execute any other calls."]
				#[doc = ""]
				#[doc = "## Complexity"]
				#[doc = "- `O(1)` (Note that implementations of `OnTimestampSet` must also be `O(1)`)"]
				#[doc = "- 1 storage read and 1 storage mutation (codec `O(1)` because of `DidUpdate::take` in"]
				#[doc = "  `on_finalize`)"]
				#[doc = "- 1 event handler `on_timestamp_set`. Must be `O(1)`."]
				pub fn set(
					&self,
					now: types::set::Now,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Set> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Timestamp",
						"set",
						types::Set { now },
						[
							37u8, 95u8, 49u8, 218u8, 24u8, 22u8, 0u8, 95u8, 72u8, 35u8, 155u8,
							199u8, 213u8, 54u8, 207u8, 22u8, 185u8, 193u8, 221u8, 70u8, 18u8,
							200u8, 4u8, 231u8, 195u8, 173u8, 6u8, 122u8, 11u8, 203u8, 231u8, 227u8,
						],
					)
				}
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod now {
					use super::runtime_types;
					pub type Now = ::core::primitive::u64;
				}
				pub mod did_update {
					use super::runtime_types;
					pub type DidUpdate = ::core::primitive::bool;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The current time for the current block."]
				pub fn now(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::now::Now,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Timestamp",
						"Now",
						(),
						[
							44u8, 50u8, 80u8, 30u8, 195u8, 146u8, 123u8, 238u8, 8u8, 163u8, 187u8,
							92u8, 61u8, 39u8, 51u8, 29u8, 173u8, 169u8, 217u8, 158u8, 85u8, 187u8,
							141u8, 26u8, 12u8, 115u8, 51u8, 11u8, 200u8, 244u8, 138u8, 152u8,
						],
					)
				}
				#[doc = " Whether the timestamp has been updated in this block."]
				#[doc = ""]
				#[doc = " This value is updated to `true` upon successful submission of a timestamp by a node."]
				#[doc = " It is then checked at the end of each block execution in the `on_finalize` hook."]
				pub fn did_update(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::did_update::DidUpdate,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Timestamp",
						"DidUpdate",
						(),
						[
							229u8, 175u8, 246u8, 102u8, 237u8, 158u8, 212u8, 229u8, 238u8, 214u8,
							205u8, 160u8, 164u8, 252u8, 195u8, 75u8, 139u8, 110u8, 22u8, 34u8,
							248u8, 204u8, 107u8, 46u8, 20u8, 200u8, 238u8, 167u8, 71u8, 41u8,
							214u8, 140u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " The minimum period between blocks."]
				#[doc = ""]
				#[doc = " Be aware that this is different to the *expected* period that the block production"]
				#[doc = " apparatus provides. Your chosen consensus system will generally work with this to"]
				#[doc = " determine a sensible block time. For example, in the Aura pallet it will be double this"]
				#[doc = " period on default settings."]
				pub fn minimum_period(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u64,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Timestamp",
						"MinimumPeriod",
						[
							128u8, 214u8, 205u8, 242u8, 181u8, 142u8, 124u8, 231u8, 190u8, 146u8,
							59u8, 226u8, 157u8, 101u8, 103u8, 117u8, 249u8, 65u8, 18u8, 191u8,
							103u8, 119u8, 53u8, 85u8, 81u8, 96u8, 220u8, 42u8, 184u8, 239u8, 42u8,
							246u8,
						],
					)
				}
			}
		}
	}
	pub mod aura {
		use super::root_mod;
		use super::runtime_types;
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod authorities {
					use super::runtime_types;
					pub type Authorities =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							runtime_types::sp_consensus_aura::sr25519::app_sr25519::Public,
						>;
				}
				pub mod current_slot {
					use super::runtime_types;
					pub type CurrentSlot = runtime_types::sp_consensus_slots::Slot;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The current authority set."]
				pub fn authorities(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::authorities::Authorities,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Aura",
						"Authorities",
						(),
						[
							95u8, 52u8, 203u8, 53u8, 254u8, 107u8, 134u8, 122u8, 95u8, 253u8, 51u8,
							137u8, 142u8, 106u8, 237u8, 248u8, 159u8, 80u8, 41u8, 233u8, 137u8,
							133u8, 13u8, 217u8, 176u8, 88u8, 132u8, 199u8, 241u8, 47u8, 125u8,
							27u8,
						],
					)
				}
				#[doc = " The current slot of this block."]
				#[doc = ""]
				#[doc = " This will be set in `on_initialize`."]
				pub fn current_slot(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::current_slot::CurrentSlot,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Aura",
						"CurrentSlot",
						(),
						[
							112u8, 199u8, 115u8, 248u8, 217u8, 242u8, 45u8, 231u8, 178u8, 53u8,
							236u8, 167u8, 219u8, 238u8, 81u8, 243u8, 39u8, 140u8, 68u8, 19u8,
							201u8, 169u8, 211u8, 133u8, 135u8, 213u8, 150u8, 105u8, 60u8, 252u8,
							43u8, 57u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " The slot duration Aura should run with, expressed in milliseconds."]
				#[doc = " The effective value of this type should not change while the chain is running."]
				#[doc = ""]
				#[doc = " For backwards compatibility either use [`MinimumPeriodTimesTwo`] or a const."]
				pub fn slot_duration(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u64,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Aura",
						"SlotDuration",
						[
							128u8, 214u8, 205u8, 242u8, 181u8, 142u8, 124u8, 231u8, 190u8, 146u8,
							59u8, 226u8, 157u8, 101u8, 103u8, 117u8, 249u8, 65u8, 18u8, 191u8,
							103u8, 119u8, 53u8, 85u8, 81u8, 96u8, 220u8, 42u8, 184u8, 239u8, 42u8,
							246u8,
						],
					)
				}
			}
		}
	}
	pub mod grandpa {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_grandpa::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_grandpa::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
				#[doc = "equivocation proof and validate the given key ownership proof"]
				#[doc = "against the extracted offender. If both are valid, the offence"]
				#[doc = "will be reported."]
				pub struct ReportEquivocation {
					pub equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
						report_equivocation::EquivocationProof,
					>,
					pub key_owner_proof: report_equivocation::KeyOwnerProof,
				}
				pub mod report_equivocation {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_grandpa::EquivocationProof<
							::subxt::ext::subxt_core::utils::H256,
							::core::primitive::u32,
						>;
					pub type KeyOwnerProof = runtime_types::sp_core::Void;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ReportEquivocation {
					const PALLET: &'static str = "Grandpa";
					const CALL: &'static str = "report_equivocation";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
				#[doc = "equivocation proof and validate the given key ownership proof"]
				#[doc = "against the extracted offender. If both are valid, the offence"]
				#[doc = "will be reported."]
				#[doc = ""]
				#[doc = "This extrinsic must be called unsigned and it is expected that only"]
				#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
				#[doc = "if the block author is defined it will be defined as the equivocation"]
				#[doc = "reporter."]
				pub struct ReportEquivocationUnsigned {
					pub equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
						report_equivocation_unsigned::EquivocationProof,
					>,
					pub key_owner_proof: report_equivocation_unsigned::KeyOwnerProof,
				}
				pub mod report_equivocation_unsigned {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_grandpa::EquivocationProof<
							::subxt::ext::subxt_core::utils::H256,
							::core::primitive::u32,
						>;
					pub type KeyOwnerProof = runtime_types::sp_core::Void;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ReportEquivocationUnsigned {
					const PALLET: &'static str = "Grandpa";
					const CALL: &'static str = "report_equivocation_unsigned";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Note that the current authority set of the GRANDPA finality gadget has stalled."]
				#[doc = ""]
				#[doc = "This will trigger a forced authority set change at the beginning of the next session, to"]
				#[doc = "be enacted `delay` blocks after that. The `delay` should be high enough to safely assume"]
				#[doc = "that the block signalling the forced change will not be re-orged e.g. 1000 blocks."]
				#[doc = "The block production rate (which may be slowed down because of finality lagging) should"]
				#[doc = "be taken into account when choosing the `delay`. The GRANDPA voters based on the new"]
				#[doc = "authority will start voting on top of `best_finalized_block_number` for new finalized"]
				#[doc = "blocks. `best_finalized_block_number` should be the highest of the latest finalized"]
				#[doc = "block of all validators of the new authority set."]
				#[doc = ""]
				#[doc = "Only callable by root."]
				pub struct NoteStalled {
					pub delay: note_stalled::Delay,
					pub best_finalized_block_number: note_stalled::BestFinalizedBlockNumber,
				}
				pub mod note_stalled {
					use super::runtime_types;
					pub type Delay = ::core::primitive::u32;
					pub type BestFinalizedBlockNumber = ::core::primitive::u32;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for NoteStalled {
					const PALLET: &'static str = "Grandpa";
					const CALL: &'static str = "note_stalled";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
				#[doc = "equivocation proof and validate the given key ownership proof"]
				#[doc = "against the extracted offender. If both are valid, the offence"]
				#[doc = "will be reported."]
				pub fn report_equivocation(
					&self,
					equivocation_proof: types::report_equivocation::EquivocationProof,
					key_owner_proof: types::report_equivocation::KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ReportEquivocation>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Grandpa",
						"report_equivocation",
						types::ReportEquivocation {
							equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box::new(
								equivocation_proof,
							),
							key_owner_proof,
						},
						[
							187u8, 224u8, 115u8, 5u8, 236u8, 32u8, 180u8, 155u8, 218u8, 109u8,
							238u8, 253u8, 30u8, 225u8, 4u8, 225u8, 132u8, 232u8, 243u8, 54u8, 56u8,
							158u8, 94u8, 192u8, 94u8, 206u8, 189u8, 61u8, 14u8, 49u8, 48u8, 131u8,
						],
					)
				}
				#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
				#[doc = "equivocation proof and validate the given key ownership proof"]
				#[doc = "against the extracted offender. If both are valid, the offence"]
				#[doc = "will be reported."]
				#[doc = ""]
				#[doc = "This extrinsic must be called unsigned and it is expected that only"]
				#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
				#[doc = "if the block author is defined it will be defined as the equivocation"]
				#[doc = "reporter."]
				pub fn report_equivocation_unsigned(
					&self,
					equivocation_proof: types::report_equivocation_unsigned::EquivocationProof,
					key_owner_proof: types::report_equivocation_unsigned::KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::ReportEquivocationUnsigned,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Grandpa",
						"report_equivocation_unsigned",
						types::ReportEquivocationUnsigned {
							equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box::new(
								equivocation_proof,
							),
							key_owner_proof,
						},
						[
							98u8, 103u8, 6u8, 54u8, 0u8, 200u8, 166u8, 163u8, 202u8, 45u8, 131u8,
							226u8, 114u8, 166u8, 237u8, 174u8, 207u8, 214u8, 2u8, 227u8, 32u8,
							166u8, 47u8, 83u8, 166u8, 239u8, 232u8, 72u8, 224u8, 242u8, 156u8,
							44u8,
						],
					)
				}
				#[doc = "Note that the current authority set of the GRANDPA finality gadget has stalled."]
				#[doc = ""]
				#[doc = "This will trigger a forced authority set change at the beginning of the next session, to"]
				#[doc = "be enacted `delay` blocks after that. The `delay` should be high enough to safely assume"]
				#[doc = "that the block signalling the forced change will not be re-orged e.g. 1000 blocks."]
				#[doc = "The block production rate (which may be slowed down because of finality lagging) should"]
				#[doc = "be taken into account when choosing the `delay`. The GRANDPA voters based on the new"]
				#[doc = "authority will start voting on top of `best_finalized_block_number` for new finalized"]
				#[doc = "blocks. `best_finalized_block_number` should be the highest of the latest finalized"]
				#[doc = "block of all validators of the new authority set."]
				#[doc = ""]
				#[doc = "Only callable by root."]
				pub fn note_stalled(
					&self,
					delay: types::note_stalled::Delay,
					best_finalized_block_number: types::note_stalled::BestFinalizedBlockNumber,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::NoteStalled> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Grandpa",
						"note_stalled",
						types::NoteStalled { delay, best_finalized_block_number },
						[
							158u8, 25u8, 64u8, 114u8, 131u8, 139u8, 227u8, 132u8, 42u8, 107u8,
							40u8, 249u8, 18u8, 93u8, 254u8, 86u8, 37u8, 67u8, 250u8, 35u8, 241u8,
							194u8, 209u8, 20u8, 39u8, 75u8, 186u8, 21u8, 48u8, 124u8, 151u8, 31u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_grandpa::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "New authority set has been applied."]
			pub struct NewAuthorities {
				pub authority_set: new_authorities::AuthoritySet,
			}
			pub mod new_authorities {
				use super::runtime_types;
				pub type AuthoritySet = ::subxt::ext::subxt_core::alloc::vec::Vec<(
					runtime_types::sp_consensus_grandpa::app::Public,
					::core::primitive::u64,
				)>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for NewAuthorities {
				const PALLET: &'static str = "Grandpa";
				const EVENT: &'static str = "NewAuthorities";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Current authority set has been paused."]
			pub struct Paused;
			impl ::subxt::ext::subxt_core::events::StaticEvent for Paused {
				const PALLET: &'static str = "Grandpa";
				const EVENT: &'static str = "Paused";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Current authority set has been resumed."]
			pub struct Resumed;
			impl ::subxt::ext::subxt_core::events::StaticEvent for Resumed {
				const PALLET: &'static str = "Grandpa";
				const EVENT: &'static str = "Resumed";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod state {
					use super::runtime_types;
					pub type State =
						runtime_types::pallet_grandpa::StoredState<::core::primitive::u32>;
				}
				pub mod pending_change {
					use super::runtime_types;
					pub type PendingChange =
						runtime_types::pallet_grandpa::StoredPendingChange<::core::primitive::u32>;
				}
				pub mod next_forced {
					use super::runtime_types;
					pub type NextForced = ::core::primitive::u32;
				}
				pub mod stalled {
					use super::runtime_types;
					pub type Stalled = (::core::primitive::u32, ::core::primitive::u32);
				}
				pub mod current_set_id {
					use super::runtime_types;
					pub type CurrentSetId = ::core::primitive::u64;
				}
				pub mod set_id_session {
					use super::runtime_types;
					pub type SetIdSession = ::core::primitive::u32;
					pub type Param0 = ::core::primitive::u64;
				}
				pub mod authorities {
					use super::runtime_types;
					pub type Authorities =
						runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<(
							runtime_types::sp_consensus_grandpa::app::Public,
							::core::primitive::u64,
						)>;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " State of the current authority set."]
				pub fn state(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::state::State,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Grandpa",
						"State",
						(),
						[
							73u8, 71u8, 112u8, 83u8, 238u8, 75u8, 44u8, 9u8, 180u8, 33u8, 30u8,
							121u8, 98u8, 96u8, 61u8, 133u8, 16u8, 70u8, 30u8, 249u8, 34u8, 148u8,
							15u8, 239u8, 164u8, 157u8, 52u8, 27u8, 144u8, 52u8, 223u8, 109u8,
						],
					)
				}
				#[doc = " Pending change: (signaled at, scheduled change)."]
				pub fn pending_change(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::pending_change::PendingChange,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Grandpa",
						"PendingChange",
						(),
						[
							32u8, 165u8, 141u8, 100u8, 109u8, 66u8, 58u8, 22u8, 118u8, 84u8, 92u8,
							164u8, 119u8, 130u8, 104u8, 25u8, 244u8, 111u8, 223u8, 54u8, 184u8,
							95u8, 196u8, 30u8, 244u8, 129u8, 110u8, 127u8, 200u8, 66u8, 226u8,
							26u8,
						],
					)
				}
				#[doc = " next block number where we can force a change."]
				pub fn next_forced(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::next_forced::NextForced,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Grandpa",
						"NextForced",
						(),
						[
							3u8, 231u8, 56u8, 18u8, 87u8, 112u8, 227u8, 126u8, 180u8, 131u8, 255u8,
							141u8, 82u8, 34u8, 61u8, 47u8, 234u8, 37u8, 95u8, 62u8, 33u8, 235u8,
							231u8, 122u8, 125u8, 8u8, 223u8, 95u8, 255u8, 204u8, 40u8, 97u8,
						],
					)
				}
				#[doc = " `true` if we are currently stalled."]
				pub fn stalled(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::stalled::Stalled,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Grandpa",
						"Stalled",
						(),
						[
							6u8, 81u8, 205u8, 142u8, 195u8, 48u8, 0u8, 247u8, 108u8, 170u8, 10u8,
							249u8, 72u8, 206u8, 32u8, 103u8, 109u8, 57u8, 51u8, 21u8, 144u8, 204u8,
							79u8, 8u8, 191u8, 185u8, 38u8, 34u8, 118u8, 223u8, 75u8, 241u8,
						],
					)
				}
				#[doc = " The number of changes (both in terms of keys and underlying economic responsibilities)"]
				#[doc = " in the \"set\" of Grandpa validators from genesis."]
				pub fn current_set_id(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::current_set_id::CurrentSetId,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Grandpa",
						"CurrentSetId",
						(),
						[
							234u8, 215u8, 218u8, 42u8, 30u8, 76u8, 129u8, 40u8, 125u8, 137u8,
							207u8, 47u8, 46u8, 213u8, 159u8, 50u8, 175u8, 81u8, 155u8, 123u8,
							246u8, 175u8, 156u8, 68u8, 22u8, 113u8, 135u8, 137u8, 163u8, 18u8,
							115u8, 73u8,
						],
					)
				}
				#[doc = " A mapping from grandpa set ID to the index of the *most recent* session for which its"]
				#[doc = " members were responsible."]
				#[doc = ""]
				#[doc = " This is only used for validating equivocation proofs. An equivocation proof must"]
				#[doc = " contains a key-ownership proof for a given session, therefore we need a way to tie"]
				#[doc = " together sessions and GRANDPA set ids, i.e. we need to validate that a validator"]
				#[doc = " was the owner of a given key on a given session, and what the active set ID was"]
				#[doc = " during that session."]
				#[doc = ""]
				#[doc = " TWOX-NOTE: `SetId` is not under user control."]
				pub fn set_id_session_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::set_id_session::SetIdSession,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Grandpa",
						"SetIdSession",
						(),
						[
							47u8, 0u8, 239u8, 121u8, 187u8, 213u8, 254u8, 50u8, 238u8, 10u8, 162u8,
							65u8, 189u8, 166u8, 37u8, 74u8, 82u8, 81u8, 160u8, 20u8, 180u8, 253u8,
							238u8, 18u8, 209u8, 203u8, 38u8, 148u8, 16u8, 105u8, 72u8, 169u8,
						],
					)
				}
				#[doc = " A mapping from grandpa set ID to the index of the *most recent* session for which its"]
				#[doc = " members were responsible."]
				#[doc = ""]
				#[doc = " This is only used for validating equivocation proofs. An equivocation proof must"]
				#[doc = " contains a key-ownership proof for a given session, therefore we need a way to tie"]
				#[doc = " together sessions and GRANDPA set ids, i.e. we need to validate that a validator"]
				#[doc = " was the owner of a given key on a given session, and what the active set ID was"]
				#[doc = " during that session."]
				#[doc = ""]
				#[doc = " TWOX-NOTE: `SetId` is not under user control."]
				pub fn set_id_session(
					&self,
					_0: impl ::core::borrow::Borrow<types::set_id_session::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::set_id_session::Param0,
					>,
					types::set_id_session::SetIdSession,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Grandpa",
						"SetIdSession",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							47u8, 0u8, 239u8, 121u8, 187u8, 213u8, 254u8, 50u8, 238u8, 10u8, 162u8,
							65u8, 189u8, 166u8, 37u8, 74u8, 82u8, 81u8, 160u8, 20u8, 180u8, 253u8,
							238u8, 18u8, 209u8, 203u8, 38u8, 148u8, 16u8, 105u8, 72u8, 169u8,
						],
					)
				}
				#[doc = " The current list of authorities."]
				pub fn authorities(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::authorities::Authorities,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Grandpa",
						"Authorities",
						(),
						[
							192u8, 157u8, 98u8, 244u8, 104u8, 38u8, 195u8, 114u8, 183u8, 62u8,
							247u8, 18u8, 31u8, 152u8, 246u8, 206u8, 97u8, 13u8, 118u8, 211u8,
							104u8, 54u8, 150u8, 152u8, 126u8, 170u8, 228u8, 158u8, 108u8, 129u8,
							134u8, 44u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " Max Authorities in use"]
				pub fn max_authorities(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Grandpa",
						"MaxAuthorities",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " The maximum number of nominators for each validator."]
				pub fn max_nominators(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Grandpa",
						"MaxNominators",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " The maximum number of entries to keep in the set id to session index mapping."]
				#[doc = ""]
				#[doc = " Since the `SetIdSession` map is only used for validating equivocations this"]
				#[doc = " value should relate to the bonding duration of whatever staking system is"]
				#[doc = " being used (if any). If equivocation handling is not enabled then this value"]
				#[doc = " can be zero."]
				pub fn max_set_id_session_entries(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u64,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Grandpa",
						"MaxSetIdSessionEntries",
						[
							128u8, 214u8, 205u8, 242u8, 181u8, 142u8, 124u8, 231u8, 190u8, 146u8,
							59u8, 226u8, 157u8, 101u8, 103u8, 117u8, 249u8, 65u8, 18u8, 191u8,
							103u8, 119u8, 53u8, 85u8, 81u8, 96u8, 220u8, 42u8, 184u8, 239u8, 42u8,
							246u8,
						],
					)
				}
			}
		}
	}
	pub mod sidechain {
		use super::root_mod;
		use super::runtime_types;
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod epoch_number {
					use super::runtime_types;
					pub type EpochNumber = runtime_types::sidechain_domain::ScEpochNumber;
				}
				pub mod slots_per_epoch {
					use super::runtime_types;
					pub type SlotsPerEpoch = runtime_types::sidechain_slots::SlotsPerEpoch;
				}
				pub mod genesis_utxo {
					use super::runtime_types;
					pub type GenesisUtxo = runtime_types::sidechain_domain::UtxoId;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " Current epoch number"]
				pub fn epoch_number(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::epoch_number::EpochNumber,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Sidechain",
						"EpochNumber",
						(),
						[
							169u8, 245u8, 25u8, 130u8, 0u8, 133u8, 35u8, 2u8, 179u8, 121u8, 41u8,
							231u8, 187u8, 155u8, 223u8, 210u8, 120u8, 202u8, 152u8, 24u8, 138u8,
							99u8, 163u8, 96u8, 107u8, 56u8, 234u8, 163u8, 7u8, 108u8, 89u8, 6u8,
						],
					)
				}
				#[doc = " Number of slots per epoch. Currently this value must not change for a running chain."]
				pub fn slots_per_epoch(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::slots_per_epoch::SlotsPerEpoch,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Sidechain",
						"SlotsPerEpoch",
						(),
						[
							206u8, 148u8, 72u8, 129u8, 218u8, 144u8, 229u8, 3u8, 125u8, 75u8,
							142u8, 42u8, 73u8, 39u8, 14u8, 156u8, 177u8, 253u8, 9u8, 41u8, 155u8,
							52u8, 244u8, 95u8, 14u8, 178u8, 174u8, 202u8, 28u8, 232u8, 90u8, 20u8,
						],
					)
				}
				#[doc = " Genesis Cardano UTXO of the Partner Chain"]
				#[doc = ""]
				#[doc = " This is the UTXO that is burned by the transaction that establishes Partner Chain"]
				#[doc = " governance on Cardano and serves as the identifier of the Partner Chain. It is also"]
				#[doc = " included in various signed messages to prevent replay attacks on other Partner Chains."]
				pub fn genesis_utxo(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::genesis_utxo::GenesisUtxo,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Sidechain",
						"GenesisUtxo",
						(),
						[
							48u8, 53u8, 243u8, 149u8, 23u8, 224u8, 171u8, 155u8, 42u8, 68u8, 200u8,
							200u8, 101u8, 241u8, 170u8, 131u8, 12u8, 106u8, 58u8, 144u8, 239u8,
							158u8, 184u8, 242u8, 127u8, 161u8, 211u8, 8u8, 17u8, 193u8, 78u8,
							135u8,
						],
					)
				}
			}
		}
	}
	pub mod midnight {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_midnight::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_midnight::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SendMnTransaction {
					pub midnight_tx: send_mn_transaction::MidnightTx,
				}
				pub mod send_mn_transaction {
					use super::runtime_types;
					pub type MidnightTx =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SendMnTransaction {
					const PALLET: &'static str = "Midnight";
					const CALL: &'static str = "send_mn_transaction";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SetMnTxWeight {
					pub new_weight: set_mn_tx_weight::NewWeight,
				}
				pub mod set_mn_tx_weight {
					use super::runtime_types;
					pub type NewWeight = runtime_types::sp_weights::weight_v2::Weight;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetMnTxWeight {
					const PALLET: &'static str = "Midnight";
					const CALL: &'static str = "set_mn_tx_weight";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct OverrideDParameter {
					pub d_parameter_override: override_d_parameter::DParameterOverride,
				}
				pub mod override_d_parameter {
					use super::runtime_types;
					pub type DParameterOverride =
						::core::option::Option<(::core::primitive::u16, ::core::primitive::u16)>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for OverrideDParameter {
					const PALLET: &'static str = "Midnight";
					const CALL: &'static str = "override_d_parameter";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SetContractCallWeight {
					pub new_weight: set_contract_call_weight::NewWeight,
				}
				pub mod set_contract_call_weight {
					use super::runtime_types;
					pub type NewWeight = runtime_types::sp_weights::weight_v2::Weight;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetContractCallWeight {
					const PALLET: &'static str = "Midnight";
					const CALL: &'static str = "set_contract_call_weight";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SetTxSizeWeight {
					pub new_weight: set_tx_size_weight::NewWeight,
				}
				pub mod set_tx_size_weight {
					use super::runtime_types;
					pub type NewWeight = runtime_types::sp_weights::weight_v2::Weight;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetTxSizeWeight {
					const PALLET: &'static str = "Midnight";
					const CALL: &'static str = "set_tx_size_weight";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SetSafeMode {
					pub mode: set_safe_mode::Mode,
				}
				pub mod set_safe_mode {
					use super::runtime_types;
					pub type Mode = ::core::primitive::bool;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetSafeMode {
					const PALLET: &'static str = "Midnight";
					const CALL: &'static str = "set_safe_mode";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				pub fn send_mn_transaction(
					&self,
					midnight_tx: types::send_mn_transaction::MidnightTx,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SendMnTransaction>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Midnight",
						"send_mn_transaction",
						types::SendMnTransaction { midnight_tx },
						[
							30u8, 219u8, 177u8, 72u8, 75u8, 62u8, 191u8, 249u8, 44u8, 133u8, 160u8,
							164u8, 141u8, 99u8, 152u8, 207u8, 255u8, 213u8, 245u8, 6u8, 251u8,
							99u8, 224u8, 203u8, 63u8, 66u8, 61u8, 50u8, 138u8, 90u8, 180u8, 34u8,
						],
					)
				}
				pub fn set_mn_tx_weight(
					&self,
					new_weight: types::set_mn_tx_weight::NewWeight,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetMnTxWeight>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Midnight",
						"set_mn_tx_weight",
						types::SetMnTxWeight { new_weight },
						[
							104u8, 74u8, 227u8, 99u8, 135u8, 138u8, 153u8, 107u8, 235u8, 146u8,
							146u8, 25u8, 180u8, 222u8, 52u8, 81u8, 7u8, 71u8, 201u8, 82u8, 66u8,
							67u8, 45u8, 12u8, 175u8, 153u8, 242u8, 174u8, 11u8, 121u8, 34u8, 90u8,
						],
					)
				}
				pub fn override_d_parameter(
					&self,
					d_parameter_override: types::override_d_parameter::DParameterOverride,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::OverrideDParameter>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Midnight",
						"override_d_parameter",
						types::OverrideDParameter { d_parameter_override },
						[
							51u8, 31u8, 55u8, 19u8, 74u8, 216u8, 104u8, 167u8, 61u8, 3u8, 115u8,
							119u8, 220u8, 196u8, 188u8, 41u8, 226u8, 11u8, 33u8, 129u8, 171u8,
							57u8, 237u8, 57u8, 204u8, 154u8, 190u8, 247u8, 209u8, 210u8, 80u8,
							192u8,
						],
					)
				}
				pub fn set_contract_call_weight(
					&self,
					new_weight: types::set_contract_call_weight::NewWeight,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::SetContractCallWeight,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Midnight",
						"set_contract_call_weight",
						types::SetContractCallWeight { new_weight },
						[
							161u8, 38u8, 14u8, 20u8, 98u8, 174u8, 229u8, 115u8, 23u8, 25u8, 112u8,
							239u8, 30u8, 108u8, 199u8, 62u8, 7u8, 47u8, 150u8, 142u8, 70u8, 213u8,
							145u8, 220u8, 174u8, 184u8, 95u8, 176u8, 222u8, 36u8, 229u8, 248u8,
						],
					)
				}
				pub fn set_tx_size_weight(
					&self,
					new_weight: types::set_tx_size_weight::NewWeight,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetTxSizeWeight>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Midnight",
						"set_tx_size_weight",
						types::SetTxSizeWeight { new_weight },
						[
							246u8, 125u8, 6u8, 11u8, 248u8, 87u8, 203u8, 128u8, 207u8, 38u8, 178u8,
							128u8, 47u8, 29u8, 248u8, 199u8, 74u8, 157u8, 66u8, 185u8, 44u8, 176u8,
							190u8, 52u8, 45u8, 202u8, 35u8, 164u8, 186u8, 238u8, 81u8, 93u8,
						],
					)
				}
				pub fn set_safe_mode(
					&self,
					mode: types::set_safe_mode::Mode,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetSafeMode> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Midnight",
						"set_safe_mode",
						types::SetSafeMode { mode },
						[
							89u8, 212u8, 170u8, 204u8, 214u8, 194u8, 45u8, 211u8, 36u8, 152u8,
							71u8, 233u8, 51u8, 136u8, 93u8, 251u8, 62u8, 215u8, 25u8, 93u8, 130u8,
							78u8, 65u8, 147u8, 159u8, 22u8, 106u8, 121u8, 167u8, 220u8, 175u8,
							168u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_midnight::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A contract was called."]
			pub struct ContractCall(pub contract_call::Field0);
			pub mod contract_call {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_midnight::pallet::CallDetails;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ContractCall {
				const PALLET: &'static str = "Midnight";
				const EVENT: &'static str = "ContractCall";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A contract has been deployed."]
			pub struct ContractDeploy(pub contract_deploy::Field0);
			pub mod contract_deploy {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_midnight::pallet::DeploymentDetails;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ContractDeploy {
				const PALLET: &'static str = "Midnight";
				const EVENT: &'static str = "ContractDeploy";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A transaction has been applied (both the guaranteed and conditional part)."]
			pub struct TxApplied(pub tx_applied::Field0);
			pub mod tx_applied {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_midnight::pallet::TxAppliedDetails;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for TxApplied {
				const PALLET: &'static str = "Midnight";
				const EVENT: &'static str = "TxApplied";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Contract ownership changes to enable snark upgrades"]
			pub struct ContractMaintain(pub contract_maintain::Field0);
			pub mod contract_maintain {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_midnight::pallet::MaintainDetails;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ContractMaintain {
				const PALLET: &'static str = "Midnight";
				const EVENT: &'static str = "ContractMaintain";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "New payout minted."]
			pub struct PayoutMinted(pub payout_minted::Field0);
			pub mod payout_minted {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_midnight::pallet::PayoutDetails;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for PayoutMinted {
				const PALLET: &'static str = "Midnight";
				const EVENT: &'static str = "PayoutMinted";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Payout was claimed."]
			pub struct ClaimMint(pub claim_mint::Field0);
			pub mod claim_mint {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_midnight::pallet::ClaimMintDetails;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ClaimMint {
				const PALLET: &'static str = "Midnight";
				const EVENT: &'static str = "ClaimMint";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Unshielded Tokens Trasfers"]
			pub struct UnshieldedTokens(pub unshielded_tokens::Field0);
			pub mod unshielded_tokens {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_midnight::pallet::UnshieldedTokensDetails;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for UnshieldedTokens {
				const PALLET: &'static str = "Midnight";
				const EVENT: &'static str = "UnshieldedTokens";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Partial Success."]
			pub struct TxPartialSuccess(pub tx_partial_success::Field0);
			pub mod tx_partial_success {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_midnight::pallet::TxAppliedDetails;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for TxPartialSuccess {
				const PALLET: &'static str = "Midnight";
				const EVENT: &'static str = "TxPartialSuccess";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod state_key {
					use super::runtime_types;
					pub type StateKey = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>;
				}
				pub mod network_id {
					use super::runtime_types;
					pub type NetworkId =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>;
				}
				pub mod d_parameter_override {
					use super::runtime_types;
					pub type DParameterOverride = (::core::primitive::u16, ::core::primitive::u16);
				}
				pub mod configurable_weight {
					use super::runtime_types;
					pub type ConfigurableWeight = runtime_types::sp_weights::weight_v2::Weight;
				}
				pub mod configurable_contract_call_weight {
					use super::runtime_types;
					pub type ConfigurableContractCallWeight =
						runtime_types::sp_weights::weight_v2::Weight;
				}
				pub mod configurable_transaction_size_weight {
					use super::runtime_types;
					pub type ConfigurableTransactionSizeWeight =
						runtime_types::sp_weights::weight_v2::Weight;
				}
				pub mod safe_mode {
					use super::runtime_types;
					pub type SafeMode = ::core::primitive::bool;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				pub fn state_key(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::state_key::StateKey,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Midnight",
						"StateKey",
						(),
						[
							212u8, 142u8, 129u8, 134u8, 84u8, 14u8, 60u8, 1u8, 24u8, 246u8, 197u8,
							162u8, 93u8, 99u8, 96u8, 0u8, 143u8, 118u8, 126u8, 190u8, 22u8, 131u8,
							62u8, 59u8, 176u8, 253u8, 227u8, 53u8, 55u8, 5u8, 107u8, 45u8,
						],
					)
				}
				pub fn network_id(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::network_id::NetworkId,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Midnight",
						"NetworkId",
						(),
						[
							96u8, 119u8, 161u8, 231u8, 43u8, 41u8, 144u8, 248u8, 254u8, 246u8,
							14u8, 98u8, 57u8, 202u8, 188u8, 174u8, 158u8, 229u8, 53u8, 29u8, 125u8,
							202u8, 240u8, 57u8, 28u8, 171u8, 167u8, 30u8, 138u8, 210u8, 75u8, 86u8,
						],
					)
				}
				pub fn d_parameter_override(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::d_parameter_override::DParameterOverride,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Midnight",
						"DParameterOverride",
						(),
						[
							100u8, 138u8, 23u8, 203u8, 225u8, 121u8, 163u8, 93u8, 167u8, 53u8,
							205u8, 3u8, 72u8, 121u8, 225u8, 40u8, 190u8, 41u8, 8u8, 205u8, 53u8,
							123u8, 56u8, 61u8, 101u8, 108u8, 199u8, 222u8, 146u8, 240u8, 52u8,
							77u8,
						],
					)
				}
				pub fn configurable_weight(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::configurable_weight::ConfigurableWeight,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Midnight",
						"ConfigurableWeight",
						(),
						[
							17u8, 90u8, 84u8, 209u8, 6u8, 121u8, 244u8, 230u8, 13u8, 21u8, 171u8,
							193u8, 91u8, 49u8, 195u8, 169u8, 186u8, 158u8, 177u8, 242u8, 31u8,
							235u8, 167u8, 247u8, 1u8, 226u8, 121u8, 243u8, 236u8, 164u8, 44u8,
							69u8,
						],
					)
				}
				pub fn configurable_contract_call_weight(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::configurable_contract_call_weight::ConfigurableContractCallWeight,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Midnight",
						"ConfigurableContractCallWeight",
						(),
						[
							52u8, 233u8, 216u8, 89u8, 221u8, 249u8, 163u8, 3u8, 242u8, 174u8,
							114u8, 123u8, 47u8, 22u8, 168u8, 128u8, 150u8, 139u8, 136u8, 36u8,
							119u8, 37u8, 144u8, 115u8, 37u8, 53u8, 38u8, 20u8, 124u8, 114u8, 90u8,
							115u8,
						],
					)
				}
				pub fn configurable_transaction_size_weight(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::configurable_transaction_size_weight::ConfigurableTransactionSizeWeight,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Midnight",
						"ConfigurableTransactionSizeWeight",
						(),
						[
							252u8, 96u8, 158u8, 244u8, 225u8, 164u8, 0u8, 197u8, 2u8, 1u8, 63u8,
							186u8, 132u8, 135u8, 140u8, 137u8, 26u8, 95u8, 91u8, 68u8, 229u8,
							100u8, 34u8, 82u8, 30u8, 216u8, 37u8, 200u8, 21u8, 208u8, 23u8, 116u8,
						],
					)
				}
				pub fn safe_mode(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::safe_mode::SafeMode,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Midnight",
						"SafeMode",
						(),
						[
							135u8, 95u8, 207u8, 15u8, 164u8, 150u8, 245u8, 49u8, 12u8, 253u8, 8u8,
							28u8, 188u8, 244u8, 71u8, 95u8, 192u8, 220u8, 129u8, 53u8, 62u8, 59u8,
							91u8, 253u8, 235u8, 11u8, 86u8, 136u8, 121u8, 93u8, 93u8, 198u8,
						],
					)
				}
			}
		}
	}
	pub mod balances {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_balances::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_balances::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Transfer some liquid free balance to another account."]
				#[doc = ""]
				#[doc = "`transfer_allow_death` will set the `FreeBalance` of the sender and receiver."]
				#[doc = "If the sender's account is below the existential deposit as a result"]
				#[doc = "of the transfer, the account will be reaped."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be `Signed` by the transactor."]
				pub struct TransferAllowDeath {
					pub dest: transfer_allow_death::Dest,
					#[codec(compact)]
					pub value: transfer_allow_death::Value,
				}
				pub mod transfer_allow_death {
					use super::runtime_types;
					pub type Dest = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type Value = ::core::primitive::u128;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for TransferAllowDeath {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "transfer_allow_death";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Exactly as `transfer_allow_death`, except the origin must be root and the source account"]
				#[doc = "may be specified."]
				pub struct ForceTransfer {
					pub source: force_transfer::Source,
					pub dest: force_transfer::Dest,
					#[codec(compact)]
					pub value: force_transfer::Value,
				}
				pub mod force_transfer {
					use super::runtime_types;
					pub type Source = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type Dest = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type Value = ::core::primitive::u128;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ForceTransfer {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "force_transfer";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Same as the [`transfer_allow_death`] call, but with a check that the transfer will not"]
				#[doc = "kill the origin account."]
				#[doc = ""]
				#[doc = "99% of the time you want [`transfer_allow_death`] instead."]
				#[doc = ""]
				#[doc = "[`transfer_allow_death`]: struct.Pallet.html#method.transfer"]
				pub struct TransferKeepAlive {
					pub dest: transfer_keep_alive::Dest,
					#[codec(compact)]
					pub value: transfer_keep_alive::Value,
				}
				pub mod transfer_keep_alive {
					use super::runtime_types;
					pub type Dest = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type Value = ::core::primitive::u128;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for TransferKeepAlive {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "transfer_keep_alive";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Transfer the entire transferable balance from the caller account."]
				#[doc = ""]
				#[doc = "NOTE: This function only attempts to transfer _transferable_ balances. This means that"]
				#[doc = "any locked, reserved, or existential deposits (when `keep_alive` is `true`), will not be"]
				#[doc = "transferred by this function. To ensure that this function results in a killed account,"]
				#[doc = "you might need to prepare the account by removing any reference counters, storage"]
				#[doc = "deposits, etc..."]
				#[doc = ""]
				#[doc = "The dispatch origin of this call must be Signed."]
				#[doc = ""]
				#[doc = "- `dest`: The recipient of the transfer."]
				#[doc = "- `keep_alive`: A boolean to determine if the `transfer_all` operation should send all"]
				#[doc = "  of the funds the account has, causing the sender account to be killed (false), or"]
				#[doc = "  transfer everything except at least the existential deposit, which will guarantee to"]
				#[doc = "  keep the sender account alive (true)."]
				pub struct TransferAll {
					pub dest: transfer_all::Dest,
					pub keep_alive: transfer_all::KeepAlive,
				}
				pub mod transfer_all {
					use super::runtime_types;
					pub type Dest = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type KeepAlive = ::core::primitive::bool;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for TransferAll {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "transfer_all";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Unreserve some balance from a user by force."]
				#[doc = ""]
				#[doc = "Can only be called by ROOT."]
				pub struct ForceUnreserve {
					pub who: force_unreserve::Who,
					pub amount: force_unreserve::Amount,
				}
				pub mod force_unreserve {
					use super::runtime_types;
					pub type Who = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type Amount = ::core::primitive::u128;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ForceUnreserve {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "force_unreserve";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Upgrade a specified account."]
				#[doc = ""]
				#[doc = "- `origin`: Must be `Signed`."]
				#[doc = "- `who`: The account to be upgraded."]
				#[doc = ""]
				#[doc = "This will waive the transaction fee if at least all but 10% of the accounts needed to"]
				#[doc = "be upgraded. (We let some not have to be upgraded just in order to allow for the"]
				#[doc = "possibility of churn)."]
				pub struct UpgradeAccounts {
					pub who: upgrade_accounts::Who,
				}
				pub mod upgrade_accounts {
					use super::runtime_types;
					pub type Who = ::subxt::ext::subxt_core::alloc::vec::Vec<
						::subxt::ext::subxt_core::utils::AccountId32,
					>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for UpgradeAccounts {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "upgrade_accounts";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Set the regular balance of a given account."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call is `root`."]
				pub struct ForceSetBalance {
					pub who: force_set_balance::Who,
					#[codec(compact)]
					pub new_free: force_set_balance::NewFree,
				}
				pub mod force_set_balance {
					use super::runtime_types;
					pub type Who = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type NewFree = ::core::primitive::u128;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ForceSetBalance {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "force_set_balance";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Adjust the total issuance in a saturating way."]
				#[doc = ""]
				#[doc = "Can only be called by root and always needs a positive `delta`."]
				#[doc = ""]
				#[doc = "# Example"]
				pub struct ForceAdjustTotalIssuance {
					pub direction: force_adjust_total_issuance::Direction,
					#[codec(compact)]
					pub delta: force_adjust_total_issuance::Delta,
				}
				pub mod force_adjust_total_issuance {
					use super::runtime_types;
					pub type Direction = runtime_types::pallet_balances::types::AdjustmentDirection;
					pub type Delta = ::core::primitive::u128;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ForceAdjustTotalIssuance {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "force_adjust_total_issuance";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Burn the specified liquid free balance from the origin account."]
				#[doc = ""]
				#[doc = "If the origin's account ends up below the existential deposit as a result"]
				#[doc = "of the burn and `keep_alive` is false, the account will be reaped."]
				#[doc = ""]
				#[doc = "Unlike sending funds to a _burn_ address, which merely makes the funds inaccessible,"]
				#[doc = "this `burn` operation will reduce total issuance by the amount _burned_."]
				pub struct Burn {
					#[codec(compact)]
					pub value: burn::Value,
					pub keep_alive: burn::KeepAlive,
				}
				pub mod burn {
					use super::runtime_types;
					pub type Value = ::core::primitive::u128;
					pub type KeepAlive = ::core::primitive::bool;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Burn {
					const PALLET: &'static str = "Balances";
					const CALL: &'static str = "burn";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Transfer some liquid free balance to another account."]
				#[doc = ""]
				#[doc = "`transfer_allow_death` will set the `FreeBalance` of the sender and receiver."]
				#[doc = "If the sender's account is below the existential deposit as a result"]
				#[doc = "of the transfer, the account will be reaped."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be `Signed` by the transactor."]
				pub fn transfer_allow_death(
					&self,
					dest: types::transfer_allow_death::Dest,
					value: types::transfer_allow_death::Value,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::TransferAllowDeath>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"transfer_allow_death",
						types::TransferAllowDeath { dest, value },
						[
							51u8, 166u8, 195u8, 10u8, 139u8, 218u8, 55u8, 130u8, 6u8, 194u8, 35u8,
							140u8, 27u8, 205u8, 214u8, 222u8, 102u8, 43u8, 143u8, 145u8, 86u8,
							219u8, 210u8, 147u8, 13u8, 39u8, 51u8, 21u8, 237u8, 179u8, 132u8,
							130u8,
						],
					)
				}
				#[doc = "Exactly as `transfer_allow_death`, except the origin must be root and the source account"]
				#[doc = "may be specified."]
				pub fn force_transfer(
					&self,
					source: types::force_transfer::Source,
					dest: types::force_transfer::Dest,
					value: types::force_transfer::Value,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ForceTransfer>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"force_transfer",
						types::ForceTransfer { source, dest, value },
						[
							154u8, 93u8, 222u8, 27u8, 12u8, 248u8, 63u8, 213u8, 224u8, 86u8, 250u8,
							153u8, 249u8, 102u8, 83u8, 160u8, 79u8, 125u8, 105u8, 222u8, 77u8,
							180u8, 90u8, 105u8, 81u8, 217u8, 60u8, 25u8, 213u8, 51u8, 185u8, 96u8,
						],
					)
				}
				#[doc = "Same as the [`transfer_allow_death`] call, but with a check that the transfer will not"]
				#[doc = "kill the origin account."]
				#[doc = ""]
				#[doc = "99% of the time you want [`transfer_allow_death`] instead."]
				#[doc = ""]
				#[doc = "[`transfer_allow_death`]: struct.Pallet.html#method.transfer"]
				pub fn transfer_keep_alive(
					&self,
					dest: types::transfer_keep_alive::Dest,
					value: types::transfer_keep_alive::Value,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::TransferKeepAlive>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"transfer_keep_alive",
						types::TransferKeepAlive { dest, value },
						[
							245u8, 14u8, 190u8, 193u8, 32u8, 210u8, 74u8, 92u8, 25u8, 182u8, 76u8,
							55u8, 247u8, 83u8, 114u8, 75u8, 143u8, 236u8, 117u8, 25u8, 54u8, 157u8,
							208u8, 207u8, 233u8, 89u8, 70u8, 161u8, 235u8, 242u8, 222u8, 59u8,
						],
					)
				}
				#[doc = "Transfer the entire transferable balance from the caller account."]
				#[doc = ""]
				#[doc = "NOTE: This function only attempts to transfer _transferable_ balances. This means that"]
				#[doc = "any locked, reserved, or existential deposits (when `keep_alive` is `true`), will not be"]
				#[doc = "transferred by this function. To ensure that this function results in a killed account,"]
				#[doc = "you might need to prepare the account by removing any reference counters, storage"]
				#[doc = "deposits, etc..."]
				#[doc = ""]
				#[doc = "The dispatch origin of this call must be Signed."]
				#[doc = ""]
				#[doc = "- `dest`: The recipient of the transfer."]
				#[doc = "- `keep_alive`: A boolean to determine if the `transfer_all` operation should send all"]
				#[doc = "  of the funds the account has, causing the sender account to be killed (false), or"]
				#[doc = "  transfer everything except at least the existential deposit, which will guarantee to"]
				#[doc = "  keep the sender account alive (true)."]
				pub fn transfer_all(
					&self,
					dest: types::transfer_all::Dest,
					keep_alive: types::transfer_all::KeepAlive,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::TransferAll> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"transfer_all",
						types::TransferAll { dest, keep_alive },
						[
							105u8, 132u8, 49u8, 144u8, 195u8, 250u8, 34u8, 46u8, 213u8, 248u8,
							112u8, 188u8, 81u8, 228u8, 136u8, 18u8, 67u8, 172u8, 37u8, 38u8, 238u8,
							9u8, 34u8, 15u8, 67u8, 34u8, 148u8, 195u8, 223u8, 29u8, 154u8, 6u8,
						],
					)
				}
				#[doc = "Unreserve some balance from a user by force."]
				#[doc = ""]
				#[doc = "Can only be called by ROOT."]
				pub fn force_unreserve(
					&self,
					who: types::force_unreserve::Who,
					amount: types::force_unreserve::Amount,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ForceUnreserve>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"force_unreserve",
						types::ForceUnreserve { who, amount },
						[
							142u8, 151u8, 64u8, 205u8, 46u8, 64u8, 62u8, 122u8, 108u8, 49u8, 223u8,
							140u8, 120u8, 153u8, 35u8, 165u8, 187u8, 38u8, 157u8, 200u8, 123u8,
							199u8, 198u8, 168u8, 208u8, 159u8, 39u8, 134u8, 92u8, 103u8, 84u8,
							171u8,
						],
					)
				}
				#[doc = "Upgrade a specified account."]
				#[doc = ""]
				#[doc = "- `origin`: Must be `Signed`."]
				#[doc = "- `who`: The account to be upgraded."]
				#[doc = ""]
				#[doc = "This will waive the transaction fee if at least all but 10% of the accounts needed to"]
				#[doc = "be upgraded. (We let some not have to be upgraded just in order to allow for the"]
				#[doc = "possibility of churn)."]
				pub fn upgrade_accounts(
					&self,
					who: types::upgrade_accounts::Who,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::UpgradeAccounts>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"upgrade_accounts",
						types::UpgradeAccounts { who },
						[
							66u8, 200u8, 179u8, 104u8, 65u8, 2u8, 101u8, 56u8, 130u8, 161u8, 224u8,
							233u8, 255u8, 124u8, 70u8, 122u8, 8u8, 49u8, 103u8, 178u8, 68u8, 47u8,
							214u8, 166u8, 217u8, 116u8, 178u8, 50u8, 212u8, 164u8, 98u8, 226u8,
						],
					)
				}
				#[doc = "Set the regular balance of a given account."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call is `root`."]
				pub fn force_set_balance(
					&self,
					who: types::force_set_balance::Who,
					new_free: types::force_set_balance::NewFree,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ForceSetBalance>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"force_set_balance",
						types::ForceSetBalance { who, new_free },
						[
							114u8, 229u8, 59u8, 204u8, 180u8, 83u8, 17u8, 4u8, 59u8, 4u8, 55u8,
							39u8, 151u8, 196u8, 124u8, 60u8, 209u8, 65u8, 193u8, 11u8, 44u8, 164u8,
							116u8, 93u8, 169u8, 30u8, 199u8, 165u8, 55u8, 231u8, 223u8, 43u8,
						],
					)
				}
				#[doc = "Adjust the total issuance in a saturating way."]
				#[doc = ""]
				#[doc = "Can only be called by root and always needs a positive `delta`."]
				#[doc = ""]
				#[doc = "# Example"]
				pub fn force_adjust_total_issuance(
					&self,
					direction: types::force_adjust_total_issuance::Direction,
					delta: types::force_adjust_total_issuance::Delta,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::ForceAdjustTotalIssuance,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"force_adjust_total_issuance",
						types::ForceAdjustTotalIssuance { direction, delta },
						[
							208u8, 134u8, 56u8, 133u8, 232u8, 164u8, 10u8, 213u8, 53u8, 193u8,
							190u8, 63u8, 236u8, 186u8, 96u8, 122u8, 104u8, 87u8, 173u8, 38u8, 58u8,
							176u8, 21u8, 78u8, 42u8, 106u8, 46u8, 248u8, 251u8, 190u8, 150u8,
							202u8,
						],
					)
				}
				#[doc = "Burn the specified liquid free balance from the origin account."]
				#[doc = ""]
				#[doc = "If the origin's account ends up below the existential deposit as a result"]
				#[doc = "of the burn and `keep_alive` is false, the account will be reaped."]
				#[doc = ""]
				#[doc = "Unlike sending funds to a _burn_ address, which merely makes the funds inaccessible,"]
				#[doc = "this `burn` operation will reduce total issuance by the amount _burned_."]
				pub fn burn(
					&self,
					value: types::burn::Value,
					keep_alive: types::burn::KeepAlive,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Burn> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Balances",
						"burn",
						types::Burn { value, keep_alive },
						[
							176u8, 64u8, 7u8, 109u8, 16u8, 44u8, 145u8, 125u8, 147u8, 152u8, 130u8,
							114u8, 221u8, 201u8, 150u8, 162u8, 118u8, 71u8, 52u8, 92u8, 240u8,
							116u8, 203u8, 98u8, 5u8, 22u8, 43u8, 102u8, 94u8, 208u8, 101u8, 57u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_balances::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An account was created with some free balance."]
			pub struct Endowed {
				pub account: endowed::Account,
				pub free_balance: endowed::FreeBalance,
			}
			pub mod endowed {
				use super::runtime_types;
				pub type Account = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type FreeBalance = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Endowed {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Endowed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An account was removed whose balance was non-zero but below ExistentialDeposit,"]
			#[doc = "resulting in an outright loss."]
			pub struct DustLost {
				pub account: dust_lost::Account,
				pub amount: dust_lost::Amount,
			}
			pub mod dust_lost {
				use super::runtime_types;
				pub type Account = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for DustLost {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "DustLost";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Transfer succeeded."]
			pub struct Transfer {
				pub from: transfer::From,
				pub to: transfer::To,
				pub amount: transfer::Amount,
			}
			pub mod transfer {
				use super::runtime_types;
				pub type From = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type To = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Transfer {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Transfer";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A balance was set by root."]
			pub struct BalanceSet {
				pub who: balance_set::Who,
				pub free: balance_set::Free,
			}
			pub mod balance_set {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Free = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for BalanceSet {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "BalanceSet";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some balance was reserved (moved from free to reserved)."]
			pub struct Reserved {
				pub who: reserved::Who,
				pub amount: reserved::Amount,
			}
			pub mod reserved {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Reserved {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Reserved";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some balance was unreserved (moved from reserved to free)."]
			pub struct Unreserved {
				pub who: unreserved::Who,
				pub amount: unreserved::Amount,
			}
			pub mod unreserved {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Unreserved {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Unreserved";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some balance was moved from the reserve of the first account to the second account."]
			#[doc = "Final argument indicates the destination balance type."]
			pub struct ReserveRepatriated {
				pub from: reserve_repatriated::From,
				pub to: reserve_repatriated::To,
				pub amount: reserve_repatriated::Amount,
				pub destination_status: reserve_repatriated::DestinationStatus,
			}
			pub mod reserve_repatriated {
				use super::runtime_types;
				pub type From = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type To = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
				pub type DestinationStatus =
					runtime_types::frame_support::traits::tokens::misc::BalanceStatus;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ReserveRepatriated {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "ReserveRepatriated";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some amount was deposited (e.g. for transaction fees)."]
			pub struct Deposit {
				pub who: deposit::Who,
				pub amount: deposit::Amount,
			}
			pub mod deposit {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Deposit {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Deposit";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some amount was withdrawn from the account (e.g. for transaction fees)."]
			pub struct Withdraw {
				pub who: withdraw::Who,
				pub amount: withdraw::Amount,
			}
			pub mod withdraw {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Withdraw {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Withdraw";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some amount was removed from the account (e.g. for misbehavior)."]
			pub struct Slashed {
				pub who: slashed::Who,
				pub amount: slashed::Amount,
			}
			pub mod slashed {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Slashed {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Slashed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some amount was minted into an account."]
			pub struct Minted {
				pub who: minted::Who,
				pub amount: minted::Amount,
			}
			pub mod minted {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Minted {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Minted";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some amount was burned from an account."]
			pub struct Burned {
				pub who: burned::Who,
				pub amount: burned::Amount,
			}
			pub mod burned {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Burned {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Burned";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some amount was suspended from an account (it can be restored later)."]
			pub struct Suspended {
				pub who: suspended::Who,
				pub amount: suspended::Amount,
			}
			pub mod suspended {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Suspended {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Suspended";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some amount was restored into an account."]
			pub struct Restored {
				pub who: restored::Who,
				pub amount: restored::Amount,
			}
			pub mod restored {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Restored {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Restored";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An account was upgraded."]
			pub struct Upgraded {
				pub who: upgraded::Who,
			}
			pub mod upgraded {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Upgraded {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Upgraded";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Total issuance was increased by `amount`, creating a credit to be balanced."]
			pub struct Issued {
				pub amount: issued::Amount,
			}
			pub mod issued {
				use super::runtime_types;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Issued {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Issued";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Total issuance was decreased by `amount`, creating a debt to be balanced."]
			pub struct Rescinded {
				pub amount: rescinded::Amount,
			}
			pub mod rescinded {
				use super::runtime_types;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Rescinded {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Rescinded";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some balance was locked."]
			pub struct Locked {
				pub who: locked::Who,
				pub amount: locked::Amount,
			}
			pub mod locked {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Locked {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Locked";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some balance was unlocked."]
			pub struct Unlocked {
				pub who: unlocked::Who,
				pub amount: unlocked::Amount,
			}
			pub mod unlocked {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Unlocked {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Unlocked";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some balance was frozen."]
			pub struct Frozen {
				pub who: frozen::Who,
				pub amount: frozen::Amount,
			}
			pub mod frozen {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Frozen {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Frozen";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Some balance was thawed."]
			pub struct Thawed {
				pub who: thawed::Who,
				pub amount: thawed::Amount,
			}
			pub mod thawed {
				use super::runtime_types;
				pub type Who = ::subxt::ext::subxt_core::utils::AccountId32;
				pub type Amount = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Thawed {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "Thawed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The `TotalIssuance` was forcefully changed."]
			pub struct TotalIssuanceForced {
				pub old: total_issuance_forced::Old,
				pub new: total_issuance_forced::New,
			}
			pub mod total_issuance_forced {
				use super::runtime_types;
				pub type Old = ::core::primitive::u128;
				pub type New = ::core::primitive::u128;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for TotalIssuanceForced {
				const PALLET: &'static str = "Balances";
				const EVENT: &'static str = "TotalIssuanceForced";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod total_issuance {
					use super::runtime_types;
					pub type TotalIssuance = ::core::primitive::u128;
				}
				pub mod inactive_issuance {
					use super::runtime_types;
					pub type InactiveIssuance = ::core::primitive::u128;
				}
				pub mod account {
					use super::runtime_types;
					pub type Account =
						runtime_types::pallet_balances::types::AccountData<::core::primitive::u128>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::AccountId32;
				}
				pub mod locks {
					use super::runtime_types;
					pub type Locks =
						runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<
							runtime_types::pallet_balances::types::BalanceLock<
								::core::primitive::u128,
							>,
						>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::AccountId32;
				}
				pub mod reserves {
					use super::runtime_types;
					pub type Reserves = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						runtime_types::pallet_balances::types::ReserveData<
							[::core::primitive::u8; 8usize],
							::core::primitive::u128,
						>,
					>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::AccountId32;
				}
				pub mod holds {
					use super::runtime_types;
					pub type Holds = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						runtime_types::frame_support::traits::tokens::misc::IdAmount<
							(),
							::core::primitive::u128,
						>,
					>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::AccountId32;
				}
				pub mod freezes {
					use super::runtime_types;
					pub type Freezes = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						runtime_types::frame_support::traits::tokens::misc::IdAmount<
							(),
							::core::primitive::u128,
						>,
					>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::AccountId32;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The total units issued in the system."]
				pub fn total_issuance(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::total_issuance::TotalIssuance,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"TotalIssuance",
						(),
						[
							116u8, 70u8, 119u8, 194u8, 69u8, 37u8, 116u8, 206u8, 171u8, 70u8,
							171u8, 210u8, 226u8, 111u8, 184u8, 204u8, 206u8, 11u8, 68u8, 72u8,
							255u8, 19u8, 194u8, 11u8, 27u8, 194u8, 81u8, 204u8, 59u8, 224u8, 202u8,
							185u8,
						],
					)
				}
				#[doc = " The total units of outstanding deactivated balance in the system."]
				pub fn inactive_issuance(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::inactive_issuance::InactiveIssuance,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"InactiveIssuance",
						(),
						[
							212u8, 185u8, 19u8, 50u8, 250u8, 72u8, 173u8, 50u8, 4u8, 104u8, 161u8,
							249u8, 77u8, 247u8, 204u8, 248u8, 11u8, 18u8, 57u8, 4u8, 82u8, 110u8,
							30u8, 216u8, 16u8, 37u8, 87u8, 67u8, 189u8, 235u8, 214u8, 155u8,
						],
					)
				}
				#[doc = " The Balances pallet example of storing the balance of an account."]
				#[doc = ""]
				#[doc = " # Example"]
				#[doc = ""]
				#[doc = " ```nocompile"]
				#[doc = "  impl pallet_balances::Config for Runtime {"]
				#[doc = "    type AccountStore = StorageMapShim<Self::Account<Runtime>, frame_system::Provider<Runtime>, AccountId, Self::AccountData<Balance>>"]
				#[doc = "  }"]
				#[doc = " ```"]
				#[doc = ""]
				#[doc = " You can also store the balance of an account in the `System` pallet."]
				#[doc = ""]
				#[doc = " # Example"]
				#[doc = ""]
				#[doc = " ```nocompile"]
				#[doc = "  impl pallet_balances::Config for Runtime {"]
				#[doc = "   type AccountStore = System"]
				#[doc = "  }"]
				#[doc = " ```"]
				#[doc = ""]
				#[doc = " But this comes with tradeoffs, storing account balances in the system pallet stores"]
				#[doc = " `frame_system` data alongside the account data contrary to storing account balances in the"]
				#[doc = " `Balances` pallet, which uses a `StorageMap` to store balances data only."]
				#[doc = " NOTE: This is only used in the case that this pallet is used to store balances."]
				pub fn account_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::account::Account,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Account",
						(),
						[
							213u8, 38u8, 200u8, 69u8, 218u8, 0u8, 112u8, 181u8, 160u8, 23u8, 96u8,
							90u8, 3u8, 88u8, 126u8, 22u8, 103u8, 74u8, 64u8, 69u8, 29u8, 247u8,
							18u8, 17u8, 234u8, 143u8, 189u8, 22u8, 247u8, 194u8, 154u8, 249u8,
						],
					)
				}
				#[doc = " The Balances pallet example of storing the balance of an account."]
				#[doc = ""]
				#[doc = " # Example"]
				#[doc = ""]
				#[doc = " ```nocompile"]
				#[doc = "  impl pallet_balances::Config for Runtime {"]
				#[doc = "    type AccountStore = StorageMapShim<Self::Account<Runtime>, frame_system::Provider<Runtime>, AccountId, Self::AccountData<Balance>>"]
				#[doc = "  }"]
				#[doc = " ```"]
				#[doc = ""]
				#[doc = " You can also store the balance of an account in the `System` pallet."]
				#[doc = ""]
				#[doc = " # Example"]
				#[doc = ""]
				#[doc = " ```nocompile"]
				#[doc = "  impl pallet_balances::Config for Runtime {"]
				#[doc = "   type AccountStore = System"]
				#[doc = "  }"]
				#[doc = " ```"]
				#[doc = ""]
				#[doc = " But this comes with tradeoffs, storing account balances in the system pallet stores"]
				#[doc = " `frame_system` data alongside the account data contrary to storing account balances in the"]
				#[doc = " `Balances` pallet, which uses a `StorageMap` to store balances data only."]
				#[doc = " NOTE: This is only used in the case that this pallet is used to store balances."]
				pub fn account(
					&self,
					_0: impl ::core::borrow::Borrow<types::account::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::account::Param0,
					>,
					types::account::Account,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Account",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							213u8, 38u8, 200u8, 69u8, 218u8, 0u8, 112u8, 181u8, 160u8, 23u8, 96u8,
							90u8, 3u8, 88u8, 126u8, 22u8, 103u8, 74u8, 64u8, 69u8, 29u8, 247u8,
							18u8, 17u8, 234u8, 143u8, 189u8, 22u8, 247u8, 194u8, 154u8, 249u8,
						],
					)
				}
				#[doc = " Any liquidity locks on some account balances."]
				#[doc = " NOTE: Should only be accessed when setting, changing and freeing a lock."]
				#[doc = ""]
				#[doc = " Use of locks is deprecated in favour of freezes. See `https://github.com/paritytech/substrate/pull/12951/`"]
				pub fn locks_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::locks::Locks,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Locks",
						(),
						[
							10u8, 223u8, 55u8, 0u8, 249u8, 69u8, 168u8, 41u8, 75u8, 35u8, 120u8,
							167u8, 18u8, 132u8, 9u8, 20u8, 91u8, 51u8, 27u8, 69u8, 136u8, 187u8,
							13u8, 220u8, 163u8, 122u8, 26u8, 141u8, 174u8, 249u8, 85u8, 37u8,
						],
					)
				}
				#[doc = " Any liquidity locks on some account balances."]
				#[doc = " NOTE: Should only be accessed when setting, changing and freeing a lock."]
				#[doc = ""]
				#[doc = " Use of locks is deprecated in favour of freezes. See `https://github.com/paritytech/substrate/pull/12951/`"]
				pub fn locks(
					&self,
					_0: impl ::core::borrow::Borrow<types::locks::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::locks::Param0,
					>,
					types::locks::Locks,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Locks",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							10u8, 223u8, 55u8, 0u8, 249u8, 69u8, 168u8, 41u8, 75u8, 35u8, 120u8,
							167u8, 18u8, 132u8, 9u8, 20u8, 91u8, 51u8, 27u8, 69u8, 136u8, 187u8,
							13u8, 220u8, 163u8, 122u8, 26u8, 141u8, 174u8, 249u8, 85u8, 37u8,
						],
					)
				}
				#[doc = " Named reserves on some account balances."]
				#[doc = ""]
				#[doc = " Use of reserves is deprecated in favour of holds. See `https://github.com/paritytech/substrate/pull/12951/`"]
				pub fn reserves_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::reserves::Reserves,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Reserves",
						(),
						[
							112u8, 10u8, 241u8, 77u8, 64u8, 187u8, 106u8, 159u8, 13u8, 153u8,
							140u8, 178u8, 182u8, 50u8, 1u8, 55u8, 149u8, 92u8, 196u8, 229u8, 170u8,
							106u8, 193u8, 88u8, 255u8, 244u8, 2u8, 193u8, 62u8, 235u8, 204u8, 91u8,
						],
					)
				}
				#[doc = " Named reserves on some account balances."]
				#[doc = ""]
				#[doc = " Use of reserves is deprecated in favour of holds. See `https://github.com/paritytech/substrate/pull/12951/`"]
				pub fn reserves(
					&self,
					_0: impl ::core::borrow::Borrow<types::reserves::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::reserves::Param0,
					>,
					types::reserves::Reserves,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Reserves",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							112u8, 10u8, 241u8, 77u8, 64u8, 187u8, 106u8, 159u8, 13u8, 153u8,
							140u8, 178u8, 182u8, 50u8, 1u8, 55u8, 149u8, 92u8, 196u8, 229u8, 170u8,
							106u8, 193u8, 88u8, 255u8, 244u8, 2u8, 193u8, 62u8, 235u8, 204u8, 91u8,
						],
					)
				}
				#[doc = " Holds on account balances."]
				pub fn holds_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::holds::Holds,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Holds",
						(),
						[
							53u8, 126u8, 215u8, 237u8, 42u8, 223u8, 188u8, 150u8, 230u8, 107u8,
							95u8, 24u8, 26u8, 235u8, 158u8, 149u8, 193u8, 191u8, 10u8, 194u8,
							231u8, 59u8, 35u8, 167u8, 186u8, 89u8, 43u8, 126u8, 215u8, 117u8, 1u8,
							202u8,
						],
					)
				}
				#[doc = " Holds on account balances."]
				pub fn holds(
					&self,
					_0: impl ::core::borrow::Borrow<types::holds::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::holds::Param0,
					>,
					types::holds::Holds,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Holds",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							53u8, 126u8, 215u8, 237u8, 42u8, 223u8, 188u8, 150u8, 230u8, 107u8,
							95u8, 24u8, 26u8, 235u8, 158u8, 149u8, 193u8, 191u8, 10u8, 194u8,
							231u8, 59u8, 35u8, 167u8, 186u8, 89u8, 43u8, 126u8, 215u8, 117u8, 1u8,
							202u8,
						],
					)
				}
				#[doc = " Freeze locks on account balances."]
				pub fn freezes_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::freezes::Freezes,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Freezes",
						(),
						[
							69u8, 49u8, 165u8, 76u8, 135u8, 142u8, 179u8, 118u8, 50u8, 109u8, 53u8,
							112u8, 110u8, 94u8, 30u8, 93u8, 173u8, 38u8, 27u8, 142u8, 19u8, 5u8,
							163u8, 4u8, 68u8, 218u8, 179u8, 224u8, 118u8, 218u8, 115u8, 64u8,
						],
					)
				}
				#[doc = " Freeze locks on account balances."]
				pub fn freezes(
					&self,
					_0: impl ::core::borrow::Borrow<types::freezes::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::freezes::Param0,
					>,
					types::freezes::Freezes,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Balances",
						"Freezes",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							69u8, 49u8, 165u8, 76u8, 135u8, 142u8, 179u8, 118u8, 50u8, 109u8, 53u8,
							112u8, 110u8, 94u8, 30u8, 93u8, 173u8, 38u8, 27u8, 142u8, 19u8, 5u8,
							163u8, 4u8, 68u8, 218u8, 179u8, 224u8, 118u8, 218u8, 115u8, 64u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " The minimum amount required to keep an account open. MUST BE GREATER THAN ZERO!"]
				#[doc = ""]
				#[doc = " If you *really* need it to be zero, you can enable the feature `insecure_zero_ed` for"]
				#[doc = " this pallet. However, you do so at your own risk: this will open up a major DoS vector."]
				#[doc = " In case you have multiple sources of provider references, you may also get unexpected"]
				#[doc = " behaviour if you set this to zero."]
				#[doc = ""]
				#[doc = " Bottom line: Do yourself a favour and make it at least one!"]
				pub fn existential_deposit(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u128,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Balances",
						"ExistentialDeposit",
						[
							84u8, 157u8, 140u8, 4u8, 93u8, 57u8, 29u8, 133u8, 105u8, 200u8, 214u8,
							27u8, 144u8, 208u8, 218u8, 160u8, 130u8, 109u8, 101u8, 54u8, 210u8,
							136u8, 71u8, 63u8, 49u8, 237u8, 234u8, 15u8, 178u8, 98u8, 148u8, 156u8,
						],
					)
				}
				#[doc = " The maximum number of locks that should exist on an account."]
				#[doc = " Not strictly enforced, but used for weight estimation."]
				#[doc = ""]
				#[doc = " Use of locks is deprecated in favour of freezes. See `https://github.com/paritytech/substrate/pull/12951/`"]
				pub fn max_locks(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Balances",
						"MaxLocks",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " The maximum number of named reserves that can exist on an account."]
				#[doc = ""]
				#[doc = " Use of reserves is deprecated in favour of holds. See `https://github.com/paritytech/substrate/pull/12951/`"]
				pub fn max_reserves(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Balances",
						"MaxReserves",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " The maximum number of individual freeze locks that can exist on an account at any time."]
				pub fn max_freezes(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Balances",
						"MaxFreezes",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
			}
		}
	}
	pub mod sudo {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "Error for the Sudo pallet."]
		pub type Error = runtime_types::pallet_sudo::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_sudo::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
				pub struct Sudo {
					pub call: ::subxt::ext::subxt_core::alloc::boxed::Box<sudo::Call>,
				}
				pub mod sudo {
					use super::runtime_types;
					pub type Call = runtime_types::midnight_node_runtime::RuntimeCall;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Sudo {
					const PALLET: &'static str = "Sudo";
					const CALL: &'static str = "sudo";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
				#[doc = "This function does not check the weight of the call, and instead allows the"]
				#[doc = "Sudo user to specify the weight of the call."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be _Signed_."]
				pub struct SudoUncheckedWeight {
					pub call:
						::subxt::ext::subxt_core::alloc::boxed::Box<sudo_unchecked_weight::Call>,
					pub weight: sudo_unchecked_weight::Weight,
				}
				pub mod sudo_unchecked_weight {
					use super::runtime_types;
					pub type Call = runtime_types::midnight_node_runtime::RuntimeCall;
					pub type Weight = runtime_types::sp_weights::weight_v2::Weight;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SudoUncheckedWeight {
					const PALLET: &'static str = "Sudo";
					const CALL: &'static str = "sudo_unchecked_weight";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo"]
				#[doc = "key."]
				pub struct SetKey {
					pub new: set_key::New,
				}
				pub mod set_key {
					use super::runtime_types;
					pub type New = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetKey {
					const PALLET: &'static str = "Sudo";
					const CALL: &'static str = "set_key";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Authenticates the sudo key and dispatches a function call with `Signed` origin from"]
				#[doc = "a given account."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be _Signed_."]
				pub struct SudoAs {
					pub who: sudo_as::Who,
					pub call: ::subxt::ext::subxt_core::alloc::boxed::Box<sudo_as::Call>,
				}
				pub mod sudo_as {
					use super::runtime_types;
					pub type Who = ::subxt::ext::subxt_core::utils::MultiAddress<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type Call = runtime_types::midnight_node_runtime::RuntimeCall;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SudoAs {
					const PALLET: &'static str = "Sudo";
					const CALL: &'static str = "sudo_as";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Permanently removes the sudo key."]
				#[doc = ""]
				#[doc = "**This cannot be un-done.**"]
				pub struct RemoveKey;
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for RemoveKey {
					const PALLET: &'static str = "Sudo";
					const CALL: &'static str = "remove_key";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
				pub fn sudo(
					&self,
					call: types::sudo::Call,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Sudo> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Sudo",
						"sudo",
						types::Sudo {
							call: ::subxt::ext::subxt_core::alloc::boxed::Box::new(call),
						},
						[
							219u8, 177u8, 58u8, 24u8, 89u8, 215u8, 177u8, 225u8, 202u8, 117u8,
							109u8, 201u8, 189u8, 178u8, 110u8, 147u8, 85u8, 41u8, 56u8, 101u8,
							133u8, 200u8, 153u8, 84u8, 11u8, 128u8, 192u8, 129u8, 169u8, 150u8,
							82u8, 194u8,
						],
					)
				}
				#[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
				#[doc = "This function does not check the weight of the call, and instead allows the"]
				#[doc = "Sudo user to specify the weight of the call."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be _Signed_."]
				pub fn sudo_unchecked_weight(
					&self,
					call: types::sudo_unchecked_weight::Call,
					weight: types::sudo_unchecked_weight::Weight,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SudoUncheckedWeight>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Sudo",
						"sudo_unchecked_weight",
						types::SudoUncheckedWeight {
							call: ::subxt::ext::subxt_core::alloc::boxed::Box::new(call),
							weight,
						},
						[
							227u8, 134u8, 71u8, 108u8, 35u8, 76u8, 233u8, 204u8, 33u8, 128u8,
							218u8, 174u8, 77u8, 94u8, 82u8, 123u8, 90u8, 54u8, 79u8, 217u8, 241u8,
							250u8, 149u8, 238u8, 133u8, 14u8, 232u8, 126u8, 218u8, 111u8, 187u8,
							37u8,
						],
					)
				}
				#[doc = "Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo"]
				#[doc = "key."]
				pub fn set_key(
					&self,
					new: types::set_key::New,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetKey> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Sudo",
						"set_key",
						types::SetKey { new },
						[
							9u8, 73u8, 39u8, 205u8, 188u8, 127u8, 143u8, 54u8, 128u8, 94u8, 8u8,
							227u8, 197u8, 44u8, 70u8, 93u8, 228u8, 196u8, 64u8, 165u8, 226u8,
							158u8, 101u8, 192u8, 22u8, 193u8, 102u8, 84u8, 21u8, 35u8, 92u8, 198u8,
						],
					)
				}
				#[doc = "Authenticates the sudo key and dispatches a function call with `Signed` origin from"]
				#[doc = "a given account."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be _Signed_."]
				pub fn sudo_as(
					&self,
					who: types::sudo_as::Who,
					call: types::sudo_as::Call,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SudoAs> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Sudo",
						"sudo_as",
						types::SudoAs {
							who,
							call: ::subxt::ext::subxt_core::alloc::boxed::Box::new(call),
						},
						[
							70u8, 214u8, 225u8, 236u8, 117u8, 204u8, 18u8, 179u8, 200u8, 188u8,
							108u8, 74u8, 61u8, 22u8, 204u8, 9u8, 65u8, 57u8, 174u8, 43u8, 116u8,
							206u8, 16u8, 54u8, 144u8, 173u8, 132u8, 221u8, 217u8, 119u8, 237u8,
							111u8,
						],
					)
				}
				#[doc = "Permanently removes the sudo key."]
				#[doc = ""]
				#[doc = "**This cannot be un-done.**"]
				pub fn remove_key(
					&self,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::RemoveKey> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Sudo",
						"remove_key",
						types::RemoveKey {},
						[
							133u8, 253u8, 54u8, 175u8, 202u8, 239u8, 5u8, 198u8, 180u8, 138u8,
							25u8, 28u8, 109u8, 40u8, 30u8, 56u8, 126u8, 100u8, 52u8, 205u8, 250u8,
							191u8, 61u8, 195u8, 172u8, 142u8, 184u8, 239u8, 247u8, 10u8, 211u8,
							79u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_sudo::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A sudo call just took place."]
			pub struct Sudid {
				pub sudo_result: sudid::SudoResult,
			}
			pub mod sudid {
				use super::runtime_types;
				pub type SudoResult =
					::core::result::Result<(), runtime_types::sp_runtime::DispatchError>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Sudid {
				const PALLET: &'static str = "Sudo";
				const EVENT: &'static str = "Sudid";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The sudo key has been updated."]
			pub struct KeyChanged {
				pub old: key_changed::Old,
				pub new: key_changed::New,
			}
			pub mod key_changed {
				use super::runtime_types;
				pub type Old = ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>;
				pub type New = ::subxt::ext::subxt_core::utils::AccountId32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for KeyChanged {
				const PALLET: &'static str = "Sudo";
				const EVENT: &'static str = "KeyChanged";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The key was permanently removed."]
			pub struct KeyRemoved;
			impl ::subxt::ext::subxt_core::events::StaticEvent for KeyRemoved {
				const PALLET: &'static str = "Sudo";
				const EVENT: &'static str = "KeyRemoved";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A [sudo_as](Pallet::sudo_as) call just took place."]
			pub struct SudoAsDone {
				pub sudo_result: sudo_as_done::SudoResult,
			}
			pub mod sudo_as_done {
				use super::runtime_types;
				pub type SudoResult =
					::core::result::Result<(), runtime_types::sp_runtime::DispatchError>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for SudoAsDone {
				const PALLET: &'static str = "Sudo";
				const EVENT: &'static str = "SudoAsDone";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod key {
					use super::runtime_types;
					pub type Key = ::subxt::ext::subxt_core::utils::AccountId32;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The `AccountId` of the sudo key."]
				pub fn key(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::key::Key,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Sudo",
						"Key",
						(),
						[
							72u8, 14u8, 225u8, 162u8, 205u8, 247u8, 227u8, 105u8, 116u8, 57u8, 4u8,
							31u8, 84u8, 137u8, 227u8, 228u8, 133u8, 245u8, 206u8, 227u8, 117u8,
							36u8, 252u8, 151u8, 107u8, 15u8, 180u8, 4u8, 4u8, 152u8, 195u8, 144u8,
						],
					)
				}
			}
		}
	}
	pub mod session_committee_management {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_session_validator_management::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_session_validator_management::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "'for_epoch_number' parameter is needed only for validation purposes, because we need to make sure that"]
				#[doc = "check_inherent uses the same epoch_number as was used to create inherent data."]
				#[doc = "Alternative approach would be to put epoch number inside InherentData. However, sidechain"]
				#[doc = "epoch number is set in Runtime, thus, inherent data provider doesn't have to know about it."]
				#[doc = "On top of that, the latter approach is slightly more complicated to code."]
				pub struct Set {
					pub validators: set::Validators,
					pub for_epoch_number: set::ForEpochNumber,
					pub selection_inputs_hash: set::SelectionInputsHash,
				}
				pub mod set {
					use super::runtime_types;
					pub type Validators = runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < runtime_types :: authority_selection_inherents :: CommitteeMember < runtime_types :: midnight_node_runtime :: opaque :: cross_chain_app :: Public , runtime_types :: midnight_node_runtime :: opaque :: SessionKeys > > ;
					pub type ForEpochNumber = runtime_types::sidechain_domain::ScEpochNumber;
					pub type SelectionInputsHash =
						runtime_types::sidechain_domain::byte_string::SizedByteString;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Set {
					const PALLET: &'static str = "SessionCommitteeManagement";
					const CALL: &'static str = "set";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Changes the main chain scripts used for committee rotation."]
				#[doc = ""]
				#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
				pub struct SetMainChainScripts {
					pub committee_candidate_address:
						set_main_chain_scripts::CommitteeCandidateAddress,
					pub d_parameter_policy_id: set_main_chain_scripts::DParameterPolicyId,
					pub permissioned_candidates_policy_id:
						set_main_chain_scripts::PermissionedCandidatesPolicyId,
				}
				pub mod set_main_chain_scripts {
					use super::runtime_types;
					pub type CommitteeCandidateAddress =
						runtime_types::sidechain_domain::MainchainAddress;
					pub type DParameterPolicyId = runtime_types::sidechain_domain::PolicyId;
					pub type PermissionedCandidatesPolicyId =
						runtime_types::sidechain_domain::PolicyId;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetMainChainScripts {
					const PALLET: &'static str = "SessionCommitteeManagement";
					const CALL: &'static str = "set_main_chain_scripts";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "'for_epoch_number' parameter is needed only for validation purposes, because we need to make sure that"]
				#[doc = "check_inherent uses the same epoch_number as was used to create inherent data."]
				#[doc = "Alternative approach would be to put epoch number inside InherentData. However, sidechain"]
				#[doc = "epoch number is set in Runtime, thus, inherent data provider doesn't have to know about it."]
				#[doc = "On top of that, the latter approach is slightly more complicated to code."]
				pub fn set(
					&self,
					validators: types::set::Validators,
					for_epoch_number: types::set::ForEpochNumber,
					selection_inputs_hash: types::set::SelectionInputsHash,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Set> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"SessionCommitteeManagement",
						"set",
						types::Set { validators, for_epoch_number, selection_inputs_hash },
						[
							150u8, 142u8, 165u8, 43u8, 187u8, 104u8, 235u8, 142u8, 223u8, 140u8,
							252u8, 98u8, 130u8, 50u8, 231u8, 41u8, 49u8, 154u8, 241u8, 140u8,
							227u8, 106u8, 170u8, 11u8, 179u8, 112u8, 142u8, 122u8, 48u8, 21u8,
							20u8, 122u8,
						],
					)
				}
				#[doc = "Changes the main chain scripts used for committee rotation."]
				#[doc = ""]
				#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
				pub fn set_main_chain_scripts(
					&self,
					committee_candidate_address : types :: set_main_chain_scripts :: CommitteeCandidateAddress,
					d_parameter_policy_id: types::set_main_chain_scripts::DParameterPolicyId,
					permissioned_candidates_policy_id : types :: set_main_chain_scripts :: PermissionedCandidatesPolicyId,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetMainChainScripts>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"SessionCommitteeManagement",
						"set_main_chain_scripts",
						types::SetMainChainScripts {
							committee_candidate_address,
							d_parameter_policy_id,
							permissioned_candidates_policy_id,
						},
						[
							137u8, 57u8, 157u8, 46u8, 215u8, 63u8, 209u8, 128u8, 112u8, 136u8,
							42u8, 145u8, 3u8, 142u8, 226u8, 156u8, 207u8, 8u8, 23u8, 54u8, 224u8,
							97u8, 24u8, 101u8, 214u8, 152u8, 162u8, 25u8, 134u8, 196u8, 153u8,
							241u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_session_validator_management::pallet::Event;
		pub mod events {
			use super::runtime_types;
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod current_committee {
					use super::runtime_types;
					pub type CurrentCommittee = runtime_types :: pallet_session_validator_management :: pallet :: CommitteeInfo < runtime_types :: sidechain_domain :: ScEpochNumber , runtime_types :: authority_selection_inherents :: CommitteeMember < runtime_types :: midnight_node_runtime :: opaque :: cross_chain_app :: Public , runtime_types :: midnight_node_runtime :: opaque :: SessionKeys > > ;
				}
				pub mod next_committee {
					use super::runtime_types;
					pub type NextCommittee = runtime_types :: pallet_session_validator_management :: pallet :: CommitteeInfo < runtime_types :: sidechain_domain :: ScEpochNumber , runtime_types :: authority_selection_inherents :: CommitteeMember < runtime_types :: midnight_node_runtime :: opaque :: cross_chain_app :: Public , runtime_types :: midnight_node_runtime :: opaque :: SessionKeys > > ;
				}
				pub mod main_chain_scripts_configuration {
					use super::runtime_types;
					pub type MainChainScriptsConfiguration =
						runtime_types::sp_session_validator_management::MainChainScripts;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				pub fn current_committee(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::current_committee::CurrentCommittee,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"SessionCommitteeManagement",
						"CurrentCommittee",
						(),
						[
							189u8, 217u8, 219u8, 237u8, 44u8, 31u8, 1u8, 146u8, 84u8, 196u8, 45u8,
							60u8, 3u8, 102u8, 138u8, 199u8, 79u8, 76u8, 59u8, 208u8, 164u8, 195u8,
							227u8, 162u8, 231u8, 206u8, 172u8, 36u8, 48u8, 118u8, 196u8, 146u8,
						],
					)
				}
				pub fn next_committee(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::next_committee::NextCommittee,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"SessionCommitteeManagement",
						"NextCommittee",
						(),
						[
							180u8, 79u8, 111u8, 145u8, 206u8, 22u8, 187u8, 75u8, 107u8, 251u8,
							21u8, 176u8, 195u8, 205u8, 85u8, 9u8, 97u8, 204u8, 162u8, 166u8, 136u8,
							54u8, 138u8, 221u8, 216u8, 222u8, 134u8, 174u8, 207u8, 87u8, 13u8,
							169u8,
						],
					)
				}
				pub fn main_chain_scripts_configuration(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::main_chain_scripts_configuration::MainChainScriptsConfiguration,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"SessionCommitteeManagement",
						"MainChainScriptsConfiguration",
						(),
						[
							72u8, 5u8, 217u8, 148u8, 30u8, 219u8, 221u8, 119u8, 254u8, 152u8, 9u8,
							164u8, 19u8, 77u8, 255u8, 0u8, 198u8, 193u8, 142u8, 145u8, 74u8, 194u8,
							98u8, 16u8, 122u8, 221u8, 44u8, 251u8, 73u8, 169u8, 223u8, 39u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " Maximum amount of validators."]
				pub fn max_validators(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"SessionCommitteeManagement",
						"MaxValidators",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
			}
		}
	}
	pub mod runtime_upgrade {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_upgrade::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_upgrade::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Vote on a proposed runtime upgrade that is represented by an onchain preimage request"]
				#[doc = ""]
				#[doc = "This call should be invoked exactly once per block due to its inherent nature."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be _None_."]
				#[doc = ""]
				#[doc = "This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware"]
				#[doc = "that changing the complexity of this call could result exhausting the resources in a"]
				#[doc = "block to execute any other calls."]
				pub struct ProposeOrVoteUpgrade {
					pub upgrade: propose_or_vote_upgrade::Upgrade,
				}
				pub mod propose_or_vote_upgrade {
					use super::runtime_types;
					pub type Upgrade = runtime_types::midnight_primitives_upgrade::UpgradeProposal;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ProposeOrVoteUpgrade {
					const PALLET: &'static str = "RuntimeUpgrade";
					const CALL: &'static str = "propose_or_vote_upgrade";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Vote on a proposed runtime upgrade that is represented by an onchain preimage request"]
				#[doc = ""]
				#[doc = "This call should be invoked exactly once per block due to its inherent nature."]
				#[doc = ""]
				#[doc = "The dispatch origin for this call must be _None_."]
				#[doc = ""]
				#[doc = "This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware"]
				#[doc = "that changing the complexity of this call could result exhausting the resources in a"]
				#[doc = "block to execute any other calls."]
				pub fn propose_or_vote_upgrade(
					&self,
					upgrade: types::propose_or_vote_upgrade::Upgrade,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ProposeOrVoteUpgrade>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"RuntimeUpgrade",
						"propose_or_vote_upgrade",
						types::ProposeOrVoteUpgrade { upgrade },
						[
							66u8, 97u8, 31u8, 155u8, 150u8, 17u8, 237u8, 181u8, 4u8, 80u8, 30u8,
							218u8, 149u8, 147u8, 102u8, 72u8, 29u8, 152u8, 92u8, 226u8, 3u8, 14u8,
							194u8, 66u8, 208u8, 206u8, 38u8, 204u8, 67u8, 104u8, 61u8, 220u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_upgrade::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Signal an issue when attempting a runtime upgrade, in a context where pallet errors are not accessible"]
			pub struct CouldNotScheduleRuntimeUpgrade {
				pub runtime_hash: could_not_schedule_runtime_upgrade::RuntimeHash,
				pub spec_version: could_not_schedule_runtime_upgrade::SpecVersion,
			}
			pub mod could_not_schedule_runtime_upgrade {
				use super::runtime_types;
				pub type RuntimeHash = ::subxt::ext::subxt_core::utils::H256;
				pub type SpecVersion = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for CouldNotScheduleRuntimeUpgrade {
				const PALLET: &'static str = "RuntimeUpgrade";
				const EVENT: &'static str = "CouldNotScheduleRuntimeUpgrade";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "No votes were made this round"]
			pub struct NoVotes;
			impl ::subxt::ext::subxt_core::events::StaticEvent for NoVotes {
				const PALLET: &'static str = "RuntimeUpgrade";
				const EVENT: &'static str = "NoVotes";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Code upgrade managed by this pallet was scheduled"]
			pub struct UpgradeScheduled {
				pub runtime_hash: upgrade_scheduled::RuntimeHash,
				pub spec_version: upgrade_scheduled::SpecVersion,
				pub scheduled_for: upgrade_scheduled::ScheduledFor,
			}
			pub mod upgrade_scheduled {
				use super::runtime_types;
				pub type RuntimeHash = ::subxt::ext::subxt_core::utils::H256;
				pub type SpecVersion = ::core::primitive::u32;
				pub type ScheduledFor = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for UpgradeScheduled {
				const PALLET: &'static str = "RuntimeUpgrade";
				const EVENT: &'static str = "UpgradeScheduled";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Validators could not agree on an upgrade, and voting will be reset"]
			pub struct NoConsensusOnUpgrade;
			impl ::subxt::ext::subxt_core::events::StaticEvent for NoConsensusOnUpgrade {
				const PALLET: &'static str = "RuntimeUpgrade";
				const EVENT: &'static str = "NoConsensusOnUpgrade";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Upgrade was not performed because a preimage of the upgrade request was not found"]
			pub struct NoUpgradePreimageMissing {
				pub preimage_hash: no_upgrade_preimage_missing::PreimageHash,
			}
			pub mod no_upgrade_preimage_missing {
				use super::runtime_types;
				pub type PreimageHash = ::subxt::ext::subxt_core::utils::H256;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for NoUpgradePreimageMissing {
				const PALLET: &'static str = "RuntimeUpgrade";
				const EVENT: &'static str = "NoUpgradePreimageMissing";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Upgrade was not performed because the request for its preimage was not found"]
			pub struct NoUpgradePreimageNotRequested {
				pub preimage_hash: no_upgrade_preimage_not_requested::PreimageHash,
			}
			pub mod no_upgrade_preimage_not_requested {
				use super::runtime_types;
				pub type PreimageHash = ::subxt::ext::subxt_core::utils::H256;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for NoUpgradePreimageNotRequested {
				const PALLET: &'static str = "RuntimeUpgrade";
				const EVENT: &'static str = "NoUpgradePreimageNotRequested";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "An upgrade was attempted, but the call size exceeded the configured bounds"]
			pub struct UpgradeCallTooLarge {
				pub runtime_hash: upgrade_call_too_large::RuntimeHash,
				pub spec_version: upgrade_call_too_large::SpecVersion,
			}
			pub mod upgrade_call_too_large {
				use super::runtime_types;
				pub type RuntimeHash = ::subxt::ext::subxt_core::utils::H256;
				pub type SpecVersion = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for UpgradeCallTooLarge {
				const PALLET: &'static str = "RuntimeUpgrade";
				const EVENT: &'static str = "UpgradeCallTooLarge";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A validator has voted on an upgrade"]
			pub struct Voted {
				pub voter: voted::Voter,
				pub target: voted::Target,
			}
			pub mod voted {
				use super::runtime_types;
				pub type Voter = runtime_types::sp_consensus_aura::sr25519::app_sr25519::Public;
				pub type Target = runtime_types::midnight_primitives_upgrade::UpgradeProposal;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Voted {
				const PALLET: &'static str = "RuntimeUpgrade";
				const EVENT: &'static str = "Voted";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod runtime_upgrade_votes {
					use super::runtime_types;
					pub type RuntimeUpgradeVotes =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<(
							runtime_types::midnight_primitives_upgrade::UpgradeProposal,
							::core::primitive::u32,
						)>;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				pub fn runtime_upgrade_votes(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::runtime_upgrade_votes::RuntimeUpgradeVotes,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"RuntimeUpgrade",
						"RuntimeUpgradeVotes",
						(),
						[
							138u8, 7u8, 39u8, 216u8, 43u8, 49u8, 245u8, 47u8, 91u8, 225u8, 190u8,
							79u8, 55u8, 139u8, 11u8, 225u8, 197u8, 160u8, 3u8, 137u8, 31u8, 137u8,
							245u8, 200u8, 9u8, 217u8, 78u8, 143u8, 222u8, 232u8, 129u8, 77u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " The Lottery's pallet id"]
				pub fn pallet_id(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					runtime_types::frame_support::PalletId,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"RuntimeUpgrade",
						"PalletId",
						[
							56u8, 243u8, 53u8, 83u8, 154u8, 179u8, 170u8, 80u8, 133u8, 173u8, 61u8,
							161u8, 47u8, 225u8, 146u8, 21u8, 50u8, 229u8, 248u8, 27u8, 104u8, 58u8,
							129u8, 197u8, 102u8, 160u8, 168u8, 205u8, 154u8, 42u8, 217u8, 53u8,
						],
					)
				}
				#[doc = " Number of blocks before any given scheduled upgrade occurs."]
				pub fn upgrade_delay(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"RuntimeUpgrade",
						"UpgradeDelay",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " Percentage of the current validator set who must vote on the upgrade in order for it to pass"]
				pub fn upgrade_vote_threshold(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					runtime_types::sp_arithmetic::per_things::Percent,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"RuntimeUpgrade",
						"UpgradeVoteThreshold",
						[
							40u8, 171u8, 69u8, 196u8, 34u8, 184u8, 50u8, 128u8, 139u8, 192u8, 63u8,
							231u8, 249u8, 200u8, 252u8, 73u8, 244u8, 170u8, 51u8, 177u8, 106u8,
							47u8, 114u8, 234u8, 84u8, 104u8, 62u8, 118u8, 227u8, 50u8, 225u8,
							122u8,
						],
					)
				}
			}
		}
	}
	pub mod node_version {
		use super::root_mod;
		use super::runtime_types;
	}
	pub mod native_token_management {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "Error type used by the pallet's extrinsics"]
		pub type Error = runtime_types::pallet_native_token_management::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_native_token_management::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Inherent that registers new native token transfer from the Cardano main chain and triggers"]
				#[doc = "the handler configured in [Config::TokenTransferHandler]."]
				#[doc = ""]
				#[doc = "Arguments:"]
				#[doc = "- `token_amount`: the total amount of tokens transferred since the last invocation of the inherent"]
				pub struct TransferTokens {
					pub token_amount: transfer_tokens::TokenAmount,
				}
				pub mod transfer_tokens {
					use super::runtime_types;
					pub type TokenAmount = runtime_types::sidechain_domain::NativeTokenAmount;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for TransferTokens {
					const PALLET: &'static str = "NativeTokenManagement";
					const CALL: &'static str = "transfer_tokens";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Changes the main chain scripts used for observing native token transfers."]
				#[doc = ""]
				#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
				pub struct SetMainChainScripts {
					pub native_token_policy_id: set_main_chain_scripts::NativeTokenPolicyId,
					pub native_token_asset_name: set_main_chain_scripts::NativeTokenAssetName,
					pub illiquid_supply_validator_address:
						set_main_chain_scripts::IlliquidSupplyValidatorAddress,
				}
				pub mod set_main_chain_scripts {
					use super::runtime_types;
					pub type NativeTokenPolicyId = runtime_types::sidechain_domain::PolicyId;
					pub type NativeTokenAssetName = runtime_types::sidechain_domain::AssetName;
					pub type IlliquidSupplyValidatorAddress =
						runtime_types::sidechain_domain::MainchainAddress;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetMainChainScripts {
					const PALLET: &'static str = "NativeTokenManagement";
					const CALL: &'static str = "set_main_chain_scripts";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Inherent that registers new native token transfer from the Cardano main chain and triggers"]
				#[doc = "the handler configured in [Config::TokenTransferHandler]."]
				#[doc = ""]
				#[doc = "Arguments:"]
				#[doc = "- `token_amount`: the total amount of tokens transferred since the last invocation of the inherent"]
				pub fn transfer_tokens(
					&self,
					token_amount: types::transfer_tokens::TokenAmount,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::TransferTokens>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"NativeTokenManagement",
						"transfer_tokens",
						types::TransferTokens { token_amount },
						[
							97u8, 155u8, 193u8, 152u8, 91u8, 195u8, 124u8, 173u8, 195u8, 110u8,
							200u8, 59u8, 106u8, 176u8, 147u8, 158u8, 169u8, 111u8, 236u8, 11u8,
							209u8, 184u8, 46u8, 213u8, 67u8, 160u8, 38u8, 14u8, 38u8, 7u8, 183u8,
							187u8,
						],
					)
				}
				#[doc = "Changes the main chain scripts used for observing native token transfers."]
				#[doc = ""]
				#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
				pub fn set_main_chain_scripts(
					&self,
					native_token_policy_id: types::set_main_chain_scripts::NativeTokenPolicyId,
					native_token_asset_name: types::set_main_chain_scripts::NativeTokenAssetName,
					illiquid_supply_validator_address : types :: set_main_chain_scripts :: IlliquidSupplyValidatorAddress,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetMainChainScripts>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"NativeTokenManagement",
						"set_main_chain_scripts",
						types::SetMainChainScripts {
							native_token_policy_id,
							native_token_asset_name,
							illiquid_supply_validator_address,
						},
						[
							8u8, 15u8, 182u8, 168u8, 11u8, 63u8, 140u8, 95u8, 201u8, 37u8, 234u8,
							149u8, 241u8, 208u8, 42u8, 109u8, 53u8, 27u8, 173u8, 212u8, 0u8, 185u8,
							88u8, 134u8, 185u8, 217u8, 75u8, 115u8, 206u8, 86u8, 13u8, 125u8,
						],
					)
				}
			}
		}
		#[doc = "Events emitted by this pallet"]
		pub type Event = runtime_types::pallet_native_token_management::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Signals that a new native token transfer has been processed by the pallet"]
			pub struct TokensTransfered(pub tokens_transfered::Field0);
			pub mod tokens_transfered {
				use super::runtime_types;
				pub type Field0 = runtime_types::sidechain_domain::NativeTokenAmount;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for TokensTransfered {
				const PALLET: &'static str = "NativeTokenManagement";
				const EVENT: &'static str = "TokensTransfered";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod main_chain_scripts_configuration {
					use super::runtime_types;
					pub type MainChainScriptsConfiguration =
						runtime_types::sp_native_token_management::MainChainScripts;
				}
				pub mod initialized {
					use super::runtime_types;
					pub type Initialized = ::core::primitive::bool;
				}
				pub mod transfered_this_block {
					use super::runtime_types;
					pub type TransferedThisBlock =
						runtime_types::sidechain_domain::NativeTokenAmount;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				pub fn main_chain_scripts_configuration(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::main_chain_scripts_configuration::MainChainScriptsConfiguration,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenManagement",
						"MainChainScriptsConfiguration",
						(),
						[
							25u8, 111u8, 159u8, 47u8, 78u8, 16u8, 143u8, 108u8, 126u8, 67u8, 57u8,
							55u8, 174u8, 207u8, 88u8, 138u8, 73u8, 65u8, 29u8, 60u8, 83u8, 25u8,
							167u8, 188u8, 172u8, 48u8, 29u8, 111u8, 87u8, 209u8, 226u8, 180u8,
						],
					)
				}
				#[doc = " Stores the pallet's initialization state."]
				#[doc = ""]
				#[doc = " The pallet is considered initialized if its inherent has been successfuly called at least once since"]
				#[doc = " genesis or the last invocation of [set_main_chain_scripts][Pallet::set_main_chain_scripts]."]
				pub fn initialized(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::initialized::Initialized,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenManagement",
						"Initialized",
						(),
						[
							113u8, 248u8, 192u8, 99u8, 251u8, 135u8, 33u8, 245u8, 102u8, 117u8,
							5u8, 231u8, 111u8, 153u8, 197u8, 250u8, 144u8, 110u8, 49u8, 205u8,
							112u8, 23u8, 210u8, 167u8, 172u8, 43u8, 65u8, 117u8, 230u8, 153u8,
							58u8, 138u8,
						],
					)
				}
				#[doc = " Transient storage containing the amount of native token transfer registered in the current block."]
				#[doc = ""]
				#[doc = " Any value in this storage is only present during execution of a block and is emptied on block finalization."]
				pub fn transfered_this_block(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::transfered_this_block::TransferedThisBlock,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenManagement",
						"TransferedThisBlock",
						(),
						[
							55u8, 242u8, 83u8, 93u8, 158u8, 89u8, 198u8, 232u8, 196u8, 9u8, 122u8,
							59u8, 184u8, 56u8, 140u8, 164u8, 202u8, 187u8, 216u8, 232u8, 22u8, 3u8,
							194u8, 100u8, 34u8, 157u8, 248u8, 118u8, 106u8, 215u8, 89u8, 38u8,
						],
					)
				}
			}
		}
	}
	pub mod native_token_observation {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_native_token_observation::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_native_token_observation::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ProcessTokens {
					pub utxos: process_tokens::Utxos,
					pub next_cardano_position: process_tokens::NextCardanoPosition,
				}
				pub mod process_tokens {
					use super::runtime_types;
					pub type Utxos = ::subxt::ext::subxt_core::alloc::vec::Vec<
						runtime_types::midnight_primitives_mainchain_follower::types::ObservedUtxo,
					>;
					pub type NextCardanoPosition = runtime_types :: midnight_primitives_native_token_observation :: CardanoPosition ;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ProcessTokens {
					const PALLET: &'static str = "NativeTokenObservation";
					const CALL: &'static str = "process_tokens";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Changes the mainchain address for the mapping validator contract"]
				#[doc = ""]
				#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
				pub struct SetMappingValidatorContractAddress {
					pub address: set_mapping_validator_contract_address::Address,
				}
				pub mod set_mapping_validator_contract_address {
					use super::runtime_types;
					pub type Address =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetMappingValidatorContractAddress {
					const PALLET: &'static str = "NativeTokenObservation";
					const CALL: &'static str = "set_mapping_validator_contract_address";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				pub fn process_tokens(
					&self,
					utxos: types::process_tokens::Utxos,
					next_cardano_position: types::process_tokens::NextCardanoPosition,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ProcessTokens>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"NativeTokenObservation",
						"process_tokens",
						types::ProcessTokens { utxos, next_cardano_position },
						[
							88u8, 192u8, 107u8, 217u8, 103u8, 230u8, 104u8, 40u8, 44u8, 15u8,
							129u8, 126u8, 77u8, 56u8, 129u8, 80u8, 201u8, 27u8, 132u8, 249u8,
							246u8, 213u8, 206u8, 78u8, 212u8, 25u8, 177u8, 150u8, 137u8, 68u8,
							74u8, 246u8,
						],
					)
				}
				#[doc = "Changes the mainchain address for the mapping validator contract"]
				#[doc = ""]
				#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
				pub fn set_mapping_validator_contract_address(
					&self,
					address: types::set_mapping_validator_contract_address::Address,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::SetMappingValidatorContractAddress,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"NativeTokenObservation",
						"set_mapping_validator_contract_address",
						types::SetMappingValidatorContractAddress { address },
						[
							176u8, 56u8, 98u8, 216u8, 203u8, 26u8, 84u8, 117u8, 143u8, 111u8,
							149u8, 216u8, 132u8, 136u8, 149u8, 30u8, 101u8, 68u8, 151u8, 94u8,
							242u8, 127u8, 136u8, 207u8, 250u8, 230u8, 98u8, 232u8, 92u8, 35u8,
							239u8, 226u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_native_token_observation::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct Added(pub added::Field0);
			pub mod added {
				use super::runtime_types;
				pub type Field0 = (
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						runtime_types::pallet_native_token_observation::pallet::MappingEntry,
					>,
				);
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Added {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "Added";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Tried to remove an element, but it was not found in the list of registrations"]
			pub struct AttemptedRemoveNonexistantElement(
				pub attempted_remove_nonexistant_element::Field0,
			);
			pub mod attempted_remove_nonexistant_element {
				use super::runtime_types;
				pub type Field0 =
					runtime_types::pallet_native_token_observation::pallet::MappingEntry;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for AttemptedRemoveNonexistantElement {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "AttemptedRemoveNonexistantElement";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Could not add registration"]
			pub struct CouldNotAddRegistration;
			impl ::subxt::ext::subxt_core::events::StaticEvent for CouldNotAddRegistration {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "CouldNotAddRegistration";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct DuplicatedRegistration(pub duplicated_registration::Field0);
			pub mod duplicated_registration {
				use super::runtime_types;
				pub type Field0 = (
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						runtime_types::pallet_native_token_observation::pallet::MappingEntry,
					>,
				);
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for DuplicatedRegistration {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "DuplicatedRegistration";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct InvalidCardanoAddress;
			impl ::subxt::ext::subxt_core::events::StaticEvent for InvalidCardanoAddress {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "InvalidCardanoAddress";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct InvalidDustAddress;
			impl ::subxt::ext::subxt_core::events::StaticEvent for InvalidDustAddress {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "InvalidDustAddress";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct Registered(pub registered::Field0);
			pub mod registered {
				use super::runtime_types;
				pub type Field0 = (
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						runtime_types::pallet_native_token_observation::pallet::MappingEntry,
					>,
				);
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Registered {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "Registered";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Removed registrations"]
			pub struct Removed(pub removed::Field0);
			pub mod removed {
				use super::runtime_types;
				pub type Field0 = (
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
					runtime_types::pallet_native_token_observation::pallet::MappingEntry,
				);
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Removed {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "Removed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Removed single registration in order to add a new registration in order to respect length bound of registration list"]
			pub struct RemovedOld(pub removed_old::Field0);
			pub mod removed_old {
				use super::runtime_types;
				pub type Field0 = (
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
					runtime_types::pallet_native_token_observation::pallet::MappingEntry,
				);
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for RemovedOld {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "RemovedOld";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "System transaction - the `SystemTx` struct is defined in the Node for now, but this event will contain a Ledger System Transaction"]
			pub struct SystemTx(pub system_tx::Field0);
			pub mod system_tx {
				use super::runtime_types;
				pub type Field0 = runtime_types::pallet_native_token_observation::SystemTx;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for SystemTx {
				const PALLET: &'static str = "NativeTokenObservation";
				const EVENT: &'static str = "SystemTx";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod main_chain_generation_registrants_address {
					use super::runtime_types;
					pub type MainChainGenerationRegistrantsAddress =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>;
				}
				pub mod registrations {
					use super::runtime_types;
					pub type Registrations =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							runtime_types::pallet_native_token_observation::pallet::MappingEntry,
						>;
					pub type Param0 = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>;
				}
				pub mod utxo_owners {
					use super::runtime_types;
					pub type UtxoOwners =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>;
					pub type Param0 = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>;
				}
				pub mod next_cardano_position {
					use super::runtime_types;
					pub type NextCardanoPosition = runtime_types :: midnight_primitives_native_token_observation :: CardanoPosition ;
				}
				pub mod native_asset_identifier {
					use super::runtime_types;
					pub type NativeAssetIdentifier = (
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
					);
				}
				pub mod cardano_block_window_size {
					use super::runtime_types;
					pub type CardanoBlockWindowSize = ::core::primitive::u32;
				}
				pub mod cardano_tx_capacity_per_block {
					use super::runtime_types;
					pub type CardanoTxCapacityPerBlock = ::core::primitive::u32;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				pub fn main_chain_generation_registrants_address (& self ,) -> :: subxt :: ext :: subxt_core :: storage :: address :: StaticAddress :: < () , types :: main_chain_generation_registrants_address :: MainChainGenerationRegistrantsAddress , :: subxt :: ext :: subxt_core :: utils :: Yes , :: subxt :: ext :: subxt_core :: utils :: Yes , () >{
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"MainChainGenerationRegistrantsAddress",
						(),
						[
							183u8, 129u8, 201u8, 187u8, 127u8, 175u8, 233u8, 195u8, 21u8, 36u8,
							81u8, 178u8, 199u8, 60u8, 50u8, 10u8, 143u8, 211u8, 50u8, 20u8, 16u8,
							48u8, 219u8, 139u8, 254u8, 177u8, 62u8, 107u8, 157u8, 173u8, 191u8,
							153u8,
						],
					)
				}
				pub fn registrations_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::registrations::Registrations,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"Registrations",
						(),
						[
							245u8, 120u8, 244u8, 194u8, 55u8, 36u8, 111u8, 142u8, 84u8, 208u8,
							237u8, 68u8, 199u8, 55u8, 153u8, 86u8, 154u8, 253u8, 217u8, 187u8,
							97u8, 109u8, 23u8, 180u8, 114u8, 14u8, 3u8, 205u8, 182u8, 250u8, 211u8,
							118u8,
						],
					)
				}
				pub fn registrations(
					&self,
					_0: impl ::core::borrow::Borrow<types::registrations::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::registrations::Param0,
					>,
					types::registrations::Registrations,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"Registrations",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							245u8, 120u8, 244u8, 194u8, 55u8, 36u8, 111u8, 142u8, 84u8, 208u8,
							237u8, 68u8, 199u8, 55u8, 153u8, 86u8, 154u8, 253u8, 217u8, 187u8,
							97u8, 109u8, 23u8, 180u8, 114u8, 14u8, 3u8, 205u8, 182u8, 250u8, 211u8,
							118u8,
						],
					)
				}
				pub fn utxo_owners_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::utxo_owners::UtxoOwners,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"UtxoOwners",
						(),
						[
							163u8, 148u8, 172u8, 227u8, 186u8, 153u8, 252u8, 179u8, 74u8, 196u8,
							85u8, 98u8, 100u8, 150u8, 82u8, 16u8, 159u8, 177u8, 103u8, 16u8, 198u8,
							250u8, 100u8, 118u8, 135u8, 157u8, 50u8, 15u8, 97u8, 111u8, 131u8,
							248u8,
						],
					)
				}
				pub fn utxo_owners(
					&self,
					_0: impl ::core::borrow::Borrow<types::utxo_owners::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::utxo_owners::Param0,
					>,
					types::utxo_owners::UtxoOwners,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"UtxoOwners",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							163u8, 148u8, 172u8, 227u8, 186u8, 153u8, 252u8, 179u8, 74u8, 196u8,
							85u8, 98u8, 100u8, 150u8, 82u8, 16u8, 159u8, 177u8, 103u8, 16u8, 198u8,
							250u8, 100u8, 118u8, 135u8, 157u8, 50u8, 15u8, 97u8, 111u8, 131u8,
							248u8,
						],
					)
				}
				pub fn next_cardano_position(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::next_cardano_position::NextCardanoPosition,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"NextCardanoPosition",
						(),
						[
							157u8, 123u8, 101u8, 33u8, 72u8, 134u8, 166u8, 174u8, 205u8, 140u8,
							18u8, 151u8, 193u8, 26u8, 149u8, 134u8, 247u8, 162u8, 47u8, 253u8,
							109u8, 167u8, 38u8, 26u8, 17u8, 68u8, 164u8, 50u8, 1u8, 157u8, 80u8,
							110u8,
						],
					)
				}
				pub fn native_asset_identifier(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::native_asset_identifier::NativeAssetIdentifier,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"NativeAssetIdentifier",
						(),
						[
							125u8, 247u8, 237u8, 154u8, 21u8, 142u8, 239u8, 57u8, 244u8, 127u8,
							108u8, 97u8, 78u8, 144u8, 120u8, 97u8, 60u8, 91u8, 149u8, 52u8, 226u8,
							51u8, 127u8, 30u8, 233u8, 202u8, 85u8, 223u8, 196u8, 255u8, 150u8,
							86u8,
						],
					)
				}
				pub fn cardano_block_window_size(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::cardano_block_window_size::CardanoBlockWindowSize,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"CardanoBlockWindowSize",
						(),
						[
							85u8, 124u8, 54u8, 148u8, 158u8, 68u8, 21u8, 158u8, 90u8, 174u8, 134u8,
							8u8, 251u8, 139u8, 250u8, 191u8, 166u8, 55u8, 238u8, 23u8, 59u8, 12u8,
							173u8, 164u8, 110u8, 232u8, 32u8, 218u8, 172u8, 228u8, 85u8, 208u8,
						],
					)
				}
				#[doc = " Max amount of Cardano transactions that can be processed per block"]
				pub fn cardano_tx_capacity_per_block(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::cardano_tx_capacity_per_block::CardanoTxCapacityPerBlock,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"CardanoTxCapacityPerBlock",
						(),
						[
							81u8, 165u8, 152u8, 127u8, 105u8, 140u8, 214u8, 89u8, 18u8, 139u8,
							85u8, 98u8, 199u8, 107u8, 192u8, 237u8, 201u8, 228u8, 39u8, 211u8,
							236u8, 119u8, 92u8, 234u8, 147u8, 213u8, 138u8, 6u8, 168u8, 12u8,
							230u8, 37u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				pub fn max_registrations_per_cardano_address(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"NativeTokenObservation",
						"MaxRegistrationsPerCardanoAddress",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
			}
		}
	}
	pub mod preimage {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_preimage::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_preimage::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Register a preimage on-chain."]
				#[doc = ""]
				#[doc = "If the preimage was previously requested, no fees or deposits are taken for providing"]
				#[doc = "the preimage. Otherwise, a deposit is taken proportional to the size of the preimage."]
				pub struct NotePreimage {
					pub bytes: note_preimage::Bytes,
				}
				pub mod note_preimage {
					use super::runtime_types;
					pub type Bytes =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for NotePreimage {
					const PALLET: &'static str = "Preimage";
					const CALL: &'static str = "note_preimage";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Clear an unrequested preimage from the runtime storage."]
				#[doc = ""]
				#[doc = "If `len` is provided, then it will be a much cheaper operation."]
				#[doc = ""]
				#[doc = "- `hash`: The hash of the preimage to be removed from the store."]
				#[doc = "- `len`: The length of the preimage of `hash`."]
				pub struct UnnotePreimage {
					pub hash: unnote_preimage::Hash,
				}
				pub mod unnote_preimage {
					use super::runtime_types;
					pub type Hash = ::subxt::ext::subxt_core::utils::H256;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for UnnotePreimage {
					const PALLET: &'static str = "Preimage";
					const CALL: &'static str = "unnote_preimage";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Request a preimage be uploaded to the chain without paying any fees or deposits."]
				#[doc = ""]
				#[doc = "If the preimage requests has already been provided on-chain, we unreserve any deposit"]
				#[doc = "a user may have paid, and take the control of the preimage out of their hands."]
				pub struct RequestPreimage {
					pub hash: request_preimage::Hash,
				}
				pub mod request_preimage {
					use super::runtime_types;
					pub type Hash = ::subxt::ext::subxt_core::utils::H256;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for RequestPreimage {
					const PALLET: &'static str = "Preimage";
					const CALL: &'static str = "request_preimage";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Clear a previously made request for a preimage."]
				#[doc = ""]
				#[doc = "NOTE: THIS MUST NOT BE CALLED ON `hash` MORE TIMES THAN `request_preimage`."]
				pub struct UnrequestPreimage {
					pub hash: unrequest_preimage::Hash,
				}
				pub mod unrequest_preimage {
					use super::runtime_types;
					pub type Hash = ::subxt::ext::subxt_core::utils::H256;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for UnrequestPreimage {
					const PALLET: &'static str = "Preimage";
					const CALL: &'static str = "unrequest_preimage";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Ensure that the bulk of pre-images is upgraded."]
				#[doc = ""]
				#[doc = "The caller pays no fee if at least 90% of pre-images were successfully updated."]
				pub struct EnsureUpdated {
					pub hashes: ensure_updated::Hashes,
				}
				pub mod ensure_updated {
					use super::runtime_types;
					pub type Hashes = ::subxt::ext::subxt_core::alloc::vec::Vec<
						::subxt::ext::subxt_core::utils::H256,
					>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for EnsureUpdated {
					const PALLET: &'static str = "Preimage";
					const CALL: &'static str = "ensure_updated";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Register a preimage on-chain."]
				#[doc = ""]
				#[doc = "If the preimage was previously requested, no fees or deposits are taken for providing"]
				#[doc = "the preimage. Otherwise, a deposit is taken proportional to the size of the preimage."]
				pub fn note_preimage(
					&self,
					bytes: types::note_preimage::Bytes,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::NotePreimage> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Preimage",
						"note_preimage",
						types::NotePreimage { bytes },
						[
							121u8, 88u8, 18u8, 92u8, 176u8, 15u8, 192u8, 198u8, 146u8, 198u8, 38u8,
							242u8, 213u8, 83u8, 7u8, 230u8, 14u8, 110u8, 235u8, 32u8, 215u8, 26u8,
							192u8, 217u8, 113u8, 224u8, 206u8, 96u8, 177u8, 198u8, 246u8, 33u8,
						],
					)
				}
				#[doc = "Clear an unrequested preimage from the runtime storage."]
				#[doc = ""]
				#[doc = "If `len` is provided, then it will be a much cheaper operation."]
				#[doc = ""]
				#[doc = "- `hash`: The hash of the preimage to be removed from the store."]
				#[doc = "- `len`: The length of the preimage of `hash`."]
				pub fn unnote_preimage(
					&self,
					hash: types::unnote_preimage::Hash,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::UnnotePreimage>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Preimage",
						"unnote_preimage",
						types::UnnotePreimage { hash },
						[
							188u8, 116u8, 222u8, 22u8, 127u8, 215u8, 2u8, 133u8, 96u8, 202u8,
							190u8, 123u8, 203u8, 43u8, 200u8, 161u8, 226u8, 24u8, 49u8, 36u8,
							221u8, 160u8, 130u8, 119u8, 30u8, 138u8, 144u8, 85u8, 5u8, 164u8,
							252u8, 222u8,
						],
					)
				}
				#[doc = "Request a preimage be uploaded to the chain without paying any fees or deposits."]
				#[doc = ""]
				#[doc = "If the preimage requests has already been provided on-chain, we unreserve any deposit"]
				#[doc = "a user may have paid, and take the control of the preimage out of their hands."]
				pub fn request_preimage(
					&self,
					hash: types::request_preimage::Hash,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::RequestPreimage>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Preimage",
						"request_preimage",
						types::RequestPreimage { hash },
						[
							87u8, 0u8, 204u8, 111u8, 43u8, 115u8, 64u8, 209u8, 133u8, 13u8, 83u8,
							45u8, 164u8, 166u8, 233u8, 105u8, 242u8, 238u8, 235u8, 208u8, 113u8,
							134u8, 93u8, 242u8, 86u8, 32u8, 7u8, 152u8, 107u8, 208u8, 79u8, 59u8,
						],
					)
				}
				#[doc = "Clear a previously made request for a preimage."]
				#[doc = ""]
				#[doc = "NOTE: THIS MUST NOT BE CALLED ON `hash` MORE TIMES THAN `request_preimage`."]
				pub fn unrequest_preimage(
					&self,
					hash: types::unrequest_preimage::Hash,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::UnrequestPreimage>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Preimage",
						"unrequest_preimage",
						types::UnrequestPreimage { hash },
						[
							55u8, 37u8, 224u8, 149u8, 142u8, 120u8, 8u8, 68u8, 183u8, 225u8, 255u8,
							240u8, 254u8, 111u8, 58u8, 200u8, 113u8, 217u8, 177u8, 203u8, 107u8,
							104u8, 233u8, 87u8, 252u8, 53u8, 33u8, 112u8, 116u8, 254u8, 117u8,
							134u8,
						],
					)
				}
				#[doc = "Ensure that the bulk of pre-images is upgraded."]
				#[doc = ""]
				#[doc = "The caller pays no fee if at least 90% of pre-images were successfully updated."]
				pub fn ensure_updated(
					&self,
					hashes: types::ensure_updated::Hashes,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::EnsureUpdated>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Preimage",
						"ensure_updated",
						types::EnsureUpdated { hashes },
						[
							254u8, 228u8, 88u8, 44u8, 126u8, 235u8, 188u8, 153u8, 61u8, 27u8,
							103u8, 253u8, 163u8, 161u8, 113u8, 243u8, 87u8, 136u8, 2u8, 231u8,
							209u8, 188u8, 215u8, 106u8, 192u8, 225u8, 75u8, 125u8, 224u8, 96u8,
							221u8, 90u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_preimage::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A preimage has been noted."]
			pub struct Noted {
				pub hash: noted::Hash,
			}
			pub mod noted {
				use super::runtime_types;
				pub type Hash = ::subxt::ext::subxt_core::utils::H256;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Noted {
				const PALLET: &'static str = "Preimage";
				const EVENT: &'static str = "Noted";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A preimage has been requested."]
			pub struct Requested {
				pub hash: requested::Hash,
			}
			pub mod requested {
				use super::runtime_types;
				pub type Hash = ::subxt::ext::subxt_core::utils::H256;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Requested {
				const PALLET: &'static str = "Preimage";
				const EVENT: &'static str = "Requested";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A preimage has ben cleared."]
			pub struct Cleared {
				pub hash: cleared::Hash,
			}
			pub mod cleared {
				use super::runtime_types;
				pub type Hash = ::subxt::ext::subxt_core::utils::H256;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Cleared {
				const PALLET: &'static str = "Preimage";
				const EVENT: &'static str = "Cleared";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod status_for {
					use super::runtime_types;
					pub type StatusFor = runtime_types::pallet_preimage::OldRequestStatus<
						::subxt::ext::subxt_core::utils::AccountId32,
						::core::primitive::u128,
					>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::H256;
				}
				pub mod request_status_for {
					use super::runtime_types;
					pub type RequestStatusFor = runtime_types::pallet_preimage::RequestStatus<
						::subxt::ext::subxt_core::utils::AccountId32,
						(),
					>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::H256;
				}
				pub mod preimage_for {
					use super::runtime_types;
					pub type PreimageFor =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>;
					pub type Param0 = ::subxt::ext::subxt_core::utils::H256;
					pub type Param1 = ::core::primitive::u32;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The request status of a given hash."]
				pub fn status_for_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::status_for::StatusFor,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Preimage",
						"StatusFor",
						(),
						[
							187u8, 100u8, 54u8, 112u8, 96u8, 129u8, 36u8, 149u8, 127u8, 226u8,
							126u8, 171u8, 72u8, 189u8, 59u8, 126u8, 204u8, 125u8, 67u8, 204u8,
							231u8, 6u8, 212u8, 135u8, 166u8, 252u8, 5u8, 46u8, 111u8, 120u8, 54u8,
							209u8,
						],
					)
				}
				#[doc = " The request status of a given hash."]
				pub fn status_for(
					&self,
					_0: impl ::core::borrow::Borrow<types::status_for::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::status_for::Param0,
					>,
					types::status_for::StatusFor,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Preimage",
						"StatusFor",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							187u8, 100u8, 54u8, 112u8, 96u8, 129u8, 36u8, 149u8, 127u8, 226u8,
							126u8, 171u8, 72u8, 189u8, 59u8, 126u8, 204u8, 125u8, 67u8, 204u8,
							231u8, 6u8, 212u8, 135u8, 166u8, 252u8, 5u8, 46u8, 111u8, 120u8, 54u8,
							209u8,
						],
					)
				}
				#[doc = " The request status of a given hash."]
				pub fn request_status_for_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::request_status_for::RequestStatusFor,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Preimage",
						"RequestStatusFor",
						(),
						[
							60u8, 36u8, 88u8, 121u8, 15u8, 71u8, 245u8, 91u8, 235u8, 58u8, 109u8,
							17u8, 249u8, 135u8, 4u8, 132u8, 170u8, 173u8, 142u8, 101u8, 167u8,
							86u8, 125u8, 175u8, 4u8, 54u8, 226u8, 173u8, 20u8, 39u8, 242u8, 96u8,
						],
					)
				}
				#[doc = " The request status of a given hash."]
				pub fn request_status_for(
					&self,
					_0: impl ::core::borrow::Borrow<types::request_status_for::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::request_status_for::Param0,
					>,
					types::request_status_for::RequestStatusFor,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Preimage",
						"RequestStatusFor",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							60u8, 36u8, 88u8, 121u8, 15u8, 71u8, 245u8, 91u8, 235u8, 58u8, 109u8,
							17u8, 249u8, 135u8, 4u8, 132u8, 170u8, 173u8, 142u8, 101u8, 167u8,
							86u8, 125u8, 175u8, 4u8, 54u8, 226u8, 173u8, 20u8, 39u8, 242u8, 96u8,
						],
					)
				}
				pub fn preimage_for_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::preimage_for::PreimageFor,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Preimage",
						"PreimageFor",
						(),
						[
							106u8, 5u8, 17u8, 46u8, 6u8, 184u8, 177u8, 113u8, 169u8, 34u8, 119u8,
							141u8, 117u8, 40u8, 30u8, 94u8, 187u8, 35u8, 206u8, 216u8, 143u8,
							208u8, 49u8, 156u8, 200u8, 255u8, 109u8, 200u8, 210u8, 134u8, 24u8,
							139u8,
						],
					)
				}
				pub fn preimage_for_iter1(
					&self,
					_0: impl ::core::borrow::Borrow<types::preimage_for::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::preimage_for::Param0,
					>,
					types::preimage_for::PreimageFor,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Preimage",
						"PreimageFor",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							106u8, 5u8, 17u8, 46u8, 6u8, 184u8, 177u8, 113u8, 169u8, 34u8, 119u8,
							141u8, 117u8, 40u8, 30u8, 94u8, 187u8, 35u8, 206u8, 216u8, 143u8,
							208u8, 49u8, 156u8, 200u8, 255u8, 109u8, 200u8, 210u8, 134u8, 24u8,
							139u8,
						],
					)
				}
				pub fn preimage_for(
					&self,
					_0: impl ::core::borrow::Borrow<types::preimage_for::Param0>,
					_1: impl ::core::borrow::Borrow<types::preimage_for::Param1>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(
						::subxt::ext::subxt_core::storage::address::StaticStorageKey<
							types::preimage_for::Param0,
						>,
						::subxt::ext::subxt_core::storage::address::StaticStorageKey<
							types::preimage_for::Param1,
						>,
					),
					types::preimage_for::PreimageFor,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Preimage",
						"PreimageFor",
						(
							::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
								_0.borrow(),
							),
							::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
								_1.borrow(),
							),
						),
						[
							106u8, 5u8, 17u8, 46u8, 6u8, 184u8, 177u8, 113u8, 169u8, 34u8, 119u8,
							141u8, 117u8, 40u8, 30u8, 94u8, 187u8, 35u8, 206u8, 216u8, 143u8,
							208u8, 49u8, 156u8, 200u8, 255u8, 109u8, 200u8, 210u8, 134u8, 24u8,
							139u8,
						],
					)
				}
			}
		}
	}
	pub mod multi_block_migrations {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_migrations::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_migrations::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Allows root to set a cursor to forcefully start, stop or forward the migration process."]
				#[doc = ""]
				#[doc = "Should normally not be needed and is only in place as emergency measure. Note that"]
				#[doc = "restarting the migration process in this manner will not call the"]
				#[doc = "[`MigrationStatusHandler::started`] hook or emit an `UpgradeStarted` event."]
				pub struct ForceSetCursor {
					pub cursor: force_set_cursor::Cursor,
				}
				pub mod force_set_cursor {
					use super::runtime_types;
					pub type Cursor = ::core::option::Option<
						runtime_types::pallet_migrations::MigrationCursor<
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
							::core::primitive::u32,
						>,
					>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ForceSetCursor {
					const PALLET: &'static str = "MultiBlockMigrations";
					const CALL: &'static str = "force_set_cursor";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Allows root to set an active cursor to forcefully start/forward the migration process."]
				#[doc = ""]
				#[doc = "This is an edge-case version of [`Self::force_set_cursor`] that allows to set the"]
				#[doc = "`started_at` value to the next block number. Otherwise this would not be possible, since"]
				#[doc = "`force_set_cursor` takes an absolute block number. Setting `started_at` to `None`"]
				#[doc = "indicates that the current block number plus one should be used."]
				pub struct ForceSetActiveCursor {
					pub index: force_set_active_cursor::Index,
					pub inner_cursor: force_set_active_cursor::InnerCursor,
					pub started_at: force_set_active_cursor::StartedAt,
				}
				pub mod force_set_active_cursor {
					use super::runtime_types;
					pub type Index = ::core::primitive::u32;
					pub type InnerCursor = ::core::option::Option<
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
					>;
					pub type StartedAt = ::core::option::Option<::core::primitive::u32>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ForceSetActiveCursor {
					const PALLET: &'static str = "MultiBlockMigrations";
					const CALL: &'static str = "force_set_active_cursor";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Forces the onboarding of the migrations."]
				#[doc = ""]
				#[doc = "This process happens automatically on a runtime upgrade. It is in place as an emergency"]
				#[doc = "measurement. The cursor needs to be `None` for this to succeed."]
				pub struct ForceOnboardMbms;
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ForceOnboardMbms {
					const PALLET: &'static str = "MultiBlockMigrations";
					const CALL: &'static str = "force_onboard_mbms";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Clears the `Historic` set."]
				#[doc = ""]
				#[doc = "`map_cursor` must be set to the last value that was returned by the"]
				#[doc = "`HistoricCleared` event. The first time `None` can be used. `limit` must be chosen in a"]
				#[doc = "way that will result in a sensible weight."]
				pub struct ClearHistoric {
					pub selector: clear_historic::Selector,
				}
				pub mod clear_historic {
					use super::runtime_types;
					pub type Selector = runtime_types::pallet_migrations::HistoricCleanupSelector<
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
					>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ClearHistoric {
					const PALLET: &'static str = "MultiBlockMigrations";
					const CALL: &'static str = "clear_historic";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Allows root to set a cursor to forcefully start, stop or forward the migration process."]
				#[doc = ""]
				#[doc = "Should normally not be needed and is only in place as emergency measure. Note that"]
				#[doc = "restarting the migration process in this manner will not call the"]
				#[doc = "[`MigrationStatusHandler::started`] hook or emit an `UpgradeStarted` event."]
				pub fn force_set_cursor(
					&self,
					cursor: types::force_set_cursor::Cursor,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ForceSetCursor>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"MultiBlockMigrations",
						"force_set_cursor",
						types::ForceSetCursor { cursor },
						[
							19u8, 44u8, 153u8, 100u8, 215u8, 46u8, 69u8, 184u8, 161u8, 215u8, 80u8,
							229u8, 65u8, 166u8, 63u8, 102u8, 97u8, 214u8, 112u8, 219u8, 227u8,
							213u8, 16u8, 50u8, 246u8, 54u8, 69u8, 172u8, 105u8, 216u8, 197u8,
							184u8,
						],
					)
				}
				#[doc = "Allows root to set an active cursor to forcefully start/forward the migration process."]
				#[doc = ""]
				#[doc = "This is an edge-case version of [`Self::force_set_cursor`] that allows to set the"]
				#[doc = "`started_at` value to the next block number. Otherwise this would not be possible, since"]
				#[doc = "`force_set_cursor` takes an absolute block number. Setting `started_at` to `None`"]
				#[doc = "indicates that the current block number plus one should be used."]
				pub fn force_set_active_cursor(
					&self,
					index: types::force_set_active_cursor::Index,
					inner_cursor: types::force_set_active_cursor::InnerCursor,
					started_at: types::force_set_active_cursor::StartedAt,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ForceSetActiveCursor>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"MultiBlockMigrations",
						"force_set_active_cursor",
						types::ForceSetActiveCursor { index, inner_cursor, started_at },
						[
							171u8, 245u8, 214u8, 230u8, 7u8, 61u8, 186u8, 60u8, 43u8, 235u8, 110u8,
							188u8, 30u8, 215u8, 48u8, 194u8, 249u8, 215u8, 224u8, 229u8, 89u8,
							179u8, 189u8, 70u8, 62u8, 162u8, 245u8, 199u8, 3u8, 234u8, 154u8, 9u8,
						],
					)
				}
				#[doc = "Forces the onboarding of the migrations."]
				#[doc = ""]
				#[doc = "This process happens automatically on a runtime upgrade. It is in place as an emergency"]
				#[doc = "measurement. The cursor needs to be `None` for this to succeed."]
				pub fn force_onboard_mbms(
					&self,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ForceOnboardMbms>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"MultiBlockMigrations",
						"force_onboard_mbms",
						types::ForceOnboardMbms {},
						[
							201u8, 148u8, 226u8, 128u8, 88u8, 120u8, 31u8, 159u8, 78u8, 5u8, 148u8,
							189u8, 71u8, 195u8, 136u8, 24u8, 131u8, 107u8, 56u8, 17u8, 26u8, 25u8,
							189u8, 102u8, 220u8, 148u8, 44u8, 92u8, 79u8, 232u8, 236u8, 86u8,
						],
					)
				}
				#[doc = "Clears the `Historic` set."]
				#[doc = ""]
				#[doc = "`map_cursor` must be set to the last value that was returned by the"]
				#[doc = "`HistoricCleared` event. The first time `None` can be used. `limit` must be chosen in a"]
				#[doc = "way that will result in a sensible weight."]
				pub fn clear_historic(
					&self,
					selector: types::clear_historic::Selector,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ClearHistoric>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"MultiBlockMigrations",
						"clear_historic",
						types::ClearHistoric { selector },
						[
							160u8, 68u8, 166u8, 164u8, 115u8, 190u8, 214u8, 167u8, 171u8, 238u8,
							176u8, 191u8, 30u8, 228u8, 237u8, 125u8, 183u8, 147u8, 186u8, 246u8,
							59u8, 186u8, 40u8, 193u8, 13u8, 252u8, 99u8, 199u8, 207u8, 149u8, 20u8,
							95u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_migrations::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A Runtime upgrade started."]
			#[doc = ""]
			#[doc = "Its end is indicated by `UpgradeCompleted` or `UpgradeFailed`."]
			pub struct UpgradeStarted {
				pub migrations: upgrade_started::Migrations,
			}
			pub mod upgrade_started {
				use super::runtime_types;
				pub type Migrations = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for UpgradeStarted {
				const PALLET: &'static str = "MultiBlockMigrations";
				const EVENT: &'static str = "UpgradeStarted";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The current runtime upgrade completed."]
			#[doc = ""]
			#[doc = "This implies that all of its migrations completed successfully as well."]
			pub struct UpgradeCompleted;
			impl ::subxt::ext::subxt_core::events::StaticEvent for UpgradeCompleted {
				const PALLET: &'static str = "MultiBlockMigrations";
				const EVENT: &'static str = "UpgradeCompleted";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Runtime upgrade failed."]
			#[doc = ""]
			#[doc = "This is very bad and will require governance intervention."]
			pub struct UpgradeFailed;
			impl ::subxt::ext::subxt_core::events::StaticEvent for UpgradeFailed {
				const PALLET: &'static str = "MultiBlockMigrations";
				const EVENT: &'static str = "UpgradeFailed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A migration was skipped since it was already executed in the past."]
			pub struct MigrationSkipped {
				pub index: migration_skipped::Index,
			}
			pub mod migration_skipped {
				use super::runtime_types;
				pub type Index = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for MigrationSkipped {
				const PALLET: &'static str = "MultiBlockMigrations";
				const EVENT: &'static str = "MigrationSkipped";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A migration progressed."]
			pub struct MigrationAdvanced {
				pub index: migration_advanced::Index,
				pub took: migration_advanced::Took,
			}
			pub mod migration_advanced {
				use super::runtime_types;
				pub type Index = ::core::primitive::u32;
				pub type Took = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for MigrationAdvanced {
				const PALLET: &'static str = "MultiBlockMigrations";
				const EVENT: &'static str = "MigrationAdvanced";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A Migration completed."]
			pub struct MigrationCompleted {
				pub index: migration_completed::Index,
				pub took: migration_completed::Took,
			}
			pub mod migration_completed {
				use super::runtime_types;
				pub type Index = ::core::primitive::u32;
				pub type Took = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for MigrationCompleted {
				const PALLET: &'static str = "MultiBlockMigrations";
				const EVENT: &'static str = "MigrationCompleted";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "A Migration failed."]
			#[doc = ""]
			#[doc = "This implies that the whole upgrade failed and governance intervention is required."]
			pub struct MigrationFailed {
				pub index: migration_failed::Index,
				pub took: migration_failed::Took,
			}
			pub mod migration_failed {
				use super::runtime_types;
				pub type Index = ::core::primitive::u32;
				pub type Took = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for MigrationFailed {
				const PALLET: &'static str = "MultiBlockMigrations";
				const EVENT: &'static str = "MigrationFailed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The set of historical migrations has been cleared."]
			pub struct HistoricCleared {
				pub next_cursor: historic_cleared::NextCursor,
			}
			pub mod historic_cleared {
				use super::runtime_types;
				pub type NextCursor = ::core::option::Option<
					::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for HistoricCleared {
				const PALLET: &'static str = "MultiBlockMigrations";
				const EVENT: &'static str = "HistoricCleared";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod cursor {
					use super::runtime_types;
					pub type Cursor = runtime_types::pallet_migrations::MigrationCursor<
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
						::core::primitive::u32,
					>;
				}
				pub mod historic {
					use super::runtime_types;
					pub type Historic = ();
					pub type Param0 = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The currently active migration to run and its cursor."]
				#[doc = ""]
				#[doc = " `None` indicates that no migration is running."]
				pub fn cursor(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::cursor::Cursor,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"MultiBlockMigrations",
						"Cursor",
						(),
						[
							158u8, 123u8, 0u8, 37u8, 29u8, 141u8, 51u8, 91u8, 165u8, 167u8, 35u8,
							72u8, 219u8, 39u8, 135u8, 23u8, 215u8, 63u8, 127u8, 57u8, 55u8, 169u8,
							178u8, 177u8, 77u8, 209u8, 230u8, 152u8, 136u8, 145u8, 58u8, 33u8,
						],
					)
				}
				#[doc = " Set of all successfully executed migrations."]
				#[doc = ""]
				#[doc = " This is used as blacklist, to not re-execute migrations that have not been removed from the"]
				#[doc = " codebase yet. Governance can regularly clear this out via `clear_historic`."]
				pub fn historic_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::historic::Historic,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"MultiBlockMigrations",
						"Historic",
						(),
						[
							142u8, 100u8, 97u8, 146u8, 174u8, 131u8, 203u8, 224u8, 127u8, 74u8,
							118u8, 165u8, 237u8, 128u8, 89u8, 198u8, 80u8, 250u8, 231u8, 29u8, 1u8,
							128u8, 201u8, 51u8, 150u8, 168u8, 3u8, 17u8, 35u8, 175u8, 153u8, 141u8,
						],
					)
				}
				#[doc = " Set of all successfully executed migrations."]
				#[doc = ""]
				#[doc = " This is used as blacklist, to not re-execute migrations that have not been removed from the"]
				#[doc = " codebase yet. Governance can regularly clear this out via `clear_historic`."]
				pub fn historic(
					&self,
					_0: impl ::core::borrow::Borrow<types::historic::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::historic::Param0,
					>,
					types::historic::Historic,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"MultiBlockMigrations",
						"Historic",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							142u8, 100u8, 97u8, 146u8, 174u8, 131u8, 203u8, 224u8, 127u8, 74u8,
							118u8, 165u8, 237u8, 128u8, 89u8, 198u8, 80u8, 250u8, 231u8, 29u8, 1u8,
							128u8, 201u8, 51u8, 150u8, 168u8, 3u8, 17u8, 35u8, 175u8, 153u8, 141u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " The maximal length of an encoded cursor."]
				#[doc = ""]
				#[doc = " A good default needs to selected such that no migration will ever have a cursor with MEL"]
				#[doc = " above this limit. This is statically checked in `integrity_test`."]
				pub fn cursor_max_len(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"MultiBlockMigrations",
						"CursorMaxLen",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " The maximal length of an encoded identifier."]
				#[doc = ""]
				#[doc = " A good default needs to selected such that no migration will ever have an identifier"]
				#[doc = " with MEL above this limit. This is statically checked in `integrity_test`."]
				pub fn identifier_max_len(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"MultiBlockMigrations",
						"IdentifierMaxLen",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
			}
		}
	}
	pub mod pallet_session {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "Error for the session pallet."]
		pub type Error = runtime_types::pallet_session::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_session::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Sets the session key(s) of the function caller to `keys`."]
				#[doc = "Allows an account to set its session key prior to becoming a validator."]
				#[doc = "This doesn't take effect until the next session."]
				#[doc = ""]
				#[doc = "The dispatch origin of this function must be signed."]
				#[doc = ""]
				#[doc = "## Complexity"]
				#[doc = "- `O(1)`. Actual cost depends on the number of length of `T::Keys::key_ids()` which is"]
				#[doc = "  fixed."]
				pub struct SetKeys {
					pub keys: set_keys::Keys,
					pub proof: set_keys::Proof,
				}
				pub mod set_keys {
					use super::runtime_types;
					pub type Keys = runtime_types::midnight_node_runtime::opaque::SessionKeys;
					pub type Proof =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetKeys {
					const PALLET: &'static str = "PalletSession";
					const CALL: &'static str = "set_keys";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Removes any session key(s) of the function caller."]
				#[doc = ""]
				#[doc = "This doesn't take effect until the next session."]
				#[doc = ""]
				#[doc = "The dispatch origin of this function must be Signed and the account must be either be"]
				#[doc = "convertible to a validator ID using the chain's typical addressing system (this usually"]
				#[doc = "means being a controller account) or directly convertible into a validator ID (which"]
				#[doc = "usually means being a stash account)."]
				#[doc = ""]
				#[doc = "## Complexity"]
				#[doc = "- `O(1)` in number of key types. Actual cost depends on the number of length of"]
				#[doc = "  `T::Keys::key_ids()` which is fixed."]
				pub struct PurgeKeys;
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for PurgeKeys {
					const PALLET: &'static str = "PalletSession";
					const CALL: &'static str = "purge_keys";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Sets the session key(s) of the function caller to `keys`."]
				#[doc = "Allows an account to set its session key prior to becoming a validator."]
				#[doc = "This doesn't take effect until the next session."]
				#[doc = ""]
				#[doc = "The dispatch origin of this function must be signed."]
				#[doc = ""]
				#[doc = "## Complexity"]
				#[doc = "- `O(1)`. Actual cost depends on the number of length of `T::Keys::key_ids()` which is"]
				#[doc = "  fixed."]
				pub fn set_keys(
					&self,
					keys: types::set_keys::Keys,
					proof: types::set_keys::Proof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetKeys> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"PalletSession",
						"set_keys",
						types::SetKeys { keys, proof },
						[
							48u8, 188u8, 0u8, 161u8, 245u8, 138u8, 159u8, 146u8, 33u8, 52u8, 30u8,
							20u8, 17u8, 25u8, 176u8, 221u8, 45u8, 5u8, 84u8, 75u8, 108u8, 142u8,
							120u8, 102u8, 70u8, 9u8, 199u8, 158u8, 39u8, 155u8, 125u8, 28u8,
						],
					)
				}
				#[doc = "Removes any session key(s) of the function caller."]
				#[doc = ""]
				#[doc = "This doesn't take effect until the next session."]
				#[doc = ""]
				#[doc = "The dispatch origin of this function must be Signed and the account must be either be"]
				#[doc = "convertible to a validator ID using the chain's typical addressing system (this usually"]
				#[doc = "means being a controller account) or directly convertible into a validator ID (which"]
				#[doc = "usually means being a stash account)."]
				#[doc = ""]
				#[doc = "## Complexity"]
				#[doc = "- `O(1)` in number of key types. Actual cost depends on the number of length of"]
				#[doc = "  `T::Keys::key_ids()` which is fixed."]
				pub fn purge_keys(
					&self,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::PurgeKeys> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"PalletSession",
						"purge_keys",
						types::PurgeKeys {},
						[
							215u8, 204u8, 146u8, 236u8, 32u8, 78u8, 198u8, 79u8, 85u8, 214u8, 15u8,
							151u8, 158u8, 31u8, 146u8, 119u8, 119u8, 204u8, 151u8, 169u8, 226u8,
							67u8, 217u8, 39u8, 241u8, 245u8, 203u8, 240u8, 203u8, 172u8, 16u8,
							209u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_session::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "New session has happened. Note that the argument is the session index, not the"]
			#[doc = "block number as the type might suggest."]
			pub struct NewSession {
				pub session_index: new_session::SessionIndex,
			}
			pub mod new_session {
				use super::runtime_types;
				pub type SessionIndex = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for NewSession {
				const PALLET: &'static str = "PalletSession";
				const EVENT: &'static str = "NewSession";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Validator has been disabled."]
			pub struct ValidatorDisabled {
				pub validator: validator_disabled::Validator,
			}
			pub mod validator_disabled {
				use super::runtime_types;
				pub type Validator = ::subxt::ext::subxt_core::utils::AccountId32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ValidatorDisabled {
				const PALLET: &'static str = "PalletSession";
				const EVENT: &'static str = "ValidatorDisabled";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Validator has been re-enabled."]
			pub struct ValidatorReenabled {
				pub validator: validator_reenabled::Validator,
			}
			pub mod validator_reenabled {
				use super::runtime_types;
				pub type Validator = ::subxt::ext::subxt_core::utils::AccountId32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for ValidatorReenabled {
				const PALLET: &'static str = "PalletSession";
				const EVENT: &'static str = "ValidatorReenabled";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod validators {
					use super::runtime_types;
					pub type Validators = ::subxt::ext::subxt_core::alloc::vec::Vec<
						::subxt::ext::subxt_core::utils::AccountId32,
					>;
				}
				pub mod current_index {
					use super::runtime_types;
					pub type CurrentIndex = ::core::primitive::u32;
				}
				pub mod queued_changed {
					use super::runtime_types;
					pub type QueuedChanged = ::core::primitive::bool;
				}
				pub mod queued_keys {
					use super::runtime_types;
					pub type QueuedKeys = ::subxt::ext::subxt_core::alloc::vec::Vec<(
						::subxt::ext::subxt_core::utils::AccountId32,
						runtime_types::midnight_node_runtime::opaque::SessionKeys,
					)>;
				}
				pub mod disabled_validators {
					use super::runtime_types;
					pub type DisabledValidators = ::subxt::ext::subxt_core::alloc::vec::Vec<(
						::core::primitive::u32,
						runtime_types::sp_staking::offence::OffenceSeverity,
					)>;
				}
				pub mod next_keys {
					use super::runtime_types;
					pub type NextKeys = runtime_types::midnight_node_runtime::opaque::SessionKeys;
					pub type Param0 = ::subxt::ext::subxt_core::utils::AccountId32;
				}
				pub mod key_owner {
					use super::runtime_types;
					pub type KeyOwner = ::subxt::ext::subxt_core::utils::AccountId32;
					pub type Param0 = runtime_types::sp_core::crypto::KeyTypeId;
					pub type Param1 = [::core::primitive::u8];
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The current set of validators."]
				pub fn validators(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::validators::Validators,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"Validators",
						(),
						[
							50u8, 86u8, 154u8, 222u8, 249u8, 209u8, 156u8, 22u8, 155u8, 25u8,
							133u8, 194u8, 210u8, 50u8, 38u8, 28u8, 139u8, 201u8, 90u8, 139u8,
							115u8, 12u8, 12u8, 141u8, 4u8, 178u8, 201u8, 241u8, 223u8, 234u8, 6u8,
							86u8,
						],
					)
				}
				#[doc = " Current index of the session."]
				pub fn current_index(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::current_index::CurrentIndex,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"CurrentIndex",
						(),
						[
							167u8, 151u8, 125u8, 150u8, 159u8, 21u8, 78u8, 217u8, 237u8, 183u8,
							135u8, 65u8, 187u8, 114u8, 188u8, 206u8, 16u8, 32u8, 69u8, 208u8,
							134u8, 159u8, 232u8, 224u8, 243u8, 27u8, 31u8, 166u8, 145u8, 44u8,
							221u8, 230u8,
						],
					)
				}
				#[doc = " True if the underlying economic identities or weighting behind the validators"]
				#[doc = " has changed in the queued validator set."]
				pub fn queued_changed(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::queued_changed::QueuedChanged,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"QueuedChanged",
						(),
						[
							184u8, 137u8, 224u8, 137u8, 31u8, 236u8, 95u8, 164u8, 102u8, 225u8,
							198u8, 227u8, 140u8, 37u8, 113u8, 57u8, 59u8, 4u8, 202u8, 102u8, 117u8,
							36u8, 226u8, 64u8, 113u8, 141u8, 199u8, 111u8, 99u8, 144u8, 198u8,
							153u8,
						],
					)
				}
				#[doc = " The queued keys for the next session. When the next session begins, these keys"]
				#[doc = " will be used to determine the validator's session keys."]
				pub fn queued_keys(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::queued_keys::QueuedKeys,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"QueuedKeys",
						(),
						[
							162u8, 181u8, 111u8, 231u8, 243u8, 120u8, 191u8, 220u8, 10u8, 186u8,
							5u8, 41u8, 158u8, 179u8, 194u8, 79u8, 23u8, 207u8, 237u8, 149u8, 110u8,
							217u8, 252u8, 98u8, 152u8, 64u8, 200u8, 133u8, 97u8, 236u8, 143u8,
							91u8,
						],
					)
				}
				#[doc = " Indices of disabled validators."]
				#[doc = ""]
				#[doc = " The vec is always kept sorted so that we can find whether a given validator is"]
				#[doc = " disabled using binary search. It gets cleared when `on_session_ending` returns"]
				#[doc = " a new set of identities."]
				pub fn disabled_validators(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::disabled_validators::DisabledValidators,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"DisabledValidators",
						(),
						[
							214u8, 48u8, 28u8, 150u8, 143u8, 29u8, 183u8, 40u8, 236u8, 227u8,
							195u8, 5u8, 202u8, 54u8, 184u8, 26u8, 239u8, 237u8, 113u8, 39u8, 200u8,
							111u8, 163u8, 3u8, 24u8, 101u8, 107u8, 91u8, 228u8, 135u8, 12u8, 86u8,
						],
					)
				}
				#[doc = " The next session keys for a validator."]
				pub fn next_keys_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::next_keys::NextKeys,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"NextKeys",
						(),
						[
							165u8, 93u8, 209u8, 105u8, 147u8, 150u8, 218u8, 68u8, 116u8, 100u8,
							43u8, 135u8, 232u8, 100u8, 184u8, 210u8, 169u8, 240u8, 154u8, 169u8,
							28u8, 194u8, 180u8, 162u8, 148u8, 192u8, 113u8, 236u8, 62u8, 99u8,
							148u8, 187u8,
						],
					)
				}
				#[doc = " The next session keys for a validator."]
				pub fn next_keys(
					&self,
					_0: impl ::core::borrow::Borrow<types::next_keys::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::next_keys::Param0,
					>,
					types::next_keys::NextKeys,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"NextKeys",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							165u8, 93u8, 209u8, 105u8, 147u8, 150u8, 218u8, 68u8, 116u8, 100u8,
							43u8, 135u8, 232u8, 100u8, 184u8, 210u8, 169u8, 240u8, 154u8, 169u8,
							28u8, 194u8, 180u8, 162u8, 148u8, 192u8, 113u8, 236u8, 62u8, 99u8,
							148u8, 187u8,
						],
					)
				}
				#[doc = " The owner of a key. The key is the `KeyTypeId` + the encoded key."]
				pub fn key_owner_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::key_owner::KeyOwner,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"KeyOwner",
						(),
						[
							217u8, 204u8, 21u8, 114u8, 247u8, 129u8, 32u8, 242u8, 93u8, 91u8,
							253u8, 253u8, 248u8, 90u8, 12u8, 202u8, 195u8, 25u8, 18u8, 100u8,
							253u8, 109u8, 88u8, 77u8, 217u8, 140u8, 51u8, 40u8, 118u8, 35u8, 107u8,
							206u8,
						],
					)
				}
				#[doc = " The owner of a key. The key is the `KeyTypeId` + the encoded key."]
				pub fn key_owner_iter1(
					&self,
					_0: impl ::core::borrow::Borrow<types::key_owner::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::key_owner::Param0,
					>,
					types::key_owner::KeyOwner,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"KeyOwner",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							217u8, 204u8, 21u8, 114u8, 247u8, 129u8, 32u8, 242u8, 93u8, 91u8,
							253u8, 253u8, 248u8, 90u8, 12u8, 202u8, 195u8, 25u8, 18u8, 100u8,
							253u8, 109u8, 88u8, 77u8, 217u8, 140u8, 51u8, 40u8, 118u8, 35u8, 107u8,
							206u8,
						],
					)
				}
				#[doc = " The owner of a key. The key is the `KeyTypeId` + the encoded key."]
				pub fn key_owner(
					&self,
					_0: impl ::core::borrow::Borrow<types::key_owner::Param0>,
					_1: impl ::core::borrow::Borrow<types::key_owner::Param1>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(
						::subxt::ext::subxt_core::storage::address::StaticStorageKey<
							types::key_owner::Param0,
						>,
						::subxt::ext::subxt_core::storage::address::StaticStorageKey<
							types::key_owner::Param1,
						>,
					),
					types::key_owner::KeyOwner,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"PalletSession",
						"KeyOwner",
						(
							::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
								_0.borrow(),
							),
							::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
								_1.borrow(),
							),
						),
						[
							217u8, 204u8, 21u8, 114u8, 247u8, 129u8, 32u8, 242u8, 93u8, 91u8,
							253u8, 253u8, 248u8, 90u8, 12u8, 202u8, 195u8, 25u8, 18u8, 100u8,
							253u8, 109u8, 88u8, 77u8, 217u8, 140u8, 51u8, 40u8, 118u8, 35u8, 107u8,
							206u8,
						],
					)
				}
			}
		}
	}
	pub mod scheduler {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_scheduler::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_scheduler::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Anonymously schedule a task."]
				pub struct Schedule {
					pub when: schedule::When,
					pub maybe_periodic: schedule::MaybePeriodic,
					pub priority: schedule::Priority,
					pub call: ::subxt::ext::subxt_core::alloc::boxed::Box<schedule::Call>,
				}
				pub mod schedule {
					use super::runtime_types;
					pub type When = ::core::primitive::u32;
					pub type MaybePeriodic =
						::core::option::Option<(::core::primitive::u32, ::core::primitive::u32)>;
					pub type Priority = ::core::primitive::u8;
					pub type Call = runtime_types::midnight_node_runtime::RuntimeCall;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Schedule {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "schedule";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Cancel an anonymously scheduled task."]
				pub struct Cancel {
					pub when: cancel::When,
					pub index: cancel::Index,
				}
				pub mod cancel {
					use super::runtime_types;
					pub type When = ::core::primitive::u32;
					pub type Index = ::core::primitive::u32;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Cancel {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "cancel";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Schedule a named task."]
				pub struct ScheduleNamed {
					pub id: schedule_named::Id,
					pub when: schedule_named::When,
					pub maybe_periodic: schedule_named::MaybePeriodic,
					pub priority: schedule_named::Priority,
					pub call: ::subxt::ext::subxt_core::alloc::boxed::Box<schedule_named::Call>,
				}
				pub mod schedule_named {
					use super::runtime_types;
					pub type Id = [::core::primitive::u8; 32usize];
					pub type When = ::core::primitive::u32;
					pub type MaybePeriodic =
						::core::option::Option<(::core::primitive::u32, ::core::primitive::u32)>;
					pub type Priority = ::core::primitive::u8;
					pub type Call = runtime_types::midnight_node_runtime::RuntimeCall;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ScheduleNamed {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "schedule_named";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Cancel a named scheduled task."]
				pub struct CancelNamed {
					pub id: cancel_named::Id,
				}
				pub mod cancel_named {
					use super::runtime_types;
					pub type Id = [::core::primitive::u8; 32usize];
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for CancelNamed {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "cancel_named";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Anonymously schedule a task after a delay."]
				pub struct ScheduleAfter {
					pub after: schedule_after::After,
					pub maybe_periodic: schedule_after::MaybePeriodic,
					pub priority: schedule_after::Priority,
					pub call: ::subxt::ext::subxt_core::alloc::boxed::Box<schedule_after::Call>,
				}
				pub mod schedule_after {
					use super::runtime_types;
					pub type After = ::core::primitive::u32;
					pub type MaybePeriodic =
						::core::option::Option<(::core::primitive::u32, ::core::primitive::u32)>;
					pub type Priority = ::core::primitive::u8;
					pub type Call = runtime_types::midnight_node_runtime::RuntimeCall;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ScheduleAfter {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "schedule_after";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Schedule a named task after a delay."]
				pub struct ScheduleNamedAfter {
					pub id: schedule_named_after::Id,
					pub after: schedule_named_after::After,
					pub maybe_periodic: schedule_named_after::MaybePeriodic,
					pub priority: schedule_named_after::Priority,
					pub call:
						::subxt::ext::subxt_core::alloc::boxed::Box<schedule_named_after::Call>,
				}
				pub mod schedule_named_after {
					use super::runtime_types;
					pub type Id = [::core::primitive::u8; 32usize];
					pub type After = ::core::primitive::u32;
					pub type MaybePeriodic =
						::core::option::Option<(::core::primitive::u32, ::core::primitive::u32)>;
					pub type Priority = ::core::primitive::u8;
					pub type Call = runtime_types::midnight_node_runtime::RuntimeCall;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ScheduleNamedAfter {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "schedule_named_after";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Set a retry configuration for a task so that, in case its scheduled run fails, it will"]
				#[doc = "be retried after `period` blocks, for a total amount of `retries` retries or until it"]
				#[doc = "succeeds."]
				#[doc = ""]
				#[doc = "Tasks which need to be scheduled for a retry are still subject to weight metering and"]
				#[doc = "agenda space, same as a regular task. If a periodic task fails, it will be scheduled"]
				#[doc = "normally while the task is retrying."]
				#[doc = ""]
				#[doc = "Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic"]
				#[doc = "clones of the original task. Their retry configuration will be derived from the"]
				#[doc = "original task's configuration, but will have a lower value for `remaining` than the"]
				#[doc = "original `total_retries`."]
				pub struct SetRetry {
					pub task: set_retry::Task,
					pub retries: set_retry::Retries,
					pub period: set_retry::Period,
				}
				pub mod set_retry {
					use super::runtime_types;
					pub type Task = (::core::primitive::u32, ::core::primitive::u32);
					pub type Retries = ::core::primitive::u8;
					pub type Period = ::core::primitive::u32;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetRetry {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "set_retry";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Set a retry configuration for a named task so that, in case its scheduled run fails, it"]
				#[doc = "will be retried after `period` blocks, for a total amount of `retries` retries or until"]
				#[doc = "it succeeds."]
				#[doc = ""]
				#[doc = "Tasks which need to be scheduled for a retry are still subject to weight metering and"]
				#[doc = "agenda space, same as a regular task. If a periodic task fails, it will be scheduled"]
				#[doc = "normally while the task is retrying."]
				#[doc = ""]
				#[doc = "Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic"]
				#[doc = "clones of the original task. Their retry configuration will be derived from the"]
				#[doc = "original task's configuration, but will have a lower value for `remaining` than the"]
				#[doc = "original `total_retries`."]
				pub struct SetRetryNamed {
					pub id: set_retry_named::Id,
					pub retries: set_retry_named::Retries,
					pub period: set_retry_named::Period,
				}
				pub mod set_retry_named {
					use super::runtime_types;
					pub type Id = [::core::primitive::u8; 32usize];
					pub type Retries = ::core::primitive::u8;
					pub type Period = ::core::primitive::u32;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetRetryNamed {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "set_retry_named";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Removes the retry configuration of a task."]
				pub struct CancelRetry {
					pub task: cancel_retry::Task,
				}
				pub mod cancel_retry {
					use super::runtime_types;
					pub type Task = (::core::primitive::u32, ::core::primitive::u32);
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for CancelRetry {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "cancel_retry";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Cancel the retry configuration of a named task."]
				pub struct CancelRetryNamed {
					pub id: cancel_retry_named::Id,
				}
				pub mod cancel_retry_named {
					use super::runtime_types;
					pub type Id = [::core::primitive::u8; 32usize];
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for CancelRetryNamed {
					const PALLET: &'static str = "Scheduler";
					const CALL: &'static str = "cancel_retry_named";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Anonymously schedule a task."]
				pub fn schedule(
					&self,
					when: types::schedule::When,
					maybe_periodic: types::schedule::MaybePeriodic,
					priority: types::schedule::Priority,
					call: types::schedule::Call,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Schedule> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"schedule",
						types::Schedule {
							when,
							maybe_periodic,
							priority,
							call: ::subxt::ext::subxt_core::alloc::boxed::Box::new(call),
						},
						[
							65u8, 77u8, 171u8, 35u8, 62u8, 128u8, 133u8, 142u8, 240u8, 99u8, 27u8,
							65u8, 251u8, 156u8, 202u8, 45u8, 139u8, 184u8, 65u8, 229u8, 179u8,
							209u8, 185u8, 149u8, 234u8, 57u8, 86u8, 36u8, 99u8, 187u8, 172u8, 0u8,
						],
					)
				}
				#[doc = "Cancel an anonymously scheduled task."]
				pub fn cancel(
					&self,
					when: types::cancel::When,
					index: types::cancel::Index,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Cancel> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"cancel",
						types::Cancel { when, index },
						[
							183u8, 204u8, 143u8, 86u8, 17u8, 130u8, 132u8, 91u8, 133u8, 168u8,
							103u8, 129u8, 114u8, 56u8, 123u8, 42u8, 123u8, 120u8, 221u8, 211u8,
							26u8, 85u8, 82u8, 246u8, 192u8, 39u8, 254u8, 45u8, 147u8, 56u8, 178u8,
							133u8,
						],
					)
				}
				#[doc = "Schedule a named task."]
				pub fn schedule_named(
					&self,
					id: types::schedule_named::Id,
					when: types::schedule_named::When,
					maybe_periodic: types::schedule_named::MaybePeriodic,
					priority: types::schedule_named::Priority,
					call: types::schedule_named::Call,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ScheduleNamed>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"schedule_named",
						types::ScheduleNamed {
							id,
							when,
							maybe_periodic,
							priority,
							call: ::subxt::ext::subxt_core::alloc::boxed::Box::new(call),
						},
						[
							203u8, 103u8, 46u8, 8u8, 136u8, 35u8, 30u8, 97u8, 195u8, 94u8, 49u8,
							193u8, 102u8, 95u8, 102u8, 201u8, 102u8, 212u8, 222u8, 245u8, 233u8,
							145u8, 192u8, 41u8, 70u8, 2u8, 68u8, 216u8, 6u8, 251u8, 24u8, 185u8,
						],
					)
				}
				#[doc = "Cancel a named scheduled task."]
				pub fn cancel_named(
					&self,
					id: types::cancel_named::Id,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::CancelNamed> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"cancel_named",
						types::CancelNamed { id },
						[
							205u8, 35u8, 28u8, 57u8, 224u8, 7u8, 49u8, 233u8, 236u8, 163u8, 93u8,
							236u8, 103u8, 69u8, 65u8, 51u8, 121u8, 84u8, 9u8, 196u8, 147u8, 122u8,
							227u8, 200u8, 181u8, 233u8, 62u8, 240u8, 174u8, 83u8, 129u8, 193u8,
						],
					)
				}
				#[doc = "Anonymously schedule a task after a delay."]
				pub fn schedule_after(
					&self,
					after: types::schedule_after::After,
					maybe_periodic: types::schedule_after::MaybePeriodic,
					priority: types::schedule_after::Priority,
					call: types::schedule_after::Call,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ScheduleAfter>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"schedule_after",
						types::ScheduleAfter {
							after,
							maybe_periodic,
							priority,
							call: ::subxt::ext::subxt_core::alloc::boxed::Box::new(call),
						},
						[
							116u8, 96u8, 31u8, 91u8, 175u8, 255u8, 240u8, 209u8, 93u8, 233u8,
							178u8, 236u8, 190u8, 26u8, 134u8, 224u8, 152u8, 53u8, 89u8, 150u8,
							33u8, 187u8, 189u8, 140u8, 45u8, 21u8, 6u8, 249u8, 40u8, 123u8, 150u8,
							194u8,
						],
					)
				}
				#[doc = "Schedule a named task after a delay."]
				pub fn schedule_named_after(
					&self,
					id: types::schedule_named_after::Id,
					after: types::schedule_named_after::After,
					maybe_periodic: types::schedule_named_after::MaybePeriodic,
					priority: types::schedule_named_after::Priority,
					call: types::schedule_named_after::Call,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ScheduleNamedAfter>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"schedule_named_after",
						types::ScheduleNamedAfter {
							id,
							after,
							maybe_periodic,
							priority,
							call: ::subxt::ext::subxt_core::alloc::boxed::Box::new(call),
						},
						[
							176u8, 226u8, 49u8, 90u8, 38u8, 156u8, 27u8, 112u8, 16u8, 165u8, 175u8,
							243u8, 182u8, 114u8, 114u8, 201u8, 255u8, 246u8, 152u8, 147u8, 59u8,
							205u8, 4u8, 129u8, 151u8, 129u8, 128u8, 52u8, 121u8, 94u8, 126u8, 33u8,
						],
					)
				}
				#[doc = "Set a retry configuration for a task so that, in case its scheduled run fails, it will"]
				#[doc = "be retried after `period` blocks, for a total amount of `retries` retries or until it"]
				#[doc = "succeeds."]
				#[doc = ""]
				#[doc = "Tasks which need to be scheduled for a retry are still subject to weight metering and"]
				#[doc = "agenda space, same as a regular task. If a periodic task fails, it will be scheduled"]
				#[doc = "normally while the task is retrying."]
				#[doc = ""]
				#[doc = "Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic"]
				#[doc = "clones of the original task. Their retry configuration will be derived from the"]
				#[doc = "original task's configuration, but will have a lower value for `remaining` than the"]
				#[doc = "original `total_retries`."]
				pub fn set_retry(
					&self,
					task: types::set_retry::Task,
					retries: types::set_retry::Retries,
					period: types::set_retry::Period,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetRetry> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"set_retry",
						types::SetRetry { task, retries, period },
						[
							2u8, 242u8, 180u8, 69u8, 237u8, 168u8, 243u8, 93u8, 47u8, 222u8, 189u8,
							74u8, 233u8, 106u8, 54u8, 40u8, 160u8, 61u8, 78u8, 138u8, 232u8, 20u8,
							243u8, 17u8, 151u8, 194u8, 67u8, 200u8, 186u8, 192u8, 210u8, 214u8,
						],
					)
				}
				#[doc = "Set a retry configuration for a named task so that, in case its scheduled run fails, it"]
				#[doc = "will be retried after `period` blocks, for a total amount of `retries` retries or until"]
				#[doc = "it succeeds."]
				#[doc = ""]
				#[doc = "Tasks which need to be scheduled for a retry are still subject to weight metering and"]
				#[doc = "agenda space, same as a regular task. If a periodic task fails, it will be scheduled"]
				#[doc = "normally while the task is retrying."]
				#[doc = ""]
				#[doc = "Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic"]
				#[doc = "clones of the original task. Their retry configuration will be derived from the"]
				#[doc = "original task's configuration, but will have a lower value for `remaining` than the"]
				#[doc = "original `total_retries`."]
				pub fn set_retry_named(
					&self,
					id: types::set_retry_named::Id,
					retries: types::set_retry_named::Retries,
					period: types::set_retry_named::Period,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetRetryNamed>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"set_retry_named",
						types::SetRetryNamed { id, retries, period },
						[
							240u8, 102u8, 255u8, 253u8, 52u8, 81u8, 164u8, 170u8, 184u8, 178u8,
							254u8, 126u8, 41u8, 247u8, 121u8, 22u8, 254u8, 136u8, 237u8, 37u8,
							11u8, 42u8, 227u8, 234u8, 132u8, 83u8, 109u8, 168u8, 31u8, 44u8, 231u8,
							70u8,
						],
					)
				}
				#[doc = "Removes the retry configuration of a task."]
				pub fn cancel_retry(
					&self,
					task: types::cancel_retry::Task,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::CancelRetry> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"cancel_retry",
						types::CancelRetry { task },
						[
							142u8, 126u8, 127u8, 216u8, 64u8, 189u8, 42u8, 126u8, 63u8, 249u8,
							211u8, 202u8, 224u8, 197u8, 199u8, 240u8, 58u8, 94u8, 219u8, 177u8,
							20u8, 210u8, 153u8, 0u8, 127u8, 255u8, 235u8, 238u8, 170u8, 240u8,
							44u8, 49u8,
						],
					)
				}
				#[doc = "Cancel the retry configuration of a named task."]
				pub fn cancel_retry_named(
					&self,
					id: types::cancel_retry_named::Id,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::CancelRetryNamed>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Scheduler",
						"cancel_retry_named",
						types::CancelRetryNamed { id },
						[
							76u8, 157u8, 253u8, 113u8, 162u8, 54u8, 98u8, 21u8, 62u8, 44u8, 155u8,
							202u8, 2u8, 28u8, 153u8, 219u8, 67u8, 166u8, 206u8, 79u8, 139u8, 3u8,
							119u8, 182u8, 254u8, 134u8, 143u8, 121u8, 155u8, 220u8, 192u8, 209u8,
						],
					)
				}
			}
		}
		#[doc = "Events type."]
		pub type Event = runtime_types::pallet_scheduler::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Scheduled some task."]
			pub struct Scheduled {
				pub when: scheduled::When,
				pub index: scheduled::Index,
			}
			pub mod scheduled {
				use super::runtime_types;
				pub type When = ::core::primitive::u32;
				pub type Index = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Scheduled {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "Scheduled";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Canceled some task."]
			pub struct Canceled {
				pub when: canceled::When,
				pub index: canceled::Index,
			}
			pub mod canceled {
				use super::runtime_types;
				pub type When = ::core::primitive::u32;
				pub type Index = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Canceled {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "Canceled";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Dispatched some task."]
			pub struct Dispatched {
				pub task: dispatched::Task,
				pub id: dispatched::Id,
				pub result: dispatched::Result,
			}
			pub mod dispatched {
				use super::runtime_types;
				pub type Task = (::core::primitive::u32, ::core::primitive::u32);
				pub type Id = ::core::option::Option<[::core::primitive::u8; 32usize]>;
				pub type Result =
					::core::result::Result<(), runtime_types::sp_runtime::DispatchError>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for Dispatched {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "Dispatched";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Set a retry configuration for some task."]
			pub struct RetrySet {
				pub task: retry_set::Task,
				pub id: retry_set::Id,
				pub period: retry_set::Period,
				pub retries: retry_set::Retries,
			}
			pub mod retry_set {
				use super::runtime_types;
				pub type Task = (::core::primitive::u32, ::core::primitive::u32);
				pub type Id = ::core::option::Option<[::core::primitive::u8; 32usize]>;
				pub type Period = ::core::primitive::u32;
				pub type Retries = ::core::primitive::u8;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for RetrySet {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "RetrySet";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Cancel a retry configuration for some task."]
			pub struct RetryCancelled {
				pub task: retry_cancelled::Task,
				pub id: retry_cancelled::Id,
			}
			pub mod retry_cancelled {
				use super::runtime_types;
				pub type Task = (::core::primitive::u32, ::core::primitive::u32);
				pub type Id = ::core::option::Option<[::core::primitive::u8; 32usize]>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for RetryCancelled {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "RetryCancelled";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The call for the provided hash was not found so the task has been aborted."]
			pub struct CallUnavailable {
				pub task: call_unavailable::Task,
				pub id: call_unavailable::Id,
			}
			pub mod call_unavailable {
				use super::runtime_types;
				pub type Task = (::core::primitive::u32, ::core::primitive::u32);
				pub type Id = ::core::option::Option<[::core::primitive::u8; 32usize]>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for CallUnavailable {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "CallUnavailable";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The given task was unable to be renewed since the agenda is full at that block."]
			pub struct PeriodicFailed {
				pub task: periodic_failed::Task,
				pub id: periodic_failed::Id,
			}
			pub mod periodic_failed {
				use super::runtime_types;
				pub type Task = (::core::primitive::u32, ::core::primitive::u32);
				pub type Id = ::core::option::Option<[::core::primitive::u8; 32usize]>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for PeriodicFailed {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "PeriodicFailed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The given task was unable to be retried since the agenda is full at that block or there"]
			#[doc = "was not enough weight to reschedule it."]
			pub struct RetryFailed {
				pub task: retry_failed::Task,
				pub id: retry_failed::Id,
			}
			pub mod retry_failed {
				use super::runtime_types;
				pub type Task = (::core::primitive::u32, ::core::primitive::u32);
				pub type Id = ::core::option::Option<[::core::primitive::u8; 32usize]>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for RetryFailed {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "RetryFailed";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "The given task can never be executed since it is overweight."]
			pub struct PermanentlyOverweight {
				pub task: permanently_overweight::Task,
				pub id: permanently_overweight::Id,
			}
			pub mod permanently_overweight {
				use super::runtime_types;
				pub type Task = (::core::primitive::u32, ::core::primitive::u32);
				pub type Id = ::core::option::Option<[::core::primitive::u8; 32usize]>;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for PermanentlyOverweight {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "PermanentlyOverweight";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "Agenda is incomplete from `when`."]
			pub struct AgendaIncomplete {
				pub when: agenda_incomplete::When,
			}
			pub mod agenda_incomplete {
				use super::runtime_types;
				pub type When = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for AgendaIncomplete {
				const PALLET: &'static str = "Scheduler";
				const EVENT: &'static str = "AgendaIncomplete";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod incomplete_since {
					use super::runtime_types;
					pub type IncompleteSince = ::core::primitive::u32;
				}
				pub mod agenda {
					use super::runtime_types;
					pub type Agenda = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::option::Option<
							runtime_types::pallet_scheduler::Scheduled<
								[::core::primitive::u8; 32usize],
								runtime_types::frame_support::traits::preimages::Bounded<
									runtime_types::midnight_node_runtime::RuntimeCall,
									runtime_types::sp_runtime::traits::BlakeTwo256,
								>,
								::core::primitive::u32,
								runtime_types::midnight_node_runtime::OriginCaller,
								::subxt::ext::subxt_core::utils::AccountId32,
							>,
						>,
					>;
					pub type Param0 = ::core::primitive::u32;
				}
				pub mod retries {
					use super::runtime_types;
					pub type Retries =
						runtime_types::pallet_scheduler::RetryConfig<::core::primitive::u32>;
					pub type Param0 = ::core::primitive::u32;
					pub type Param1 = ::core::primitive::u32;
				}
				pub mod lookup {
					use super::runtime_types;
					pub type Lookup = (::core::primitive::u32, ::core::primitive::u32);
					pub type Param0 = [::core::primitive::u8; 32usize];
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " Block number at which the agenda began incomplete execution."]
				pub fn incomplete_since(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::incomplete_since::IncompleteSince,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Scheduler",
						"IncompleteSince",
						(),
						[
							250u8, 83u8, 64u8, 167u8, 205u8, 59u8, 225u8, 97u8, 205u8, 12u8, 76u8,
							130u8, 197u8, 4u8, 111u8, 208u8, 92u8, 217u8, 145u8, 119u8, 38u8,
							135u8, 1u8, 242u8, 228u8, 143u8, 56u8, 25u8, 115u8, 233u8, 227u8, 66u8,
						],
					)
				}
				#[doc = " Items to be executed, indexed by the block number that they should be executed on."]
				pub fn agenda_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::agenda::Agenda,
					(),
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Scheduler",
						"Agenda",
						(),
						[
							42u8, 151u8, 150u8, 25u8, 237u8, 183u8, 51u8, 166u8, 119u8, 225u8,
							127u8, 149u8, 239u8, 117u8, 25u8, 137u8, 244u8, 178u8, 139u8, 237u8,
							152u8, 173u8, 155u8, 228u8, 47u8, 46u8, 178u8, 131u8, 128u8, 185u8,
							79u8, 12u8,
						],
					)
				}
				#[doc = " Items to be executed, indexed by the block number that they should be executed on."]
				pub fn agenda(
					&self,
					_0: impl ::core::borrow::Borrow<types::agenda::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::agenda::Param0,
					>,
					types::agenda::Agenda,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Scheduler",
						"Agenda",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							42u8, 151u8, 150u8, 25u8, 237u8, 183u8, 51u8, 166u8, 119u8, 225u8,
							127u8, 149u8, 239u8, 117u8, 25u8, 137u8, 244u8, 178u8, 139u8, 237u8,
							152u8, 173u8, 155u8, 228u8, 47u8, 46u8, 178u8, 131u8, 128u8, 185u8,
							79u8, 12u8,
						],
					)
				}
				#[doc = " Retry configurations for items to be executed, indexed by task address."]
				pub fn retries_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::retries::Retries,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Scheduler",
						"Retries",
						(),
						[
							164u8, 27u8, 208u8, 185u8, 19u8, 232u8, 190u8, 97u8, 137u8, 73u8,
							146u8, 10u8, 241u8, 176u8, 251u8, 140u8, 133u8, 65u8, 190u8, 162u8,
							59u8, 32u8, 77u8, 201u8, 27u8, 78u8, 183u8, 164u8, 74u8, 46u8, 139u8,
							145u8,
						],
					)
				}
				#[doc = " Retry configurations for items to be executed, indexed by task address."]
				pub fn retries_iter1(
					&self,
					_0: impl ::core::borrow::Borrow<types::retries::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::retries::Param0,
					>,
					types::retries::Retries,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Scheduler",
						"Retries",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							164u8, 27u8, 208u8, 185u8, 19u8, 232u8, 190u8, 97u8, 137u8, 73u8,
							146u8, 10u8, 241u8, 176u8, 251u8, 140u8, 133u8, 65u8, 190u8, 162u8,
							59u8, 32u8, 77u8, 201u8, 27u8, 78u8, 183u8, 164u8, 74u8, 46u8, 139u8,
							145u8,
						],
					)
				}
				#[doc = " Retry configurations for items to be executed, indexed by task address."]
				pub fn retries(
					&self,
					_0: impl ::core::borrow::Borrow<types::retries::Param0>,
					_1: impl ::core::borrow::Borrow<types::retries::Param1>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(
						::subxt::ext::subxt_core::storage::address::StaticStorageKey<
							types::retries::Param0,
						>,
						::subxt::ext::subxt_core::storage::address::StaticStorageKey<
							types::retries::Param1,
						>,
					),
					types::retries::Retries,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Scheduler",
						"Retries",
						(
							::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
								_0.borrow(),
							),
							::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
								_1.borrow(),
							),
						),
						[
							164u8, 27u8, 208u8, 185u8, 19u8, 232u8, 190u8, 97u8, 137u8, 73u8,
							146u8, 10u8, 241u8, 176u8, 251u8, 140u8, 133u8, 65u8, 190u8, 162u8,
							59u8, 32u8, 77u8, 201u8, 27u8, 78u8, 183u8, 164u8, 74u8, 46u8, 139u8,
							145u8,
						],
					)
				}
				#[doc = " Lookup from a name to the block number and index of the task."]
				#[doc = ""]
				#[doc = " For v3 -> v4 the previously unbounded identities are Blake2-256 hashed to form the v4"]
				#[doc = " identities."]
				pub fn lookup_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::lookup::Lookup,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Scheduler",
						"Lookup",
						(),
						[
							24u8, 87u8, 96u8, 127u8, 136u8, 205u8, 238u8, 174u8, 71u8, 110u8, 65u8,
							98u8, 228u8, 167u8, 99u8, 71u8, 171u8, 186u8, 12u8, 218u8, 137u8, 70u8,
							70u8, 228u8, 153u8, 111u8, 165u8, 114u8, 229u8, 136u8, 118u8, 131u8,
						],
					)
				}
				#[doc = " Lookup from a name to the block number and index of the task."]
				#[doc = ""]
				#[doc = " For v3 -> v4 the previously unbounded identities are Blake2-256 hashed to form the v4"]
				#[doc = " identities."]
				pub fn lookup(
					&self,
					_0: impl ::core::borrow::Borrow<types::lookup::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::lookup::Param0,
					>,
					types::lookup::Lookup,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Scheduler",
						"Lookup",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							24u8, 87u8, 96u8, 127u8, 136u8, 205u8, 238u8, 174u8, 71u8, 110u8, 65u8,
							98u8, 228u8, 167u8, 99u8, 71u8, 171u8, 186u8, 12u8, 218u8, 137u8, 70u8,
							70u8, 228u8, 153u8, 111u8, 165u8, 114u8, 229u8, 136u8, 118u8, 131u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " The maximum weight that may be scheduled per block for any dispatchables."]
				pub fn maximum_weight(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					runtime_types::sp_weights::weight_v2::Weight,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Scheduler",
						"MaximumWeight",
						[
							149u8, 252u8, 129u8, 80u8, 169u8, 36u8, 79u8, 127u8, 240u8, 156u8,
							56u8, 202u8, 219u8, 86u8, 5u8, 65u8, 245u8, 148u8, 138u8, 243u8, 210u8,
							128u8, 234u8, 216u8, 240u8, 219u8, 123u8, 235u8, 21u8, 158u8, 237u8,
							112u8,
						],
					)
				}
				#[doc = " The maximum number of scheduled calls in the queue for a single block."]
				#[doc = ""]
				#[doc = " NOTE:"]
				#[doc = " + Dependent pallets' benchmarks might require a higher limit for the setting. Set a"]
				#[doc = " higher limit under `runtime-benchmarks` feature."]
				pub fn max_scheduled_per_block(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Scheduler",
						"MaxScheduledPerBlock",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
			}
		}
	}
	pub mod tx_pause {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_tx_pause::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_tx_pause::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Pause a call."]
				#[doc = ""]
				#[doc = "Can only be called by [`Config::PauseOrigin`]."]
				#[doc = "Emits an [`Event::CallPaused`] event on success."]
				pub struct Pause {
					pub full_name: pause::FullName,
				}
				pub mod pause {
					use super::runtime_types;
					pub type FullName = (
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
					);
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Pause {
					const PALLET: &'static str = "TxPause";
					const CALL: &'static str = "pause";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Un-pause a call."]
				#[doc = ""]
				#[doc = "Can only be called by [`Config::UnpauseOrigin`]."]
				#[doc = "Emits an [`Event::CallUnpaused`] event on success."]
				pub struct Unpause {
					pub ident: unpause::Ident,
				}
				pub mod unpause {
					use super::runtime_types;
					pub type Ident = (
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
					);
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for Unpause {
					const PALLET: &'static str = "TxPause";
					const CALL: &'static str = "unpause";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Pause a call."]
				#[doc = ""]
				#[doc = "Can only be called by [`Config::PauseOrigin`]."]
				#[doc = "Emits an [`Event::CallPaused`] event on success."]
				pub fn pause(
					&self,
					full_name: types::pause::FullName,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Pause> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"TxPause",
						"pause",
						types::Pause { full_name },
						[
							244u8, 112u8, 104u8, 148u8, 17u8, 164u8, 228u8, 229u8, 103u8, 212u8,
							137u8, 16u8, 194u8, 167u8, 150u8, 148u8, 151u8, 233u8, 15u8, 2u8, 54u8,
							96u8, 158u8, 43u8, 222u8, 128u8, 199u8, 87u8, 74u8, 38u8, 6u8, 215u8,
						],
					)
				}
				#[doc = "Un-pause a call."]
				#[doc = ""]
				#[doc = "Can only be called by [`Config::UnpauseOrigin`]."]
				#[doc = "Emits an [`Event::CallUnpaused`] event on success."]
				pub fn unpause(
					&self,
					ident: types::unpause::Ident,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::Unpause> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"TxPause",
						"unpause",
						types::Unpause { ident },
						[
							213u8, 245u8, 75u8, 131u8, 24u8, 188u8, 101u8, 168u8, 39u8, 246u8,
							228u8, 155u8, 255u8, 146u8, 245u8, 218u8, 68u8, 102u8, 75u8, 133u8,
							54u8, 142u8, 191u8, 87u8, 148u8, 59u8, 99u8, 11u8, 33u8, 184u8, 24u8,
							179u8,
						],
					)
				}
			}
		}
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_tx_pause::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "This pallet, or a specific call is now paused."]
			pub struct CallPaused {
				pub full_name: call_paused::FullName,
			}
			pub mod call_paused {
				use super::runtime_types;
				pub type FullName = (
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
				);
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for CallPaused {
				const PALLET: &'static str = "TxPause";
				const EVENT: &'static str = "CallPaused";
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "This pallet, or a specific call is now unpaused."]
			pub struct CallUnpaused {
				pub full_name: call_unpaused::FullName,
			}
			pub mod call_unpaused {
				use super::runtime_types;
				pub type FullName = (
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
				);
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for CallUnpaused {
				const PALLET: &'static str = "TxPause";
				const EVENT: &'static str = "CallUnpaused";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod paused_calls {
					use super::runtime_types;
					pub type PausedCalls = ();
					pub type Param0 = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>;
					pub type Param1 = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The set of calls that are explicitly paused."]
				pub fn paused_calls_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::paused_calls::PausedCalls,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"TxPause",
						"PausedCalls",
						(),
						[
							36u8, 9u8, 29u8, 154u8, 39u8, 47u8, 237u8, 97u8, 176u8, 241u8, 153u8,
							131u8, 20u8, 16u8, 73u8, 63u8, 27u8, 21u8, 107u8, 5u8, 147u8, 198u8,
							82u8, 212u8, 38u8, 162u8, 1u8, 203u8, 57u8, 187u8, 53u8, 132u8,
						],
					)
				}
				#[doc = " The set of calls that are explicitly paused."]
				pub fn paused_calls_iter1(
					&self,
					_0: impl ::core::borrow::Borrow<types::paused_calls::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::paused_calls::Param0,
					>,
					types::paused_calls::PausedCalls,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"TxPause",
						"PausedCalls",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							36u8, 9u8, 29u8, 154u8, 39u8, 47u8, 237u8, 97u8, 176u8, 241u8, 153u8,
							131u8, 20u8, 16u8, 73u8, 63u8, 27u8, 21u8, 107u8, 5u8, 147u8, 198u8,
							82u8, 212u8, 38u8, 162u8, 1u8, 203u8, 57u8, 187u8, 53u8, 132u8,
						],
					)
				}
				#[doc = " The set of calls that are explicitly paused."]
				pub fn paused_calls(
					&self,
					_0: impl ::core::borrow::Borrow<types::paused_calls::Param0>,
					_1: impl ::core::borrow::Borrow<types::paused_calls::Param1>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(
						::subxt::ext::subxt_core::storage::address::StaticStorageKey<
							types::paused_calls::Param0,
						>,
						::subxt::ext::subxt_core::storage::address::StaticStorageKey<
							types::paused_calls::Param1,
						>,
					),
					types::paused_calls::PausedCalls,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"TxPause",
						"PausedCalls",
						(
							::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
								_0.borrow(),
							),
							::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
								_1.borrow(),
							),
						),
						[
							36u8, 9u8, 29u8, 154u8, 39u8, 47u8, 237u8, 97u8, 176u8, 241u8, 153u8,
							131u8, 20u8, 16u8, 73u8, 63u8, 27u8, 21u8, 107u8, 5u8, 147u8, 198u8,
							82u8, 212u8, 38u8, 162u8, 1u8, 203u8, 57u8, 187u8, 53u8, 132u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " Maximum length for pallet name and call name SCALE encoded string names."]
				#[doc = ""]
				#[doc = " TOO LONG NAMES WILL BE TREATED AS PAUSED."]
				pub fn max_name_len(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"TxPause",
						"MaxNameLen",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
			}
		}
	}
	pub mod beefy {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Error` enum of this pallet."]
		pub type Error = runtime_types::pallet_beefy::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_beefy::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
				#[doc = "equivocation proof and validate the given key ownership proof"]
				#[doc = "against the extracted offender. If both are valid, the offence"]
				#[doc = "will be reported."]
				pub struct ReportDoubleVoting {
					pub equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
						report_double_voting::EquivocationProof,
					>,
					pub key_owner_proof: report_double_voting::KeyOwnerProof,
				}
				pub mod report_double_voting {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_beefy::DoubleVotingProof<
							::core::primitive::u32,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Signature,
						>;
					pub type KeyOwnerProof = runtime_types::sp_core::Void;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ReportDoubleVoting {
					const PALLET: &'static str = "Beefy";
					const CALL: &'static str = "report_double_voting";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
				#[doc = "equivocation proof and validate the given key ownership proof"]
				#[doc = "against the extracted offender. If both are valid, the offence"]
				#[doc = "will be reported."]
				#[doc = ""]
				#[doc = "This extrinsic must be called unsigned and it is expected that only"]
				#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
				#[doc = "if the block author is defined it will be defined as the equivocation"]
				#[doc = "reporter."]
				pub struct ReportDoubleVotingUnsigned {
					pub equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
						report_double_voting_unsigned::EquivocationProof,
					>,
					pub key_owner_proof: report_double_voting_unsigned::KeyOwnerProof,
				}
				pub mod report_double_voting_unsigned {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_beefy::DoubleVotingProof<
							::core::primitive::u32,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Signature,
						>;
					pub type KeyOwnerProof = runtime_types::sp_core::Void;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ReportDoubleVotingUnsigned {
					const PALLET: &'static str = "Beefy";
					const CALL: &'static str = "report_double_voting_unsigned";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Reset BEEFY consensus by setting a new BEEFY genesis at `delay_in_blocks` blocks in the"]
				#[doc = "future."]
				#[doc = ""]
				#[doc = "Note: `delay_in_blocks` has to be at least 1."]
				pub struct SetNewGenesis {
					pub delay_in_blocks: set_new_genesis::DelayInBlocks,
				}
				pub mod set_new_genesis {
					use super::runtime_types;
					pub type DelayInBlocks = ::core::primitive::u32;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetNewGenesis {
					const PALLET: &'static str = "Beefy";
					const CALL: &'static str = "set_new_genesis";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Report fork voting equivocation. This method will verify the equivocation proof"]
				#[doc = "and validate the given key ownership proof against the extracted offender."]
				#[doc = "If both are valid, the offence will be reported."]
				pub struct ReportForkVoting {
					pub equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
						report_fork_voting::EquivocationProof,
					>,
					pub key_owner_proof: report_fork_voting::KeyOwnerProof,
				}
				pub mod report_fork_voting {
					use super::runtime_types;
					pub type EquivocationProof = runtime_types::sp_consensus_beefy::ForkVotingProof<
						runtime_types::sp_runtime::generic::header::Header<::core::primitive::u32>,
						runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
						runtime_types::sp_mmr_primitives::AncestryProof<
							::subxt::ext::subxt_core::utils::H256,
						>,
					>;
					pub type KeyOwnerProof = runtime_types::sp_core::Void;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ReportForkVoting {
					const PALLET: &'static str = "Beefy";
					const CALL: &'static str = "report_fork_voting";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Report fork voting equivocation. This method will verify the equivocation proof"]
				#[doc = "and validate the given key ownership proof against the extracted offender."]
				#[doc = "If both are valid, the offence will be reported."]
				#[doc = ""]
				#[doc = "This extrinsic must be called unsigned and it is expected that only"]
				#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
				#[doc = "if the block author is defined it will be defined as the equivocation"]
				#[doc = "reporter."]
				pub struct ReportForkVotingUnsigned {
					pub equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
						report_fork_voting_unsigned::EquivocationProof,
					>,
					pub key_owner_proof: report_fork_voting_unsigned::KeyOwnerProof,
				}
				pub mod report_fork_voting_unsigned {
					use super::runtime_types;
					pub type EquivocationProof = runtime_types::sp_consensus_beefy::ForkVotingProof<
						runtime_types::sp_runtime::generic::header::Header<::core::primitive::u32>,
						runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
						runtime_types::sp_mmr_primitives::AncestryProof<
							::subxt::ext::subxt_core::utils::H256,
						>,
					>;
					pub type KeyOwnerProof = runtime_types::sp_core::Void;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ReportForkVotingUnsigned {
					const PALLET: &'static str = "Beefy";
					const CALL: &'static str = "report_fork_voting_unsigned";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Report future block voting equivocation. This method will verify the equivocation proof"]
				#[doc = "and validate the given key ownership proof against the extracted offender."]
				#[doc = "If both are valid, the offence will be reported."]
				pub struct ReportFutureBlockVoting {
					pub equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
						report_future_block_voting::EquivocationProof,
					>,
					pub key_owner_proof: report_future_block_voting::KeyOwnerProof,
				}
				pub mod report_future_block_voting {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_beefy::FutureBlockVotingProof<
							::core::primitive::u32,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
						>;
					pub type KeyOwnerProof = runtime_types::sp_core::Void;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ReportFutureBlockVoting {
					const PALLET: &'static str = "Beefy";
					const CALL: &'static str = "report_future_block_voting";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Report future block voting equivocation. This method will verify the equivocation proof"]
				#[doc = "and validate the given key ownership proof against the extracted offender."]
				#[doc = "If both are valid, the offence will be reported."]
				#[doc = ""]
				#[doc = "This extrinsic must be called unsigned and it is expected that only"]
				#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
				#[doc = "if the block author is defined it will be defined as the equivocation"]
				#[doc = "reporter."]
				pub struct ReportFutureBlockVotingUnsigned {
					pub equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
						report_future_block_voting_unsigned::EquivocationProof,
					>,
					pub key_owner_proof: report_future_block_voting_unsigned::KeyOwnerProof,
				}
				pub mod report_future_block_voting_unsigned {
					use super::runtime_types;
					pub type EquivocationProof =
						runtime_types::sp_consensus_beefy::FutureBlockVotingProof<
							::core::primitive::u32,
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
						>;
					pub type KeyOwnerProof = runtime_types::sp_core::Void;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for ReportFutureBlockVotingUnsigned {
					const PALLET: &'static str = "Beefy";
					const CALL: &'static str = "report_future_block_voting_unsigned";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
				#[doc = "equivocation proof and validate the given key ownership proof"]
				#[doc = "against the extracted offender. If both are valid, the offence"]
				#[doc = "will be reported."]
				pub fn report_double_voting(
					&self,
					equivocation_proof: types::report_double_voting::EquivocationProof,
					key_owner_proof: types::report_double_voting::KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ReportDoubleVoting>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Beefy",
						"report_double_voting",
						types::ReportDoubleVoting {
							equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box::new(
								equivocation_proof,
							),
							key_owner_proof,
						},
						[
							161u8, 59u8, 243u8, 96u8, 178u8, 29u8, 60u8, 203u8, 239u8, 53u8, 234u8,
							186u8, 119u8, 246u8, 235u8, 227u8, 235u8, 14u8, 16u8, 44u8, 65u8, 35u8,
							137u8, 152u8, 236u8, 16u8, 26u8, 207u8, 242u8, 151u8, 140u8, 43u8,
						],
					)
				}
				#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
				#[doc = "equivocation proof and validate the given key ownership proof"]
				#[doc = "against the extracted offender. If both are valid, the offence"]
				#[doc = "will be reported."]
				#[doc = ""]
				#[doc = "This extrinsic must be called unsigned and it is expected that only"]
				#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
				#[doc = "if the block author is defined it will be defined as the equivocation"]
				#[doc = "reporter."]
				pub fn report_double_voting_unsigned(
					&self,
					equivocation_proof: types::report_double_voting_unsigned::EquivocationProof,
					key_owner_proof: types::report_double_voting_unsigned::KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::ReportDoubleVotingUnsigned,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Beefy",
						"report_double_voting_unsigned",
						types::ReportDoubleVotingUnsigned {
							equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box::new(
								equivocation_proof,
							),
							key_owner_proof,
						},
						[
							216u8, 15u8, 195u8, 228u8, 154u8, 79u8, 26u8, 139u8, 177u8, 200u8,
							137u8, 159u8, 13u8, 12u8, 58u8, 148u8, 182u8, 228u8, 207u8, 34u8,
							193u8, 136u8, 105u8, 19u8, 191u8, 144u8, 138u8, 66u8, 85u8, 130u8,
							205u8, 212u8,
						],
					)
				}
				#[doc = "Reset BEEFY consensus by setting a new BEEFY genesis at `delay_in_blocks` blocks in the"]
				#[doc = "future."]
				#[doc = ""]
				#[doc = "Note: `delay_in_blocks` has to be at least 1."]
				pub fn set_new_genesis(
					&self,
					delay_in_blocks: types::set_new_genesis::DelayInBlocks,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetNewGenesis>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Beefy",
						"set_new_genesis",
						types::SetNewGenesis { delay_in_blocks },
						[
							147u8, 6u8, 252u8, 43u8, 77u8, 91u8, 170u8, 45u8, 112u8, 155u8, 158u8,
							79u8, 1u8, 116u8, 162u8, 146u8, 181u8, 9u8, 171u8, 48u8, 198u8, 210u8,
							243u8, 64u8, 229u8, 35u8, 28u8, 177u8, 144u8, 22u8, 165u8, 163u8,
						],
					)
				}
				#[doc = "Report fork voting equivocation. This method will verify the equivocation proof"]
				#[doc = "and validate the given key ownership proof against the extracted offender."]
				#[doc = "If both are valid, the offence will be reported."]
				pub fn report_fork_voting(
					&self,
					equivocation_proof: types::report_fork_voting::EquivocationProof,
					key_owner_proof: types::report_fork_voting::KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::ReportForkVoting>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Beefy",
						"report_fork_voting",
						types::ReportForkVoting {
							equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box::new(
								equivocation_proof,
							),
							key_owner_proof,
						},
						[
							111u8, 170u8, 72u8, 91u8, 4u8, 199u8, 169u8, 36u8, 35u8, 250u8, 254u8,
							202u8, 78u8, 23u8, 194u8, 132u8, 148u8, 192u8, 55u8, 110u8, 112u8,
							246u8, 30u8, 72u8, 80u8, 109u8, 6u8, 103u8, 66u8, 245u8, 208u8, 105u8,
						],
					)
				}
				#[doc = "Report fork voting equivocation. This method will verify the equivocation proof"]
				#[doc = "and validate the given key ownership proof against the extracted offender."]
				#[doc = "If both are valid, the offence will be reported."]
				#[doc = ""]
				#[doc = "This extrinsic must be called unsigned and it is expected that only"]
				#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
				#[doc = "if the block author is defined it will be defined as the equivocation"]
				#[doc = "reporter."]
				pub fn report_fork_voting_unsigned(
					&self,
					equivocation_proof: types::report_fork_voting_unsigned::EquivocationProof,
					key_owner_proof: types::report_fork_voting_unsigned::KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::ReportForkVotingUnsigned,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Beefy",
						"report_fork_voting_unsigned",
						types::ReportForkVotingUnsigned {
							equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box::new(
								equivocation_proof,
							),
							key_owner_proof,
						},
						[
							173u8, 9u8, 106u8, 217u8, 197u8, 84u8, 118u8, 201u8, 90u8, 5u8, 57u8,
							255u8, 144u8, 251u8, 167u8, 190u8, 56u8, 41u8, 63u8, 110u8, 133u8,
							50u8, 242u8, 18u8, 81u8, 136u8, 123u8, 120u8, 109u8, 229u8, 137u8,
							169u8,
						],
					)
				}
				#[doc = "Report future block voting equivocation. This method will verify the equivocation proof"]
				#[doc = "and validate the given key ownership proof against the extracted offender."]
				#[doc = "If both are valid, the offence will be reported."]
				pub fn report_future_block_voting(
					&self,
					equivocation_proof: types::report_future_block_voting::EquivocationProof,
					key_owner_proof: types::report_future_block_voting::KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::ReportFutureBlockVoting,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Beefy",
						"report_future_block_voting",
						types::ReportFutureBlockVoting {
							equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box::new(
								equivocation_proof,
							),
							key_owner_proof,
						},
						[
							239u8, 160u8, 121u8, 167u8, 164u8, 60u8, 25u8, 186u8, 9u8, 153u8,
							127u8, 233u8, 117u8, 21u8, 105u8, 255u8, 204u8, 119u8, 52u8, 18u8,
							193u8, 223u8, 123u8, 52u8, 160u8, 78u8, 65u8, 179u8, 120u8, 142u8,
							17u8, 158u8,
						],
					)
				}
				#[doc = "Report future block voting equivocation. This method will verify the equivocation proof"]
				#[doc = "and validate the given key ownership proof against the extracted offender."]
				#[doc = "If both are valid, the offence will be reported."]
				#[doc = ""]
				#[doc = "This extrinsic must be called unsigned and it is expected that only"]
				#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
				#[doc = "if the block author is defined it will be defined as the equivocation"]
				#[doc = "reporter."]
				pub fn report_future_block_voting_unsigned(
					&self,
					equivocation_proof : types :: report_future_block_voting_unsigned :: EquivocationProof,
					key_owner_proof: types::report_future_block_voting_unsigned::KeyOwnerProof,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<
					types::ReportFutureBlockVotingUnsigned,
				> {
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"Beefy",
						"report_future_block_voting_unsigned",
						types::ReportFutureBlockVotingUnsigned {
							equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box::new(
								equivocation_proof,
							),
							key_owner_proof,
						},
						[
							65u8, 205u8, 168u8, 183u8, 134u8, 29u8, 3u8, 171u8, 195u8, 133u8,
							148u8, 67u8, 174u8, 179u8, 208u8, 185u8, 242u8, 91u8, 166u8, 85u8,
							138u8, 21u8, 145u8, 39u8, 11u8, 133u8, 223u8, 170u8, 198u8, 111u8, 0u8,
							96u8,
						],
					)
				}
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod authorities {
					use super::runtime_types;
					pub type Authorities =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
						>;
				}
				pub mod validator_set_id {
					use super::runtime_types;
					pub type ValidatorSetId = ::core::primitive::u64;
				}
				pub mod next_authorities {
					use super::runtime_types;
					pub type NextAuthorities =
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
						>;
				}
				pub mod set_id_session {
					use super::runtime_types;
					pub type SetIdSession = ::core::primitive::u32;
					pub type Param0 = ::core::primitive::u64;
				}
				pub mod genesis_block {
					use super::runtime_types;
					pub type GenesisBlock = ::core::option::Option<::core::primitive::u32>;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " The current authorities set"]
				pub fn authorities(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::authorities::Authorities,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Beefy",
						"Authorities",
						(),
						[
							135u8, 146u8, 24u8, 144u8, 245u8, 251u8, 253u8, 63u8, 44u8, 46u8, 72u8,
							115u8, 190u8, 93u8, 201u8, 244u8, 80u8, 80u8, 156u8, 171u8, 69u8,
							126u8, 67u8, 51u8, 196u8, 136u8, 93u8, 176u8, 111u8, 247u8, 19u8,
							161u8,
						],
					)
				}
				#[doc = " The current validator set id"]
				pub fn validator_set_id(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::validator_set_id::ValidatorSetId,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Beefy",
						"ValidatorSetId",
						(),
						[
							168u8, 84u8, 23u8, 134u8, 153u8, 30u8, 183u8, 176u8, 206u8, 100u8,
							109u8, 86u8, 109u8, 126u8, 146u8, 175u8, 173u8, 1u8, 253u8, 42u8,
							122u8, 207u8, 71u8, 4u8, 145u8, 83u8, 148u8, 29u8, 243u8, 52u8, 29u8,
							78u8,
						],
					)
				}
				#[doc = " Authorities set scheduled to be used with the next session"]
				pub fn next_authorities(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::next_authorities::NextAuthorities,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Beefy",
						"NextAuthorities",
						(),
						[
							183u8, 185u8, 111u8, 92u8, 70u8, 221u8, 225u8, 184u8, 218u8, 97u8,
							108u8, 66u8, 60u8, 148u8, 40u8, 73u8, 236u8, 28u8, 169u8, 32u8, 202u8,
							125u8, 238u8, 246u8, 210u8, 51u8, 25u8, 61u8, 220u8, 195u8, 156u8,
							217u8,
						],
					)
				}
				#[doc = " A mapping from BEEFY set ID to the index of the *most recent* session for which its"]
				#[doc = " members were responsible."]
				#[doc = ""]
				#[doc = " This is only used for validating equivocation proofs. An equivocation proof must"]
				#[doc = " contains a key-ownership proof for a given session, therefore we need a way to tie"]
				#[doc = " together sessions and BEEFY set ids, i.e. we need to validate that a validator"]
				#[doc = " was the owner of a given key on a given session, and what the active set ID was"]
				#[doc = " during that session."]
				#[doc = ""]
				#[doc = " TWOX-NOTE: `ValidatorSetId` is not under user control."]
				pub fn set_id_session_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::set_id_session::SetIdSession,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Beefy",
						"SetIdSession",
						(),
						[
							47u8, 0u8, 239u8, 121u8, 187u8, 213u8, 254u8, 50u8, 238u8, 10u8, 162u8,
							65u8, 189u8, 166u8, 37u8, 74u8, 82u8, 81u8, 160u8, 20u8, 180u8, 253u8,
							238u8, 18u8, 209u8, 203u8, 38u8, 148u8, 16u8, 105u8, 72u8, 169u8,
						],
					)
				}
				#[doc = " A mapping from BEEFY set ID to the index of the *most recent* session for which its"]
				#[doc = " members were responsible."]
				#[doc = ""]
				#[doc = " This is only used for validating equivocation proofs. An equivocation proof must"]
				#[doc = " contains a key-ownership proof for a given session, therefore we need a way to tie"]
				#[doc = " together sessions and BEEFY set ids, i.e. we need to validate that a validator"]
				#[doc = " was the owner of a given key on a given session, and what the active set ID was"]
				#[doc = " during that session."]
				#[doc = ""]
				#[doc = " TWOX-NOTE: `ValidatorSetId` is not under user control."]
				pub fn set_id_session(
					&self,
					_0: impl ::core::borrow::Borrow<types::set_id_session::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::set_id_session::Param0,
					>,
					types::set_id_session::SetIdSession,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Beefy",
						"SetIdSession",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							47u8, 0u8, 239u8, 121u8, 187u8, 213u8, 254u8, 50u8, 238u8, 10u8, 162u8,
							65u8, 189u8, 166u8, 37u8, 74u8, 82u8, 81u8, 160u8, 20u8, 180u8, 253u8,
							238u8, 18u8, 209u8, 203u8, 38u8, 148u8, 16u8, 105u8, 72u8, 169u8,
						],
					)
				}
				#[doc = " Block number where BEEFY consensus is enabled/started."]
				#[doc = " By changing this (through privileged `set_new_genesis()`), BEEFY consensus is effectively"]
				#[doc = " restarted from the newly set block number."]
				pub fn genesis_block(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::genesis_block::GenesisBlock,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Beefy",
						"GenesisBlock",
						(),
						[
							198u8, 155u8, 11u8, 240u8, 189u8, 245u8, 159u8, 127u8, 55u8, 33u8,
							48u8, 29u8, 209u8, 119u8, 163u8, 24u8, 28u8, 22u8, 163u8, 163u8, 124u8,
							88u8, 126u8, 4u8, 193u8, 158u8, 29u8, 243u8, 212u8, 4u8, 41u8, 22u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " The maximum number of authorities that can be added."]
				pub fn max_authorities(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Beefy",
						"MaxAuthorities",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " The maximum number of nominators for each validator."]
				pub fn max_nominators(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Beefy",
						"MaxNominators",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " The maximum number of entries to keep in the set id to session index mapping."]
				#[doc = ""]
				#[doc = " Since the `SetIdSession` map is only used for validating equivocations this"]
				#[doc = " value should relate to the bonding duration of whatever staking system is"]
				#[doc = " being used (if any). If equivocation handling is not enabled then this value"]
				#[doc = " can be zero."]
				pub fn max_set_id_session_entries(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u64,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"Beefy",
						"MaxSetIdSessionEntries",
						[
							128u8, 214u8, 205u8, 242u8, 181u8, 142u8, 124u8, 231u8, 190u8, 146u8,
							59u8, 226u8, 157u8, 101u8, 103u8, 117u8, 249u8, 65u8, 18u8, 191u8,
							103u8, 119u8, 53u8, 85u8, 81u8, 96u8, 220u8, 42u8, 184u8, 239u8, 42u8,
							246u8,
						],
					)
				}
			}
		}
	}
	pub mod mmr {
		use super::root_mod;
		use super::runtime_types;
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod root_hash {
					use super::runtime_types;
					pub type RootHash = ::subxt::ext::subxt_core::utils::H256;
				}
				pub mod number_of_leaves {
					use super::runtime_types;
					pub type NumberOfLeaves = ::core::primitive::u64;
				}
				pub mod nodes {
					use super::runtime_types;
					pub type Nodes = ::subxt::ext::subxt_core::utils::H256;
					pub type Param0 = ::core::primitive::u64;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " Latest MMR Root hash."]
				pub fn root_hash(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::root_hash::RootHash,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Mmr",
						"RootHash",
						(),
						[
							111u8, 206u8, 173u8, 92u8, 67u8, 49u8, 150u8, 113u8, 90u8, 245u8, 38u8,
							254u8, 76u8, 250u8, 167u8, 66u8, 130u8, 129u8, 251u8, 220u8, 172u8,
							229u8, 162u8, 251u8, 36u8, 227u8, 43u8, 189u8, 7u8, 106u8, 23u8, 13u8,
						],
					)
				}
				#[doc = " Current size of the MMR (number of leaves)."]
				pub fn number_of_leaves(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::number_of_leaves::NumberOfLeaves,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Mmr",
						"NumberOfLeaves",
						(),
						[
							123u8, 58u8, 149u8, 174u8, 85u8, 45u8, 20u8, 115u8, 241u8, 0u8, 51u8,
							174u8, 234u8, 60u8, 230u8, 59u8, 237u8, 144u8, 170u8, 32u8, 4u8, 0u8,
							34u8, 163u8, 238u8, 205u8, 93u8, 208u8, 53u8, 38u8, 141u8, 195u8,
						],
					)
				}
				#[doc = " Hashes of the nodes in the MMR."]
				#[doc = ""]
				#[doc = " Note this collection only contains MMR peaks, the inner nodes (and leaves)"]
				#[doc = " are pruned and only stored in the Offchain DB."]
				pub fn nodes_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::nodes::Nodes,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Mmr",
						"Nodes",
						(),
						[
							27u8, 84u8, 41u8, 195u8, 146u8, 81u8, 211u8, 189u8, 63u8, 125u8, 173u8,
							206u8, 69u8, 198u8, 202u8, 213u8, 89u8, 31u8, 89u8, 177u8, 76u8, 154u8,
							249u8, 197u8, 133u8, 78u8, 142u8, 71u8, 183u8, 3u8, 132u8, 25u8,
						],
					)
				}
				#[doc = " Hashes of the nodes in the MMR."]
				#[doc = ""]
				#[doc = " Note this collection only contains MMR peaks, the inner nodes (and leaves)"]
				#[doc = " are pruned and only stored in the Offchain DB."]
				pub fn nodes(
					&self,
					_0: impl ::core::borrow::Borrow<types::nodes::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::nodes::Param0,
					>,
					types::nodes::Nodes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Mmr",
						"Nodes",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							27u8, 84u8, 41u8, 195u8, 146u8, 81u8, 211u8, 189u8, 63u8, 125u8, 173u8,
							206u8, 69u8, 198u8, 202u8, 213u8, 89u8, 31u8, 89u8, 177u8, 76u8, 154u8,
							249u8, 197u8, 133u8, 78u8, 142u8, 71u8, 183u8, 3u8, 132u8, 25u8,
						],
					)
				}
			}
		}
	}
	pub mod beefy_mmr_leaf {
		use super::root_mod;
		use super::runtime_types;
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod beefy_authorities {
					use super::runtime_types;
					pub type BeefyAuthorities =
						runtime_types::sp_consensus_beefy::mmr::BeefyAuthoritySet<
							::subxt::ext::subxt_core::utils::H256,
						>;
				}
				pub mod beefy_next_authorities {
					use super::runtime_types;
					pub type BeefyNextAuthorities =
						runtime_types::sp_consensus_beefy::mmr::BeefyAuthoritySet<
							::subxt::ext::subxt_core::utils::H256,
						>;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " Details of current BEEFY authority set."]
				pub fn beefy_authorities(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::beefy_authorities::BeefyAuthorities,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"BeefyMmrLeaf",
						"BeefyAuthorities",
						(),
						[
							128u8, 35u8, 176u8, 79u8, 224u8, 58u8, 214u8, 234u8, 231u8, 71u8,
							227u8, 153u8, 180u8, 189u8, 66u8, 44u8, 47u8, 174u8, 0u8, 83u8, 121u8,
							182u8, 226u8, 44u8, 224u8, 173u8, 237u8, 102u8, 231u8, 146u8, 110u8,
							7u8,
						],
					)
				}
				#[doc = " Details of next BEEFY authority set."]
				#[doc = ""]
				#[doc = " This storage entry is used as cache for calls to `update_beefy_next_authority_set`."]
				pub fn beefy_next_authorities(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::beefy_next_authorities::BeefyNextAuthorities,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"BeefyMmrLeaf",
						"BeefyNextAuthorities",
						(),
						[
							97u8, 71u8, 52u8, 111u8, 120u8, 251u8, 183u8, 155u8, 177u8, 100u8,
							236u8, 142u8, 204u8, 117u8, 95u8, 40u8, 201u8, 36u8, 32u8, 82u8, 38u8,
							234u8, 135u8, 39u8, 224u8, 69u8, 94u8, 85u8, 12u8, 89u8, 97u8, 218u8,
						],
					)
				}
			}
		}
	}
	pub mod session {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "The `Event` enum of this pallet"]
		pub type Event = runtime_types::pallet_partner_chains_session::pallet::Event;
		pub mod events {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			#[doc = "New session has happened. Note that the argument is the session index, not the"]
			#[doc = "block number as the type might suggest."]
			pub struct NewSession {
				pub session_index: new_session::SessionIndex,
			}
			pub mod new_session {
				use super::runtime_types;
				pub type SessionIndex = ::core::primitive::u32;
			}
			impl ::subxt::ext::subxt_core::events::StaticEvent for NewSession {
				const PALLET: &'static str = "Session";
				const EVENT: &'static str = "NewSession";
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod validators {
					use super::runtime_types;
					pub type Validators = ::subxt::ext::subxt_core::alloc::vec::Vec<
						::subxt::ext::subxt_core::utils::AccountId32,
					>;
				}
				pub mod validators_and_keys {
					use super::runtime_types;
					pub type ValidatorsAndKeys = ::subxt::ext::subxt_core::alloc::vec::Vec<(
						::subxt::ext::subxt_core::utils::AccountId32,
						runtime_types::midnight_node_runtime::opaque::SessionKeys,
					)>;
				}
				pub mod current_index {
					use super::runtime_types;
					pub type CurrentIndex = ::core::primitive::u32;
				}
				pub mod disabled_validators {
					use super::runtime_types;
					pub type DisabledValidators =
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u32>;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				pub fn validators(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::validators::Validators,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Session",
						"Validators",
						(),
						[
							50u8, 86u8, 154u8, 222u8, 249u8, 209u8, 156u8, 22u8, 155u8, 25u8,
							133u8, 194u8, 210u8, 50u8, 38u8, 28u8, 139u8, 201u8, 90u8, 139u8,
							115u8, 12u8, 12u8, 141u8, 4u8, 178u8, 201u8, 241u8, 223u8, 234u8, 6u8,
							86u8,
						],
					)
				}
				pub fn validators_and_keys(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::validators_and_keys::ValidatorsAndKeys,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Session",
						"ValidatorsAndKeys",
						(),
						[
							203u8, 73u8, 180u8, 42u8, 56u8, 26u8, 8u8, 128u8, 100u8, 87u8, 242u8,
							38u8, 33u8, 173u8, 243u8, 129u8, 12u8, 111u8, 135u8, 165u8, 13u8,
							216u8, 29u8, 190u8, 173u8, 112u8, 74u8, 151u8, 82u8, 218u8, 205u8,
							142u8,
						],
					)
				}
				#[doc = " Current index of the session."]
				pub fn current_index(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::current_index::CurrentIndex,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Session",
						"CurrentIndex",
						(),
						[
							167u8, 151u8, 125u8, 150u8, 159u8, 21u8, 78u8, 217u8, 237u8, 183u8,
							135u8, 65u8, 187u8, 114u8, 188u8, 206u8, 16u8, 32u8, 69u8, 208u8,
							134u8, 159u8, 232u8, 224u8, 243u8, 27u8, 31u8, 166u8, 145u8, 44u8,
							221u8, 230u8,
						],
					)
				}
				#[doc = " Indices of disabled validators."]
				#[doc = ""]
				#[doc = " The vec is always kept sorted so that we can find whether a given validator is"]
				#[doc = " disabled using binary search. It gets cleared when `on_session_ending` returns"]
				#[doc = " a new set of identities."]
				pub fn disabled_validators(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::disabled_validators::DisabledValidators,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"Session",
						"DisabledValidators",
						(),
						[
							213u8, 19u8, 168u8, 234u8, 187u8, 200u8, 180u8, 97u8, 234u8, 189u8,
							36u8, 233u8, 158u8, 184u8, 45u8, 35u8, 129u8, 213u8, 133u8, 8u8, 104u8,
							183u8, 46u8, 68u8, 154u8, 240u8, 132u8, 22u8, 247u8, 11u8, 54u8, 221u8,
						],
					)
				}
			}
		}
	}
	pub mod governed_map {
		use super::root_mod;
		use super::runtime_types;
		#[doc = "Error type used  by this pallet's extrinsics"]
		pub type Error = runtime_types::pallet_governed_map::pallet::Error;
		#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
		pub type Call = runtime_types::pallet_governed_map::pallet::Call;
		pub mod calls {
			use super::root_mod;
			use super::runtime_types;
			type DispatchError = runtime_types::sp_runtime::DispatchError;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Inherent to register any changes in the state of the Governed Map on Cardano compared to the state currently stored in the pallet."]
				pub struct RegisterChanges {
					pub changes: register_changes::Changes,
				}
				pub mod register_changes {
					use super::runtime_types;
					pub type Changes =
						runtime_types::bounded_collections::bounded_btree_map::BoundedBTreeMap<
							runtime_types::sidechain_domain::byte_string::BoundedString,
							::core::option::Option<
								runtime_types::bounded_collections::bounded_vec::BoundedVec<
									::core::primitive::u8,
								>,
							>,
						>;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for RegisterChanges {
					const PALLET: &'static str = "GovernedMap";
					const CALL: &'static str = "register_changes";
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Changes the address of the Governed Map validator used for observation."]
				#[doc = ""]
				#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
				pub struct SetMainChainScripts {
					pub new_main_chain_script: set_main_chain_scripts::NewMainChainScript,
				}
				pub mod set_main_chain_scripts {
					use super::runtime_types;
					pub type NewMainChainScript =
						runtime_types::sp_governed_map::MainChainScriptsV1;
				}
				impl ::subxt::ext::subxt_core::blocks::StaticExtrinsic for SetMainChainScripts {
					const PALLET: &'static str = "GovernedMap";
					const CALL: &'static str = "set_main_chain_scripts";
				}
			}
			pub struct TransactionApi;
			impl TransactionApi {
				#[doc = "Inherent to register any changes in the state of the Governed Map on Cardano compared to the state currently stored in the pallet."]
				pub fn register_changes(
					&self,
					changes: types::register_changes::Changes,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::RegisterChanges>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"GovernedMap",
						"register_changes",
						types::RegisterChanges { changes },
						[
							55u8, 134u8, 12u8, 118u8, 117u8, 57u8, 196u8, 165u8, 204u8, 181u8,
							79u8, 0u8, 60u8, 252u8, 67u8, 43u8, 62u8, 67u8, 14u8, 35u8, 177u8,
							29u8, 240u8, 58u8, 14u8, 73u8, 106u8, 122u8, 172u8, 82u8, 17u8, 28u8,
						],
					)
				}
				#[doc = "Changes the address of the Governed Map validator used for observation."]
				#[doc = ""]
				#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
				pub fn set_main_chain_scripts(
					&self,
					new_main_chain_script: types::set_main_chain_scripts::NewMainChainScript,
				) -> ::subxt::ext::subxt_core::tx::payload::StaticPayload<types::SetMainChainScripts>
				{
					::subxt::ext::subxt_core::tx::payload::StaticPayload::new_static(
						"GovernedMap",
						"set_main_chain_scripts",
						types::SetMainChainScripts { new_main_chain_script },
						[
							153u8, 95u8, 242u8, 23u8, 188u8, 114u8, 175u8, 108u8, 58u8, 127u8,
							175u8, 197u8, 36u8, 200u8, 21u8, 175u8, 70u8, 124u8, 62u8, 165u8, 34u8,
							36u8, 89u8, 149u8, 239u8, 44u8, 80u8, 100u8, 238u8, 29u8, 56u8, 226u8,
						],
					)
				}
			}
		}
		pub mod storage {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				pub mod initialized {
					use super::runtime_types;
					pub type Initialized = ::core::primitive::bool;
				}
				pub mod last_update_block {
					use super::runtime_types;
					pub type LastUpdateBlock = ::core::primitive::u32;
				}
				pub mod mapping {
					use super::runtime_types;
					pub type Mapping = runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>;
					pub type Param0 = runtime_types::sidechain_domain::byte_string::BoundedString;
				}
				pub mod main_chain_scripts {
					use super::runtime_types;
					pub type MainChainScripts = runtime_types::sp_governed_map::MainChainScriptsV1;
				}
			}
			pub struct StorageApi;
			impl StorageApi {
				#[doc = " Stores the initialization state of the pallet"]
				#[doc = ""]
				#[doc = " The pallet is considered uninitialized if no inherent was executed since the genesis block or"]
				#[doc = " since the last change of the main chain scripts."]
				pub fn initialized(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::initialized::Initialized,
					::subxt::ext::subxt_core::utils::Yes,
					::subxt::ext::subxt_core::utils::Yes,
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"GovernedMap",
						"Initialized",
						(),
						[
							113u8, 248u8, 192u8, 99u8, 251u8, 135u8, 33u8, 245u8, 102u8, 117u8,
							5u8, 231u8, 111u8, 153u8, 197u8, 250u8, 144u8, 110u8, 49u8, 205u8,
							112u8, 23u8, 210u8, 167u8, 172u8, 43u8, 65u8, 117u8, 230u8, 153u8,
							58u8, 138u8,
						],
					)
				}
				#[doc = " Stores the block number of the last time mapping changes were registered"]
				pub fn last_update_block(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::last_update_block::LastUpdateBlock,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"GovernedMap",
						"LastUpdateBlock",
						(),
						[
							125u8, 197u8, 7u8, 156u8, 206u8, 194u8, 135u8, 187u8, 252u8, 117u8,
							100u8, 253u8, 180u8, 67u8, 251u8, 231u8, 119u8, 243u8, 231u8, 152u8,
							230u8, 56u8, 155u8, 33u8, 217u8, 23u8, 191u8, 62u8, 37u8, 83u8, 15u8,
							108u8,
						],
					)
				}
				#[doc = " Stores the latest state of the Governed Map that was observed on Cardano."]
				pub fn mapping_iter(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::mapping::Mapping,
					(),
					(),
					::subxt::ext::subxt_core::utils::Yes,
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"GovernedMap",
						"Mapping",
						(),
						[
							255u8, 48u8, 52u8, 236u8, 74u8, 74u8, 49u8, 132u8, 100u8, 101u8, 196u8,
							236u8, 61u8, 87u8, 19u8, 63u8, 23u8, 56u8, 194u8, 204u8, 226u8, 173u8,
							165u8, 224u8, 38u8, 132u8, 59u8, 9u8, 179u8, 169u8, 86u8, 171u8,
						],
					)
				}
				#[doc = " Stores the latest state of the Governed Map that was observed on Cardano."]
				pub fn mapping(
					&self,
					_0: impl ::core::borrow::Borrow<types::mapping::Param0>,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					::subxt::ext::subxt_core::storage::address::StaticStorageKey<
						types::mapping::Param0,
					>,
					types::mapping::Mapping,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"GovernedMap",
						"Mapping",
						::subxt::ext::subxt_core::storage::address::StaticStorageKey::new(
							_0.borrow(),
						),
						[
							255u8, 48u8, 52u8, 236u8, 74u8, 74u8, 49u8, 132u8, 100u8, 101u8, 196u8,
							236u8, 61u8, 87u8, 19u8, 63u8, 23u8, 56u8, 194u8, 204u8, 226u8, 173u8,
							165u8, 224u8, 38u8, 132u8, 59u8, 9u8, 179u8, 169u8, 86u8, 171u8,
						],
					)
				}
				#[doc = " Cardano address of the Governed Map validator."]
				#[doc = ""]
				#[doc = " This address is used by the observability component to query current state of the mapping"]
				pub fn main_chain_scripts(
					&self,
				) -> ::subxt::ext::subxt_core::storage::address::StaticAddress<
					(),
					types::main_chain_scripts::MainChainScripts,
					::subxt::ext::subxt_core::utils::Yes,
					(),
					(),
				> {
					::subxt::ext::subxt_core::storage::address::StaticAddress::new_static(
						"GovernedMap",
						"MainChainScripts",
						(),
						[
							17u8, 195u8, 82u8, 227u8, 222u8, 70u8, 37u8, 106u8, 38u8, 60u8, 214u8,
							39u8, 76u8, 184u8, 88u8, 153u8, 169u8, 144u8, 85u8, 175u8, 193u8,
							176u8, 241u8, 124u8, 52u8, 1u8, 5u8, 226u8, 120u8, 84u8, 187u8, 155u8,
						],
					)
				}
			}
		}
		pub mod constants {
			use super::runtime_types;
			pub struct ConstantsApi;
			impl ConstantsApi {
				#[doc = " Maximum number of changes that can be registered in a single inherent."]
				#[doc = ""]
				#[doc = " This value *must* be high enough for all changes to be registered in one block."]
				#[doc = " Setting this to a value higher than the total number of parameters in the Governed Map guarantees that."]
				pub fn max_changes(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"GovernedMap",
						"MaxChanges",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " Maximum length of the key in the Governed Map in bytes."]
				#[doc = ""]
				#[doc = " This value *must* be high enough not to be exceeded by any key stored on Cardano."]
				pub fn max_key_length(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"GovernedMap",
						"MaxKeyLength",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
				#[doc = " Maximum length of data stored under a single key in the Governed Map"]
				#[doc = ""]
				#[doc = " This value *must* be high enough not to be exceeded by any value stored on Cardano."]
				pub fn max_value_length(
					&self,
				) -> ::subxt::ext::subxt_core::constants::address::StaticAddress<
					::core::primitive::u32,
				> {
					::subxt::ext::subxt_core::constants::address::StaticAddress::new_static(
						"GovernedMap",
						"MaxValueLength",
						[
							98u8, 252u8, 116u8, 72u8, 26u8, 180u8, 225u8, 83u8, 200u8, 157u8,
							125u8, 151u8, 53u8, 76u8, 168u8, 26u8, 10u8, 9u8, 98u8, 68u8, 9u8,
							178u8, 197u8, 113u8, 31u8, 79u8, 200u8, 90u8, 203u8, 100u8, 41u8,
							145u8,
						],
					)
				}
			}
		}
	}
	pub mod runtime_types {
		use super::runtime_types;
		pub mod authority_selection_inherents {
			use super::runtime_types;
			pub mod authority_selection_inputs {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct AuthoritySelectionInputs {
					pub d_parameter: runtime_types::sidechain_domain::DParameter,
					pub permissioned_candidates: ::subxt::ext::subxt_core::alloc::vec::Vec<
						runtime_types::sidechain_domain::PermissionedCandidateData,
					>,
					pub registered_candidates: ::subxt::ext::subxt_core::alloc::vec::Vec<
						runtime_types::sidechain_domain::CandidateRegistrations,
					>,
					pub epoch_nonce: runtime_types::sidechain_domain::EpochNonce,
				}
			}
			pub mod filter_invalid_candidates {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum PermissionedCandidateDataError {
					#[codec(index = 0)]
					InvalidSidechainPubKey,
					#[codec(index = 1)]
					InvalidAuraKey,
					#[codec(index = 2)]
					InvalidGrandpaKey,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum RegistrationDataError {
					#[codec(index = 0)]
					InvalidMainchainSignature,
					#[codec(index = 1)]
					InvalidSidechainSignature,
					#[codec(index = 2)]
					InvalidTxInput,
					#[codec(index = 3)]
					InvalidMainchainPubKey,
					#[codec(index = 4)]
					InvalidSidechainPubKey,
					#[codec(index = 5)]
					InvalidAuraKey,
					#[codec(index = 6)]
					InvalidGrandpaKey,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum StakeError {
					#[codec(index = 0)]
					InvalidStake,
					#[codec(index = 1)]
					UnknownStake,
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum CommitteeMember<_0, _1> {
				#[codec(index = 0)]
				Permissioned { id: _0, keys: _1 },
				#[codec(index = 1)]
				Registered {
					id: _0,
					keys: _1,
					stake_pool_pub_key: runtime_types::sidechain_domain::StakePoolPublicKey,
				},
			}
		}
		pub mod bounded_collections {
			use super::runtime_types;
			pub mod bounded_btree_map {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BoundedBTreeMap<_0, _1>(
					pub ::subxt::ext::subxt_core::utils::KeyedVec<_0, _1>,
				);
			}
			pub mod bounded_vec {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BoundedVec<_0>(pub ::subxt::ext::subxt_core::alloc::vec::Vec<_0>);
			}
			pub mod weak_bounded_vec {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct WeakBoundedVec<_0>(pub ::subxt::ext::subxt_core::alloc::vec::Vec<_0>);
			}
		}
		pub mod finality_grandpa {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct Equivocation<_0, _1, _2> {
				pub round_number: ::core::primitive::u64,
				pub identity: _0,
				pub first: (_1, _2),
				pub second: (_1, _2),
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct Precommit<_0, _1> {
				pub target_hash: _0,
				pub target_number: _1,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct Prevote<_0, _1> {
				pub target_hash: _0,
				pub target_number: _1,
			}
		}
		pub mod frame_support {
			use super::runtime_types;
			pub mod dispatch {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum DispatchClass {
					#[codec(index = 0)]
					Normal,
					#[codec(index = 1)]
					Operational,
					#[codec(index = 2)]
					Mandatory,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum Pays {
					#[codec(index = 0)]
					Yes,
					#[codec(index = 1)]
					No,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct PerDispatchClass<_0> {
					pub normal: _0,
					pub operational: _0,
					pub mandatory: _0,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum RawOrigin<_0> {
					#[codec(index = 0)]
					Root,
					#[codec(index = 1)]
					Signed(_0),
					#[codec(index = 2)]
					None,
				}
			}
			pub mod traits {
				use super::runtime_types;
				pub mod preimages {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub enum Bounded<_0, _1> {
						#[codec(index = 0)]
						Legacy {
							hash: ::subxt::ext::subxt_core::utils::H256,
						},
						#[codec(index = 1)]
						Inline(
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
						),
						#[codec(index = 2)]
						Lookup {
							hash: ::subxt::ext::subxt_core::utils::H256,
							len: ::core::primitive::u32,
						},
						__Ignore(::core::marker::PhantomData<(_0, _1)>),
					}
				}
				pub mod tokens {
					use super::runtime_types;
					pub mod misc {
						use super::runtime_types;
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub enum BalanceStatus {
							#[codec(index = 0)]
							Free,
							#[codec(index = 1)]
							Reserved,
						}
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub struct IdAmount<_0, _1> {
							pub id: _0,
							pub amount: _1,
						}
					}
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct PalletId(pub [::core::primitive::u8; 8usize]);
		}
		pub mod frame_system {
			use super::runtime_types;
			pub mod extensions {
				use super::runtime_types;
				pub mod check_genesis {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct CheckGenesis;
				}
				pub mod check_mortality {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct CheckMortality(pub runtime_types::sp_runtime::generic::era::Era);
				}
				pub mod check_non_zero_sender {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct CheckNonZeroSender;
				}
				pub mod check_nonce {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct CheckNonce(#[codec(compact)] pub ::core::primitive::u32);
				}
				pub mod check_spec_version {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct CheckSpecVersion;
				}
				pub mod check_tx_version {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct CheckTxVersion;
				}
				pub mod check_weight {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct CheckWeight;
				}
			}
			pub mod limits {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BlockLength {
					pub max: runtime_types::frame_support::dispatch::PerDispatchClass<
						::core::primitive::u32,
					>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BlockWeights {
					pub base_block: runtime_types::sp_weights::weight_v2::Weight,
					pub max_block: runtime_types::sp_weights::weight_v2::Weight,
					pub per_class: runtime_types::frame_support::dispatch::PerDispatchClass<
						runtime_types::frame_system::limits::WeightsPerClass,
					>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct WeightsPerClass {
					pub base_extrinsic: runtime_types::sp_weights::weight_v2::Weight,
					pub max_extrinsic:
						::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
					pub max_total:
						::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
					pub reserved:
						::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
				}
			}
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Make some on-chain remark."]
					#[doc = ""]
					#[doc = "Can be executed by every `origin`."]
					remark {
						remark: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					},
					#[codec(index = 1)]
					#[doc = "Set the number of pages in the WebAssembly environment's heap."]
					set_heap_pages { pages: ::core::primitive::u64 },
					#[codec(index = 2)]
					#[doc = "Set the new runtime code."]
					set_code {
						code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					},
					#[codec(index = 3)]
					#[doc = "Set the new runtime code without doing any checks of the given `code`."]
					#[doc = ""]
					#[doc = "Note that runtime upgrades will not run if this is called with a not-increasing spec"]
					#[doc = "version!"]
					set_code_without_checks {
						code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					},
					#[codec(index = 4)]
					#[doc = "Set some items of storage."]
					set_storage {
						items: ::subxt::ext::subxt_core::alloc::vec::Vec<(
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						)>,
					},
					#[codec(index = 5)]
					#[doc = "Kill some items from storage."]
					kill_storage {
						keys: ::subxt::ext::subxt_core::alloc::vec::Vec<
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						>,
					},
					#[codec(index = 6)]
					#[doc = "Kill all storage items with a key that starts with the given prefix."]
					#[doc = ""]
					#[doc = "**NOTE:** We rely on the Root origin to provide us the number of subkeys under"]
					#[doc = "the prefix we are removing to accurately calculate the weight of this function."]
					kill_prefix {
						prefix: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						subkeys: ::core::primitive::u32,
					},
					#[codec(index = 7)]
					#[doc = "Make some on-chain remark and emit event."]
					remark_with_event {
						remark: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					},
					#[codec(index = 9)]
					#[doc = "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied"]
					#[doc = "later."]
					#[doc = ""]
					#[doc = "This call requires Root origin."]
					authorize_upgrade { code_hash: ::subxt::ext::subxt_core::utils::H256 },
					#[codec(index = 10)]
					#[doc = "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied"]
					#[doc = "later."]
					#[doc = ""]
					#[doc = "WARNING: This authorizes an upgrade that will take place without any safety checks, for"]
					#[doc = "example that the spec name remains the same and that the version number increases. Not"]
					#[doc = "recommended for normal use. Use `authorize_upgrade` instead."]
					#[doc = ""]
					#[doc = "This call requires Root origin."]
					authorize_upgrade_without_checks {
						code_hash: ::subxt::ext::subxt_core::utils::H256,
					},
					#[codec(index = 11)]
					#[doc = "Provide the preimage (runtime binary) `code` for an upgrade that has been authorized."]
					#[doc = ""]
					#[doc = "If the authorization required a version check, this call will ensure the spec name"]
					#[doc = "remains unchanged and that the spec version has increased."]
					#[doc = ""]
					#[doc = "Depending on the runtime's `OnSetCode` configuration, this function may directly apply"]
					#[doc = "the new `code` in the same block or attempt to schedule the upgrade."]
					#[doc = ""]
					#[doc = "All origins are allowed."]
					apply_authorized_upgrade {
						code: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Error for the System pallet"]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "The name of specification does not match between the current runtime"]
					#[doc = "and the new runtime."]
					InvalidSpecName,
					#[codec(index = 1)]
					#[doc = "The specification version is not allowed to decrease between the current runtime"]
					#[doc = "and the new runtime."]
					SpecVersionNeedsToIncrease,
					#[codec(index = 2)]
					#[doc = "Failed to extract the runtime version from the new runtime."]
					#[doc = ""]
					#[doc = "Either calling `Core_version` or decoding `RuntimeVersion` failed."]
					FailedToExtractRuntimeVersion,
					#[codec(index = 3)]
					#[doc = "Suicide called when the account has non-default composite data."]
					NonDefaultComposite,
					#[codec(index = 4)]
					#[doc = "There is a non-zero reference count preventing the account from being purged."]
					NonZeroRefCount,
					#[codec(index = 5)]
					#[doc = "The origin filter prevent the call to be dispatched."]
					CallFiltered,
					#[codec(index = 6)]
					#[doc = "A multi-block migration is ongoing and prevents the current code from being replaced."]
					MultiBlockMigrationsOngoing,
					#[codec(index = 7)]
					#[doc = "No upgrade authorized."]
					NothingAuthorized,
					#[codec(index = 8)]
					#[doc = "The submitted code is not authorized."]
					Unauthorized,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Event for the System pallet."]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "An extrinsic completed successfully."]
					ExtrinsicSuccess {
						dispatch_info: runtime_types::frame_system::DispatchEventInfo,
					},
					#[codec(index = 1)]
					#[doc = "An extrinsic failed."]
					ExtrinsicFailed {
						dispatch_error: runtime_types::sp_runtime::DispatchError,
						dispatch_info: runtime_types::frame_system::DispatchEventInfo,
					},
					#[codec(index = 2)]
					#[doc = "`:code` was updated."]
					CodeUpdated,
					#[codec(index = 3)]
					#[doc = "A new account was created."]
					NewAccount { account: ::subxt::ext::subxt_core::utils::AccountId32 },
					#[codec(index = 4)]
					#[doc = "An account was reaped."]
					KilledAccount { account: ::subxt::ext::subxt_core::utils::AccountId32 },
					#[codec(index = 5)]
					#[doc = "On on-chain remark happened."]
					Remarked {
						sender: ::subxt::ext::subxt_core::utils::AccountId32,
						hash: ::subxt::ext::subxt_core::utils::H256,
					},
					#[codec(index = 6)]
					#[doc = "An upgrade was authorized."]
					UpgradeAuthorized {
						code_hash: ::subxt::ext::subxt_core::utils::H256,
						check_version: ::core::primitive::bool,
					},
					#[codec(index = 7)]
					#[doc = "An invalid authorized upgrade was rejected while trying to apply it."]
					RejectedInvalidAuthorizedUpgrade {
						code_hash: ::subxt::ext::subxt_core::utils::H256,
						error: runtime_types::sp_runtime::DispatchError,
					},
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct AccountInfo<_0, _1> {
				pub nonce: _0,
				pub consumers: ::core::primitive::u32,
				pub providers: ::core::primitive::u32,
				pub sufficients: ::core::primitive::u32,
				pub data: _1,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct CodeUpgradeAuthorization {
				pub code_hash: ::subxt::ext::subxt_core::utils::H256,
				pub check_version: ::core::primitive::bool,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct DispatchEventInfo {
				pub weight: runtime_types::sp_weights::weight_v2::Weight,
				pub class: runtime_types::frame_support::dispatch::DispatchClass,
				pub pays_fee: runtime_types::frame_support::dispatch::Pays,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct EventRecord<_0, _1> {
				pub phase: runtime_types::frame_system::Phase,
				pub event: _0,
				pub topics: ::subxt::ext::subxt_core::alloc::vec::Vec<_1>,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct LastRuntimeUpgradeInfo {
				#[codec(compact)]
				pub spec_version: ::core::primitive::u32,
				pub spec_name: ::subxt::ext::subxt_core::alloc::string::String,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum Phase {
				#[codec(index = 0)]
				ApplyExtrinsic(::core::primitive::u32),
				#[codec(index = 1)]
				Finalization,
				#[codec(index = 2)]
				Initialization,
			}
		}
		pub mod midnight_node_ledger {
			use super::runtime_types;
			pub mod common {
				use super::runtime_types;
				pub mod types {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub enum Op {
						#[codec(index = 0)]
						Call {
							address:
								::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
							entry_point:
								::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						},
						#[codec(index = 1)]
						Deploy {
							address:
								::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						},
						#[codec(index = 2)]
						Maintain {
							address:
								::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						},
						#[codec(index = 3)]
						ClaimMint {
							value: ::core::primitive::u128,
							coin_type: [::core::primitive::u8; 32usize],
						},
					}
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct Tx {
						pub hash: [::core::primitive::u8; 32usize],
						pub operations: ::subxt::ext::subxt_core::alloc::vec::Vec<
							runtime_types::midnight_node_ledger::common::types::Op,
						>,
						pub identifiers: ::subxt::ext::subxt_core::alloc::vec::Vec<
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						>,
						pub has_fallible_coins: ::core::primitive::bool,
						pub has_guaranteed_coins: ::core::primitive::bool,
					}
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct UtxoInfo {
						pub address: [::core::primitive::u8; 32usize],
						pub token_type: [::core::primitive::u8; 32usize],
						pub intent_hash: [::core::primitive::u8; 32usize],
						pub value: ::core::primitive::u128,
						pub output_no: ::core::primitive::u32,
					}
				}
			}
			pub mod latest {
				use super::runtime_types;
				pub mod common {
					use super::runtime_types;
					pub mod types {
						use super::runtime_types;
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub enum DeserializationError {
							#[codec(index = 0)]
							NetworkId,
							#[codec(index = 1)]
							Transaction,
							#[codec(index = 2)]
							LedgerState,
							#[codec(index = 3)]
							ContractAddress,
							#[codec(index = 4)]
							PublicKey,
							#[codec(index = 5)]
							VersionedArenaKey,
							#[codec(index = 6)]
							UserAddress,
						}
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub enum InvalidError {
							#[codec(index = 0)]
							EffectsMismatch,
							#[codec(index = 1)]
							ContractAlreadyDeployed,
							#[codec(index = 2)]
							ContractNotPresent,
							#[codec(index = 3)]
							Zswap,
							#[codec(index = 4)]
							Transcript,
							#[codec(index = 5)]
							InsufficientClaimable,
							#[codec(index = 6)]
							VerifierKeyNotFound,
							#[codec(index = 7)]
							VerifierKeyAlreadyPresent,
							#[codec(index = 8)]
							ReplayCounterMismatch,
							#[codec(index = 9)]
							UnknownError,
						}
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub enum LedgerApiError {
							# [codec (index = 0)] Deserialization (runtime_types :: midnight_node_ledger :: latest :: common :: types :: DeserializationError ,) , # [codec (index = 1)] Serialization (runtime_types :: midnight_node_ledger :: latest :: common :: types :: SerializationError ,) , # [codec (index = 2)] Transaction (runtime_types :: midnight_node_ledger :: latest :: common :: types :: TransactionError ,) , # [codec (index = 3)] LedgerCacheError , # [codec (index = 4)] NoLedgerState , # [codec (index = 5)] LedgerStateScaleDecodingError , # [codec (index = 6)] ContractCallCostError , }
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub enum MalformedError {
							#[codec(index = 0)]
							VerifierKeyNotSet,
							#[codec(index = 1)]
							TransactionTooLarge,
							#[codec(index = 2)]
							VerifierKeyTooLarge,
							#[codec(index = 3)]
							VerifierKeyNotPresent,
							#[codec(index = 4)]
							ContractNotPresent,
							#[codec(index = 5)]
							InvalidProof,
							#[codec(index = 6)]
							BindingCommitmentOpeningInvalid,
							#[codec(index = 7)]
							NotNormalized,
							#[codec(index = 8)]
							FallibleWithoutCheckpoint,
							#[codec(index = 9)]
							ClaimReceiveFailed,
							#[codec(index = 10)]
							ClaimSpendFailed,
							#[codec(index = 11)]
							ClaimNullifierFailed,
							#[codec(index = 12)]
							ClaimCallFailed,
							#[codec(index = 13)]
							InvalidSchnorrProof,
							#[codec(index = 14)]
							UnclaimedCoinCom,
							#[codec(index = 15)]
							UnclaimedNullifier,
							#[codec(index = 16)]
							Unbalanced,
							#[codec(index = 17)]
							Zswap,
							#[codec(index = 18)]
							BuiltinDecode,
							#[codec(index = 19)]
							GuaranteedLimit,
							#[codec(index = 20)]
							MergingContracts,
							#[codec(index = 21)]
							CantMergeTypes,
							#[codec(index = 22)]
							ClaimOverflow,
							#[codec(index = 23)]
							ClaimCoinMismatch,
							#[codec(index = 24)]
							KeyNotInCommittee,
							#[codec(index = 25)]
							InvalidCommitteeSignature,
							#[codec(index = 26)]
							ThresholdMissed,
							#[codec(index = 27)]
							TooManyZswapEntries,
							#[codec(index = 28)]
							UnknownError,
						}
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub enum SerializationError {
							#[codec(index = 0)]
							TransactionIdentifier,
							#[codec(index = 1)]
							ZswapState,
							#[codec(index = 2)]
							LedgerState,
							#[codec(index = 3)]
							LedgerParameters,
							#[codec(index = 4)]
							ContractAddress,
							#[codec(index = 5)]
							ContractState,
							#[codec(index = 6)]
							ContractStateToJson,
							#[codec(index = 7)]
							UnknownType,
							#[codec(index = 8)]
							MerkleTreeDigest,
							#[codec(index = 9)]
							VersionedArenaKey,
						}
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub enum SystemTransactionError {
							#[codec(index = 0)]
							IllegalMint,
							#[codec(index = 1)]
							InsufficientTreasuryFunds,
							#[codec(index = 2)]
							CommitmentAlreadyPresent,
							#[codec(index = 3)]
							UnknownError,
							#[codec(index = 4)]
							ReplayProtectionFailure,
						}
						#[derive(
							:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
							:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
							:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
							:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
							Debug,
						)]
						# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
						#[decode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
						)]
						#[encode_as_type(
							crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
						)]
						pub enum TransactionError {
							# [codec (index = 0)] Invalid (runtime_types :: midnight_node_ledger :: latest :: common :: types :: InvalidError ,) , # [codec (index = 1)] Malformed (runtime_types :: midnight_node_ledger :: latest :: common :: types :: MalformedError ,) , # [codec (index = 2)] SystemTransaction (runtime_types :: midnight_node_ledger :: latest :: common :: types :: SystemTransactionError ,) , }
					}
				}
			}
		}
		pub mod midnight_node_runtime {
			use super::runtime_types;
			pub mod opaque {
				use super::runtime_types;
				pub mod cross_chain_app {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct Public(pub [::core::primitive::u8; 33usize]);
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SessionKeys {
					pub aura: runtime_types::sp_consensus_aura::sr25519::app_sr25519::Public,
					pub grandpa: runtime_types::sp_consensus_grandpa::app::Public,
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum OriginCaller {
				#[codec(index = 0)]
				system(
					runtime_types::frame_support::dispatch::RawOrigin<
						::subxt::ext::subxt_core::utils::AccountId32,
					>,
				),
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct Runtime;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum RuntimeCall {
				#[codec(index = 0)]
				System(runtime_types::frame_system::pallet::Call),
				#[codec(index = 1)]
				Timestamp(runtime_types::pallet_timestamp::pallet::Call),
				#[codec(index = 3)]
				Grandpa(runtime_types::pallet_grandpa::pallet::Call),
				#[codec(index = 5)]
				Midnight(runtime_types::pallet_midnight::pallet::Call),
				#[codec(index = 6)]
				Balances(runtime_types::pallet_balances::pallet::Call),
				#[codec(index = 7)]
				Sudo(runtime_types::pallet_sudo::pallet::Call),
				#[codec(index = 8)]
				SessionCommitteeManagement(
					runtime_types::pallet_session_validator_management::pallet::Call,
				),
				#[codec(index = 10)]
				RuntimeUpgrade(runtime_types::pallet_upgrade::pallet::Call),
				#[codec(index = 12)]
				NativeTokenManagement(runtime_types::pallet_native_token_management::pallet::Call),
				#[codec(index = 13)]
				NativeTokenObservation(
					runtime_types::pallet_native_token_observation::pallet::Call,
				),
				#[codec(index = 15)]
				Preimage(runtime_types::pallet_preimage::pallet::Call),
				#[codec(index = 16)]
				MultiBlockMigrations(runtime_types::pallet_migrations::pallet::Call),
				#[codec(index = 17)]
				PalletSession(runtime_types::pallet_session::pallet::Call),
				#[codec(index = 18)]
				Scheduler(runtime_types::pallet_scheduler::pallet::Call),
				#[codec(index = 19)]
				TxPause(runtime_types::pallet_tx_pause::pallet::Call),
				#[codec(index = 21)]
				Beefy(runtime_types::pallet_beefy::pallet::Call),
				#[codec(index = 31)]
				GovernedMap(runtime_types::pallet_governed_map::pallet::Call),
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum RuntimeError {
				#[codec(index = 0)]
				System(runtime_types::frame_system::pallet::Error),
				#[codec(index = 3)]
				Grandpa(runtime_types::pallet_grandpa::pallet::Error),
				#[codec(index = 5)]
				Midnight(runtime_types::pallet_midnight::pallet::Error),
				#[codec(index = 6)]
				Balances(runtime_types::pallet_balances::pallet::Error),
				#[codec(index = 7)]
				Sudo(runtime_types::pallet_sudo::pallet::Error),
				#[codec(index = 8)]
				SessionCommitteeManagement(
					runtime_types::pallet_session_validator_management::pallet::Error,
				),
				#[codec(index = 10)]
				RuntimeUpgrade(runtime_types::pallet_upgrade::pallet::Error),
				#[codec(index = 12)]
				NativeTokenManagement(runtime_types::pallet_native_token_management::pallet::Error),
				#[codec(index = 13)]
				NativeTokenObservation(
					runtime_types::pallet_native_token_observation::pallet::Error,
				),
				#[codec(index = 15)]
				Preimage(runtime_types::pallet_preimage::pallet::Error),
				#[codec(index = 16)]
				MultiBlockMigrations(runtime_types::pallet_migrations::pallet::Error),
				#[codec(index = 17)]
				PalletSession(runtime_types::pallet_session::pallet::Error),
				#[codec(index = 18)]
				Scheduler(runtime_types::pallet_scheduler::pallet::Error),
				#[codec(index = 19)]
				TxPause(runtime_types::pallet_tx_pause::pallet::Error),
				#[codec(index = 21)]
				Beefy(runtime_types::pallet_beefy::pallet::Error),
				#[codec(index = 31)]
				GovernedMap(runtime_types::pallet_governed_map::pallet::Error),
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum RuntimeEvent {
				#[codec(index = 0)]
				System(runtime_types::frame_system::pallet::Event),
				#[codec(index = 3)]
				Grandpa(runtime_types::pallet_grandpa::pallet::Event),
				#[codec(index = 5)]
				Midnight(runtime_types::pallet_midnight::pallet::Event),
				#[codec(index = 6)]
				Balances(runtime_types::pallet_balances::pallet::Event),
				#[codec(index = 7)]
				Sudo(runtime_types::pallet_sudo::pallet::Event),
				#[codec(index = 8)]
				SessionCommitteeManagement(
					runtime_types::pallet_session_validator_management::pallet::Event,
				),
				#[codec(index = 10)]
				RuntimeUpgrade(runtime_types::pallet_upgrade::pallet::Event),
				#[codec(index = 12)]
				NativeTokenManagement(runtime_types::pallet_native_token_management::pallet::Event),
				#[codec(index = 13)]
				NativeTokenObservation(
					runtime_types::pallet_native_token_observation::pallet::Event,
				),
				#[codec(index = 15)]
				Preimage(runtime_types::pallet_preimage::pallet::Event),
				#[codec(index = 16)]
				MultiBlockMigrations(runtime_types::pallet_migrations::pallet::Event),
				#[codec(index = 17)]
				PalletSession(runtime_types::pallet_session::pallet::Event),
				#[codec(index = 18)]
				Scheduler(runtime_types::pallet_scheduler::pallet::Event),
				#[codec(index = 19)]
				TxPause(runtime_types::pallet_tx_pause::pallet::Event),
				#[codec(index = 30)]
				Session(runtime_types::pallet_partner_chains_session::pallet::Event),
			}
		}
		pub mod midnight_primitives_mainchain_follower {
			use super::runtime_types;
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct CreateData {
					pub value: ::core::primitive::u128,
					pub owner: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					pub utxo_tx_hash: [::core::primitive::u8; 32usize],
					pub utxo_tx_index: ::core::primitive::u16,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct DeregistrationData {
					pub cardano_address:
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					pub dust_address:
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ObservedUtxo { pub header : runtime_types :: midnight_primitives_mainchain_follower :: types :: ObservedUtxoHeader , pub data : runtime_types :: midnight_primitives_mainchain_follower :: types :: ObservedUtxoData , }
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum ObservedUtxoData {
					# [codec (index = 0)] Registration (runtime_types :: midnight_primitives_mainchain_follower :: types :: RegistrationData ,) , # [codec (index = 1)] Deregistration (runtime_types :: midnight_primitives_mainchain_follower :: types :: DeregistrationData ,) , # [codec (index = 2)] AssetCreate (runtime_types :: midnight_primitives_mainchain_follower :: types :: CreateData ,) , # [codec (index = 3)] AssetSpend (runtime_types :: midnight_primitives_mainchain_follower :: types :: SpendData ,) , }
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ObservedUtxoHeader { pub tx_position : runtime_types :: midnight_primitives_native_token_observation :: CardanoPosition , pub tx_hash : runtime_types :: sidechain_domain :: McTxHash , pub utxo_tx_hash : runtime_types :: sidechain_domain :: McTxHash , pub utxo_index : runtime_types :: midnight_primitives_mainchain_follower :: types :: UtxoIndexInTx , }
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct RegistrationData {
					pub cardano_address:
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					pub dust_address:
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SpendData {
					pub value: ::core::primitive::u128,
					pub owner: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					pub utxo_tx_hash: [::core::primitive::u8; 32usize],
					pub utxo_tx_index: ::core::primitive::u16,
					pub spending_tx_hash: [::core::primitive::u8; 32usize],
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct UtxoIndexInTx(pub ::core::primitive::u16);
			}
		}
		pub mod midnight_primitives_native_token_observation {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct CardanoPosition {
				pub block_hash: [::core::primitive::u8; 32usize],
				pub block_number: ::core::primitive::u32,
				pub tx_index_in_block: ::core::primitive::u32,
			}
		}
		pub mod midnight_primitives_upgrade {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct UpgradeProposal {
				pub spec_version: ::core::primitive::u32,
				pub runtime_hash: ::subxt::ext::subxt_core::utils::H256,
			}
		}
		pub mod pallet_balances {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Transfer some liquid free balance to another account."]
					#[doc = ""]
					#[doc = "`transfer_allow_death` will set the `FreeBalance` of the sender and receiver."]
					#[doc = "If the sender's account is below the existential deposit as a result"]
					#[doc = "of the transfer, the account will be reaped."]
					#[doc = ""]
					#[doc = "The dispatch origin for this call must be `Signed` by the transactor."]
					transfer_allow_death {
						dest: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
						#[codec(compact)]
						value: ::core::primitive::u128,
					},
					#[codec(index = 2)]
					#[doc = "Exactly as `transfer_allow_death`, except the origin must be root and the source account"]
					#[doc = "may be specified."]
					force_transfer {
						source: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
						dest: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
						#[codec(compact)]
						value: ::core::primitive::u128,
					},
					#[codec(index = 3)]
					#[doc = "Same as the [`transfer_allow_death`] call, but with a check that the transfer will not"]
					#[doc = "kill the origin account."]
					#[doc = ""]
					#[doc = "99% of the time you want [`transfer_allow_death`] instead."]
					#[doc = ""]
					#[doc = "[`transfer_allow_death`]: struct.Pallet.html#method.transfer"]
					transfer_keep_alive {
						dest: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
						#[codec(compact)]
						value: ::core::primitive::u128,
					},
					#[codec(index = 4)]
					#[doc = "Transfer the entire transferable balance from the caller account."]
					#[doc = ""]
					#[doc = "NOTE: This function only attempts to transfer _transferable_ balances. This means that"]
					#[doc = "any locked, reserved, or existential deposits (when `keep_alive` is `true`), will not be"]
					#[doc = "transferred by this function. To ensure that this function results in a killed account,"]
					#[doc = "you might need to prepare the account by removing any reference counters, storage"]
					#[doc = "deposits, etc..."]
					#[doc = ""]
					#[doc = "The dispatch origin of this call must be Signed."]
					#[doc = ""]
					#[doc = "- `dest`: The recipient of the transfer."]
					#[doc = "- `keep_alive`: A boolean to determine if the `transfer_all` operation should send all"]
					#[doc = "  of the funds the account has, causing the sender account to be killed (false), or"]
					#[doc = "  transfer everything except at least the existential deposit, which will guarantee to"]
					#[doc = "  keep the sender account alive (true)."]
					transfer_all {
						dest: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
						keep_alive: ::core::primitive::bool,
					},
					#[codec(index = 5)]
					#[doc = "Unreserve some balance from a user by force."]
					#[doc = ""]
					#[doc = "Can only be called by ROOT."]
					force_unreserve {
						who: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 6)]
					#[doc = "Upgrade a specified account."]
					#[doc = ""]
					#[doc = "- `origin`: Must be `Signed`."]
					#[doc = "- `who`: The account to be upgraded."]
					#[doc = ""]
					#[doc = "This will waive the transaction fee if at least all but 10% of the accounts needed to"]
					#[doc = "be upgraded. (We let some not have to be upgraded just in order to allow for the"]
					#[doc = "possibility of churn)."]
					upgrade_accounts {
						who: ::subxt::ext::subxt_core::alloc::vec::Vec<
							::subxt::ext::subxt_core::utils::AccountId32,
						>,
					},
					#[codec(index = 8)]
					#[doc = "Set the regular balance of a given account."]
					#[doc = ""]
					#[doc = "The dispatch origin for this call is `root`."]
					force_set_balance {
						who: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
						#[codec(compact)]
						new_free: ::core::primitive::u128,
					},
					#[codec(index = 9)]
					#[doc = "Adjust the total issuance in a saturating way."]
					#[doc = ""]
					#[doc = "Can only be called by root and always needs a positive `delta`."]
					#[doc = ""]
					#[doc = "# Example"]
					force_adjust_total_issuance {
						direction: runtime_types::pallet_balances::types::AdjustmentDirection,
						#[codec(compact)]
						delta: ::core::primitive::u128,
					},
					#[codec(index = 10)]
					#[doc = "Burn the specified liquid free balance from the origin account."]
					#[doc = ""]
					#[doc = "If the origin's account ends up below the existential deposit as a result"]
					#[doc = "of the burn and `keep_alive` is false, the account will be reaped."]
					#[doc = ""]
					#[doc = "Unlike sending funds to a _burn_ address, which merely makes the funds inaccessible,"]
					#[doc = "this `burn` operation will reduce total issuance by the amount _burned_."]
					burn {
						#[codec(compact)]
						value: ::core::primitive::u128,
						keep_alive: ::core::primitive::bool,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Vesting balance too high to send value."]
					VestingBalance,
					#[codec(index = 1)]
					#[doc = "Account liquidity restrictions prevent withdrawal."]
					LiquidityRestrictions,
					#[codec(index = 2)]
					#[doc = "Balance too low to send value."]
					InsufficientBalance,
					#[codec(index = 3)]
					#[doc = "Value too low to create account due to existential deposit."]
					ExistentialDeposit,
					#[codec(index = 4)]
					#[doc = "Transfer/payment would kill account."]
					Expendability,
					#[codec(index = 5)]
					#[doc = "A vesting schedule already exists for this account."]
					ExistingVestingSchedule,
					#[codec(index = 6)]
					#[doc = "Beneficiary account must pre-exist."]
					DeadAccount,
					#[codec(index = 7)]
					#[doc = "Number of named reserves exceed `MaxReserves`."]
					TooManyReserves,
					#[codec(index = 8)]
					#[doc = "Number of holds exceed `VariantCountOf<T::RuntimeHoldReason>`."]
					TooManyHolds,
					#[codec(index = 9)]
					#[doc = "Number of freezes exceed `MaxFreezes`."]
					TooManyFreezes,
					#[codec(index = 10)]
					#[doc = "The issuance cannot be modified since it is already deactivated."]
					IssuanceDeactivated,
					#[codec(index = 11)]
					#[doc = "The delta cannot be zero."]
					DeltaZero,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "An account was created with some free balance."]
					Endowed {
						account: ::subxt::ext::subxt_core::utils::AccountId32,
						free_balance: ::core::primitive::u128,
					},
					#[codec(index = 1)]
					#[doc = "An account was removed whose balance was non-zero but below ExistentialDeposit,"]
					#[doc = "resulting in an outright loss."]
					DustLost {
						account: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 2)]
					#[doc = "Transfer succeeded."]
					Transfer {
						from: ::subxt::ext::subxt_core::utils::AccountId32,
						to: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 3)]
					#[doc = "A balance was set by root."]
					BalanceSet {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						free: ::core::primitive::u128,
					},
					#[codec(index = 4)]
					#[doc = "Some balance was reserved (moved from free to reserved)."]
					Reserved {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 5)]
					#[doc = "Some balance was unreserved (moved from reserved to free)."]
					Unreserved {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 6)]
					#[doc = "Some balance was moved from the reserve of the first account to the second account."]
					#[doc = "Final argument indicates the destination balance type."]
					ReserveRepatriated {
						from: ::subxt::ext::subxt_core::utils::AccountId32,
						to: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
						destination_status:
							runtime_types::frame_support::traits::tokens::misc::BalanceStatus,
					},
					#[codec(index = 7)]
					#[doc = "Some amount was deposited (e.g. for transaction fees)."]
					Deposit {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 8)]
					#[doc = "Some amount was withdrawn from the account (e.g. for transaction fees)."]
					Withdraw {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 9)]
					#[doc = "Some amount was removed from the account (e.g. for misbehavior)."]
					Slashed {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 10)]
					#[doc = "Some amount was minted into an account."]
					Minted {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 11)]
					#[doc = "Some amount was burned from an account."]
					Burned {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 12)]
					#[doc = "Some amount was suspended from an account (it can be restored later)."]
					Suspended {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 13)]
					#[doc = "Some amount was restored into an account."]
					Restored {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 14)]
					#[doc = "An account was upgraded."]
					Upgraded { who: ::subxt::ext::subxt_core::utils::AccountId32 },
					#[codec(index = 15)]
					#[doc = "Total issuance was increased by `amount`, creating a credit to be balanced."]
					Issued { amount: ::core::primitive::u128 },
					#[codec(index = 16)]
					#[doc = "Total issuance was decreased by `amount`, creating a debt to be balanced."]
					Rescinded { amount: ::core::primitive::u128 },
					#[codec(index = 17)]
					#[doc = "Some balance was locked."]
					Locked {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 18)]
					#[doc = "Some balance was unlocked."]
					Unlocked {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 19)]
					#[doc = "Some balance was frozen."]
					Frozen {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 20)]
					#[doc = "Some balance was thawed."]
					Thawed {
						who: ::subxt::ext::subxt_core::utils::AccountId32,
						amount: ::core::primitive::u128,
					},
					#[codec(index = 21)]
					#[doc = "The `TotalIssuance` was forcefully changed."]
					TotalIssuanceForced {
						old: ::core::primitive::u128,
						new: ::core::primitive::u128,
					},
				}
			}
			pub mod types {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct AccountData<_0> {
					pub free: _0,
					pub reserved: _0,
					pub frozen: _0,
					pub flags: runtime_types::pallet_balances::types::ExtraFlags,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum AdjustmentDirection {
					#[codec(index = 0)]
					Increase,
					#[codec(index = 1)]
					Decrease,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BalanceLock<_0> {
					pub id: [::core::primitive::u8; 8usize],
					pub amount: _0,
					pub reasons: runtime_types::pallet_balances::types::Reasons,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ExtraFlags(pub ::core::primitive::u128);
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum Reasons {
					#[codec(index = 0)]
					Fee,
					#[codec(index = 1)]
					Misc,
					#[codec(index = 2)]
					All,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ReserveData<_0, _1> {
					pub id: _0,
					pub amount: _1,
				}
			}
		}
		pub mod pallet_beefy {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
					#[doc = "equivocation proof and validate the given key ownership proof"]
					#[doc = "against the extracted offender. If both are valid, the offence"]
					#[doc = "will be reported."]
					report_double_voting {
						equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::sp_consensus_beefy::DoubleVotingProof<
								::core::primitive::u32,
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Signature,
							>,
						>,
						key_owner_proof: runtime_types::sp_core::Void,
					},
					#[codec(index = 1)]
					#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
					#[doc = "equivocation proof and validate the given key ownership proof"]
					#[doc = "against the extracted offender. If both are valid, the offence"]
					#[doc = "will be reported."]
					#[doc = ""]
					#[doc = "This extrinsic must be called unsigned and it is expected that only"]
					#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
					#[doc = "if the block author is defined it will be defined as the equivocation"]
					#[doc = "reporter."]
					report_double_voting_unsigned {
						equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::sp_consensus_beefy::DoubleVotingProof<
								::core::primitive::u32,
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Signature,
							>,
						>,
						key_owner_proof: runtime_types::sp_core::Void,
					},
					#[codec(index = 2)]
					#[doc = "Reset BEEFY consensus by setting a new BEEFY genesis at `delay_in_blocks` blocks in the"]
					#[doc = "future."]
					#[doc = ""]
					#[doc = "Note: `delay_in_blocks` has to be at least 1."]
					set_new_genesis { delay_in_blocks: ::core::primitive::u32 },
					#[codec(index = 3)]
					#[doc = "Report fork voting equivocation. This method will verify the equivocation proof"]
					#[doc = "and validate the given key ownership proof against the extracted offender."]
					#[doc = "If both are valid, the offence will be reported."]
					report_fork_voting {
						equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::sp_consensus_beefy::ForkVotingProof<
								runtime_types::sp_runtime::generic::header::Header<
									::core::primitive::u32,
								>,
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
								runtime_types::sp_mmr_primitives::AncestryProof<
									::subxt::ext::subxt_core::utils::H256,
								>,
							>,
						>,
						key_owner_proof: runtime_types::sp_core::Void,
					},
					#[codec(index = 4)]
					#[doc = "Report fork voting equivocation. This method will verify the equivocation proof"]
					#[doc = "and validate the given key ownership proof against the extracted offender."]
					#[doc = "If both are valid, the offence will be reported."]
					#[doc = ""]
					#[doc = "This extrinsic must be called unsigned and it is expected that only"]
					#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
					#[doc = "if the block author is defined it will be defined as the equivocation"]
					#[doc = "reporter."]
					report_fork_voting_unsigned {
						equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::sp_consensus_beefy::ForkVotingProof<
								runtime_types::sp_runtime::generic::header::Header<
									::core::primitive::u32,
								>,
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
								runtime_types::sp_mmr_primitives::AncestryProof<
									::subxt::ext::subxt_core::utils::H256,
								>,
							>,
						>,
						key_owner_proof: runtime_types::sp_core::Void,
					},
					#[codec(index = 5)]
					#[doc = "Report future block voting equivocation. This method will verify the equivocation proof"]
					#[doc = "and validate the given key ownership proof against the extracted offender."]
					#[doc = "If both are valid, the offence will be reported."]
					report_future_block_voting {
						equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::sp_consensus_beefy::FutureBlockVotingProof<
								::core::primitive::u32,
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
							>,
						>,
						key_owner_proof: runtime_types::sp_core::Void,
					},
					#[codec(index = 6)]
					#[doc = "Report future block voting equivocation. This method will verify the equivocation proof"]
					#[doc = "and validate the given key ownership proof against the extracted offender."]
					#[doc = "If both are valid, the offence will be reported."]
					#[doc = ""]
					#[doc = "This extrinsic must be called unsigned and it is expected that only"]
					#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
					#[doc = "if the block author is defined it will be defined as the equivocation"]
					#[doc = "reporter."]
					report_future_block_voting_unsigned {
						equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::sp_consensus_beefy::FutureBlockVotingProof<
								::core::primitive::u32,
								runtime_types::sp_consensus_beefy::ecdsa_crypto::Public,
							>,
						>,
						key_owner_proof: runtime_types::sp_core::Void,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "A key ownership proof provided as part of an equivocation report is invalid."]
					InvalidKeyOwnershipProof,
					#[codec(index = 1)]
					#[doc = "A double voting proof provided as part of an equivocation report is invalid."]
					InvalidDoubleVotingProof,
					#[codec(index = 2)]
					#[doc = "A fork voting proof provided as part of an equivocation report is invalid."]
					InvalidForkVotingProof,
					#[codec(index = 3)]
					#[doc = "A future block voting proof provided as part of an equivocation report is invalid."]
					InvalidFutureBlockVotingProof,
					#[codec(index = 4)]
					#[doc = "The session of the equivocation proof is invalid"]
					InvalidEquivocationProofSession,
					#[codec(index = 5)]
					#[doc = "A given equivocation report is valid but already previously reported."]
					DuplicateOffenceReport,
					#[codec(index = 6)]
					#[doc = "Submitted configuration is invalid."]
					InvalidConfiguration,
				}
			}
		}
		pub mod pallet_governed_map {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Inherent to register any changes in the state of the Governed Map on Cardano compared to the state currently stored in the pallet."]
					register_changes {
						changes:
							runtime_types::bounded_collections::bounded_btree_map::BoundedBTreeMap<
								runtime_types::sidechain_domain::byte_string::BoundedString,
								::core::option::Option<
									runtime_types::bounded_collections::bounded_vec::BoundedVec<
										::core::primitive::u8,
									>,
								>,
							>,
					},
					#[codec(index = 1)]
					#[doc = "Changes the address of the Governed Map validator used for observation."]
					#[doc = ""]
					#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
					set_main_chain_scripts {
						new_main_chain_script: runtime_types::sp_governed_map::MainChainScriptsV1,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Error type used  by this pallet's extrinsics"]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Signals that the inherent has been called again in the same block"]
					InherentCalledTwice,
					#[codec(index = 1)]
					#[doc = "MainChainScript is not set, registration of changes is not allowed"]
					MainChainScriptNotSet,
				}
			}
		}
		pub mod pallet_grandpa {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
					#[doc = "equivocation proof and validate the given key ownership proof"]
					#[doc = "against the extracted offender. If both are valid, the offence"]
					#[doc = "will be reported."]
					report_equivocation {
						equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::sp_consensus_grandpa::EquivocationProof<
								::subxt::ext::subxt_core::utils::H256,
								::core::primitive::u32,
							>,
						>,
						key_owner_proof: runtime_types::sp_core::Void,
					},
					#[codec(index = 1)]
					#[doc = "Report voter equivocation/misbehavior. This method will verify the"]
					#[doc = "equivocation proof and validate the given key ownership proof"]
					#[doc = "against the extracted offender. If both are valid, the offence"]
					#[doc = "will be reported."]
					#[doc = ""]
					#[doc = "This extrinsic must be called unsigned and it is expected that only"]
					#[doc = "block authors will call it (validated in `ValidateUnsigned`), as such"]
					#[doc = "if the block author is defined it will be defined as the equivocation"]
					#[doc = "reporter."]
					report_equivocation_unsigned {
						equivocation_proof: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::sp_consensus_grandpa::EquivocationProof<
								::subxt::ext::subxt_core::utils::H256,
								::core::primitive::u32,
							>,
						>,
						key_owner_proof: runtime_types::sp_core::Void,
					},
					#[codec(index = 2)]
					#[doc = "Note that the current authority set of the GRANDPA finality gadget has stalled."]
					#[doc = ""]
					#[doc = "This will trigger a forced authority set change at the beginning of the next session, to"]
					#[doc = "be enacted `delay` blocks after that. The `delay` should be high enough to safely assume"]
					#[doc = "that the block signalling the forced change will not be re-orged e.g. 1000 blocks."]
					#[doc = "The block production rate (which may be slowed down because of finality lagging) should"]
					#[doc = "be taken into account when choosing the `delay`. The GRANDPA voters based on the new"]
					#[doc = "authority will start voting on top of `best_finalized_block_number` for new finalized"]
					#[doc = "blocks. `best_finalized_block_number` should be the highest of the latest finalized"]
					#[doc = "block of all validators of the new authority set."]
					#[doc = ""]
					#[doc = "Only callable by root."]
					note_stalled {
						delay: ::core::primitive::u32,
						best_finalized_block_number: ::core::primitive::u32,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Attempt to signal GRANDPA pause when the authority set isn't live"]
					#[doc = "(either paused or already pending pause)."]
					PauseFailed,
					#[codec(index = 1)]
					#[doc = "Attempt to signal GRANDPA resume when the authority set isn't paused"]
					#[doc = "(either live or already pending resume)."]
					ResumeFailed,
					#[codec(index = 2)]
					#[doc = "Attempt to signal GRANDPA change with one already pending."]
					ChangePending,
					#[codec(index = 3)]
					#[doc = "Cannot signal forced change so soon after last."]
					TooSoon,
					#[codec(index = 4)]
					#[doc = "A key ownership proof provided as part of an equivocation report is invalid."]
					InvalidKeyOwnershipProof,
					#[codec(index = 5)]
					#[doc = "An equivocation proof provided as part of an equivocation report is invalid."]
					InvalidEquivocationProof,
					#[codec(index = 6)]
					#[doc = "A given equivocation report is valid but already previously reported."]
					DuplicateOffenceReport,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "New authority set has been applied."]
					NewAuthorities {
						authority_set: ::subxt::ext::subxt_core::alloc::vec::Vec<(
							runtime_types::sp_consensus_grandpa::app::Public,
							::core::primitive::u64,
						)>,
					},
					#[codec(index = 1)]
					#[doc = "Current authority set has been paused."]
					Paused,
					#[codec(index = 2)]
					#[doc = "Current authority set has been resumed."]
					Resumed,
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct StoredPendingChange<_0> {
				pub scheduled_at: _0,
				pub delay: _0,
				pub next_authorities:
					runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<(
						runtime_types::sp_consensus_grandpa::app::Public,
						::core::primitive::u64,
					)>,
				pub forced: ::core::option::Option<_0>,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum StoredState<_0> {
				#[codec(index = 0)]
				Live,
				#[codec(index = 1)]
				PendingPause { scheduled_at: _0, delay: _0 },
				#[codec(index = 2)]
				Paused,
				#[codec(index = 3)]
				PendingResume { scheduled_at: _0, delay: _0 },
			}
		}
		pub mod pallet_midnight {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					send_mn_transaction {
						midnight_tx:
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					},
					#[codec(index = 1)]
					set_mn_tx_weight { new_weight: runtime_types::sp_weights::weight_v2::Weight },
					#[codec(index = 2)]
					override_d_parameter {
						d_parameter_override: ::core::option::Option<(
							::core::primitive::u16,
							::core::primitive::u16,
						)>,
					},
					#[codec(index = 3)]
					set_contract_call_weight {
						new_weight: runtime_types::sp_weights::weight_v2::Weight,
					},
					#[codec(index = 4)]
					set_tx_size_weight { new_weight: runtime_types::sp_weights::weight_v2::Weight },
					#[codec(index = 5)]
					set_safe_mode { mode: ::core::primitive::bool },
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct CallDetails {
					pub tx_hash: [::core::primitive::u8; 32usize],
					pub contract_address:
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ClaimMintDetails {
					pub tx_hash: [::core::primitive::u8; 32usize],
					pub coin_type: [::core::primitive::u8; 32usize],
					pub value: ::core::primitive::u128,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct DeploymentDetails {
					pub tx_hash: [::core::primitive::u8; 32usize],
					pub contract_address:
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					# [codec (index = 0)] NewStateOutOfBounds , # [codec (index = 1)] Deserialization (runtime_types :: midnight_node_ledger :: latest :: common :: types :: DeserializationError ,) , # [codec (index = 2)] Serialization (runtime_types :: midnight_node_ledger :: latest :: common :: types :: SerializationError ,) , # [codec (index = 3)] Transaction (runtime_types :: midnight_node_ledger :: latest :: common :: types :: TransactionError ,) , # [codec (index = 4)] LedgerCacheError , # [codec (index = 5)] NoLedgerState , # [codec (index = 6)] LedgerStateScaleDecodingError , # [codec (index = 7)] ContractCallCostError , }
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "A contract was called."]
					ContractCall(runtime_types::pallet_midnight::pallet::CallDetails),
					#[codec(index = 1)]
					#[doc = "A contract has been deployed."]
					ContractDeploy(runtime_types::pallet_midnight::pallet::DeploymentDetails),
					#[codec(index = 2)]
					#[doc = "A transaction has been applied (both the guaranteed and conditional part)."]
					TxApplied(runtime_types::pallet_midnight::pallet::TxAppliedDetails),
					#[codec(index = 3)]
					#[doc = "Contract ownership changes to enable snark upgrades"]
					ContractMaintain(runtime_types::pallet_midnight::pallet::MaintainDetails),
					#[codec(index = 4)]
					#[doc = "New payout minted."]
					PayoutMinted(runtime_types::pallet_midnight::pallet::PayoutDetails),
					#[codec(index = 5)]
					#[doc = "Payout was claimed."]
					ClaimMint(runtime_types::pallet_midnight::pallet::ClaimMintDetails),
					#[codec(index = 6)]
					#[doc = "Unshielded Tokens Trasfers"]
					UnshieldedTokens(
						runtime_types::pallet_midnight::pallet::UnshieldedTokensDetails,
					),
					#[codec(index = 7)]
					#[doc = "Partial Success."]
					TxPartialSuccess(runtime_types::pallet_midnight::pallet::TxAppliedDetails),
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct MaintainDetails {
					pub tx_hash: [::core::primitive::u8; 32usize],
					pub contract_address:
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct PayoutDetails {
					pub amount: ::core::primitive::u128,
					pub receiver: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct TxAppliedDetails {
					pub tx_hash: [::core::primitive::u8; 32usize],
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct UnshieldedTokensDetails {
					pub spent: ::subxt::ext::subxt_core::alloc::vec::Vec<
						runtime_types::midnight_node_ledger::common::types::UtxoInfo,
					>,
					pub created: ::subxt::ext::subxt_core::alloc::vec::Vec<
						runtime_types::midnight_node_ledger::common::types::UtxoInfo,
					>,
				}
			}
		}
		pub mod pallet_migrations {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Allows root to set a cursor to forcefully start, stop or forward the migration process."]
					#[doc = ""]
					#[doc = "Should normally not be needed and is only in place as emergency measure. Note that"]
					#[doc = "restarting the migration process in this manner will not call the"]
					#[doc = "[`MigrationStatusHandler::started`] hook or emit an `UpgradeStarted` event."]
					force_set_cursor {
						cursor: ::core::option::Option<
							runtime_types::pallet_migrations::MigrationCursor<
								runtime_types::bounded_collections::bounded_vec::BoundedVec<
									::core::primitive::u8,
								>,
								::core::primitive::u32,
							>,
						>,
					},
					#[codec(index = 1)]
					#[doc = "Allows root to set an active cursor to forcefully start/forward the migration process."]
					#[doc = ""]
					#[doc = "This is an edge-case version of [`Self::force_set_cursor`] that allows to set the"]
					#[doc = "`started_at` value to the next block number. Otherwise this would not be possible, since"]
					#[doc = "`force_set_cursor` takes an absolute block number. Setting `started_at` to `None`"]
					#[doc = "indicates that the current block number plus one should be used."]
					force_set_active_cursor {
						index: ::core::primitive::u32,
						inner_cursor: ::core::option::Option<
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
						>,
						started_at: ::core::option::Option<::core::primitive::u32>,
					},
					#[codec(index = 2)]
					#[doc = "Forces the onboarding of the migrations."]
					#[doc = ""]
					#[doc = "This process happens automatically on a runtime upgrade. It is in place as an emergency"]
					#[doc = "measurement. The cursor needs to be `None` for this to succeed."]
					force_onboard_mbms,
					#[codec(index = 3)]
					#[doc = "Clears the `Historic` set."]
					#[doc = ""]
					#[doc = "`map_cursor` must be set to the last value that was returned by the"]
					#[doc = "`HistoricCleared` event. The first time `None` can be used. `limit` must be chosen in a"]
					#[doc = "way that will result in a sensible weight."]
					clear_historic {
						selector: runtime_types::pallet_migrations::HistoricCleanupSelector<
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
						>,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "The operation cannot complete since some MBMs are ongoing."]
					Ongoing,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "A Runtime upgrade started."]
					#[doc = ""]
					#[doc = "Its end is indicated by `UpgradeCompleted` or `UpgradeFailed`."]
					UpgradeStarted { migrations: ::core::primitive::u32 },
					#[codec(index = 1)]
					#[doc = "The current runtime upgrade completed."]
					#[doc = ""]
					#[doc = "This implies that all of its migrations completed successfully as well."]
					UpgradeCompleted,
					#[codec(index = 2)]
					#[doc = "Runtime upgrade failed."]
					#[doc = ""]
					#[doc = "This is very bad and will require governance intervention."]
					UpgradeFailed,
					#[codec(index = 3)]
					#[doc = "A migration was skipped since it was already executed in the past."]
					MigrationSkipped { index: ::core::primitive::u32 },
					#[codec(index = 4)]
					#[doc = "A migration progressed."]
					MigrationAdvanced {
						index: ::core::primitive::u32,
						took: ::core::primitive::u32,
					},
					#[codec(index = 5)]
					#[doc = "A Migration completed."]
					MigrationCompleted {
						index: ::core::primitive::u32,
						took: ::core::primitive::u32,
					},
					#[codec(index = 6)]
					#[doc = "A Migration failed."]
					#[doc = ""]
					#[doc = "This implies that the whole upgrade failed and governance intervention is required."]
					MigrationFailed { index: ::core::primitive::u32, took: ::core::primitive::u32 },
					#[codec(index = 7)]
					#[doc = "The set of historical migrations has been cleared."]
					HistoricCleared {
						next_cursor: ::core::option::Option<
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						>,
					},
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct ActiveCursor<_0, _1> {
				pub index: ::core::primitive::u32,
				pub inner_cursor: ::core::option::Option<_0>,
				pub started_at: _1,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum HistoricCleanupSelector<_0> {
				#[codec(index = 0)]
				Specific(::subxt::ext::subxt_core::alloc::vec::Vec<_0>),
				#[codec(index = 1)]
				Wildcard {
					limit: ::core::option::Option<::core::primitive::u32>,
					previous_cursor: ::core::option::Option<
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					>,
				},
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum MigrationCursor<_0, _1> {
				#[codec(index = 0)]
				Active(runtime_types::pallet_migrations::ActiveCursor<_0, _1>),
				#[codec(index = 1)]
				Stuck,
			}
		}
		pub mod pallet_native_token_management {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Inherent that registers new native token transfer from the Cardano main chain and triggers"]
					#[doc = "the handler configured in [Config::TokenTransferHandler]."]
					#[doc = ""]
					#[doc = "Arguments:"]
					#[doc = "- `token_amount`: the total amount of tokens transferred since the last invocation of the inherent"]
					transfer_tokens {
						token_amount: runtime_types::sidechain_domain::NativeTokenAmount,
					},
					#[codec(index = 1)]
					#[doc = "Changes the main chain scripts used for observing native token transfers."]
					#[doc = ""]
					#[doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."]
					set_main_chain_scripts {
						native_token_policy_id: runtime_types::sidechain_domain::PolicyId,
						native_token_asset_name: runtime_types::sidechain_domain::AssetName,
						illiquid_supply_validator_address:
							runtime_types::sidechain_domain::MainchainAddress,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Error type used by the pallet's extrinsics"]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Indicates that the inherent was called while there was no main chain scripts set in the"]
					#[doc = "pallet's storage. This is indicative of a programming bug."]
					CalledWithoutConfiguration,
					#[codec(index = 1)]
					#[doc = "Indicates that the inherent was called a second time in the same block"]
					TransferAlreadyHandled,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Events emitted by this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "Signals that a new native token transfer has been processed by the pallet"]
					TokensTransfered(runtime_types::sidechain_domain::NativeTokenAmount),
				}
			}
		}
		pub mod pallet_native_token_observation {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					# [codec (index = 0)] process_tokens { utxos : :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < runtime_types :: midnight_primitives_mainchain_follower :: types :: ObservedUtxo > , next_cardano_position : runtime_types :: midnight_primitives_native_token_observation :: CardanoPosition , } , # [codec (index = 2)] # [doc = "Changes the mainchain address for the mapping validator contract"] # [doc = ""] # [doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."] set_mapping_validator_contract_address { address : :: subxt :: ext :: subxt_core :: alloc :: vec :: Vec < :: core :: primitive :: u8 > , } , }
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "A Cardano Wallet address was sent, but was longer than expected"]
					MaxCardanoAddrLengthExceeded,
					#[codec(index = 1)]
					MaxRegistrationsExceeded,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					# [codec (index = 0)] Added ((runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < :: core :: primitive :: u8 > , runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < runtime_types :: pallet_native_token_observation :: pallet :: MappingEntry > ,) ,) , # [codec (index = 1)] # [doc = "Tried to remove an element, but it was not found in the list of registrations"] AttemptedRemoveNonexistantElement (runtime_types :: pallet_native_token_observation :: pallet :: MappingEntry ,) , # [codec (index = 2)] # [doc = "Could not add registration"] CouldNotAddRegistration , # [codec (index = 3)] DuplicatedRegistration ((runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < :: core :: primitive :: u8 > , runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < runtime_types :: pallet_native_token_observation :: pallet :: MappingEntry > ,) ,) , # [codec (index = 4)] InvalidCardanoAddress , # [codec (index = 5)] InvalidDustAddress , # [codec (index = 6)] Registered ((runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < :: core :: primitive :: u8 > , runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < runtime_types :: pallet_native_token_observation :: pallet :: MappingEntry > ,) ,) , # [codec (index = 7)] # [doc = "Removed registrations"] Removed ((runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < :: core :: primitive :: u8 > , runtime_types :: pallet_native_token_observation :: pallet :: MappingEntry ,) ,) , # [codec (index = 8)] # [doc = "Removed single registration in order to add a new registration in order to respect length bound of registration list"] RemovedOld ((runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < :: core :: primitive :: u8 > , runtime_types :: pallet_native_token_observation :: pallet :: MappingEntry ,) ,) , # [codec (index = 9)] # [doc = "System transaction - the `SystemTx` struct is defined in the Node for now, but this event will contain a Ledger System Transaction"] SystemTx (runtime_types :: pallet_native_token_observation :: SystemTx ,) , }
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct MappingEntry {
					pub cardano_address:
						runtime_types::bounded_collections::bounded_vec::BoundedVec<
							::core::primitive::u8,
						>,
					pub dust_address: runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
					pub utxo_id: [::core::primitive::u8; 32usize],
					pub utxo_index: ::core::primitive::u16,
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct CmstHeader {
				pub block_hash: [::core::primitive::u8; 32usize],
				pub tx_index_in_block: ::core::primitive::u32,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct NgDPayload {
				pub events: ::subxt::ext::subxt_core::alloc::vec::Vec<
					runtime_types::pallet_native_token_observation::NgDPayloadEntry,
				>,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct NgDPayloadEntry {
				pub value: ::core::primitive::u128,
				pub owner: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				pub time: ::core::primitive::u64,
				pub action: runtime_types::pallet_native_token_observation::UtxoActionType,
				pub nonce: [::core::primitive::u8; 32usize],
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct SystemTx {
				pub header: runtime_types::pallet_native_token_observation::CmstHeader,
				pub body: runtime_types::pallet_native_token_observation::NgDPayload,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum UtxoActionType {
				#[codec(index = 0)]
				Create,
				#[codec(index = 1)]
				Destroy,
			}
		}
		pub mod pallet_partner_chains_session {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "New session has happened. Note that the argument is the session index, not the"]
					#[doc = "block number as the type might suggest."]
					NewSession { session_index: ::core::primitive::u32 },
				}
			}
		}
		pub mod pallet_preimage {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Register a preimage on-chain."]
					#[doc = ""]
					#[doc = "If the preimage was previously requested, no fees or deposits are taken for providing"]
					#[doc = "the preimage. Otherwise, a deposit is taken proportional to the size of the preimage."]
					note_preimage {
						bytes: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					},
					#[codec(index = 1)]
					#[doc = "Clear an unrequested preimage from the runtime storage."]
					#[doc = ""]
					#[doc = "If `len` is provided, then it will be a much cheaper operation."]
					#[doc = ""]
					#[doc = "- `hash`: The hash of the preimage to be removed from the store."]
					#[doc = "- `len`: The length of the preimage of `hash`."]
					unnote_preimage { hash: ::subxt::ext::subxt_core::utils::H256 },
					#[codec(index = 2)]
					#[doc = "Request a preimage be uploaded to the chain without paying any fees or deposits."]
					#[doc = ""]
					#[doc = "If the preimage requests has already been provided on-chain, we unreserve any deposit"]
					#[doc = "a user may have paid, and take the control of the preimage out of their hands."]
					request_preimage { hash: ::subxt::ext::subxt_core::utils::H256 },
					#[codec(index = 3)]
					#[doc = "Clear a previously made request for a preimage."]
					#[doc = ""]
					#[doc = "NOTE: THIS MUST NOT BE CALLED ON `hash` MORE TIMES THAN `request_preimage`."]
					unrequest_preimage { hash: ::subxt::ext::subxt_core::utils::H256 },
					#[codec(index = 4)]
					#[doc = "Ensure that the bulk of pre-images is upgraded."]
					#[doc = ""]
					#[doc = "The caller pays no fee if at least 90% of pre-images were successfully updated."]
					ensure_updated {
						hashes: ::subxt::ext::subxt_core::alloc::vec::Vec<
							::subxt::ext::subxt_core::utils::H256,
						>,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Preimage is too large to store on-chain."]
					TooBig,
					#[codec(index = 1)]
					#[doc = "Preimage has already been noted on-chain."]
					AlreadyNoted,
					#[codec(index = 2)]
					#[doc = "The user is not authorized to perform this action."]
					NotAuthorized,
					#[codec(index = 3)]
					#[doc = "The preimage cannot be removed since it has not yet been noted."]
					NotNoted,
					#[codec(index = 4)]
					#[doc = "A preimage may not be removed when there are outstanding requests."]
					Requested,
					#[codec(index = 5)]
					#[doc = "The preimage request cannot be removed since no outstanding requests exist."]
					NotRequested,
					#[codec(index = 6)]
					#[doc = "More than `MAX_HASH_UPGRADE_BULK_COUNT` hashes were requested to be upgraded at once."]
					TooMany,
					#[codec(index = 7)]
					#[doc = "Too few hashes were requested to be upgraded (i.e. zero)."]
					TooFew,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "A preimage has been noted."]
					Noted { hash: ::subxt::ext::subxt_core::utils::H256 },
					#[codec(index = 1)]
					#[doc = "A preimage has been requested."]
					Requested { hash: ::subxt::ext::subxt_core::utils::H256 },
					#[codec(index = 2)]
					#[doc = "A preimage has ben cleared."]
					Cleared { hash: ::subxt::ext::subxt_core::utils::H256 },
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum OldRequestStatus<_0, _1> {
				#[codec(index = 0)]
				Unrequested { deposit: (_0, _1), len: ::core::primitive::u32 },
				#[codec(index = 1)]
				Requested {
					deposit: ::core::option::Option<(_0, _1)>,
					count: ::core::primitive::u32,
					len: ::core::option::Option<::core::primitive::u32>,
				},
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum RequestStatus<_0, _1> {
				#[codec(index = 0)]
				Unrequested { ticket: (_0, _1), len: ::core::primitive::u32 },
				#[codec(index = 1)]
				Requested {
					maybe_ticket: ::core::option::Option<(_0, _1)>,
					count: ::core::primitive::u32,
					maybe_len: ::core::option::Option<::core::primitive::u32>,
				},
			}
		}
		pub mod pallet_scheduler {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Anonymously schedule a task."]
					schedule {
						when: ::core::primitive::u32,
						maybe_periodic: ::core::option::Option<(
							::core::primitive::u32,
							::core::primitive::u32,
						)>,
						priority: ::core::primitive::u8,
						call: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::midnight_node_runtime::RuntimeCall,
						>,
					},
					#[codec(index = 1)]
					#[doc = "Cancel an anonymously scheduled task."]
					cancel { when: ::core::primitive::u32, index: ::core::primitive::u32 },
					#[codec(index = 2)]
					#[doc = "Schedule a named task."]
					schedule_named {
						id: [::core::primitive::u8; 32usize],
						when: ::core::primitive::u32,
						maybe_periodic: ::core::option::Option<(
							::core::primitive::u32,
							::core::primitive::u32,
						)>,
						priority: ::core::primitive::u8,
						call: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::midnight_node_runtime::RuntimeCall,
						>,
					},
					#[codec(index = 3)]
					#[doc = "Cancel a named scheduled task."]
					cancel_named { id: [::core::primitive::u8; 32usize] },
					#[codec(index = 4)]
					#[doc = "Anonymously schedule a task after a delay."]
					schedule_after {
						after: ::core::primitive::u32,
						maybe_periodic: ::core::option::Option<(
							::core::primitive::u32,
							::core::primitive::u32,
						)>,
						priority: ::core::primitive::u8,
						call: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::midnight_node_runtime::RuntimeCall,
						>,
					},
					#[codec(index = 5)]
					#[doc = "Schedule a named task after a delay."]
					schedule_named_after {
						id: [::core::primitive::u8; 32usize],
						after: ::core::primitive::u32,
						maybe_periodic: ::core::option::Option<(
							::core::primitive::u32,
							::core::primitive::u32,
						)>,
						priority: ::core::primitive::u8,
						call: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::midnight_node_runtime::RuntimeCall,
						>,
					},
					#[codec(index = 6)]
					#[doc = "Set a retry configuration for a task so that, in case its scheduled run fails, it will"]
					#[doc = "be retried after `period` blocks, for a total amount of `retries` retries or until it"]
					#[doc = "succeeds."]
					#[doc = ""]
					#[doc = "Tasks which need to be scheduled for a retry are still subject to weight metering and"]
					#[doc = "agenda space, same as a regular task. If a periodic task fails, it will be scheduled"]
					#[doc = "normally while the task is retrying."]
					#[doc = ""]
					#[doc = "Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic"]
					#[doc = "clones of the original task. Their retry configuration will be derived from the"]
					#[doc = "original task's configuration, but will have a lower value for `remaining` than the"]
					#[doc = "original `total_retries`."]
					set_retry {
						task: (::core::primitive::u32, ::core::primitive::u32),
						retries: ::core::primitive::u8,
						period: ::core::primitive::u32,
					},
					#[codec(index = 7)]
					#[doc = "Set a retry configuration for a named task so that, in case its scheduled run fails, it"]
					#[doc = "will be retried after `period` blocks, for a total amount of `retries` retries or until"]
					#[doc = "it succeeds."]
					#[doc = ""]
					#[doc = "Tasks which need to be scheduled for a retry are still subject to weight metering and"]
					#[doc = "agenda space, same as a regular task. If a periodic task fails, it will be scheduled"]
					#[doc = "normally while the task is retrying."]
					#[doc = ""]
					#[doc = "Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic"]
					#[doc = "clones of the original task. Their retry configuration will be derived from the"]
					#[doc = "original task's configuration, but will have a lower value for `remaining` than the"]
					#[doc = "original `total_retries`."]
					set_retry_named {
						id: [::core::primitive::u8; 32usize],
						retries: ::core::primitive::u8,
						period: ::core::primitive::u32,
					},
					#[codec(index = 8)]
					#[doc = "Removes the retry configuration of a task."]
					cancel_retry { task: (::core::primitive::u32, ::core::primitive::u32) },
					#[codec(index = 9)]
					#[doc = "Cancel the retry configuration of a named task."]
					cancel_retry_named { id: [::core::primitive::u8; 32usize] },
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Failed to schedule a call"]
					FailedToSchedule,
					#[codec(index = 1)]
					#[doc = "Cannot find the scheduled call."]
					NotFound,
					#[codec(index = 2)]
					#[doc = "Given target block number is in the past."]
					TargetBlockNumberInPast,
					#[codec(index = 3)]
					#[doc = "Reschedule failed because it does not change scheduled time."]
					RescheduleNoChange,
					#[codec(index = 4)]
					#[doc = "Attempt to use a non-named function on a named task."]
					Named,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Events type."]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "Scheduled some task."]
					Scheduled { when: ::core::primitive::u32, index: ::core::primitive::u32 },
					#[codec(index = 1)]
					#[doc = "Canceled some task."]
					Canceled { when: ::core::primitive::u32, index: ::core::primitive::u32 },
					#[codec(index = 2)]
					#[doc = "Dispatched some task."]
					Dispatched {
						task: (::core::primitive::u32, ::core::primitive::u32),
						id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
						result:
							::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
					},
					#[codec(index = 3)]
					#[doc = "Set a retry configuration for some task."]
					RetrySet {
						task: (::core::primitive::u32, ::core::primitive::u32),
						id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
						period: ::core::primitive::u32,
						retries: ::core::primitive::u8,
					},
					#[codec(index = 4)]
					#[doc = "Cancel a retry configuration for some task."]
					RetryCancelled {
						task: (::core::primitive::u32, ::core::primitive::u32),
						id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
					},
					#[codec(index = 5)]
					#[doc = "The call for the provided hash was not found so the task has been aborted."]
					CallUnavailable {
						task: (::core::primitive::u32, ::core::primitive::u32),
						id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
					},
					#[codec(index = 6)]
					#[doc = "The given task was unable to be renewed since the agenda is full at that block."]
					PeriodicFailed {
						task: (::core::primitive::u32, ::core::primitive::u32),
						id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
					},
					#[codec(index = 7)]
					#[doc = "The given task was unable to be retried since the agenda is full at that block or there"]
					#[doc = "was not enough weight to reschedule it."]
					RetryFailed {
						task: (::core::primitive::u32, ::core::primitive::u32),
						id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
					},
					#[codec(index = 8)]
					#[doc = "The given task can never be executed since it is overweight."]
					PermanentlyOverweight {
						task: (::core::primitive::u32, ::core::primitive::u32),
						id: ::core::option::Option<[::core::primitive::u8; 32usize]>,
					},
					#[codec(index = 9)]
					#[doc = "Agenda is incomplete from `when`."]
					AgendaIncomplete { when: ::core::primitive::u32 },
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct RetryConfig<_0> {
				pub total_retries: ::core::primitive::u8,
				pub remaining: ::core::primitive::u8,
				pub period: _0,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct Scheduled<_0, _1, _2, _3, _4> {
				pub maybe_id: ::core::option::Option<_0>,
				pub priority: ::core::primitive::u8,
				pub call: _1,
				pub maybe_periodic: ::core::option::Option<(_2, _2)>,
				pub origin: _3,
				#[codec(skip)]
				pub __ignore: ::core::marker::PhantomData<_4>,
			}
		}
		pub mod pallet_session {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Sets the session key(s) of the function caller to `keys`."]
					#[doc = "Allows an account to set its session key prior to becoming a validator."]
					#[doc = "This doesn't take effect until the next session."]
					#[doc = ""]
					#[doc = "The dispatch origin of this function must be signed."]
					#[doc = ""]
					#[doc = "## Complexity"]
					#[doc = "- `O(1)`. Actual cost depends on the number of length of `T::Keys::key_ids()` which is"]
					#[doc = "  fixed."]
					set_keys {
						keys: runtime_types::midnight_node_runtime::opaque::SessionKeys,
						proof: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					},
					#[codec(index = 1)]
					#[doc = "Removes any session key(s) of the function caller."]
					#[doc = ""]
					#[doc = "This doesn't take effect until the next session."]
					#[doc = ""]
					#[doc = "The dispatch origin of this function must be Signed and the account must be either be"]
					#[doc = "convertible to a validator ID using the chain's typical addressing system (this usually"]
					#[doc = "means being a controller account) or directly convertible into a validator ID (which"]
					#[doc = "usually means being a stash account)."]
					#[doc = ""]
					#[doc = "## Complexity"]
					#[doc = "- `O(1)` in number of key types. Actual cost depends on the number of length of"]
					#[doc = "  `T::Keys::key_ids()` which is fixed."]
					purge_keys,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Error for the session pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Invalid ownership proof."]
					InvalidProof,
					#[codec(index = 1)]
					#[doc = "No associated validator ID for account."]
					NoAssociatedValidatorId,
					#[codec(index = 2)]
					#[doc = "Registered duplicate key."]
					DuplicatedKey,
					#[codec(index = 3)]
					#[doc = "No keys are associated with this account."]
					NoKeys,
					#[codec(index = 4)]
					#[doc = "Key setting account is not live, so it's impossible to associate keys."]
					NoAccount,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "New session has happened. Note that the argument is the session index, not the"]
					#[doc = "block number as the type might suggest."]
					NewSession { session_index: ::core::primitive::u32 },
					#[codec(index = 1)]
					#[doc = "Validator has been disabled."]
					ValidatorDisabled { validator: ::subxt::ext::subxt_core::utils::AccountId32 },
					#[codec(index = 2)]
					#[doc = "Validator has been re-enabled."]
					ValidatorReenabled { validator: ::subxt::ext::subxt_core::utils::AccountId32 },
				}
			}
		}
		pub mod pallet_session_validator_management {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					# [codec (index = 0)] # [doc = "'for_epoch_number' parameter is needed only for validation purposes, because we need to make sure that"] # [doc = "check_inherent uses the same epoch_number as was used to create inherent data."] # [doc = "Alternative approach would be to put epoch number inside InherentData. However, sidechain"] # [doc = "epoch number is set in Runtime, thus, inherent data provider doesn't have to know about it."] # [doc = "On top of that, the latter approach is slightly more complicated to code."] set { validators : runtime_types :: bounded_collections :: bounded_vec :: BoundedVec < runtime_types :: authority_selection_inherents :: CommitteeMember < runtime_types :: midnight_node_runtime :: opaque :: cross_chain_app :: Public , runtime_types :: midnight_node_runtime :: opaque :: SessionKeys > > , for_epoch_number : runtime_types :: sidechain_domain :: ScEpochNumber , selection_inputs_hash : runtime_types :: sidechain_domain :: byte_string :: SizedByteString , } , # [codec (index = 1)] # [doc = "Changes the main chain scripts used for committee rotation."] # [doc = ""] # [doc = "This extrinsic must be run either using `sudo` or some other chain governance mechanism."] set_main_chain_scripts { committee_candidate_address : runtime_types :: sidechain_domain :: MainchainAddress , d_parameter_policy_id : runtime_types :: sidechain_domain :: PolicyId , permissioned_candidates_policy_id : runtime_types :: sidechain_domain :: PolicyId , } , }
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct CommitteeInfo<_0, _1> {
					pub epoch: _0,
					pub committee: runtime_types::bounded_collections::bounded_vec::BoundedVec<_1>,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "[Pallet::set] has been called with epoch number that is not current epoch + 1"]
					InvalidEpoch,
					#[codec(index = 1)]
					#[doc = "[Pallet::set] has been called a second time for the same next epoch"]
					NextCommitteeAlreadySet,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {}
			}
		}
		pub mod pallet_sudo {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
					sudo {
						call: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::midnight_node_runtime::RuntimeCall,
						>,
					},
					#[codec(index = 1)]
					#[doc = "Authenticates the sudo key and dispatches a function call with `Root` origin."]
					#[doc = "This function does not check the weight of the call, and instead allows the"]
					#[doc = "Sudo user to specify the weight of the call."]
					#[doc = ""]
					#[doc = "The dispatch origin for this call must be _Signed_."]
					sudo_unchecked_weight {
						call: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::midnight_node_runtime::RuntimeCall,
						>,
						weight: runtime_types::sp_weights::weight_v2::Weight,
					},
					#[codec(index = 2)]
					#[doc = "Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo"]
					#[doc = "key."]
					set_key {
						new: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
					},
					#[codec(index = 3)]
					#[doc = "Authenticates the sudo key and dispatches a function call with `Signed` origin from"]
					#[doc = "a given account."]
					#[doc = ""]
					#[doc = "The dispatch origin for this call must be _Signed_."]
					sudo_as {
						who: ::subxt::ext::subxt_core::utils::MultiAddress<
							::subxt::ext::subxt_core::utils::AccountId32,
							(),
						>,
						call: ::subxt::ext::subxt_core::alloc::boxed::Box<
							runtime_types::midnight_node_runtime::RuntimeCall,
						>,
					},
					#[codec(index = 4)]
					#[doc = "Permanently removes the sudo key."]
					#[doc = ""]
					#[doc = "**This cannot be un-done.**"]
					remove_key,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Error for the Sudo pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Sender must be the Sudo account."]
					RequireSudo,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "A sudo call just took place."]
					Sudid {
						sudo_result:
							::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
					},
					#[codec(index = 1)]
					#[doc = "The sudo key has been updated."]
					KeyChanged {
						old: ::core::option::Option<::subxt::ext::subxt_core::utils::AccountId32>,
						new: ::subxt::ext::subxt_core::utils::AccountId32,
					},
					#[codec(index = 2)]
					#[doc = "The key was permanently removed."]
					KeyRemoved,
					#[codec(index = 3)]
					#[doc = "A [sudo_as](Pallet::sudo_as) call just took place."]
					SudoAsDone {
						sudo_result:
							::core::result::Result<(), runtime_types::sp_runtime::DispatchError>,
					},
				}
			}
		}
		pub mod pallet_timestamp {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Set the current time."]
					#[doc = ""]
					#[doc = "This call should be invoked exactly once per block. It will panic at the finalization"]
					#[doc = "phase, if this call hasn't been invoked by that time."]
					#[doc = ""]
					#[doc = "The timestamp should be greater than the previous one by the amount specified by"]
					#[doc = "[`Config::MinimumPeriod`]."]
					#[doc = ""]
					#[doc = "The dispatch origin for this call must be _None_."]
					#[doc = ""]
					#[doc = "This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware"]
					#[doc = "that changing the complexity of this call could result exhausting the resources in a"]
					#[doc = "block to execute any other calls."]
					#[doc = ""]
					#[doc = "## Complexity"]
					#[doc = "- `O(1)` (Note that implementations of `OnTimestampSet` must also be `O(1)`)"]
					#[doc = "- 1 storage read and 1 storage mutation (codec `O(1)` because of `DidUpdate::take` in"]
					#[doc = "  `on_finalize`)"]
					#[doc = "- 1 event handler `on_timestamp_set`. Must be `O(1)`."]
					set {
						#[codec(compact)]
						now: ::core::primitive::u64,
					},
				}
			}
		}
		pub mod pallet_tx_pause {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Pause a call."]
					#[doc = ""]
					#[doc = "Can only be called by [`Config::PauseOrigin`]."]
					#[doc = "Emits an [`Event::CallPaused`] event on success."]
					pause {
						full_name: (
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
						),
					},
					#[codec(index = 1)]
					#[doc = "Un-pause a call."]
					#[doc = ""]
					#[doc = "Can only be called by [`Config::UnpauseOrigin`]."]
					#[doc = "Emits an [`Event::CallUnpaused`] event on success."]
					unpause {
						ident: (
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
						),
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "The call is paused."]
					IsPaused,
					#[codec(index = 1)]
					#[doc = "The call is unpaused."]
					IsUnpaused,
					#[codec(index = 2)]
					#[doc = "The call is whitelisted and cannot be paused."]
					Unpausable,
					#[codec(index = 3)]
					NotFound,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "This pallet, or a specific call is now paused."]
					CallPaused {
						full_name: (
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
						),
					},
					#[codec(index = 1)]
					#[doc = "This pallet, or a specific call is now unpaused."]
					CallUnpaused {
						full_name: (
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
							runtime_types::bounded_collections::bounded_vec::BoundedVec<
								::core::primitive::u8,
							>,
						),
					},
				}
			}
		}
		pub mod pallet_upgrade {
			use super::runtime_types;
			pub mod pallet {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "Contains a variant per dispatchable extrinsic that this pallet has."]
				pub enum Call {
					#[codec(index = 0)]
					#[doc = "Vote on a proposed runtime upgrade that is represented by an onchain preimage request"]
					#[doc = ""]
					#[doc = "This call should be invoked exactly once per block due to its inherent nature."]
					#[doc = ""]
					#[doc = "The dispatch origin for this call must be _None_."]
					#[doc = ""]
					#[doc = "This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware"]
					#[doc = "that changing the complexity of this call could result exhausting the resources in a"]
					#[doc = "block to execute any other calls."]
					propose_or_vote_upgrade {
						upgrade: runtime_types::midnight_primitives_upgrade::UpgradeProposal,
					},
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Error` enum of this pallet."]
				pub enum Error {
					#[codec(index = 0)]
					#[doc = "Inherent transaction requires current authority information, but this was not able to be retrived from AURA"]
					CouldNotLoadCurrentAuthority,
					#[codec(index = 1)]
					#[doc = "An error occurred when calling a runtime upgrade"]
					RuntimeUpgradeError,
					#[codec(index = 2)]
					#[doc = "Limit for votes was exceeded"]
					VoteThresholdExceeded,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				#[doc = "The `Event` enum of this pallet"]
				pub enum Event {
					#[codec(index = 0)]
					#[doc = "Signal an issue when attempting a runtime upgrade, in a context where pallet errors are not accessible"]
					CouldNotScheduleRuntimeUpgrade {
						runtime_hash: ::subxt::ext::subxt_core::utils::H256,
						spec_version: ::core::primitive::u32,
					},
					#[codec(index = 1)]
					#[doc = "No votes were made this round"]
					NoVotes,
					#[codec(index = 2)]
					#[doc = "Code upgrade managed by this pallet was scheduled"]
					UpgradeScheduled {
						runtime_hash: ::subxt::ext::subxt_core::utils::H256,
						spec_version: ::core::primitive::u32,
						scheduled_for: ::core::primitive::u32,
					},
					#[codec(index = 3)]
					#[doc = "Validators could not agree on an upgrade, and voting will be reset"]
					NoConsensusOnUpgrade,
					#[codec(index = 4)]
					#[doc = "Upgrade was not performed because a preimage of the upgrade request was not found"]
					NoUpgradePreimageMissing {
						preimage_hash: ::subxt::ext::subxt_core::utils::H256,
					},
					#[codec(index = 5)]
					#[doc = "Upgrade was not performed because the request for its preimage was not found"]
					NoUpgradePreimageNotRequested {
						preimage_hash: ::subxt::ext::subxt_core::utils::H256,
					},
					#[codec(index = 6)]
					#[doc = "An upgrade was attempted, but the call size exceeded the configured bounds"]
					UpgradeCallTooLarge {
						runtime_hash: ::subxt::ext::subxt_core::utils::H256,
						spec_version: ::core::primitive::u32,
					},
					#[codec(index = 7)]
					#[doc = "A validator has voted on an upgrade"]
					Voted {
						voter: runtime_types::sp_consensus_aura::sr25519::app_sr25519::Public,
						target: runtime_types::midnight_primitives_upgrade::UpgradeProposal,
					},
				}
			}
		}
		pub mod sidechain_domain {
			use super::runtime_types;
			pub mod byte_string {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BoundedString(
					pub  runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
				);
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ByteString(
					pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				);
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct SizedByteString(pub [::core::primitive::u8; 32usize]);
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct AssetName(
				pub  runtime_types::bounded_collections::bounded_vec::BoundedVec<
					::core::primitive::u8,
				>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct AuraPublicKey(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct CandidateRegistrations {
				pub stake_pool_public_key: runtime_types::sidechain_domain::StakePoolPublicKey,
				pub registrations: ::subxt::ext::subxt_core::alloc::vec::Vec<
					runtime_types::sidechain_domain::RegistrationData,
				>,
				pub stake_delegation:
					::core::option::Option<runtime_types::sidechain_domain::StakeDelegation>,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct CrossChainPublicKey(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct CrossChainSignature(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct DParameter {
				pub num_permissioned_candidates: ::core::primitive::u16,
				pub num_registered_candidates: ::core::primitive::u16,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct EpochNonce(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct GrandpaPublicKey(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct MainchainAddress(
				pub  runtime_types::bounded_collections::bounded_vec::BoundedVec<
					::core::primitive::u8,
				>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct MainchainSignature(pub [::core::primitive::u8; 64usize]);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct McBlockNumber(pub ::core::primitive::u32);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct McEpochNumber(pub ::core::primitive::u32);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct McSlotNumber(pub ::core::primitive::u64);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct McTxHash(pub [::core::primitive::u8; 32usize]);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct McTxIndexInBlock(pub ::core::primitive::u32);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct NativeTokenAmount(pub ::core::primitive::u128);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct PermissionedCandidateData {
				pub sidechain_public_key: runtime_types::sidechain_domain::SidechainPublicKey,
				pub aura_public_key: runtime_types::sidechain_domain::AuraPublicKey,
				pub grandpa_public_key: runtime_types::sidechain_domain::GrandpaPublicKey,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct PolicyId(pub [::core::primitive::u8; 28usize]);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct RegistrationData {
				pub registration_utxo: runtime_types::sidechain_domain::UtxoId,
				pub sidechain_signature: runtime_types::sidechain_domain::SidechainSignature,
				pub mainchain_signature: runtime_types::sidechain_domain::MainchainSignature,
				pub cross_chain_signature: runtime_types::sidechain_domain::CrossChainSignature,
				pub sidechain_pub_key: runtime_types::sidechain_domain::SidechainPublicKey,
				pub cross_chain_pub_key: runtime_types::sidechain_domain::CrossChainPublicKey,
				pub utxo_info: runtime_types::sidechain_domain::UtxoInfo,
				pub tx_inputs: ::subxt::ext::subxt_core::alloc::vec::Vec<
					runtime_types::sidechain_domain::UtxoId,
				>,
				pub aura_pub_key: runtime_types::sidechain_domain::AuraPublicKey,
				pub grandpa_pub_key: runtime_types::sidechain_domain::GrandpaPublicKey,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct ScEpochNumber(pub ::core::primitive::u64);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct ScSlotNumber(pub ::core::primitive::u64);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct SidechainPublicKey(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct SidechainSignature(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct StakeDelegation(pub ::core::primitive::u64);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct StakePoolPublicKey(pub [::core::primitive::u8; 32usize]);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct UtxoId {
				pub tx_hash: runtime_types::sidechain_domain::McTxHash,
				pub index: runtime_types::sidechain_domain::UtxoIndex,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct UtxoIndex(pub ::core::primitive::u16);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct UtxoInfo {
				pub utxo_id: runtime_types::sidechain_domain::UtxoId,
				pub epoch_number: runtime_types::sidechain_domain::McEpochNumber,
				pub block_number: runtime_types::sidechain_domain::McBlockNumber,
				pub slot_number: runtime_types::sidechain_domain::McSlotNumber,
				pub tx_index_within_block: runtime_types::sidechain_domain::McTxIndexInBlock,
			}
		}
		pub mod sidechain_slots {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct ScSlotConfig {
				pub slots_per_epoch: runtime_types::sidechain_slots::SlotsPerEpoch,
				pub slot_duration: runtime_types::sp_consensus_slots::SlotDuration,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct SlotsPerEpoch(pub ::core::primitive::u32);
		}
		pub mod sp_arithmetic {
			use super::runtime_types;
			pub mod per_things {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Perbill(pub ::core::primitive::u32);
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Percent(pub ::core::primitive::u8);
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum ArithmeticError {
				#[codec(index = 0)]
				Underflow,
				#[codec(index = 1)]
				Overflow,
				#[codec(index = 2)]
				DivisionByZero,
			}
		}
		pub mod sp_consensus_aura {
			use super::runtime_types;
			pub mod sr25519 {
				use super::runtime_types;
				pub mod app_sr25519 {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct Public(pub [::core::primitive::u8; 32usize]);
				}
			}
		}
		pub mod sp_consensus_beefy {
			use super::runtime_types;
			pub mod commitment {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Commitment<_0> {
					pub payload: runtime_types::sp_consensus_beefy::payload::Payload,
					pub block_number: _0,
					pub validator_set_id: ::core::primitive::u64,
				}
			}
			pub mod ecdsa_crypto {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Public(pub [::core::primitive::u8; 33usize]);
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Signature(pub [::core::primitive::u8; 65usize]);
			}
			pub mod mmr {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BeefyAuthoritySet<_0> {
					pub id: ::core::primitive::u64,
					pub len: ::core::primitive::u32,
					pub keyset_commitment: _0,
				}
			}
			pub mod payload {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Payload(
					pub  ::subxt::ext::subxt_core::alloc::vec::Vec<(
						[::core::primitive::u8; 2usize],
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					)>,
				);
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct DoubleVotingProof<_0, _1, _2> {
				pub first: runtime_types::sp_consensus_beefy::VoteMessage<_0, _1, _2>,
				pub second: runtime_types::sp_consensus_beefy::VoteMessage<_0, _1, _2>,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct ForkVotingProof<_0, _1, _2> {
				pub vote: runtime_types::sp_consensus_beefy::VoteMessage<
					::core::primitive::u32,
					_1,
					runtime_types::sp_consensus_beefy::ecdsa_crypto::Signature,
				>,
				pub ancestry_proof: _2,
				pub header: _0,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct FutureBlockVotingProof<_0, _1> {
				pub vote: runtime_types::sp_consensus_beefy::VoteMessage<
					_0,
					_1,
					runtime_types::sp_consensus_beefy::ecdsa_crypto::Signature,
				>,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct ValidatorSet<_0> {
				pub validators: ::subxt::ext::subxt_core::alloc::vec::Vec<_0>,
				pub id: ::core::primitive::u64,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct VoteMessage<_0, _1, _2> {
				pub commitment: runtime_types::sp_consensus_beefy::commitment::Commitment<_0>,
				pub id: _1,
				pub signature: _2,
			}
		}
		pub mod sp_consensus_grandpa {
			use super::runtime_types;
			pub mod app {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Public(pub [::core::primitive::u8; 32usize]);
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Signature(pub [::core::primitive::u8; 64usize]);
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum Equivocation<_0, _1> {
				#[codec(index = 0)]
				Prevote(
					runtime_types::finality_grandpa::Equivocation<
						runtime_types::sp_consensus_grandpa::app::Public,
						runtime_types::finality_grandpa::Prevote<_0, _1>,
						runtime_types::sp_consensus_grandpa::app::Signature,
					>,
				),
				#[codec(index = 1)]
				Precommit(
					runtime_types::finality_grandpa::Equivocation<
						runtime_types::sp_consensus_grandpa::app::Public,
						runtime_types::finality_grandpa::Precommit<_0, _1>,
						runtime_types::sp_consensus_grandpa::app::Signature,
					>,
				),
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct EquivocationProof<_0, _1> {
				pub set_id: ::core::primitive::u64,
				pub equivocation: runtime_types::sp_consensus_grandpa::Equivocation<_0, _1>,
			}
		}
		pub mod sp_consensus_slots {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct Slot(pub ::core::primitive::u64);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: CompactAs,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct SlotDuration(pub ::core::primitive::u64);
		}
		pub mod sp_core {
			use super::runtime_types;
			pub mod crypto {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct KeyTypeId(pub [::core::primitive::u8; 4usize]);
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct OpaqueMetadata(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum Void {}
		}
		pub mod sp_governed_map {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct MainChainScriptsV1 {
				pub validator_address: runtime_types::sidechain_domain::MainchainAddress,
				pub policy_id: runtime_types::sidechain_domain::PolicyId,
			}
		}
		pub mod sp_inherents {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct CheckInherentsResult {
				pub okay: ::core::primitive::bool,
				pub fatal_error: ::core::primitive::bool,
				pub errors: runtime_types::sp_inherents::InherentData,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct InherentData {
				pub data: ::subxt::ext::subxt_core::utils::KeyedVec<
					[::core::primitive::u8; 8usize],
					::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
				>,
			}
		}
		pub mod sp_mmr_primitives {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct AncestryProof<_0> {
				pub prev_peaks: ::subxt::ext::subxt_core::alloc::vec::Vec<_0>,
				pub prev_leaf_count: ::core::primitive::u64,
				pub leaf_count: ::core::primitive::u64,
				pub items: ::subxt::ext::subxt_core::alloc::vec::Vec<(::core::primitive::u64, _0)>,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct EncodableOpaqueLeaf(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum Error {
				#[codec(index = 0)]
				InvalidNumericOp,
				#[codec(index = 1)]
				Push,
				#[codec(index = 2)]
				GetRoot,
				#[codec(index = 3)]
				Commit,
				#[codec(index = 4)]
				GenerateProof,
				#[codec(index = 5)]
				Verify,
				#[codec(index = 6)]
				LeafNotFound,
				#[codec(index = 7)]
				PalletNotIncluded,
				#[codec(index = 8)]
				InvalidLeafIndex,
				#[codec(index = 9)]
				InvalidBestKnownBlock,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct LeafProof<_0> {
				pub leaf_indices: ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u64>,
				pub leaf_count: ::core::primitive::u64,
				pub items: ::subxt::ext::subxt_core::alloc::vec::Vec<_0>,
			}
		}
		pub mod sp_native_token_management {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct MainChainScripts {
				pub native_token_policy_id: runtime_types::sidechain_domain::PolicyId,
				pub native_token_asset_name: runtime_types::sidechain_domain::AssetName,
				pub illiquid_supply_validator_address:
					runtime_types::sidechain_domain::MainchainAddress,
			}
		}
		pub mod sp_runtime {
			use super::runtime_types;
			pub mod generic {
				use super::runtime_types;
				pub mod block {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct Block<_0, _1> {
						pub header: _0,
						pub extrinsics: ::subxt::ext::subxt_core::alloc::vec::Vec<_1>,
					}
				}
				pub mod digest {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct Digest {
						pub logs: ::subxt::ext::subxt_core::alloc::vec::Vec<
							runtime_types::sp_runtime::generic::digest::DigestItem,
						>,
					}
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub enum DigestItem {
						#[codec(index = 6)]
						PreRuntime(
							[::core::primitive::u8; 4usize],
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						),
						#[codec(index = 4)]
						Consensus(
							[::core::primitive::u8; 4usize],
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						),
						#[codec(index = 5)]
						Seal(
							[::core::primitive::u8; 4usize],
							::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
						),
						#[codec(index = 0)]
						Other(::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>),
						#[codec(index = 8)]
						RuntimeEnvironmentUpdated,
					}
				}
				pub mod era {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub enum Era {
						#[codec(index = 0)]
						Immortal,
						#[codec(index = 1)]
						Mortal1(::core::primitive::u8),
						#[codec(index = 2)]
						Mortal2(::core::primitive::u8),
						#[codec(index = 3)]
						Mortal3(::core::primitive::u8),
						#[codec(index = 4)]
						Mortal4(::core::primitive::u8),
						#[codec(index = 5)]
						Mortal5(::core::primitive::u8),
						#[codec(index = 6)]
						Mortal6(::core::primitive::u8),
						#[codec(index = 7)]
						Mortal7(::core::primitive::u8),
						#[codec(index = 8)]
						Mortal8(::core::primitive::u8),
						#[codec(index = 9)]
						Mortal9(::core::primitive::u8),
						#[codec(index = 10)]
						Mortal10(::core::primitive::u8),
						#[codec(index = 11)]
						Mortal11(::core::primitive::u8),
						#[codec(index = 12)]
						Mortal12(::core::primitive::u8),
						#[codec(index = 13)]
						Mortal13(::core::primitive::u8),
						#[codec(index = 14)]
						Mortal14(::core::primitive::u8),
						#[codec(index = 15)]
						Mortal15(::core::primitive::u8),
						#[codec(index = 16)]
						Mortal16(::core::primitive::u8),
						#[codec(index = 17)]
						Mortal17(::core::primitive::u8),
						#[codec(index = 18)]
						Mortal18(::core::primitive::u8),
						#[codec(index = 19)]
						Mortal19(::core::primitive::u8),
						#[codec(index = 20)]
						Mortal20(::core::primitive::u8),
						#[codec(index = 21)]
						Mortal21(::core::primitive::u8),
						#[codec(index = 22)]
						Mortal22(::core::primitive::u8),
						#[codec(index = 23)]
						Mortal23(::core::primitive::u8),
						#[codec(index = 24)]
						Mortal24(::core::primitive::u8),
						#[codec(index = 25)]
						Mortal25(::core::primitive::u8),
						#[codec(index = 26)]
						Mortal26(::core::primitive::u8),
						#[codec(index = 27)]
						Mortal27(::core::primitive::u8),
						#[codec(index = 28)]
						Mortal28(::core::primitive::u8),
						#[codec(index = 29)]
						Mortal29(::core::primitive::u8),
						#[codec(index = 30)]
						Mortal30(::core::primitive::u8),
						#[codec(index = 31)]
						Mortal31(::core::primitive::u8),
						#[codec(index = 32)]
						Mortal32(::core::primitive::u8),
						#[codec(index = 33)]
						Mortal33(::core::primitive::u8),
						#[codec(index = 34)]
						Mortal34(::core::primitive::u8),
						#[codec(index = 35)]
						Mortal35(::core::primitive::u8),
						#[codec(index = 36)]
						Mortal36(::core::primitive::u8),
						#[codec(index = 37)]
						Mortal37(::core::primitive::u8),
						#[codec(index = 38)]
						Mortal38(::core::primitive::u8),
						#[codec(index = 39)]
						Mortal39(::core::primitive::u8),
						#[codec(index = 40)]
						Mortal40(::core::primitive::u8),
						#[codec(index = 41)]
						Mortal41(::core::primitive::u8),
						#[codec(index = 42)]
						Mortal42(::core::primitive::u8),
						#[codec(index = 43)]
						Mortal43(::core::primitive::u8),
						#[codec(index = 44)]
						Mortal44(::core::primitive::u8),
						#[codec(index = 45)]
						Mortal45(::core::primitive::u8),
						#[codec(index = 46)]
						Mortal46(::core::primitive::u8),
						#[codec(index = 47)]
						Mortal47(::core::primitive::u8),
						#[codec(index = 48)]
						Mortal48(::core::primitive::u8),
						#[codec(index = 49)]
						Mortal49(::core::primitive::u8),
						#[codec(index = 50)]
						Mortal50(::core::primitive::u8),
						#[codec(index = 51)]
						Mortal51(::core::primitive::u8),
						#[codec(index = 52)]
						Mortal52(::core::primitive::u8),
						#[codec(index = 53)]
						Mortal53(::core::primitive::u8),
						#[codec(index = 54)]
						Mortal54(::core::primitive::u8),
						#[codec(index = 55)]
						Mortal55(::core::primitive::u8),
						#[codec(index = 56)]
						Mortal56(::core::primitive::u8),
						#[codec(index = 57)]
						Mortal57(::core::primitive::u8),
						#[codec(index = 58)]
						Mortal58(::core::primitive::u8),
						#[codec(index = 59)]
						Mortal59(::core::primitive::u8),
						#[codec(index = 60)]
						Mortal60(::core::primitive::u8),
						#[codec(index = 61)]
						Mortal61(::core::primitive::u8),
						#[codec(index = 62)]
						Mortal62(::core::primitive::u8),
						#[codec(index = 63)]
						Mortal63(::core::primitive::u8),
						#[codec(index = 64)]
						Mortal64(::core::primitive::u8),
						#[codec(index = 65)]
						Mortal65(::core::primitive::u8),
						#[codec(index = 66)]
						Mortal66(::core::primitive::u8),
						#[codec(index = 67)]
						Mortal67(::core::primitive::u8),
						#[codec(index = 68)]
						Mortal68(::core::primitive::u8),
						#[codec(index = 69)]
						Mortal69(::core::primitive::u8),
						#[codec(index = 70)]
						Mortal70(::core::primitive::u8),
						#[codec(index = 71)]
						Mortal71(::core::primitive::u8),
						#[codec(index = 72)]
						Mortal72(::core::primitive::u8),
						#[codec(index = 73)]
						Mortal73(::core::primitive::u8),
						#[codec(index = 74)]
						Mortal74(::core::primitive::u8),
						#[codec(index = 75)]
						Mortal75(::core::primitive::u8),
						#[codec(index = 76)]
						Mortal76(::core::primitive::u8),
						#[codec(index = 77)]
						Mortal77(::core::primitive::u8),
						#[codec(index = 78)]
						Mortal78(::core::primitive::u8),
						#[codec(index = 79)]
						Mortal79(::core::primitive::u8),
						#[codec(index = 80)]
						Mortal80(::core::primitive::u8),
						#[codec(index = 81)]
						Mortal81(::core::primitive::u8),
						#[codec(index = 82)]
						Mortal82(::core::primitive::u8),
						#[codec(index = 83)]
						Mortal83(::core::primitive::u8),
						#[codec(index = 84)]
						Mortal84(::core::primitive::u8),
						#[codec(index = 85)]
						Mortal85(::core::primitive::u8),
						#[codec(index = 86)]
						Mortal86(::core::primitive::u8),
						#[codec(index = 87)]
						Mortal87(::core::primitive::u8),
						#[codec(index = 88)]
						Mortal88(::core::primitive::u8),
						#[codec(index = 89)]
						Mortal89(::core::primitive::u8),
						#[codec(index = 90)]
						Mortal90(::core::primitive::u8),
						#[codec(index = 91)]
						Mortal91(::core::primitive::u8),
						#[codec(index = 92)]
						Mortal92(::core::primitive::u8),
						#[codec(index = 93)]
						Mortal93(::core::primitive::u8),
						#[codec(index = 94)]
						Mortal94(::core::primitive::u8),
						#[codec(index = 95)]
						Mortal95(::core::primitive::u8),
						#[codec(index = 96)]
						Mortal96(::core::primitive::u8),
						#[codec(index = 97)]
						Mortal97(::core::primitive::u8),
						#[codec(index = 98)]
						Mortal98(::core::primitive::u8),
						#[codec(index = 99)]
						Mortal99(::core::primitive::u8),
						#[codec(index = 100)]
						Mortal100(::core::primitive::u8),
						#[codec(index = 101)]
						Mortal101(::core::primitive::u8),
						#[codec(index = 102)]
						Mortal102(::core::primitive::u8),
						#[codec(index = 103)]
						Mortal103(::core::primitive::u8),
						#[codec(index = 104)]
						Mortal104(::core::primitive::u8),
						#[codec(index = 105)]
						Mortal105(::core::primitive::u8),
						#[codec(index = 106)]
						Mortal106(::core::primitive::u8),
						#[codec(index = 107)]
						Mortal107(::core::primitive::u8),
						#[codec(index = 108)]
						Mortal108(::core::primitive::u8),
						#[codec(index = 109)]
						Mortal109(::core::primitive::u8),
						#[codec(index = 110)]
						Mortal110(::core::primitive::u8),
						#[codec(index = 111)]
						Mortal111(::core::primitive::u8),
						#[codec(index = 112)]
						Mortal112(::core::primitive::u8),
						#[codec(index = 113)]
						Mortal113(::core::primitive::u8),
						#[codec(index = 114)]
						Mortal114(::core::primitive::u8),
						#[codec(index = 115)]
						Mortal115(::core::primitive::u8),
						#[codec(index = 116)]
						Mortal116(::core::primitive::u8),
						#[codec(index = 117)]
						Mortal117(::core::primitive::u8),
						#[codec(index = 118)]
						Mortal118(::core::primitive::u8),
						#[codec(index = 119)]
						Mortal119(::core::primitive::u8),
						#[codec(index = 120)]
						Mortal120(::core::primitive::u8),
						#[codec(index = 121)]
						Mortal121(::core::primitive::u8),
						#[codec(index = 122)]
						Mortal122(::core::primitive::u8),
						#[codec(index = 123)]
						Mortal123(::core::primitive::u8),
						#[codec(index = 124)]
						Mortal124(::core::primitive::u8),
						#[codec(index = 125)]
						Mortal125(::core::primitive::u8),
						#[codec(index = 126)]
						Mortal126(::core::primitive::u8),
						#[codec(index = 127)]
						Mortal127(::core::primitive::u8),
						#[codec(index = 128)]
						Mortal128(::core::primitive::u8),
						#[codec(index = 129)]
						Mortal129(::core::primitive::u8),
						#[codec(index = 130)]
						Mortal130(::core::primitive::u8),
						#[codec(index = 131)]
						Mortal131(::core::primitive::u8),
						#[codec(index = 132)]
						Mortal132(::core::primitive::u8),
						#[codec(index = 133)]
						Mortal133(::core::primitive::u8),
						#[codec(index = 134)]
						Mortal134(::core::primitive::u8),
						#[codec(index = 135)]
						Mortal135(::core::primitive::u8),
						#[codec(index = 136)]
						Mortal136(::core::primitive::u8),
						#[codec(index = 137)]
						Mortal137(::core::primitive::u8),
						#[codec(index = 138)]
						Mortal138(::core::primitive::u8),
						#[codec(index = 139)]
						Mortal139(::core::primitive::u8),
						#[codec(index = 140)]
						Mortal140(::core::primitive::u8),
						#[codec(index = 141)]
						Mortal141(::core::primitive::u8),
						#[codec(index = 142)]
						Mortal142(::core::primitive::u8),
						#[codec(index = 143)]
						Mortal143(::core::primitive::u8),
						#[codec(index = 144)]
						Mortal144(::core::primitive::u8),
						#[codec(index = 145)]
						Mortal145(::core::primitive::u8),
						#[codec(index = 146)]
						Mortal146(::core::primitive::u8),
						#[codec(index = 147)]
						Mortal147(::core::primitive::u8),
						#[codec(index = 148)]
						Mortal148(::core::primitive::u8),
						#[codec(index = 149)]
						Mortal149(::core::primitive::u8),
						#[codec(index = 150)]
						Mortal150(::core::primitive::u8),
						#[codec(index = 151)]
						Mortal151(::core::primitive::u8),
						#[codec(index = 152)]
						Mortal152(::core::primitive::u8),
						#[codec(index = 153)]
						Mortal153(::core::primitive::u8),
						#[codec(index = 154)]
						Mortal154(::core::primitive::u8),
						#[codec(index = 155)]
						Mortal155(::core::primitive::u8),
						#[codec(index = 156)]
						Mortal156(::core::primitive::u8),
						#[codec(index = 157)]
						Mortal157(::core::primitive::u8),
						#[codec(index = 158)]
						Mortal158(::core::primitive::u8),
						#[codec(index = 159)]
						Mortal159(::core::primitive::u8),
						#[codec(index = 160)]
						Mortal160(::core::primitive::u8),
						#[codec(index = 161)]
						Mortal161(::core::primitive::u8),
						#[codec(index = 162)]
						Mortal162(::core::primitive::u8),
						#[codec(index = 163)]
						Mortal163(::core::primitive::u8),
						#[codec(index = 164)]
						Mortal164(::core::primitive::u8),
						#[codec(index = 165)]
						Mortal165(::core::primitive::u8),
						#[codec(index = 166)]
						Mortal166(::core::primitive::u8),
						#[codec(index = 167)]
						Mortal167(::core::primitive::u8),
						#[codec(index = 168)]
						Mortal168(::core::primitive::u8),
						#[codec(index = 169)]
						Mortal169(::core::primitive::u8),
						#[codec(index = 170)]
						Mortal170(::core::primitive::u8),
						#[codec(index = 171)]
						Mortal171(::core::primitive::u8),
						#[codec(index = 172)]
						Mortal172(::core::primitive::u8),
						#[codec(index = 173)]
						Mortal173(::core::primitive::u8),
						#[codec(index = 174)]
						Mortal174(::core::primitive::u8),
						#[codec(index = 175)]
						Mortal175(::core::primitive::u8),
						#[codec(index = 176)]
						Mortal176(::core::primitive::u8),
						#[codec(index = 177)]
						Mortal177(::core::primitive::u8),
						#[codec(index = 178)]
						Mortal178(::core::primitive::u8),
						#[codec(index = 179)]
						Mortal179(::core::primitive::u8),
						#[codec(index = 180)]
						Mortal180(::core::primitive::u8),
						#[codec(index = 181)]
						Mortal181(::core::primitive::u8),
						#[codec(index = 182)]
						Mortal182(::core::primitive::u8),
						#[codec(index = 183)]
						Mortal183(::core::primitive::u8),
						#[codec(index = 184)]
						Mortal184(::core::primitive::u8),
						#[codec(index = 185)]
						Mortal185(::core::primitive::u8),
						#[codec(index = 186)]
						Mortal186(::core::primitive::u8),
						#[codec(index = 187)]
						Mortal187(::core::primitive::u8),
						#[codec(index = 188)]
						Mortal188(::core::primitive::u8),
						#[codec(index = 189)]
						Mortal189(::core::primitive::u8),
						#[codec(index = 190)]
						Mortal190(::core::primitive::u8),
						#[codec(index = 191)]
						Mortal191(::core::primitive::u8),
						#[codec(index = 192)]
						Mortal192(::core::primitive::u8),
						#[codec(index = 193)]
						Mortal193(::core::primitive::u8),
						#[codec(index = 194)]
						Mortal194(::core::primitive::u8),
						#[codec(index = 195)]
						Mortal195(::core::primitive::u8),
						#[codec(index = 196)]
						Mortal196(::core::primitive::u8),
						#[codec(index = 197)]
						Mortal197(::core::primitive::u8),
						#[codec(index = 198)]
						Mortal198(::core::primitive::u8),
						#[codec(index = 199)]
						Mortal199(::core::primitive::u8),
						#[codec(index = 200)]
						Mortal200(::core::primitive::u8),
						#[codec(index = 201)]
						Mortal201(::core::primitive::u8),
						#[codec(index = 202)]
						Mortal202(::core::primitive::u8),
						#[codec(index = 203)]
						Mortal203(::core::primitive::u8),
						#[codec(index = 204)]
						Mortal204(::core::primitive::u8),
						#[codec(index = 205)]
						Mortal205(::core::primitive::u8),
						#[codec(index = 206)]
						Mortal206(::core::primitive::u8),
						#[codec(index = 207)]
						Mortal207(::core::primitive::u8),
						#[codec(index = 208)]
						Mortal208(::core::primitive::u8),
						#[codec(index = 209)]
						Mortal209(::core::primitive::u8),
						#[codec(index = 210)]
						Mortal210(::core::primitive::u8),
						#[codec(index = 211)]
						Mortal211(::core::primitive::u8),
						#[codec(index = 212)]
						Mortal212(::core::primitive::u8),
						#[codec(index = 213)]
						Mortal213(::core::primitive::u8),
						#[codec(index = 214)]
						Mortal214(::core::primitive::u8),
						#[codec(index = 215)]
						Mortal215(::core::primitive::u8),
						#[codec(index = 216)]
						Mortal216(::core::primitive::u8),
						#[codec(index = 217)]
						Mortal217(::core::primitive::u8),
						#[codec(index = 218)]
						Mortal218(::core::primitive::u8),
						#[codec(index = 219)]
						Mortal219(::core::primitive::u8),
						#[codec(index = 220)]
						Mortal220(::core::primitive::u8),
						#[codec(index = 221)]
						Mortal221(::core::primitive::u8),
						#[codec(index = 222)]
						Mortal222(::core::primitive::u8),
						#[codec(index = 223)]
						Mortal223(::core::primitive::u8),
						#[codec(index = 224)]
						Mortal224(::core::primitive::u8),
						#[codec(index = 225)]
						Mortal225(::core::primitive::u8),
						#[codec(index = 226)]
						Mortal226(::core::primitive::u8),
						#[codec(index = 227)]
						Mortal227(::core::primitive::u8),
						#[codec(index = 228)]
						Mortal228(::core::primitive::u8),
						#[codec(index = 229)]
						Mortal229(::core::primitive::u8),
						#[codec(index = 230)]
						Mortal230(::core::primitive::u8),
						#[codec(index = 231)]
						Mortal231(::core::primitive::u8),
						#[codec(index = 232)]
						Mortal232(::core::primitive::u8),
						#[codec(index = 233)]
						Mortal233(::core::primitive::u8),
						#[codec(index = 234)]
						Mortal234(::core::primitive::u8),
						#[codec(index = 235)]
						Mortal235(::core::primitive::u8),
						#[codec(index = 236)]
						Mortal236(::core::primitive::u8),
						#[codec(index = 237)]
						Mortal237(::core::primitive::u8),
						#[codec(index = 238)]
						Mortal238(::core::primitive::u8),
						#[codec(index = 239)]
						Mortal239(::core::primitive::u8),
						#[codec(index = 240)]
						Mortal240(::core::primitive::u8),
						#[codec(index = 241)]
						Mortal241(::core::primitive::u8),
						#[codec(index = 242)]
						Mortal242(::core::primitive::u8),
						#[codec(index = 243)]
						Mortal243(::core::primitive::u8),
						#[codec(index = 244)]
						Mortal244(::core::primitive::u8),
						#[codec(index = 245)]
						Mortal245(::core::primitive::u8),
						#[codec(index = 246)]
						Mortal246(::core::primitive::u8),
						#[codec(index = 247)]
						Mortal247(::core::primitive::u8),
						#[codec(index = 248)]
						Mortal248(::core::primitive::u8),
						#[codec(index = 249)]
						Mortal249(::core::primitive::u8),
						#[codec(index = 250)]
						Mortal250(::core::primitive::u8),
						#[codec(index = 251)]
						Mortal251(::core::primitive::u8),
						#[codec(index = 252)]
						Mortal252(::core::primitive::u8),
						#[codec(index = 253)]
						Mortal253(::core::primitive::u8),
						#[codec(index = 254)]
						Mortal254(::core::primitive::u8),
						#[codec(index = 255)]
						Mortal255(::core::primitive::u8),
					}
				}
				pub mod header {
					use super::runtime_types;
					#[derive(
						:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
						:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
						:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
						:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
						Debug,
					)]
					# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
					#[decode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
					)]
					#[encode_as_type(
						crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
					)]
					pub struct Header<_0> {
						pub parent_hash: ::subxt::ext::subxt_core::utils::H256,
						#[codec(compact)]
						pub number: _0,
						pub state_root: ::subxt::ext::subxt_core::utils::H256,
						pub extrinsics_root: ::subxt::ext::subxt_core::utils::H256,
						pub digest: runtime_types::sp_runtime::generic::digest::Digest,
					}
				}
			}
			pub mod proving_trie {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum TrieError {
					#[codec(index = 0)]
					InvalidStateRoot,
					#[codec(index = 1)]
					IncompleteDatabase,
					#[codec(index = 2)]
					ValueAtIncompleteKey,
					#[codec(index = 3)]
					DecoderError,
					#[codec(index = 4)]
					InvalidHash,
					#[codec(index = 5)]
					DuplicateKey,
					#[codec(index = 6)]
					ExtraneousNode,
					#[codec(index = 7)]
					ExtraneousValue,
					#[codec(index = 8)]
					ExtraneousHashReference,
					#[codec(index = 9)]
					InvalidChildReference,
					#[codec(index = 10)]
					ValueMismatch,
					#[codec(index = 11)]
					IncompleteProof,
					#[codec(index = 12)]
					RootMismatch,
					#[codec(index = 13)]
					DecodeError,
				}
			}
			pub mod traits {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct BlakeTwo256;
			}
			pub mod transaction_validity {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum InvalidTransaction {
					#[codec(index = 0)]
					Call,
					#[codec(index = 1)]
					Payment,
					#[codec(index = 2)]
					Future,
					#[codec(index = 3)]
					Stale,
					#[codec(index = 4)]
					BadProof,
					#[codec(index = 5)]
					AncientBirthBlock,
					#[codec(index = 6)]
					ExhaustsResources,
					#[codec(index = 7)]
					Custom(::core::primitive::u8),
					#[codec(index = 8)]
					BadMandatory,
					#[codec(index = 9)]
					MandatoryValidation,
					#[codec(index = 10)]
					BadSigner,
					#[codec(index = 11)]
					IndeterminateImplicit,
					#[codec(index = 12)]
					UnknownOrigin,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum TransactionSource {
					#[codec(index = 0)]
					InBlock,
					#[codec(index = 1)]
					Local,
					#[codec(index = 2)]
					External,
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum TransactionValidityError {
					#[codec(index = 0)]
					Invalid(runtime_types::sp_runtime::transaction_validity::InvalidTransaction),
					#[codec(index = 1)]
					Unknown(runtime_types::sp_runtime::transaction_validity::UnknownTransaction),
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub enum UnknownTransaction {
					#[codec(index = 0)]
					CannotLookup,
					#[codec(index = 1)]
					NoUnsignedValidator,
					#[codec(index = 2)]
					Custom(::core::primitive::u8),
				}
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct ValidTransaction {
					pub priority: ::core::primitive::u64,
					pub requires: ::subxt::ext::subxt_core::alloc::vec::Vec<
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					>,
					pub provides: ::subxt::ext::subxt_core::alloc::vec::Vec<
						::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
					>,
					pub longevity: ::core::primitive::u64,
					pub propagate: ::core::primitive::bool,
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum DispatchError {
				#[codec(index = 0)]
				Other,
				#[codec(index = 1)]
				CannotLookup,
				#[codec(index = 2)]
				BadOrigin,
				#[codec(index = 3)]
				Module(runtime_types::sp_runtime::ModuleError),
				#[codec(index = 4)]
				ConsumerRemaining,
				#[codec(index = 5)]
				NoProviders,
				#[codec(index = 6)]
				TooManyConsumers,
				#[codec(index = 7)]
				Token(runtime_types::sp_runtime::TokenError),
				#[codec(index = 8)]
				Arithmetic(runtime_types::sp_arithmetic::ArithmeticError),
				#[codec(index = 9)]
				Transactional(runtime_types::sp_runtime::TransactionalError),
				#[codec(index = 10)]
				Exhausted,
				#[codec(index = 11)]
				Corruption,
				#[codec(index = 12)]
				Unavailable,
				#[codec(index = 13)]
				RootNotAllowed,
				#[codec(index = 14)]
				Trie(runtime_types::sp_runtime::proving_trie::TrieError),
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum ExtrinsicInclusionMode {
				#[codec(index = 0)]
				AllExtrinsics,
				#[codec(index = 1)]
				OnlyInherents,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct ModuleError {
				pub index: ::core::primitive::u8,
				pub error: [::core::primitive::u8; 4usize],
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum MultiSignature {
				#[codec(index = 0)]
				Ed25519([::core::primitive::u8; 64usize]),
				#[codec(index = 1)]
				Sr25519([::core::primitive::u8; 64usize]),
				#[codec(index = 2)]
				Ecdsa([::core::primitive::u8; 65usize]),
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct OpaqueValue(
				pub ::subxt::ext::subxt_core::alloc::vec::Vec<::core::primitive::u8>,
			);
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum TokenError {
				#[codec(index = 0)]
				FundsUnavailable,
				#[codec(index = 1)]
				OnlyProvider,
				#[codec(index = 2)]
				BelowMinimum,
				#[codec(index = 3)]
				CannotCreate,
				#[codec(index = 4)]
				UnknownAsset,
				#[codec(index = 5)]
				Frozen,
				#[codec(index = 6)]
				Unsupported,
				#[codec(index = 7)]
				CannotCreateHold,
				#[codec(index = 8)]
				NotExpendable,
				#[codec(index = 9)]
				Blocked,
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub enum TransactionalError {
				#[codec(index = 0)]
				LimitReached,
				#[codec(index = 1)]
				NoLayer,
			}
		}
		pub mod sp_session_validator_management {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct MainChainScripts {
				pub committee_candidate_address: runtime_types::sidechain_domain::MainchainAddress,
				pub d_parameter_policy_id: runtime_types::sidechain_domain::PolicyId,
				pub permissioned_candidates_policy_id: runtime_types::sidechain_domain::PolicyId,
			}
		}
		pub mod sp_sidechain {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct SidechainStatus {
				pub epoch: runtime_types::sidechain_domain::ScEpochNumber,
				pub slot: runtime_types::sidechain_domain::ScSlotNumber,
				pub slots_per_epoch: ::core::primitive::u32,
			}
		}
		pub mod sp_staking {
			use super::runtime_types;
			pub mod offence {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct OffenceSeverity(pub runtime_types::sp_arithmetic::per_things::Perbill);
			}
		}
		pub mod sp_version {
			use super::runtime_types;
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct RuntimeVersion {
				pub spec_name: ::subxt::ext::subxt_core::alloc::string::String,
				pub impl_name: ::subxt::ext::subxt_core::alloc::string::String,
				pub authoring_version: ::core::primitive::u32,
				pub spec_version: ::core::primitive::u32,
				pub impl_version: ::core::primitive::u32,
				pub apis: ::subxt::ext::subxt_core::alloc::vec::Vec<(
					[::core::primitive::u8; 8usize],
					::core::primitive::u32,
				)>,
				pub transaction_version: ::core::primitive::u32,
				pub system_version: ::core::primitive::u8,
			}
		}
		pub mod sp_weights {
			use super::runtime_types;
			pub mod weight_v2 {
				use super::runtime_types;
				#[derive(
					:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
					:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
					:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
					:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
					Debug,
				)]
				# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
				#[decode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode"
				)]
				#[encode_as_type(
					crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode"
				)]
				pub struct Weight {
					#[codec(compact)]
					pub ref_time: ::core::primitive::u64,
					#[codec(compact)]
					pub proof_size: ::core::primitive::u64,
				}
			}
			#[derive(
				:: subxt :: ext :: subxt_core :: ext :: codec :: Decode,
				:: subxt :: ext :: subxt_core :: ext :: codec :: Encode,
				:: subxt :: ext :: subxt_core :: ext :: scale_decode :: DecodeAsType,
				:: subxt :: ext :: subxt_core :: ext :: scale_encode :: EncodeAsType,
				Debug,
			)]
			# [codec (crate = :: subxt :: ext :: subxt_core :: ext :: codec)]
			#[decode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_decode")]
			#[encode_as_type(crate_path = ":: subxt :: ext :: subxt_core :: ext :: scale_encode")]
			pub struct RuntimeDbWeight {
				pub read: ::core::primitive::u64,
				pub write: ::core::primitive::u64,
			}
		}
	}
}
