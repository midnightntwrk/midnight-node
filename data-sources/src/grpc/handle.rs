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

use tonic::{Status, transport::Channel};

use crate::grpc::midnight_state::{
	AriadneParametersRequest, AriadneParametersResponse, BlockByHashRequest, BlockByHashResponse,
	CouncilDatumRequest, CouncilDatumResponse, EpochCandidatesRequest, EpochCandidatesResponse,
	EpochNonceRequest, EpochNonceResponse, LatestBlockRequest, LatestBlockResponse,
	LatestStableBlockRequest, LatestStableBlockResponse, StableBlockRequest, StableBlockResponse,
	TechnicalCommitteeDatumRequest, TechnicalCommitteeDatumResponse, UtxoEventsRequest,
	UtxoEventsResponse, midnight_state_client::MidnightStateClient,
};

/// Handle to the Midnight indexer: a remote gRPC channel, or — with the
/// `embedded` feature — the in-process query service invoked directly (a
/// plain async method call: no transport, no codec, no port).
#[derive(Clone)]
pub enum IndexerHandle {
	/// Remote indexer over gRPC.
	Grpc(MidnightStateClient<Channel>),
	/// In-process indexer, called through its service impl.
	#[cfg(feature = "embedded")]
	Direct(acropolis_module_midnight_state::grpc::service::MidnightStateService),
}

macro_rules! rpcs {
	($($name:ident: $req:ident => $resp:ident;)+) => {
		impl IndexerHandle {
			$(
				pub(crate) async fn $name(&self, request: $req) -> Result<$resp, Status> {
					match self {
						Self::Grpc(client) => {
							Ok(client.clone().$name(request).await?.into_inner())
						},
						#[cfg(feature = "embedded")]
						Self::Direct(service) => {
							use crate::grpc::midnight_state::midnight_state_server::MidnightState as _;
							Ok(service.$name(tonic::Request::new(request)).await?.into_inner())
						},
					}
				}
			)+
		}
	};
}

rpcs! {
	get_ariadne_parameters: AriadneParametersRequest => AriadneParametersResponse;
	get_block_by_hash: BlockByHashRequest => BlockByHashResponse;
	get_council_datum: CouncilDatumRequest => CouncilDatumResponse;
	get_epoch_candidates: EpochCandidatesRequest => EpochCandidatesResponse;
	get_epoch_nonce: EpochNonceRequest => EpochNonceResponse;
	get_latest_block: LatestBlockRequest => LatestBlockResponse;
	get_latest_stable_block: LatestStableBlockRequest => LatestStableBlockResponse;
	get_stable_block: StableBlockRequest => StableBlockResponse;
	get_technical_committee_datum: TechnicalCommitteeDatumRequest => TechnicalCommitteeDatumResponse;
	get_utxo_events: UtxoEventsRequest => UtxoEventsResponse;
}
