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

use crate::cli_parsers::{self as cli};
use clap::Args;

#[derive(Args, Clone)]
pub struct ShowSeedArgs {
	/// Wallet seed. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA identity
	/// (ledger 9+). NOTE: the output is the raw seed bytes, which are scheme-independent — both
	/// forms print the same hex; the scheme prefix is accepted only for CLI parity with the other
	/// seed commands and is a no-op here.
	#[arg(long, value_parser = cli::scheme_seed_decode)]
	seed: cli::SchemeSeed,
}

pub fn execute(args: ShowSeedArgs) -> String {
	// The scheme is intentionally discarded: `show-seed` prints the raw seed bytes, which are the
	// same regardless of the unshielded signature scheme.
	let (seed, _scheme) = args.seed.resolve();
	hex::encode(seed.as_bytes())
}
