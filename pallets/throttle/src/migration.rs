// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Storage migration that clears the `AccountUsage` map after the schema
//! changed from a 2-field tuple to the 3-field `UsageStats` struct (v0 → v1).

use crate::{AccountUsage, Pallet, pallet::Config};
use frame_support::{pallet_prelude::*, traits::OnRuntimeUpgrade};

pub struct ClearAccountUsageV1<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for ClearAccountUsageV1<T> {
	fn on_runtime_upgrade() -> Weight {
		let on_chain = Pallet::<T>::on_chain_storage_version();
		if on_chain != 0 {
			return T::DbWeight::get().reads(1);
		}

		let result = AccountUsage::<T>::clear(u32::MAX, None);
		StorageVersion::new(1).put::<Pallet<T>>();

		T::DbWeight::get().reads_writes(1 + result.unique as u64, 1 + result.unique as u64)
	}
}
