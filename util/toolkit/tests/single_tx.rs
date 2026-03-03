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

use common::test_image;
use std::time::Duration;
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

			// Wait for at least 2 blocks to be produced (6s block time).
			tokio::time::sleep(Duration::from_secs(20)).await;

			SharedNode { _container: container, ws_url: format!("ws://127.0.0.1:{port}") }
		})
		.await
		.ws_url
}

#[tokio::test]
async fn single_tx_examples() {
	let url = node_ws_url().await;

	trycmd::TestCases::new()
		.case("examples/single-tx.md")
		.env("MN_SRC_URL", url)
		.env("MN_DEST_URL", url);
}
