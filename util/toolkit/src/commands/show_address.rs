use crate::{
	DefaultDB, DerivationPath, IntoWalletAddress, NetworkId, Role, ShieldedWallet,
	UnshieldedWallet, WalletSeed,
};
use clap::Args;
use midnight_node_ledger_helpers::serialize;
use midnight_node_toolkit::cli_parsers::{self as cli};
use serde::Serialize;

#[derive(Args, Clone)]
pub struct ShowAddressArgs {
	/// Target network
	#[arg(long, value_parser = cli::network_id_decode)]
	network: NetworkId,
	/// Wallet seed
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	seed: WalletSeed,
	#[command(flatten)]
	specific_address: SpecificAddressTypeArgs,
}

#[derive(Args, Clone, Default)]
#[group(required = false, multiple = false)]
pub struct SpecificAddressTypeArgs {
	/// Shielded only
	#[arg(long)]
	shielded: bool,
	/// Unshielded only
	#[arg(long)]
	unshielded: bool,
	/// CoinPublic only
	#[arg(long)]
	coin_public: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Addresses {
	shielded: String,
	unshielded: String,
	coin_public: String,
}

#[derive(Debug)]
pub enum ShowAddress {
	SingleAddress(String),
	Addresses(Addresses),
}

pub fn execute(args: ShowAddressArgs) -> ShowAddress {
	let shielded_derivation_path = DerivationPath::default_for_role(Role::Zswap);
	let shielded_wallet =
		ShieldedWallet::<DefaultDB>::from_path(args.seed, &shielded_derivation_path);

	let unshielded_derivation_path = DerivationPath::default_for_role(Role::UnshieldedExternal);
	let unshielded_wallet = UnshieldedWallet::from_path(args.seed, &unshielded_derivation_path);

	let all = Addresses {
		shielded: shielded_wallet.address(args.network).to_bech32(),
		unshielded: unshielded_wallet.address(args.network).to_bech32(),
		coin_public: hex::encode(
			serialize(&shielded_wallet.coin_public_key).expect("failed to serialize CoinPublicKey"),
		),
	};

	if args.specific_address.shielded {
		ShowAddress::SingleAddress(all.shielded)
	} else if args.specific_address.unshielded {
		ShowAddress::SingleAddress(all.unshielded)
	} else if args.specific_address.coin_public {
		ShowAddress::SingleAddress(all.coin_public)
	} else {
		ShowAddress::Addresses(all)
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_shielded_address() {
		let mut specific_address = SpecificAddressTypeArgs::default();
		specific_address.shielded = true;

		let args: ShowAddressArgs = ShowAddressArgs {
			network: NetworkId::TestNet,
			seed: WalletSeed::try_from_hex_str(
				"0000000000000000000000000000000000000000000000000000000000000001",
			)
			.unwrap(),
			specific_address,
		};

		let address = super::execute(args);

		assert!(matches!(
			address,
			ShowAddress::SingleAddress(a) if a == "mn_shield-addr_test14gxh9wmhafr0np4gqrrx6awyus52jk7huyjy78kstym5ucnxawvqxq9k9e3s5qcpwx67zxhjfplszqlx2rx8q0egf59y0ze2827lju2mwqpnq6kr"
		));
	}

	#[test]
	fn test_coin_public() {
		let mut specific_address = SpecificAddressTypeArgs::default();
		specific_address.coin_public = true;

		let args: ShowAddressArgs = ShowAddressArgs {
			network: NetworkId::TestNet,
			seed: WalletSeed::try_from_hex_str(
				"0000000000000000000000000000000000000000000000000000000000000001",
			)
			.unwrap(),
			specific_address,
		};

		let address = super::execute(args);
		assert!(matches!(
			address,
			ShowAddress::SingleAddress(a) if a == "6d69646e696768743a7a737761702d636f696e2d7075626c69632d6b65795b76315d3aaa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98"
		));
	}

	#[test]
	fn test_all() {
		let args: ShowAddressArgs = ShowAddressArgs {
			network: NetworkId::TestNet,
			seed: WalletSeed::try_from_hex_str(
				"0000000000000000000000000000000000000000000000000000000000000001",
			)
			.unwrap(),
			specific_address: Default::default(),
		};

		let address = super::execute(args);
		assert!(matches!(address, ShowAddress::Addresses(_)));
	}
}
