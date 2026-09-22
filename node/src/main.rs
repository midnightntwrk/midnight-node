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

//! Midnight Node CLI library.
#![warn(missing_docs)]
#![allow(clippy::result_large_err)]

use midnight_node::command;

fn main() -> sc_cli::Result<()> {
	// Both `ring` (via sqlx) and `aws-lc-rs` (via reqwest) are compiled in, so rustls can't
	// auto-select a default provider and panics the first time one is needed (e.g. an outbound
	// `wss://` connection). Pinning is idempotent, so a pre-installed provider is fine.
	let _ = rustls::crypto::ring::default_provider().install_default();

	command::run()
}
