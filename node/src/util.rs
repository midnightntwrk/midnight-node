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

use std::fs;
use std::io;
use std::path::Path;

pub fn remove_dir_contents<P: AsRef<Path>>(path: P) -> io::Result<()> {
	for entry in fs::read_dir(path)? {
		let path = entry?.path();
		if path.is_dir() {
			fs::remove_dir_all(path)?;
		} else {
			fs::remove_file(path)?;
		}
	}
	Ok(())
}
