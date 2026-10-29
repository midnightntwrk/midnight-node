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

use anyhow::{anyhow, ensure, Context, Result};
use clap::Parser;
use midnight_beefy::*;
use parity_scale_codec::Encode;
use serde_json::json;
use sha2::{Digest, Sha256};
use sp1_sdk::{prelude::*, ProverClient, SP1Stdin};
use std::{fs, path::PathBuf, time::Instant};
mod select;
const ELF: Elf = include_elf!("midnight-sp1-guest");

#[derive(Parser)]
struct Args {
	#[arg(long)]
	checkpoint: Option<PathBuf>,
	#[arg(long)]
	captures: Option<PathBuf>,
	#[arg(long)]
	block: Option<u32>,
	#[arg(long)]
	out: PathBuf,
}
fn checked<T>(value: midnight_beefy::Result<T>) -> Result<T> {
	value.map_err(|e| anyhow!(e))
}
fn hx(bytes: impl AsRef<[u8]>) -> String {
	format!("0x{}", hex::encode(bytes))
}

#[tokio::main]
async fn main() -> Result<()> {
	let args = Args::parse();
	ensure!(!args.out.exists(), "refusing to overwrite an existing proof job");
	sp1_sdk::utils::setup_logger();
	let client = ProverClient::builder().cpu().build().await;
	let pk = client.setup(ELF.clone()).await.context("SP1 setup")?;
	let mut evidence = json!({"programVKey": pk.verifying_key().bytes32(),
        "elfSha256": hex::encode(Sha256::digest(ELF.as_ref())),
        "sdkVersion": "6.10.0", "circuitVersion": "v6.1.0", "profile": "ethereum-address-stake-v2-committee-mmr"});
	if let Some(checkpoint_path) = args.checkpoint {
		let checkpoint: Checkpoint = checked(decode_all(&fs::read(checkpoint_path)?))?;
		let target = args.block.context("--block required")?;
		ensure!(target > checkpoint.block_number, "stale target");
		let mut captures = Vec::new();
		for entry in fs::read_dir(args.captures.as_ref().context("--captures required")?)? {
			let path = entry?.path();
			let name = path.file_name().unwrap().to_string_lossy();
			if !(name.starts_with("beefy-") && name.ends_with(".scale")) {
				continue;
			}
			let bytes = fs::read(&path)?;
			let proof = checked(decode_justification(&bytes))?;
			if proof.commitment.block_number <= target
				&& proof.commitment.block_number > checkpoint.block_number
			{
				let block = proof.commitment.block_number;
				let committee = load_committee(args.captures.as_ref().unwrap(), block)?;
				captures.push((block, bytes, committee));
			}
		}
		let input = checked(select::select(checkpoint, captures, target))?;
		let blocks: Vec<_> = input
			.justifications
			.iter()
			.map(|bytes| decode_justification(bytes).unwrap().commitment.block_number)
			.collect();
		let expected = checked(verify_batch(&input))?;
		let encoded = input.encode();
		ensure!(encoded.len() <= 4 * 1024 * 1024, "input exceeds guest bound");
		let mut stdin = SP1Stdin::new();
		stdin.write_vec(encoded.clone());
		fs::create_dir_all(&args.out)?;
		fs::write(args.out.join("input.scale"), &encoded)?;
		let started = Instant::now();
		let proof = client.prove(&pk, stdin).groth16().await.context("real CPU Groth16 proof")?;
		ensure!(
			proof.public_values.as_slice() == expected.encode(),
			"guest/native output mismatch"
		);
		client.verify(&proof, pk.verifying_key(), None).context("SP1 verification")?;
		proof.save(args.out.join("proof.bin"))?;
		fs::write(args.out.join("checkpoint.scale"), expected.new_state.encode())?;
		evidence["proofBytes"] = json!(hx(proof.bytes()));
		evidence["publicValues"] = json!(hx(proof.public_values.as_slice()));
		evidence["previousStateHash"] = json!(hx(expected.previous_state_hash));
		evidence["checkpointHash"] = json!(hx(state_hash(&expected.new_state)));
		evidence["checkpoint"] = json!(hx(expected.new_state.encode()));
		evidence["block"] = json!(expected.new_state.block_number);
		evidence["mmrRoot"] = json!(hx(expected.mmr_root));
		evidence["verifiedBlocks"] = json!(blocks);
		evidence["committeeStateVerified"] = json!(true);
		evidence["provingSeconds"] = json!(started.elapsed().as_secs_f64());
		evidence["realGroth16"] = json!(true);
	} else {
		fs::create_dir_all(&args.out)?;
	}
	fs::write(args.out.join("proof.json"), serde_json::to_vec_pretty(&evidence)?)?;
	Ok(())
}

fn load_committee(dir: &std::path::Path, block: u32) -> Result<CommitteeProof> {
	let path = dir.join(format!("committee-{block}.json"));
	let witness: serde_json::Value = serde_json::from_slice(
		&fs::read(&path)
			.with_context(|| format!("missing runtime committee witness: {}", path.display()))?,
	)?;
	let bytes = |v: &serde_json::Value| -> Result<Vec<u8>> {
		let text = v.as_str().context("expected hex string")?;
		Ok(hex::decode(text.strip_prefix("0x").context("expected 0x prefix")?)?)
	};
	Ok(CommitteeProof {
		leaf: bytes(&witness["leaf"])?,
		leaf_index: witness["leafIndex"].as_str().context("leafIndex")?.parse()?,
		leaf_count: witness["leafCount"].as_str().context("leafCount")?.parse()?,
		items: witness["items"]
			.as_array()
			.context("items")?
			.iter()
			.map(|v| bytes(v)?.try_into().map_err(|_| anyhow!("MMR hash length")))
			.collect::<Result<_>>()?,
	})
}
