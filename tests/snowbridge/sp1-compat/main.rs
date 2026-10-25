// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use k256::ecdsa::VerifyingKey;
use midnight_beefy as original;
use midnight_beefy_poc_compat as adapted;
use parity_scale_codec::Encode;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn main() {
	let run = PathBuf::from(std::env::args().nth(1).expect("live run directory"));
	let manifest: Value =
		serde_json::from_slice(&fs::read(run.join("validation.json")).unwrap()).unwrap();
	assert_eq!(manifest["completeBridgeE2E"], true);
	let anchor: Value =
		serde_json::from_slice(&fs::read(run.join("beefy-checkpoint.json")).unwrap()).unwrap();
	let mut captures: Vec<_> = fs::read_dir(&run)
		.unwrap()
		.map(|p| p.unwrap().path())
		.filter(|p| {
			p.file_name().unwrap().to_string_lossy().starts_with("beefy-")
				&& p.extension().is_some_and(|e| e == "scale")
		})
		.collect();
	captures.sort_by_key(|p| {
		p.file_stem()
			.unwrap()
			.to_str()
			.unwrap()
			.strip_prefix("beefy-")
			.unwrap()
			.parse::<u32>()
			.unwrap()
	});
	let first = adapted::decode_justification(&fs::read(&captures[0]).unwrap()).unwrap();
	let stakes: adapted::Stakes =
		adapted::decode_all(&first.commitment.payload.iter().find(|p| p.0 == *b"cs").unwrap().1)
			.unwrap();
	assert_eq!(stakes.len(), 1, "this recorded run has one authority");
	assert_eq!(stakes[0].1, 1, "the runtime fixes the per-slot stake field to one");
	// Anchor the key against the saved live node's initial current AND next BEEFY roots.
	// Those roots hash the Ethereum address only, unlike the separate cb/nb stake roots.
	let public = VerifyingKey::from_sec1_bytes(&stakes[0].0).unwrap().to_encoded_point(false);
	let digest = adapted::keccak(&public.as_bytes()[1..]);
	let mmr_authority_root = format!("0x{}", hex::encode(adapted::keccak(&digest[12..])));
	for which in ["current", "next"] {
		assert_eq!(anchor[which]["len"], 1);
		assert_eq!(anchor[which]["root"].as_str().unwrap(), mmr_authority_root);
	}
	let mut genesis = [0; 32];
	genesis.copy_from_slice(
		&hex::decode(anchor["midnightGenesis"].as_str().unwrap().trim_start_matches("0x")).unwrap(),
	);
	let current_id = anchor["current"]["id"].as_u64().unwrap();
	let next_id = anchor["next"]["id"].as_u64().unwrap();
	let mut checkpoint = adapted::Checkpoint {
		genesis_hash: genesis,
		block_number: 0,
		current: adapted::authority_set(current_id, &stakes).unwrap(),
		next: Some(adapted::authority_set(next_id, &stakes).unwrap()),
	};
	let mut accepted = vec![];
	let mut rejected_original = 0;
	let mut negative_checks = 0;
	for path in &captures {
		let bytes = fs::read(path).unwrap();
		let raw = original::Input {
			checkpoint: original::decode_all(&checkpoint.encode()).unwrap(),
			justification: bytes.clone(),
		};
		assert_eq!(original::verify(&raw).err(), Some("current authority commitment mismatch"));
		rejected_original += 1;
		let input = adapted::Input { checkpoint: checkpoint.clone(), justification: bytes };
		let output = adapted::verify(&input).unwrap();
		let decoded = adapted::decode_justification(&input.justification).unwrap();
		let decoded_original = original::decode_justification(&input.justification).unwrap();
		assert_eq!(decoded.encode(), decoded_original.encode(), "wire protocol unchanged");
		let signed_current: adapted::AuthoritySet = adapted::decode_all(
			&decoded.commitment.payload.iter().find(|p| p.0 == *b"cb").unwrap().1,
		)
		.unwrap();
		assert_eq!(output.new_state.current, signed_current);
		let mut broken = input.clone();
		let mut proof = decoded.clone();
		proof.signatures[0][0] ^= 1;
		broken.justification = [vec![1], proof.encode()].concat();
		assert!(adapted::verify(&broken).is_err());
		negative_checks += 1;
		let mut proof = decoded.clone();
		proof.commitment.payload.iter_mut().find(|p| p.0 == *b"mh").unwrap().1[0] ^= 1;
		broken.justification = [vec![1], proof.encode()].concat();
		assert!(adapted::verify(&broken).is_err());
		negative_checks += 1;
		broken = input.clone();
		broken.checkpoint.current.root[0] ^= 1;
		if let Some(next) = &mut broken.checkpoint.next {
			next.root[0] ^= 1;
		}
		assert!(adapted::verify(&broken).is_err());
		negative_checks += 1;
		broken = input.clone();
		broken.checkpoint = output.new_state.clone();
		assert_eq!(adapted::verify(&broken).err(), Some("stale commitment"));
		negative_checks += 1;
		accepted.push(json!({"file":path.file_name().unwrap().to_str().unwrap(), "block":output.new_state.block_number,
            "setId":output.new_state.current.id, "mmrRoot":format!("0x{}",hex::encode(output.mmr_root)),
            "signatures":output.signatures_verified, "publicValuesBytes":output.encode().len()}));
		checkpoint = output.new_state;
	}
	for index in 0..2 {
		let relayed: Value = serde_json::from_slice(
			&fs::read(run.join(format!("outbound-proof-{index}.json"))).unwrap(),
		)
		.unwrap();
		let block = relayed["commitment"]["blockNumber"].as_u64().unwrap();
		let root = relayed["commitment"]["payload"]
			.as_array()
			.unwrap()
			.iter()
			.find(|p| p["payloadID"] == "0x6d68")
			.unwrap()["data"]
			.as_str()
			.unwrap();
		assert!(accepted
			.iter()
			.any(|p| p["block"].as_u64() == Some(block) && p["mmrRoot"] == root));
	}
	println!("{}", serde_json::to_string_pretty(&json!({
        "sourceRun":run, "originalVerifierRejected":rejected_original,
        "originalFailure":"current authority commitment mismatch", "adaptedNativeAccepted":accepted,
        "negativeChecks":negative_checks, "bothEthereumReleaseRootsMatched":true,
        "realSp1ProofGenerated":false, "evmSp1VerificationTested":false,
        "trustAnchor":"saved live genesis + current/next Ethereum authority roots; keys matched; fixed stake=1 per source runtime",
        "profile":"PoC Ethereum address || u64 stake; explicitly selected, no witness-controlled fallback"
    })).unwrap());
}
