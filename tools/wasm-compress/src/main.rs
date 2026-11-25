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

//! Compress a WASM runtime blob in the substrate `sp-maybe-compressed-blob` format.
//!
//! Format: 8-byte magic prefix + zstd-compressed data (level 3).
//! See `sp_maybe_compressed_blob::compress()` in polkadot-sdk for the reference impl.

use std::io::Write;

/// Magic prefix for zstd-compressed substrate WASM blobs.
/// Must match `ZSTD_PREFIX` in `sp-maybe-compressed-blob`.
const ZSTD_PREFIX: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 5];

/// Maximum decompressed size (same as `CODE_BLOB_BOMB_LIMIT`).
const BOMB_LIMIT: usize = 50 * 1024 * 1024;

fn main() {
	let args: Vec<String> = std::env::args().collect();
	if args.len() != 3 {
		eprintln!("Usage: {} <input.wasm> <output.wasm>", args[0]);
		std::process::exit(1);
	}

	let blob = std::fs::read(&args[1]).unwrap_or_else(|e| {
		eprintln!("Failed to read {}: {e}", args[1]);
		std::process::exit(1);
	});

	if blob.len() > BOMB_LIMIT {
		eprintln!("Input too large ({} bytes > {BOMB_LIMIT})", blob.len());
		std::process::exit(1);
	}

	let mut buf = ZSTD_PREFIX.to_vec();
	{
		let mut encoder = zstd::Encoder::new(&mut buf, 3).expect("zstd encoder creation failed");
		encoder.write_all(&blob).expect("zstd compression failed");
		encoder.finish().expect("zstd finish failed");
	}

	std::fs::write(&args[2], &buf).unwrap_or_else(|e| {
		eprintln!("Failed to write {}: {e}", args[2]);
		std::process::exit(1);
	});

	eprintln!("{} -> {} bytes", blob.len(), buf.len());
}
