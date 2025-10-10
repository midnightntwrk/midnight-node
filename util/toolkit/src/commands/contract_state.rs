use clap::Args;
use midnight_node_ledger_helpers::{ContractAddress, LedgerContext, serialize};
use midnight_node_toolkit::{
	ProofType, SignatureType, cli_parsers as cli,
	tx_generator::{TxGenerator, source::Source},
};
use std::{fs, path::Path};

#[derive(Args)]
pub struct ContractStateArgs {
	#[command(flatten)]
	source: Source,
	/// Contract Address
	#[arg(long, value_parser = cli::contract_address_decode)]
	contract_address: ContractAddress,
	/// Destination file to save the state
	#[arg(long, short)]
	dest_file: String,
	/// Dry-run - don't fetch anything, just print out the settings
	#[arg(long)]
	dry_run: bool,
}

pub async fn execute(
	args: ContractStateArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let source = TxGenerator::<SignatureType, ProofType>::source(args.source, args.dry_run)
		.await
		.expect("failed to init tx source");

	if args.dry_run {
		println!("Dry-run: fetch contract state for address: {:?}", args.contract_address);
		println!("Dry-run: write contract state to file: {:?}", args.dest_file);
		return Ok(());
	}

	let blocks = source.get_txs().await?;
	let network_id = blocks.network();

	let context = LedgerContext::new(network_id);
	for block in blocks.blocks {
		context.update_from_block(block.transactions, block.context, block.state_root.clone());
	}

	let state = context
		.with_ledger_state(|ledger_state| ledger_state.index(args.contract_address))
		.expect("contract state for address does not exist");

	let serialized_state = serialize(&state)?;

	let full_path = Path::new(&args.dest_file);
	if let Some(directory) = full_path.parent() {
		fs::create_dir_all(directory).expect("failed to create directories");
	}

	fs::write(full_path, serialized_state).expect("failed to create file");

	Ok(())
}

#[cfg(test)]
mod test {
	// TODO
}
