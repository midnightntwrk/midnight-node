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

//! Row source reading Cardano data from an mc-follower indexer over gRPC.
//! `proto/mc_follower.proto` is a copy of the one in the mc-follower repository.

mod datum;
mod row_source;

/// Types and client generated from `proto/mc_follower.proto`
pub mod proto {
	#![allow(missing_docs, clippy::all)]
	tonic::include_proto!("mcfollower.v1");
}

use proto::mc_follower_client::McFollowerClient;
use std::time::Duration;
use tonic::{Status, transport::Channel};

/// Bound on connecting, as the db-sync connection pools' acquire timeout
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Bound on waiting for a response, and for each message of a streamed one
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
/// Interval of the HTTP/2 pings and TCP keepalive probes detecting a dead connection
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
/// Wait for a ping's acknowledgement before the connection is considered dead
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);

/// Every error fails the data source call, as a failed db-sync query does: mc-follower reports
/// absent data in its responses, so an error never means "no data".
#[derive(Debug, thiserror::Error)]
enum McFollowerError {
	#[error("mc-follower request failed: {0}")]
	Request(Box<Status>),
	#[error("invalid mc-follower response: {0}")]
	InvalidResponse(String),
	#[error("undecodable datum in mc-follower response: {0}")]
	UndecodableDatum(String),
	#[error("mc-follower response timed out")]
	Timeout,
}

impl From<Status> for McFollowerError {
	fn from(status: Status) -> Self {
		Self::Request(Box::new(status))
	}
}

/// Row source of the main chain follower data sources reading from mc-follower
#[derive(Clone, Debug)]
pub struct McFollowerRowSource {
	client: McFollowerClient<Channel>,
}

impl McFollowerRowSource {
	/// Connects to the mc-follower gRPC endpoint, eg. `http://localhost:50051`
	pub async fn connect(
		endpoint: String,
	) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
		// `timeout` bounds only the wait for response headers, not the messages of a stream
		let channel = Channel::from_shared(endpoint)?
			.connect_timeout(CONNECT_TIMEOUT)
			.timeout(RESPONSE_TIMEOUT)
			.http2_keep_alive_interval(KEEPALIVE_INTERVAL)
			.keep_alive_timeout(KEEPALIVE_TIMEOUT)
			.keep_alive_while_idle(true)
			.tcp_keepalive(Some(KEEPALIVE_INTERVAL))
			.connect()
			.await?;
		Ok(Self { client: McFollowerClient::new(channel) })
	}

	fn client(&self) -> McFollowerClient<Channel> {
		self.client.clone()
	}
}
