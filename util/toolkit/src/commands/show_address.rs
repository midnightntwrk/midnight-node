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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Addresses {
	shielded: String,
	unshielded: String,
	coin_public: String,
}

pub fn execute(args: ShowAddressArgs) -> Addresses {
	let shielded_derivation_path = DerivationPath::default_for_role(Role::Zswap);
	let shielded_wallet =
		ShieldedWallet::<DefaultDB>::from_path(args.seed, &shielded_derivation_path);

	let unshielded_derivation_path = DerivationPath::default_for_role(Role::UnshieldedExternal);
	let unshielded_wallet = UnshieldedWallet::from_path(args.seed, &unshielded_derivation_path);

	Addresses {
		shielded: shielded_wallet.address(args.network).to_bech32(),
		unshielded: unshielded_wallet.address(args.network).to_bech32(),
		coin_public: hex::encode(
			serialize(&shielded_wallet.coin_public_key).expect("failed to serialize CoinPublicKey"),
		),
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_shielded_address() {
		let args: ShowAddressArgs = ShowAddressArgs {
			network: NetworkId::TestNet,
			seed: WalletSeed::from(
				"0000000000000000000000000000000000000000000000000000000000000001",
			),
		};

		let address = super::execute(args);
		assert_eq!(
			address.shielded,
			"mn_shield-addr_test14gxh9wmhafr0np4gqrrx6awyus52jk7huyjy78kstym5ucnxawvqxq9k9e3s5qcpwx67zxhjfplszqlx2rx8q0egf59y0ze2827lju2mwqpnq6kr"
		);
	}

	#[test]
	fn test_coin_public() {
		let args: ShowAddressArgs = ShowAddressArgs {
			network: NetworkId::TestNet,
			seed: WalletSeed::from(
				"0000000000000000000000000000000000000000000000000000000000000001",
			),
		};

		let address = super::execute(args);
		assert_eq!(
			address.coin_public,
			"6d69646e696768743a7a737761702d636f696e2d7075626c69632d6b65795b76315d3aaa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98"
		);
	}
}
