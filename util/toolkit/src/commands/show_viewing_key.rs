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
        "0000000000000000000000000000000000000000000000000000000000000000",
        "mn_shield-esk_undeployed1qvqzq0gvnuf3u4grq4whnk47edln6clqun4kl36xquptc8lnkgnh2ycztpx2f3";
        "test undemployed with all 0...00 seed"
    )]
	#[test_case(
        NetworkId::DevNet,
        "0000000000000000000000000000000000000000000000000000000000000001",
        "mn_shield-esk_dev1qvqpljf0wrewfdr5k6scfmqtertc4gvu8s2nhkpg8yrmx6n6v4t0evgc05kh2";
        "test devnet with all 0...01 seed"
    )]
	#[test_case(
        NetworkId::TestNet,
        "0000000000000000000000000000000000000000000000000000000000000001",
        "mn_shield-esk_test1qvqpljf0wrewfdr5k6scfmqtertc4gvu8s2nhkpg8yrmx6n6v4t0evgwk3tj3";
        "test testnet with all 0...01 seed"
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
