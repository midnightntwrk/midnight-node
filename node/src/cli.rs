// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

use std::str::FromStr;

use authority_selection_inherents::CommitteeMember;
use midnight_node_runtime::{CrossChainPublic, opaque::SessionKeys};
use parity_scale_codec::Encode;
use partner_chains_node_commands::{PartnerChainRuntime, PartnerChainsSubcommand};

#[derive(Debug, Clone, clap::Parser)]
pub struct RunMidnight {
	#[clap(flatten)]
	run: sc_cli::RunCmd,
}

#[derive(Debug, clap::Parser)]
/// Midnight blockchain node. Run without <COMMAND> to start the node.
/// To see full config options, run with no args with env-var SHOW_CONFIG=TRUE or run --help
#[command(version)]
pub struct Cli {
	#[command(subcommand)]
	pub subcommand: Subcommand,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Partner chain subcommands (smart contract registration etc.)
	#[clap(flatten)]
	PartnerChains(PartnerChainsSubcommand<MidnightRuntime, MidnightAddress>),

	/// Build a chain specification.
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Sub-commands concerned with benchmarking.
	#[cfg(feature = "runtime-benchmarks")]
	#[command(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}

#[derive(Clone, Debug)]
pub struct MidnightRuntime;
impl PartnerChainRuntime for MidnightRuntime {
	type AuthorityId = CrossChainPublic;
	type AuthorityKeys = SessionKeys;
	type CommitteeMember = CommitteeMember<Self::AuthorityId, Self::AuthorityKeys>;

	fn initial_member(id: Self::AuthorityId, keys: Self::AuthorityKeys) -> Self::CommitteeMember {
		Self::CommitteeMember::from((id, keys))
	}
}

// TODO: this is used for signing address associations. Which kind of midnight address do we want to associate with Cardano?
#[derive(Clone, Debug, serde::Serialize, Encode)]
pub struct MidnightAddress;

impl FromStr for MidnightAddress {
	type Err = NotImplementedError;

	fn from_str(_: &str) -> Result<Self, Self::Err> {
		Err(NotImplementedError)
	}
}

pub struct NotImplementedError;

// TODO: this is used to sign block producer metadata. Do we have a better type for that?
#[derive(serde::Deserialize, Encode)]
pub struct MidnightBlockProducerMetadata;
