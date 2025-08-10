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

use super::*;
use crate as pallet_version;

use frame_support::{derive_impl, parameter_types};
use frame_system::mocking::MockUncheckedExtrinsic;
use sp_api::__private::BlockT;
use sp_api::impl_runtime_apis;
use sp_io::TestExternalities;
use sp_runtime::{BuildStorage, Cow, generic};

pub type Header = generic::Header<u64, sp_runtime::traits::BlakeTwo256>;
type Block = generic::Block<Header, MockUncheckedExtrinsic<Test>>;

pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: Cow::Borrowed("midnight"),
	impl_name: Cow::Borrowed("midnight"),
	authoring_version: 0,
	spec_version: 4_777,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 0,
	system_version: 0,
};

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
}

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		NodeVersion: pallet_version,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl Config for Test {
	type WeightInfo = ();
	type RuntimeVersion = Version;
}

impl_runtime_apis! {
	impl sp_api::Core<Block> for Test {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(_: Block) {}

		fn initialize_block(_: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
			sp_runtime::ExtrinsicInclusionMode::OnlyInherents
		}
	}
}

pub(crate) fn new_test_ext() -> TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	TestExternalities::new(t)
}
