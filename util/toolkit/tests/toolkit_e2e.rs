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

mod common;

use clap::Parser;
use common::{
	test_image,
	toolkit_helper::{CircuitCall, ToolkitTestHelper},
	wait_for_node::wait_for_finalized_block,
};
use midnight_node_toolkit::{
	cli::{Cli, Commands, run_command},
	commands::{contract_address, show_address},
	tx_generator::builder::FUNDING_SEED,
};
use std::{path::Path, time::Duration};
use testcontainers::{
	GenericImage, ImageExt,
	core::{ContainerPort, WaitFor},
	runners::AsyncRunner,
};
use tokio::sync::OnceCell;

struct SharedNode {
	_container: testcontainers::ContainerAsync<GenericImage>,
	ws_url: String,
}

static NODE: OnceCell<SharedNode> = OnceCell::const_new();

async fn node_ws_url() -> &'static str {
	&NODE
		.get_or_init(|| async {
			let (name, tag) = test_image("midnight-node");
			let container = GenericImage::new(name, tag)
				.with_wait_for(WaitFor::message_on_stderr("Running JSON-RPC server"))
				.with_exposed_port(ContainerPort::Tcp(9944))
				.with_env_var("CFG_PRESET", "dev")
				.start()
				.await
				.expect("failed to start midnight-node container");

			let port =
				container.get_host_port_ipv4(9944).await.expect("failed to get node RPC port");
			let ws_url = format!("ws://127.0.0.1:{port}");

			// Wait for finality. The toolkit CLI calls get_block_one_hash on
			// transaction-generating commands, which fails with OnlyGenesisFinalized
			// until finalized height >= 1.
			wait_for_finalized_block(&ws_url, 1, Duration::from_secs(60)).await;

			SharedNode { _container: container, ws_url }
		})
		.await
		.ws_url
}

struct SharedPostgres {
	_container: testcontainers::ContainerAsync<GenericImage>,
	url: String,
}

static POSTGRES: OnceCell<SharedPostgres> = OnceCell::const_new();

async fn postgres_url() -> &'static str {
	&POSTGRES
		.get_or_init(|| async {
			let (name, tag) = test_image("postgres");
			let password: String =
				(0..32).map(|_| format!("{:02x}", rand::random::<u8>())).collect();
			let container = GenericImage::new(name, tag)
				.with_wait_for(WaitFor::message_on_stderr(
					"database system is ready to accept connections",
				))
				.with_env_var("POSTGRES_PASSWORD", &password)
				.with_env_var("POSTGRES_USER", "test")
				.with_env_var("POSTGRES_DB", "toolkit")
				.start()
				.await
				.expect("failed to start postgres container");

			let port =
				container.get_host_port_ipv4(5432).await.expect("failed to get postgres port");
			let url = format!("postgres://test:{password}@localhost:{port}/toolkit");
			SharedPostgres { _container: container, url }
		})
		.await
		.url
}

async fn run_cli(args: &[&str]) {
	let full_args: Vec<&str> =
		std::iter::once("midnight-node-toolkit").chain(args.iter().copied()).collect();
	let cli = Cli::parse_from(full_args);
	run_command(cli.command).await.expect("CLI command failed");
}

const RNG_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000037";

fn ledger_test_artifacts_ready() -> bool {
	let Ok(path) = std::env::var("MIDNIGHT_LEDGER_TEST_STATIC_DIR") else {
		eprintln!("Skipping contract e2e tests: MIDNIGHT_LEDGER_TEST_STATIC_DIR is not set");
		return false;
	};
	if !Path::new(&path).exists() {
		eprintln!(
			"Skipping contract e2e tests: MIDNIGHT_LEDGER_TEST_STATIC_DIR does not exist: {}",
			path
		);
		return false;
	}
	true
}

#[tokio::test]
async fn generate_batches() {
	let url = node_ws_url().await;

	// generate-txs batches
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"batches",
		"--funding-seed",
		"0000000000000000000000000000000000000000000000000000000000000003",
		"-n",
		"1",
		"-b",
		"1",
		"-s",
		url,
		"-d",
		url,
	])
	.await;

	// 8. Single-tx shielded
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"single-tx",
		"--source-seed",
		"0000000000000000000000000000000000000000000000000000000000000003",
		"--shielded-amount",
		"10",
		"--destination-address",
		"mn_shield-addr_undeployed1tdu4jzhm7xn9qhzwweleyszxmhtt7fnzfhql42g87aay2jdjvau3fljgum7nqky8cj5mmm697rd33uyh6dnw42thuucjp7da74nje0sggh42d",
		"--destination-address",
		"mn_shield-addr_undeployed1tth9g6jf8he6cmhgtme6arty0jde7wnypsg53qc3x5navl9za355jqqvfftm8asg986dx9puzwkmedeune9nfkuqvtmccmxtjwvlrvccwypcs",
		"--destination-address",
		"mn_shield-addr_undeployed1ngp7ce7cqclgucattj5kuw68v3s4826e9zwalhhmurymwet3v7psvrs4gtpv5p2zx8rd3jxpgjr4m8mxh7js7u3l33g23gcty67uq9cug4xep",
		"-s",
		url,
		"-d",
		url,
	])
	.await;
}

#[tokio::test]
async fn get_version() {
	run_cli(&["version"]).await;
}

#[tokio::test]
async fn register_dust_address() {
	let url = node_ws_url().await;

	// 3b. Extract contract address (parse CLI to get args, then call execute directly)
	let dust_address = {
		let cli = Cli::parse_from([
			"midnight-node-toolkit",
			"show-address",
			"--network",
			"undeployed",
			"--seed",
			"0000000000000000000000000000000000000000000000000000000000000002",
			"--dust",
		]);
		match cli.command {
			Commands::ShowAddress(args) => match show_address::execute(args) {
				show_address::ShowAddress::SingleAddress(addr) => addr,
				show_address::ShowAddress::Addresses(_) => panic!("should not reach this arm"),
			},
			_ => unreachable!(),
		}
	};

	// 5. Register dust address (with destination-dust)
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"register-dust-address",
		"--wallet-seed",
		"0000000000000000000000000000000000000000000000000000000000000002",
		"--funding-seed",
		"0000000000000000000000000000000000000000000000000000000000000002",
		"--destination-dust",
		&dust_address,
		"-s",
		url,
		"-d",
		url,
	])
	.await;

	// 6. Register dust address (empty wallet, no destination-dust)
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"register-dust-address",
		"--wallet-seed",
		"0000000000000000000000000000000000000000000000000000000000000052",
		"--funding-seed",
		"0000000000000000000000000000000000000000000000000000000000000002",
		"-s",
		url,
		"-d",
		url,
	])
	.await;

	// 7. Deregister dust address
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"deregister-dust-address",
		"--wallet-seed",
		"0000000000000000000000000000000000000000000000000000000000000002",
		"--funding-seed",
		"0000000000000000000000000000000000000000000000000000000000000002",
		"-s",
		url,
		"-d",
		url,
	])
	.await;
}

#[tokio::test]
async fn contract_ops() {
	if !ledger_test_artifacts_ready() {
		return;
	}

	let url = node_ws_url().await;

	// 3. Contract deploy + address + send + maintenance + call(store) + call(check)
	let tempdir = tempfile::tempdir().expect("failed to create tempdir");
	let deploy_file = tempdir.path().join("contract_deploy.mn");
	let deploy_file_str = deploy_file.to_string_lossy().to_string();

	// 3a. Generate deploy tx to file
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"--dest-file",
		&deploy_file_str,
		"contract-simple",
		"deploy",
		"--rng-seed",
		RNG_SEED,
		"-s",
		url,
	])
	.await;

	// 3b. Extract contract address (parse CLI to get args, then call execute directly)
	let contract_address = {
		let cli = Cli::parse_from([
			"midnight-node-toolkit",
			"contract-address",
			"--src-file",
			&deploy_file_str,
		]);
		match cli.command {
			Commands::ContractAddress(args) => {
				contract_address::execute(args).expect("failed to get contract address")
			},
			_ => unreachable!(),
		}
	};

	// 3c. Send the deploy tx
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		&format!("--src-file={deploy_file_str}"),
		"send",
		"-d",
		url,
	])
	.await;

	// 3d. Contract maintenance
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"contract-simple",
		"maintenance",
		"--rng-seed",
		RNG_SEED,
		"--contract-address",
		&contract_address,
		"--new-authority-seed",
		"1000000000000000000000000000000000000000000000000000000000000001",
		"-s",
		url,
		"-d",
		url,
	])
	.await;

	// 3e. Contract call (store)
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"contract-simple",
		"call",
		"--call-key",
		"store",
		"--rng-seed",
		RNG_SEED,
		"--contract-address",
		&contract_address,
		"-s",
		url,
		"-d",
		url,
	])
	.await;

	// 3f. Contract call (check)
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"contract-simple",
		"call",
		"--call-key",
		"check",
		"--rng-seed",
		RNG_SEED,
		"--contract-address",
		&contract_address,
		"-s",
		url,
		"-d",
		url,
	])
	.await;

	// 9. Fetch with redb backend
	let redb_path = tempdir.path().join("e2e_test.db");
	let redb_cache = format!("redb:{}", redb_path.to_string_lossy());
	run_cli(&["fetch", "--fetch-cache", &redb_cache, "-s", url]).await;

	// 10. Fetch with inmemory backend
	run_cli(&["fetch", "--fetch-cache", "inmemory", "-s", url]).await;

	// 11. Fetch with postgres backend
	let pg_url = postgres_url().await;
	run_cli(&["fetch", "--fetch-cache", pg_url, "-s", url]).await;
}

/// Verifies that a private witness (secret key) used in ZK proofs never leaks
/// into on-chain transaction data. Deploys a bulletin board contract, posts a
/// message using the secret key as a private witness, then asserts the key does
/// not appear anywhere in the serialized transactions.
#[tokio::test]
#[ignore = "LEDGER9-TOOLKIT-JS: toolkit-js v9 / compact-js with intent[v7] serializer not yet vendored"]
async fn bboard_private_witness_not_leaked() {
	let url = node_ws_url().await;
	let helper = ToolkitTestHelper::new(url);

	if !helper.prerequisites_ready() {
		return;
	}

	let secret_key = "deadbeefcafebabe1234567890abcdef1122334455667788aabbccddeeff0011";

	println!("1. Generating coin-public address");
	let coin_public = helper.show_address_coin_public(FUNDING_SEED);
	println!("   coin-public: {coin_public}");

	println!("2. Compiling bboard contract");
	let bboard_source = helper.load_contract_file("bboard/bboard.compact");
	let compiled_dir = helper
		.compile_contract(&bboard_source, "bboard")
		.await
		.expect("contract compilation failed");

	let config_content = helper.load_template(
		"bboard/config.template.ts",
		&[("SECRET_KEY", secret_key), ("COIN_PUBLIC", &coin_public), ("NETWORK", "undeployed")],
	);
	let config_file = helper.write_config(&config_content, "bboard/contract.config.ts");
	println!("   compiled to: {}", compiled_dir.display());

	println!("3. Deploying bboard contract");
	let deploy = helper
		.generate_intent_deploy(&config_file, &coin_public)
		.await
		.expect("generate deploy intent failed");
	let deploy_tx = helper
		.send_intent(&deploy.intent, &compiled_dir, FUNDING_SEED, None)
		.await
		.expect("send deploy intent failed");
	helper.submit_tx(&deploy_tx).await.expect("submit deploy tx failed");
	let bboard_addr =
		helper.contract_address(&deploy_tx).expect("contract address extraction failed");
	println!("   bboard address: {bboard_addr}");

	println!("4. Fetching contract state");
	let state_file = helper.work_dir.path().join("bboard_state.mn");
	helper
		.contract_state(&bboard_addr, &state_file)
		.await
		.expect("contract state fetch failed");

	println!("5. Calling post() with secret key as private witness");
	let post = helper
		.generate_intent_circuit(
			&config_file,
			&coin_public,
			&state_file,
			&deploy.private_state,
			&bboard_addr,
			CircuitCall {
				circuit_id: "post",
				call_args: &["\"Hello from Rust e2e! Privacy verification test.\""],
			},
		)
		.await
		.expect("generate post intent failed");
	let post_tx = helper
		.send_intent(&post.intent, &compiled_dir, FUNDING_SEED, Some(&post.zswap_state))
		.await
		.expect("send post intent failed");
	helper.submit_tx(&post_tx).await.expect("submit post tx failed");
	println!("   post() accepted on-chain");

	println!("6. Verifying post() transaction does not contain secret key");
	helper.assert_secret_not_in_tx(&post_tx, secret_key, "post()");

	println!("7. Fetching updated contract state");
	let state_file_2 = helper.work_dir.path().join("bboard_state_2.mn");
	helper
		.contract_state(&bboard_addr, &state_file_2)
		.await
		.expect("contract state fetch failed");

	println!("8. Calling takeDown() with same secret key");
	let takedown = helper
		.generate_intent_circuit(
			&config_file,
			&coin_public,
			&state_file_2,
			&post.private_state,
			&bboard_addr,
			CircuitCall { circuit_id: "takeDown", call_args: &[] },
		)
		.await
		.expect("generate takeDown intent failed");
	let takedown_tx = helper
		.send_intent(&takedown.intent, &compiled_dir, FUNDING_SEED, Some(&takedown.zswap_state))
		.await
		.expect("send takeDown intent failed");
	helper.submit_tx(&takedown_tx).await.expect("submit takeDown tx failed");
	println!("   takeDown() accepted on-chain");

	println!("9. Verifying takeDown() transaction does not contain secret key");
	helper.assert_secret_not_in_tx(&takedown_tx, secret_key, "takeDown()");
}
