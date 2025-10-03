use crate::{
	DefaultDB, DerivationPath, IntoWalletAddress, NetworkId, Role, ShieldedWallet,
	UnshieldedWallet, WalletSeed,
};
use clap::Args;
use hex::ToHex;
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
	/// CoinPublic untagged only
	#[arg(long)]
	coin_public_tagged: bool,
	/// Unshielded User Address only (use for contract interations)
	#[arg(long)]
	unshielded_user_address_untagged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Addresses {
	shielded: String,
	unshielded: String,
	coin_public: String,
	coin_public_tagged: String,
	unshielded_user_address_untagged: String,
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
		coin_public: shielded_wallet.coin_public_key.0.0.encode_hex(),
		coin_public_tagged: serialize(&shielded_wallet.coin_public_key)
			.expect("failed to serialize CoinPublicKey")
			.encode_hex(),
		unshielded_user_address_untagged: unshielded_wallet.user_address.0.0.encode_hex(),
	};

	// https://github.com/clap-rs/clap/issues/2621
	if args.specific_address.shielded {
		ShowAddress::SingleAddress(all.shielded)
	} else if args.specific_address.unshielded {
		ShowAddress::SingleAddress(all.unshielded)
	} else if args.specific_address.coin_public {
		ShowAddress::SingleAddress(all.coin_public)
	} else if args.specific_address.coin_public_tagged {
		ShowAddress::SingleAddress(all.coin_public_tagged)
	} else if args.specific_address.unshielded_user_address_untagged {
		ShowAddress::SingleAddress(all.unshielded_user_address_untagged)
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
			ShowAddress::SingleAddress(a) if a == "aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98"
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
