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

//! Row source reading Cardano data from an mc-follower indexer over gRPC
//!
//! mc-follower indexes the Cardano data a Midnight node reads and serves it with one RPC per
//! db-sync query of the data sources. [McFollowerRowSource] implements every row source trait
//! of the main chain follower data sources against that API, returning the same rows as the
//! db-sync implementations, so the data sources behave the same on either backend.
//!
//! `proto/mc_follower.proto` is a copy of `proto/mc_follower.proto` of the mc-follower
//! repository, which is the canonical version of the contract.

mod datum;
mod row_source;
#[cfg(test)]
mod tests;

pub use datum::{
	DatumError, c2m_metadata_like_db_sync, plutus_data_if_valid, plutus_data_like_db_sync,
};

/// Types and client generated from `proto/mc_follower.proto`
pub mod proto {
	#![allow(missing_docs, clippy::all)]
	tonic::include_proto!("mcfollower.v1");
}

use proto::mc_follower_client::McFollowerClient;
use tonic::{Code, Status, transport::Channel};

/// Error of a request to mc-follower
///
/// None of these errors means that the requested data does not exist: mc-follower reports
/// absent data in its responses, so every error fails the data source call that made the
/// request, as a failed db-sync query does.
#[derive(Debug, thiserror::Error)]
pub enum McFollowerError {
	/// mc-follower cannot answer now: it is not reachable, has not synced the requested data
	/// yet, or is backfilling a requested address or policy
	#[error("mc-follower is unavailable or not synced to the requested data: {0}")]
	Unavailable(Box<Status>),
	/// mc-follower is not configured to index a requested address or policy
	#[error("mc-follower does not index the requested address or policy: {0}")]
	NotIndexed(Box<Status>),
	/// mc-follower failed the request
	#[error("mc-follower request failed: {0}")]
	Request(Box<Status>),
	/// A response carries a value that does not fit the row it is read into
	#[error("invalid mc-follower response: {0}")]
	InvalidResponse(String),
	/// A datum that must decode, does not
	#[error("undecodable datum in mc-follower response: {0}")]
	UndecodableDatum(#[from] DatumError),
}

impl From<Status> for McFollowerError {
	fn from(status: Status) -> Self {
		match status.code() {
			Code::Unavailable => Self::Unavailable(Box::new(status)),
			Code::FailedPrecondition => Self::NotIndexed(Box::new(status)),
			_ => Self::Request(Box::new(status)),
		}
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
		Ok(Self::new(Channel::from_shared(endpoint)?.connect().await?))
	}

	/// Creates a row source sending its requests over `channel`
	pub fn new(channel: Channel) -> Self {
		Self { client: McFollowerClient::new(channel) }
	}

	fn client(&self) -> McFollowerClient<Channel> {
		self.client.clone()
	}
}
