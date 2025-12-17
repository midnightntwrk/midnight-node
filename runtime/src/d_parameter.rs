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

//! D Parameter provider trait and implementations.
//!
//! This module provides an abstraction for sourcing D Parameter values used in
//! validator selection. The D Parameter controls the ratio of permissioned to
//! registered validators in the committee.
//!
//! Currently uses a mock implementation that returns fixed values. This will be
//! replaced with `pallet-system-parameters` when available.

use sidechain_domain::DParameter;

/// Trait for providing D Parameter values for authority selection.
///
/// Implementations can source D Parameter from various locations:
/// - Mock: Returns fixed values for development/testing
/// - `pallet-system-parameters`: On-chain governance (future implementation)
///
/// When `get_d_parameter()` returns `Some(d_parameter)`, that value is used
/// for authority selection, overriding any value from inherent data.
/// When it returns `None`, the D Parameter from inherent data is used.
pub trait DParameterProvider {
	/// Returns the D Parameter to use for authority selection.
	///
	/// Returns `Some(DParameter)` to override the inherent data value,
	/// or `None` to use the inherent data value.
	fn get_d_parameter() -> Option<DParameter>;
}

/// Mock implementation of `DParameterProvider` for development and testing.
///
/// Returns fixed D Parameter values. These values should be configured
/// appropriately for the target environment.
///
/// This implementation will be replaced by `pallet-system-parameters` when available.
pub struct MockDParameterProvider;

impl DParameterProvider for MockDParameterProvider {
	fn get_d_parameter() -> Option<DParameter> {
		// Return None to use D Parameter from inherent data (main chain).
		// This maintains backward compatibility during the transition period.
		// When pallet-system-parameters is integrated, this will return
		// Some(DParameter) with values from on-chain storage.
		None
	}
}

/// Implementation that always returns a fixed D Parameter.
///
/// Useful for testing scenarios where a specific D Parameter is needed.
#[cfg(any(test, feature = "runtime-benchmarks"))]
pub struct FixedDParameterProvider<const P: u16, const R: u16>;

#[cfg(any(test, feature = "runtime-benchmarks"))]
impl<const P: u16, const R: u16> DParameterProvider for FixedDParameterProvider<P, R> {
	fn get_d_parameter() -> Option<DParameter> {
		Some(DParameter { num_permissioned_candidates: P, num_registered_candidates: R })
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn mock_provider_returns_none() {
		// During transition, mock returns None to use inherent data
		assert!(MockDParameterProvider::get_d_parameter().is_none());
	}

	#[test]
	fn fixed_provider_returns_configured_values() {
		type Provider = FixedDParameterProvider<3, 2>;
		let d_param = Provider::get_d_parameter().expect("Should return Some");
		assert_eq!(d_param.num_permissioned_candidates, 3);
		assert_eq!(d_param.num_registered_candidates, 2);
	}
}
