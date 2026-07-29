// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

//! Whole-transaction proving provider for ledger generations whose circuits are
//! all ZKIR-v2 (L7/L8). See `v2_or_v3.rs` for the ledger-9 variant, which has to
//! dispatch per circuit because Dust's spend circuit moved to ZKIR-v3.

use rand::rngs::StdRng;

use super::{
	mn_ledger::prove::Resolver, test_utilities_local::PUBLIC_PARAMS, zkir::LocalProvingProvider,
	zswap::prove::ZswapResolver,
};

/// L7/L8 never encounter v3-tagged IR, so proving is always the plain `zkir` pipeline.
pub fn make_proving_provider<'a>(
	rng: StdRng,
	resolver: &'a Resolver,
) -> LocalProvingProvider<'a, StdRng, Resolver, ZswapResolver> {
	LocalProvingProvider { rng, resolver, params: &*PUBLIC_PARAMS }
}
