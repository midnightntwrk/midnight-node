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

//! Batch one or more SCALE-encoded calls through `pallet-batch` and dispatch the result via
//! federated-authority governance.
//!
//! Each `--encoded-call` is the full SCALE-encoded `RuntimeCall` (typically the output of
//! another toolkit subcommand's `--encode-only`). They are decoded against runtime metadata,
//! wrapped in `Batch::batch_all` (or `Batch::batch` with `--allow-partial-failure`), and
//! the resulting call is forwarded to `root_call::execute` for council + technical-committee
//! voting.

use clap::Args;
use subxt::{
	Metadata, OnlineClient, SubstrateConfig,
	dynamic::{self, Value},
	ext::scale_value::scale::decode_as_type,
};
use thiserror::Error;

use crate::cli_parsers as cli;
use crate::commands::root_call::{self, RootCallArgs};

#[derive(Error, Debug)]
pub enum BatchError {
	#[error("subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("online client error: {0}")]
	OnlineClientError(#[from] subxt::error::OnlineClientError),
	#[error("online client at block error: {0}")]
	OnlineClientAtBlockError(#[from] subxt::error::OnlineClientAtBlockError),
	#[error("extrinsic error: {0}")]
	ExtrinsicError(#[from] subxt::error::ExtrinsicError),
	#[error("at least one --encoded-call is required")]
	NoCalls,
	#[error("failed to decode call {index}: {error}")]
	CallDecodeError { index: usize, error: String },
	#[error("error executing root call: {0}")]
	RootCallError(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Args)]
pub struct BatchArgs {
	/// One or more SCALE-encoded `RuntimeCall`s as hex strings (with or without `0x` prefix).
	/// Pass the flag multiple times to batch multiple calls. Order is preserved.
	#[arg(long = "encoded-call", num_args = 1.., required = true, value_parser = cli::hex_bytes)]
	pub encoded_calls: Vec<Vec<u8>>,

	/// Council member private keys as hex strings (32-byte sr25519 seeds)
	#[arg(short, long, required = true)]
	pub council_members: Vec<String>,

	/// Technical Committee member private keys as hex strings (32-byte sr25519 seeds)
	#[arg(short, long, required = true)]
	pub technical_committee_members: Vec<String>,

	/// RPC URL of the node
	#[arg(short, long, default_value = "ws://localhost:9944", env)]
	pub rpc_url: String,

	/// Use `Batch::batch` (continues on individual failures) instead of `Batch::batch_all`
	/// (atomic — reverts the whole batch on any failure). Defaults to `batch_all`.
	#[arg(long)]
	pub allow_partial_failure: bool,
}

fn decode_call(bytes: &[u8], metadata: &Metadata, index: usize) -> Result<Value, BatchError> {
	let call_ty_id = metadata.outer_enums().call_enum_ty();
	decode_as_type(&mut &bytes[..], call_ty_id, metadata.types())
		.map(|v| v.remove_context())
		.map_err(|e| BatchError::CallDecodeError { index, error: format!("{:?}", e) })
}

pub async fn execute(args: BatchArgs) -> Result<(), BatchError> {
	if args.encoded_calls.is_empty() {
		return Err(BatchError::NoCalls);
	}

	log::info!(
		"Batching {} call(s) with Batch::{}",
		args.encoded_calls.len(),
		if args.allow_partial_failure { "batch" } else { "batch_all" }
	);

	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&args.rpc_url).await?;
	let at_block = api.at_current_block().await?;
	let metadata = at_block.metadata_ref();

	let call_values: Vec<Value> = args
		.encoded_calls
		.iter()
		.enumerate()
		.map(|(i, bytes)| decode_call(bytes, &metadata, i))
		.collect::<Result<Vec<_>, _>>()?;

	let batch_call_name = if args.allow_partial_failure { "batch" } else { "batch_all" };
	let batch_call =
		dynamic::tx("Batch", batch_call_name, vec![Value::unnamed_composite(call_values)]);
	let encoded_call = api.tx().await?.call_data(&batch_call)?;
	log::info!("Batched call ({} bytes): 0x{}", encoded_call.len(), hex::encode(&encoded_call));

	root_call::execute(RootCallArgs {
		rpc_url: args.rpc_url,
		council_keys: args.council_members,
		tc_keys: args.technical_committee_members,
		encoded_call: Some(encoded_call),
		encoded_call_file: None,
	})
	.await
	.map_err(BatchError::RootCallError)
}
