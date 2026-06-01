use partner_chains_cli::{AURA, GRANDPA, KeyDefinition};
use partner_chains_demo_runtime::opaque::SessionKeys;
use partner_chains_node_commands::{PartnerChainRuntime, PartnerChainsSubcommand};
use sc_cli::RunCmd;

#[derive(Debug, clap::Parser)]
pub struct Cli {
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[clap(flatten)]
	pub run: RunCmd,
}

#[derive(Debug, Clone)]
pub struct WizardBindings;

impl PartnerChainRuntime for WizardBindings {
	type Keys = SessionKeys;

	fn create_chain_spec(
		config: &partner_chains_cli::CreateChainSpecConfig<SessionKeys>,
	) -> serde_json::Value {
		crate::chain_spec::pc_create_chain_spec(config)
	}

	fn key_definitions() -> Vec<KeyDefinition<'static>> {
		vec![AURA, GRANDPA]
	}
}

#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	#[clap(flatten)]
	PartnerChains(PartnerChainsSubcommand<WizardBindings>),

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

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}
