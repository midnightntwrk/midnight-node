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

use frame_support::traits::{EnsureOrigin, PalletInfoAccess};
use sp_std::marker::PhantomData;

pub type AuthId = u32;

pub trait FederatedAuthorityProportion {
	fn reached_proportion(n: u32, d: u32) -> bool;
}

/// A type-level struct to hold the specification for a single federated authority.
/// - `P`: The pallet type itself (from `construct_runtime!`)
/// - `EnsureProportion`: The function that calculates if there is enough positive votes
pub struct AuthorityBody<P, EnsureProportion> {
	pub _phantom: PhantomData<(P, EnsureProportion)>,
}

/// Helper trait to check an origin against an `AuthorityBody`.
pub trait EnsureFromIdentity<O> {
	/// On success, returns the pallet index of the authority that matched.
	fn ensure_from_bodies(o: O) -> Result<AuthId, O>;

	#[allow(clippy::result_unit_err)]
	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<O, ()>;
}

impl<O, P, EnsureProportion> EnsureFromIdentity<O> for AuthorityBody<P, EnsureProportion>
where
	O: Clone,
	P: PalletInfoAccess,
	EnsureProportion: EnsureOrigin<O>,
{
	fn ensure_from_bodies(o: O) -> Result<AuthId, O> {
		EnsureProportion::try_origin(o).map(|_| P::index() as u32)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<O, ()> {
		EnsureProportion::try_successful_origin()
	}
}

#[allow(clippy::single_match)]
#[impl_trait_for_tuples::impl_for_tuples(5)]
impl<O: Clone> EnsureFromIdentity<O> for Tuple {
	fn ensure_from_bodies(o: O) -> Result<AuthId, O> {
		for_tuples!( #(
            match Tuple::ensure_from_bodies(o.clone()) {
                Ok(auth_origin_info) => return Ok(auth_origin_info),
                Err(_) => {}
            }
        )* );
		Err(o)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<O, ()> {
		for_tuples!( #(
            match Tuple::try_successful_origin() {
                Ok(successful_origin) => return Ok(successful_origin),
                Err(_) => {}
            }
        )* );
		// All tuple members returned Err, so none can provide a successful origin
		Err(())
	}
}

/// A generic `EnsureOrigin` implementation that checks an origin against a list
/// of authority specifications provided in a tuple.
pub struct FederatedAuthorityOriginManager<Authorities>(pub PhantomData<Authorities>);

impl<O, Authorities> EnsureOrigin<O> for FederatedAuthorityOriginManager<Authorities>
where
	O: Clone,
	Authorities: EnsureFromIdentity<O>,
{
	type Success = AuthId;

	fn try_origin(o: O) -> Result<Self::Success, O> {
		Authorities::ensure_from_bodies(o)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<O, ()> {
		Authorities::try_successful_origin()
	}
}

pub struct FederatedAuthorityEnsureProportionAtLeast<const N: u32, const D: u32>;

impl<const N: u32, const D: u32> FederatedAuthorityProportion
	for FederatedAuthorityEnsureProportionAtLeast<N, D>
{
	fn reached_proportion(n: u32, d: u32) -> bool {
		n * D >= N * d
	}
}
