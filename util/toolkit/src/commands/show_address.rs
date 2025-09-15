use crate::{
	DefaultDB, DerivationPath, IntoWalletAddress, NetworkId, Role, ShieldedWallet,
	UnshieldedWallet, WalletAddress, WalletSeed,
};
use clap::Args;
use midnight_node_toolkit::cli_parsers::{self as cli};

#[derive(Args, Clone)]
pub struct ShowAddressArgs {
	/// Target network
	#[arg(long, value_parser = cli::network_id_decode)]
	network: NetworkId,
	/// Wallet seed
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	seed: WalletSeed,
	/// Enable shielded mode
	#[arg(long, conflicts_with = "unshielded", default_value_t = true)]
	shielded: bool,
	/// Disable shielded mode
	#[arg(long, conflicts_with = "shielded", default_value_t = false)]
	unshielded: bool,
	/// HD structure derivation path. If present, it overrides the `shielded` value.
	#[arg(long, short)]
	path: Option<String>,
}

pub fn execute(args: ShowAddressArgs) -> WalletAddress {
	let is_shielded = args.shielded && !args.unshielded;
	let derivation_path = if let Some(path) = args.path {
		DerivationPath::new(path)
	} else {
		if is_shielded {
			DerivationPath::default_for_role(Role::Zswap)
		} else {
			DerivationPath::default_for_role(Role::UnshieldedExternal)
		}
	};

	match derivation_path.role {
		Role::UnshieldedExternal => {
			UnshieldedWallet::from_path(args.seed, &derivation_path).address(args.network)
		},
		Role::Zswap => ShieldedWallet::<DefaultDB>::from_path(args.seed, &derivation_path)
			.address(args.network),
		_ => unimplemented!(),
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
			shielded: true,
			unshielded: false,
			path: None,
		};

		let address = super::execute(args);
		assert_eq!(
			address.to_bech32(),
			"mn_shield-addr_test14gxh9wmhafr0np4gqrrx6awyus52jk7huyjy78kstym5ucnxawvqxq9k9e3s5qcpwx67zxhjfplszqlx2rx8q0egf59y0ze2827lju2mwqpnq6kr"
		);
	}

	#[test]
	fn test_unshielded_address() {
		let args: ShowAddressArgs = ShowAddressArgs {
			network: NetworkId::TestNet,
			seed: WalletSeed::from(
				"0000000000000000000000000000000000000000000000000000000000000001",
			),
			shielded: false,
			unshielded: true,
			path: None,
		};

		let address = super::execute(args);
		assert_eq!(
			address.to_bech32(),
			"mn_addr_test1h3ssm5ru2t6eqy4g3she78zlxn96e36ms6pq996aduvmateh9p9s8yz0jz"
		);
	}

	#[test]
	fn test_prefer_path_over_shielded() {
		let args: ShowAddressArgs = ShowAddressArgs {
			network: NetworkId::TestNet,
			seed: WalletSeed::from(
				"0000000000000000000000000000000000000000000000000000000000000001",
			),
			shielded: true,
			unshielded: false,
			path: Some("m/44'/2400'/0'/0/0".to_string()),
		};

		let address = super::execute(args);
		assert_eq!(
			address.to_bech32(),
			"mn_addr_test1h3ssm5ru2t6eqy4g3she78zlxn96e36ms6pq996aduvmateh9p9s8yz0jz"
		);
	}

	#[test]
	fn test_prefer_path_over_unshielded() {
		let args: ShowAddressArgs = ShowAddressArgs {
			network: NetworkId::TestNet,
			seed: WalletSeed::from(
				"0000000000000000000000000000000000000000000000000000000000000001",
			),
			shielded: false,
			unshielded: true,
			path: Some("m/44'/2400'/0'/3/0".to_string()),
		};

		let address = super::execute(args);
		assert_eq!(
			address.to_bech32(),
			"mn_shield-addr_test14gxh9wmhafr0np4gqrrx6awyus52jk7huyjy78kstym5ucnxawvqxq9k9e3s5qcpwx67zxhjfplszqlx2rx8q0egf59y0ze2827lju2mwqpnq6kr"
		);
	}
}
