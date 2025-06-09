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

use serde_valid::validation;

pub fn maybe<T>(
	opt: &Option<T>,
	func: fn(&T) -> Result<(), validation::Error>,
) -> Result<(), validation::Error> {
	if let Some(opt) = opt { func(opt) } else { Ok(()) }
}

pub fn path_exists(filename: &String) -> Result<(), validation::Error> {
	let path = std::path::Path::new(filename);
	if !path.exists() {
		return Err(validation::Error::Custom(format!("file path does not exist: {filename}")));
	}
	Ok(())
}
