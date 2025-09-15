use clap::Args;
use midnight_node_ledger_helpers::{
	DefaultDB, DerivationPath, NetworkId, Role, ShieldedWallet, WalletSeed,
};
use midnight_node_toolkit::cli_parsers as cli;
#[derive(Args)]
pub struct ShowViewingKeyArgs {
	/// Target network
	#[arg(long, value_parser = cli::network_id_decode)]
	network: NetworkId,

	/// Wallet seed
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	seed: WalletSeed,
}

pub fn execute(args: ShowViewingKeyArgs) -> String {
	let derivation_path = DerivationPath::default_for_role(Role::Zswap);

	ShieldedWallet::<DefaultDB>::from_path(args.seed, &derivation_path).viewing_key(args.network)
}

#[cfg(test)]
mod test {
	use super::{NetworkId, ShowViewingKeyArgs, cli::wallet_seed_decode, execute};
	use test_case::test_case;

	#[test_case(
        NetworkId::Undeployed,
        "0000000000000000000000000000000000000000000000000000000000000001",
        "mn_shield-esk_undeployed1d45kgmnfva58gwn9de3hy7tsw35k7m3dwdjkxun9wskkketetdmrzhf6dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcsmxqc0u";
        "test undeployed with all 0...01 seed"
    )]
	#[test_case(
        NetworkId::DevNet,
        "0000000000000000000000000000000000000000000000000000000000000002",
        "mn_shield-esk_dev1d45kgmnfva58gwn9de3hy7tsw35k7m3dwdjkxun9wskkketetdmrzhf6w0dctw9zhe2ffqw4s5qks7rnl29wy5mhl957fv9nnhtxulent80q55zd0xr";
        "test devnet with all 0...02 seed"
    )]
	#[test_case(
        NetworkId::TestNet,
        "0000000000000000000000000000000000000000000000000000000000000003",
        "mn_shield-esk_test1d45kgmnfva58gwn9de3hy7tsw35k7m3dwdjkxun9wskkketetdmrzhf6wvd5v04ykt59gglxknsdxpwwkhhhj8d6h3ghpkgdhdsszap2p53qk8ln8nz";
        "test testnet with all 0...03 seed"
    )]
	fn test_show_viewing_key(network: NetworkId, seed: &str, viewing_key: &str) {
		let args = ShowViewingKeyArgs {
			network,
			seed: wallet_seed_decode(seed).expect("should return wallet seet"),
		};

		let actual_vk = execute(args);
		assert_eq!(viewing_key, actual_vk);
	}
}
