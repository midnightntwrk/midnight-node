use clap::Args;
use midnight_node_ledger_helpers::{ContractAddress, HashOutput};
use midnight_node_toolkit::cli_parsers::{self as cli};
use serde::Serialize;

#[derive(Args, Clone)]
pub struct ShowTokenTypeArgs {
	/// Address of contract
	#[arg(long, value_parser = cli::hex_ledger_tagged_decode::<ContractAddress>)]
	contract_address: ContractAddress,
	/// Pre-image of coin token type (Domain Separator)
	#[arg(long, value_parser = cli::hex_ledger_untagged_decode::<HashOutput>)]
	domain_sep: HashOutput,
	#[command(flatten)]
	specific_token_type: SpecificTokenTypeArgs,
}

#[derive(Args, Clone, Default)]
#[group(required = false, multiple = false)]
pub struct SpecificTokenTypeArgs {
	/// Shielded only
	#[arg(long)]
	shielded: bool,
	/// Unshielded only
	#[arg(long)]
	unshielded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTypes {
	shielded: String,
	unshielded: String,
}

#[derive(Debug)]
pub enum ShowTokenType {
	SingleTokenType(String),
	TokenTypes(TokenTypes),
}

pub fn execute(args: ShowTokenTypeArgs) -> ShowTokenType {
	let all = TokenTypes {
		shielded: hex::encode(
			args.contract_address.custom_shielded_token_type(args.domain_sep).0.0,
		),
		unshielded: hex::encode(
			args.contract_address.custom_unshielded_token_type(args.domain_sep).0.0,
		),
	};

	if args.specific_token_type.shielded {
		ShowTokenType::SingleTokenType(all.shielded)
	} else if args.specific_token_type.unshielded {
		ShowTokenType::SingleTokenType(all.unshielded)
	} else {
		ShowTokenType::TokenTypes(all)
	}
}

#[cfg(test)]
mod test {
	use midnight_node_ledger_helpers::Deserializable;

	use super::*;

	#[test]
	fn test_shielded() {
		let mut specific_address = SpecificTokenTypeArgs::default();
		specific_address.unshielded = true;

		let args: ShowTokenTypeArgs = ShowTokenTypeArgs {
			contract_address: <ContractAddress as Deserializable>::deserialize(
				&mut &hex_literal::hex!(
					"55cd5312a57f19b67648e07b52bbd4a4c2542489f30c188a4bd17d5993ce1ad9"
				)[..],
				0,
			)
			.unwrap(),
			domain_sep: HashOutput(hex_literal::hex!(
				"0000000000000000000000000000000000000000000000000000000000000001"
			)),
			specific_token_type: specific_address,
		};

		let address = super::execute(args);

		assert!(matches!(
			address,
			ShowTokenType::SingleTokenType(a) if a == "85e83e67740f35ae8abac0f11eaef02cd0a9b7adc73eb880d499c57ad67ee55c"
		));
	}

	#[test]
	fn test_all() {
		let args: ShowTokenTypeArgs = ShowTokenTypeArgs {
			contract_address: <ContractAddress as Deserializable>::deserialize(
				&mut &hex_literal::hex!(
					"55cd5312a57f19b67648e07b52bbd4a4c2542489f30c188a4bd17d5993ce1ad9"
				)[..],
				0,
			)
			.unwrap(),
			domain_sep: HashOutput(hex_literal::hex!(
				"0000000000000000000000000000000000000000000000000000000000000001"
			)),
			specific_token_type: Default::default(),
		};

		let address = super::execute(args);
		assert!(matches!(address, ShowTokenType::TokenTypes(_)));
	}
}
