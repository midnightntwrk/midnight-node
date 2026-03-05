// This file is part of midnight-node.
// Copyright (C) 2025-2026 Midnight Foundation
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
use common::test_image;
use midnight_node_toolkit::cli::{Cli, run_command};
use std::{process::Command, time::Duration};
use testcontainers::{
	GenericImage, ImageExt,
	core::{ContainerPort, WaitFor},
	runners::AsyncRunner,
};

/// Generate a chain-spec JSON string by running `build-spec` in the fork-from node container.
fn generate_chainspec(image: &str, tag: &str) -> String {
	let output = Command::new("docker")
		.args(["run", "--rm", "-e", "CFG_PRESET=dev", &format!("{image}:{tag}"), "build-spec"])
		.output()
		.expect("docker run build-spec failed");
	assert!(
		output.status.success(),
		"build-spec failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout).expect("invalid utf8 chain-spec")
}

/// Run a toolkit CLI command.
async fn run_cli(args: &[&str]) {
	let full_args: Vec<&str> =
		std::iter::once("midnight-node-toolkit").chain(args.iter().copied()).collect();
	let cli = Cli::parse_from(full_args);
	run_command(cli.command).await.expect("CLI command failed");
}

#[tokio::test]
async fn hardfork_single_tx() {
	// 1. Generate chain-spec from fork-from node
	let (old_name, old_tag) = test_image("midnight-node-fork-from");
	let chainspec_json = generate_chainspec(&old_name, &old_tag);

	let tempdir = tempfile::tempdir().expect("failed to create tempdir");

	// 2. Start new node with fork-from chain-spec
	let (name, tag) = test_image("midnight-node");
	let node_image = format!("{name}:{tag}");
	let container = GenericImage::new(name, tag)
		.with_wait_for(WaitFor::message_on_stderr("Running JSON-RPC server"))
		.with_exposed_port(ContainerPort::Tcp(9944))
		.with_env_var("CFG_PRESET", "dev")
		.with_env_var("CHAIN", "/chainspec/chainspec.json")
		.with_copy_to("/chainspec/chainspec.json", chainspec_json.into_bytes())
		.start()
		.await
		.expect("failed to start midnight-node container");

	let port = container.get_host_port_ipv4(9944).await.expect("failed to get node RPC port");
	let url = format!("ws://127.0.0.1:{port}");

	// Wait for at least 2 blocks to be produced (6s block time).
	tokio::time::sleep(Duration::from_secs(20)).await;

	// 3. Pre-fork: run single-tx to verify the new node works with the fork-from chain-spec
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"single-tx",
		"--source-seed",
		"0000000000000000000000000000000000000000000000000000000000000001",
		"--unshielded-amount",
		"10",
		"--destination-address",
		"mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r",
		"--destination-address",
		"mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm",
		"--destination-address",
		"mn_addr_undeployed12vv6yst6exn50pkjjq54tkmtjpyggmr2p07jwpk6pxd088resqzqszfgak",
		"-s",
		&url,
		"-d",
		&url,
	])
	.await;

	// 4. Runtime upgrade: extract WASM from new node image and apply it
	let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
	let wasm_path_in_image =
		format!("/artifacts-{arch}/midnight_node_runtime.compact.compressed.wasm");
	let wasm_output = Command::new("docker")
		.args(["run", "--rm", "--entrypoint", "cat", &node_image, &wasm_path_in_image])
		.output()
		.expect("docker run cat wasm failed");
	assert!(
		wasm_output.status.success(),
		"failed to extract wasm: {}",
		String::from_utf8_lossy(&wasm_output.stderr)
	);
	let wasm_path = tempdir.path().join("runtime.wasm");
	std::fs::write(&wasm_path, &wasm_output.stdout).expect("write wasm");

	run_cli(&[
		"runtime-upgrade",
		"--wasm-file",
		wasm_path.to_str().unwrap(),
		"-c",
		"//Dave",
		"-c",
		"//Eve",
		"-t",
		"//Alice",
		"-t",
		"//Bob",
		"--rpc-url",
		&url,
		"--signer-key",
		"//Alice",
	])
	.await;

	// 5. Post-fork: run single-tx again to verify the node still works after the (future) upgrade
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"single-tx",
		"--source-seed",
		"0000000000000000000000000000000000000000000000000000000000000001",
		"--unshielded-amount",
		"10",
		"--destination-address",
		"mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r",
		"--destination-address",
		"mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm",
		"--destination-address",
		"mn_addr_undeployed12vv6yst6exn50pkjjq54tkmtjpyggmr2p07jwpk6pxd088resqzqszfgak",
		"-s",
		&url,
		"-d",
		&url,
	])
	.await;
}
