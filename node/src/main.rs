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
	// Pin the process-wide rustls default CryptoProvider. Both `ring` (via sqlx) and `aws-lc-rs`
	// (via reqwest) are compiled in, so rustls can't auto-select a default and would panic the
	// first time a default-provider consumer runs (e.g. an outbound `wss://` connection).
	// Idempotent: ignore the error if a provider is somehow already installed.
	let _ = rustls::crypto::ring::default_provider().install_default();

	command::run()
}
