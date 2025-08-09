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

use super::error::CfgError;
use serde::Serialize;

pub(crate) fn get_keys<T: Serialize>(struct_val: T) -> Result<Vec<String>, CfgError> {
	let value = serde_json::to_value(struct_val).map_err(CfgError::GetKeysError)?;
	Ok(value
		.as_object()
		.map(|m| m.keys().cloned().collect::<Vec<String>>())
		.unwrap_or_default())
}
