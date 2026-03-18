use crate::commands::{
	contract_address::{self, ContractAddressArgs},
	contract_state::{self, ContractStateArgs},
	dust_balance::{self, DustBalanceArgs, DustBalanceResult},
	fetch::{self, FetchArgs},
	generate_genesis::{self, GenerateGenesisArgs},
	generate_intent::{self, GenerateIntentArgs},
	generate_sample_intent::{self, GenerateSampleIntentArgs},
	generate_txs::{self, GenerateTxsArgs},
	random_address::{self, RandomAddressArgs},
	root_call::{self, RootCallArgs},
	runtime_upgrade::{self, RuntimeUpgradeArgs},
	send_intent::{self, SendIntentArgs},
	show_address::ShowAddress,
	show_address::{self, ShowAddressArgs},
	show_ledger_parameters::{self, ShowLedgerParametersArgs},
	show_seed::{self, ShowSeedArgs},
	show_token_type::{self, ShowTokenType, ShowTokenTypeArgs},
	show_transaction::{self, ShowTransactionArgs},
	show_viewing_key::{self, ShowViewingKeyArgs},
	show_wallet::{self, ShowWalletArgs, ShowWalletResult},
	update_ledger_parameters::{self, UpdateLedgerParametersArgs},
};
use crate::utils;
use clap::{Parser, Subcommand};
use midnight_node_ledger_helpers::find_dependency_version;

#[derive(Subcommand)]
pub enum Commands {
	/// Generate transactions against a genesis tx file or a live node network.
	///
	/// How you choose to generate transactions will determine in which order they may be sent. For
	/// context:
	///
	/// The ledger state is a merkle tree whose root changes after each transaction is
	/// processed. A valid transaction must be generated against either the current ledger state merkle
	/// tree root, or a past root. This means that if you generate a "tree" of transactions using a
	/// known root of a node e.g. the genesis state, executing any other transactions on the node that
	/// aren't included in your generated transaction tree will result in your generated transactions
	/// failing.
	GenerateTxs(GenerateTxsArgs),
	/// Generates the genesis transaction and state, outputting them to file in the current working
	/// directory. Genesis generation is seeded, so output is deterministic.
	GenerateGenesis(GenerateGenesisArgs),
	GenerateIntent(GenerateIntentArgs),
	/// Generate Intent Files
	GenerateSampleIntent(GenerateSampleIntentArgs),
	/// Sends a custom contract (serialized intent .mn files )
	SendIntent(SendIntentArgs),
	/// Show the state of a wallet using it's seed
	DustBalance(DustBalanceArgs),
	/// Show the state of a wallet using it's seed
	ShowWallet(ShowWalletArgs),
	/// Show the address of a wallet using it's seed
	ShowAddress(ShowAddressArgs),
	/// Show the ledger parameters
	ShowLedgerParameters(ShowLedgerParametersArgs),
	/// Show the seed of a wallet
	ShowSeed(ShowSeedArgs),
	/// Show the viewing key of a shielded wallet using its seed
	ShowViewingKey(ShowViewingKeyArgs),
	/// Show the token type for a contract address + domain sep pair
	ShowTokenType(ShowTokenTypeArgs),
	/// Show the deserialized value of a serialized transaction
	ShowTransaction(ShowTransactionArgs),
	/// Show and save in a file the Contract Address included in a DeployContract tx
	ContractAddress(ContractAddressArgs),
	/// Show and save a Contract state
	ContractState(ContractStateArgs),
	/// Generate a random `UserAddress` for a given `NetworkId`
	RandomAddress(RandomAddressArgs),
	/// Update the ledger parameters
	UpdateLedgerParameters(UpdateLedgerParametersArgs),
	/// Perform a runtime upgrade through federated governance
	RuntimeUpgrade(RuntimeUpgradeArgs),
	/// Execute a call through governance with Root origin
	///
	/// This command allows executing arbitrary runtime calls through the federated authority
	/// governance mechanism. It requires private keys from both Council and Technical Committee
	/// members to vote and approve the motion.
	RootCall(RootCallArgs),
	/// Get the version information
	Version,
	/// Fetch
	Fetch(FetchArgs),
}

/// Node Toolkit for Midnight
#[derive(Parser)]
#[command(about, long_about, verbatim_doc_comment)]
pub struct Cli {
	/// Enable verbose output (sets log level to debug)
	#[arg(long, short = 'v', conflicts_with = "quiet", global = true, env = "MN_VERBOSE")]
	pub verbose: bool,

	/// Enable verbose ledger tracing output (sets tracing level to debug)
	#[arg(long, conflicts_with = "quiet", global = true, env = "MN_VERBOSE_LEDGER")]
	pub verbose_ledger: bool,

	/// Enable verbose fetch logging (sets midnight_node_toolkit::fetcher to debug)
	#[arg(long, conflicts_with = "quiet", global = true, env = "MN_VERBOSE_FETCH")]
	pub verbose_fetch: bool,

	/// Suppress info-level logs (only show warnings and errors)
	#[arg(long, short = 'q', conflicts_with = "verbose", global = true, env = "MN_QUIET")]
	pub quiet: bool,

	/// Output logs in JSON format (for machine parsing)
	#[arg(long, global = true, env = "MN_LOG_JSON")]
	pub log_json: bool,

	/// Number of threads for parallel wallet updates during block replay.
	/// Defaults to number of CPU cores.
	#[arg(long, global = true, env = "MN_REPLAY_CONCURRENCY")]
	pub replay_concurrency: Option<usize>,

	#[command(subcommand)]
	pub command: Commands,
}

pub async fn run_command(cmd: Commands) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	match cmd {
		Commands::GenerateTxs(args) => {
			generate_txs::execute(args).await?;
			Ok(())
		},
		Commands::GenerateIntent(args) => {
			generate_intent::execute(args).await?;
			Ok(())
		},
		Commands::GenerateSampleIntent(args) => {
			generate_sample_intent::execute(args).await;
			Ok(())
		},
		Commands::SendIntent(args) => {
			send_intent::execute(args).await?;
			Ok(())
		},
		Commands::GenerateGenesis(args) => {
			let generator = generate_genesis::execute(args).await?;
			log::debug!("The tx: {:#?}", generator.txs);
			Ok(())
		},
		Commands::ShowWallet(args) => {
			let result = show_wallet::execute(args).await?;
			match result {
				ShowWalletResult::Debug(wallet_debug, utxos) => {
					println!("{}", wallet_debug);
					println!("Unshielded UTXOs: {:#?}", utxos)
				},
				ShowWalletResult::Json(json) => {
					println!("{}", serde_json::to_string_pretty(&json)?);
				},
				ShowWalletResult::DryRun(()) => (),
			}

			Ok(())
		},
		Commands::ShowAddress(args) => {
			let address = show_address::execute(args);
			match address {
				ShowAddress::Addresses(addresses) => {
					println!("{}", serde_json::to_string_pretty(&addresses)?);
				},
				ShowAddress::SingleAddress(address) => println!("{address}"),
			};

			Ok(())
		},
		Commands::ShowLedgerParameters(args) => {
			let result = show_ledger_parameters::execute(args.clone()).await?;
			if args.serialize {
				println!("{}", result.serialized);
			} else {
				println!("{:#?}", result);
			}
			Ok(())
		},
		Commands::UpdateLedgerParameters(args) => {
			update_ledger_parameters::execute(args).await?;
			Ok(())
		},
		Commands::ShowSeed(args) => {
			let seed = show_seed::execute(args);
			println!("{}", seed);
			Ok(())
		},
		Commands::ShowViewingKey(args) => {
			let viewing_key = show_viewing_key::execute(args);
			println!("{viewing_key}");
			Ok(())
		},
		Commands::ShowTransaction(args) => {
			let transaction_information = show_transaction::execute(args)?;

			println!("{transaction_information}");
			Ok(())
		},
		Commands::ContractAddress(args) => {
			let address = contract_address::execute(args)?;
			println!("{address}");
			Ok(())
		},
		Commands::ContractState(args) => contract_state::execute(args).await,
		Commands::RandomAddress(args) => {
			let address = random_address::execute(args);
			println!("{}", address);

			Ok(())
		},
		Commands::Version => {
			let node_version = utils::find_crate_version!("../../../node/Cargo.toml");
			let ledger_version =
				find_dependency_version("mn-ledger").expect("missing ledger version");
			let compactc_version = include_str!("../../../COMPACTC_VERSION").trim();

			println!(
				"Node: {}\nLedger: {}\nCompactc: {}",
				node_version, ledger_version, compactc_version
			);
			return Ok(());
		},
		Commands::ShowTokenType(args) => {
			let token_type = show_token_type::execute(args);
			match token_type {
				ShowTokenType::TokenTypes(token_types) => {
					println!("{}", serde_json::to_string_pretty(&token_types)?);
				},
				ShowTokenType::SingleTokenType(ttype) => println!("{ttype}"),
			};

			Ok(())
		},
		Commands::DustBalance(args) => {
			let result = dust_balance::execute(args).await?;
			match result {
				DustBalanceResult::Json(json) => {
					println!("{}", serde_json::to_string_pretty(&json)?);
				},
				DustBalanceResult::DryRun(()) => (),
			}

			Ok(())
		},
		Commands::RuntimeUpgrade(args) => {
			runtime_upgrade::execute(args).await?;
			Ok(())
		},
		Commands::RootCall(args) => {
			root_call::execute(args).await?;
			Ok(())
		},
		Commands::Fetch(args) => fetch::execute(args).await,
	}
}
