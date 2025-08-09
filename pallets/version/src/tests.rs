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

use frame_support::pallet_prelude::Hooks;
use sp_runtime::traits::Header as HeaderT;

use crate::mock::*;

#[test]
fn version_in_header() {
	new_test_ext().execute_with(|| {
		NodeVersion::on_initialize(1);
		let header: Header = System::finalize();
		let version = header
			.digest()
			.convert_first(NodeVersion::decode_version)
			.expect("Version digest log item not found in header");

		assert_eq!(version, VERSION.spec_version);
	});
}
