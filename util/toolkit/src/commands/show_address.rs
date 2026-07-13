use crate::cli_parsers::{self as cli};
use crate::{DefaultDB, IntoWalletAddress, ShieldedWallet, UnshieldedWallet};
use clap::Args;
use hex::ToHex;
use midnight_node_ledger_helpers::{DustWallet, serialize, serialize_untagged};
use serde::Serialize;

#[derive(Args, Clone)]
pub struct ShowAddressArgs {
	/// Target network
	#[arg(long)]
	pub network: String,
	/// Wallet seed. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA identity
	/// (ledger 9+), e.g. `--seed ecdsa:<seed>`. The scheme only affects the
	/// unshielded/verifying-key/user-address outputs; shielded, dust and coin keys are
	/// scheme-independent.
	#[arg(long, value_parser = cli::scheme_seed_decode)]
	pub seed: cli::SchemeSeed,
	#[command(flatten)]
	pub specific_address: SpecificAddressTypeArgs,
}

#[derive(Args, Clone, Default)]
#[group(required = false, multiple = false)]
pub struct SpecificAddressTypeArgs {
	/// Shielded only
	#[arg(long)]
	pub shielded: bool,
	/// Unshielded only
	#[arg(long)]
	pub unshielded: bool,
	/// Dust only
	#[arg(long)]
	pub dust: bool,
	/// DustPublic only
	#[arg(long)]
	pub dust_public: bool,
	/// CoinPublic only
	#[arg(long)]
	pub coin_public: bool,
	/// CoinPublic tagged only
	#[arg(long)]
	pub coin_public_tagged: bool,
	/// Verifying key only
	#[arg(long)]
	pub verifying_key: bool,
	/// User Address only
	#[arg(long, conflicts_with = "unshielded_user_address_untagged")]
	pub user_address: bool,
	/// User Address only (deprecated, use --user-address)
	#[arg(long, conflicts_with = "user_address")]
	pub unshielded_user_address_untagged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Addresses {
	shielded: String,
	unshielded: String,
	dust: String,
	dust_public: String,
	coin_public: String,
	coin_public_tagged: String,
	verifying_key: String,
	user_address: String,
	unshielded_user_address_untagged: String,
}

#[derive(Debug)]
pub enum ShowAddress {
	SingleAddress(String),
	Addresses(Addresses),
}

pub fn execute(args: ShowAddressArgs) -> ShowAddress {
	let (seed, scheme) = args.seed.resolve();
	let shielded_wallet = ShieldedWallet::<DefaultDB>::default(seed.clone());
	// The unshielded identity is scheme-specific; the other sub-wallets derive from the seed
	// alone and are unchanged between the Schnorr and ECDSA schemes.
	let unshielded_wallet = UnshieldedWallet::new(seed.clone(), scheme);
	let dust_wallet = DustWallet::<DefaultDB>::default(seed.clone(), None);

	let all = Addresses {
		shielded: shielded_wallet.address(&args.network).to_bech32(),
		unshielded: unshielded_wallet.address(&args.network).to_bech32(),
		dust: dust_wallet.address(&args.network).to_bech32(),
		dust_public: serialize_untagged(&dust_wallet.public_key).unwrap().encode_hex(),
		coin_public: shielded_wallet.coin_public_key.0.0.encode_hex(),
		coin_public_tagged: serialize(&shielded_wallet.coin_public_key)
			.expect("failed to serialize CoinPublicKey")
			.encode_hex(),
		verifying_key: serialize_untagged(&unshielded_wallet.verifying_key())
			.expect("failed to serialize VerifyingKey")
			.encode_hex(),
		user_address: unshielded_wallet.user_address.0.0.encode_hex(),
		unshielded_user_address_untagged: unshielded_wallet.user_address.0.0.encode_hex(),
	};
	if args.specific_address.unshielded_user_address_untagged {
		log::warn!("--unshielded-user-address-untagged is deprecated. Use --user-address instead");
	}

	// https://github.com/clap-rs/clap/issues/2621
	if args.specific_address.shielded {
		ShowAddress::SingleAddress(all.shielded)
	} else if args.specific_address.unshielded {
		ShowAddress::SingleAddress(all.unshielded)
	} else if args.specific_address.dust {
		ShowAddress::SingleAddress(all.dust)
	} else if args.specific_address.dust_public {
		ShowAddress::SingleAddress(all.dust_public)
	} else if args.specific_address.coin_public {
		ShowAddress::SingleAddress(all.coin_public)
	} else if args.specific_address.coin_public_tagged {
		ShowAddress::SingleAddress(all.coin_public_tagged)
	} else if args.specific_address.verifying_key {
		ShowAddress::SingleAddress(all.verifying_key)
	} else if args.specific_address.unshielded_user_address_untagged
		|| args.specific_address.user_address
	{
		ShowAddress::SingleAddress(all.unshielded_user_address_untagged)
	} else {
		ShowAddress::Addresses(all)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::WalletSeed;

	#[test]
	fn test_shielded_address() {
		let mut specific_address = SpecificAddressTypeArgs::default();
		specific_address.shielded = true;

		let args: ShowAddressArgs = ShowAddressArgs {
			network: "testnet".to_string(),
			seed: cli::SchemeSeed {
				seed: WalletSeed::try_from_hex_str(
					"0000000000000000000000000000000000000000000000000000000000000001",
				)
				.unwrap(),
				scheme: midnight_node_ledger_helpers::UnshieldedSignatureScheme::Schnorr,
			},
			specific_address,
		};

		let address = super::execute(args);

		assert!(matches!(
			address,
			ShowAddress::SingleAddress(a) if a == "mn_shield-addr_testnet1r020sfa7jllsz0z2wqhykz8npmphsu5223nsea7vjt9ekxs5almtvtnrpgpszud4uyd0yjrlqyp7v5xvwqljsng2g79j5w4al9c4kuqmrxx6k"
		));
	}

	#[test]
	fn test_coin_public() {
		let mut specific_address = SpecificAddressTypeArgs::default();
		specific_address.coin_public = true;

		let args: ShowAddressArgs = ShowAddressArgs {
			network: "testnet".to_string(),
			seed: cli::SchemeSeed {
				seed: WalletSeed::try_from_hex_str(
					"0000000000000000000000000000000000000000000000000000000000000001",
				)
				.unwrap(),
				scheme: midnight_node_ledger_helpers::UnshieldedSignatureScheme::Schnorr,
			},
			specific_address,
		};

		let address = super::execute(args);
		assert!(matches!(
			address,
			ShowAddress::SingleAddress(a) if a == "1bd4f827be97ff013c4a702e4b08f30ec378728a54670cf7cc92cb9b1a14eff6"
		));
	}

	#[test]
	fn test_all() {
		let args: ShowAddressArgs = ShowAddressArgs {
			network: "testnet".to_string(),
			seed: cli::SchemeSeed {
				seed: WalletSeed::try_from_hex_str(
					"0000000000000000000000000000000000000000000000000000000000000001",
				)
				.unwrap(),
				scheme: midnight_node_ledger_helpers::UnshieldedSignatureScheme::Schnorr,
			},
			specific_address: Default::default(),
		};

		let address = super::execute(args);
		assert!(matches!(address, ShowAddress::Addresses(_)));
	}

	#[test]
	fn schnorr_and_ecdsa_yield_distinct_unshielded_identities() {
		let hex = "0000000000000000000000000000000000000000000000000000000000000001";
		let unshielded_for = |ecdsa: bool| {
			let seed = WalletSeed::try_from_hex_str(hex).unwrap();
			let scheme = if ecdsa {
				midnight_node_ledger_helpers::UnshieldedSignatureScheme::Ecdsa
			} else {
				midnight_node_ledger_helpers::UnshieldedSignatureScheme::Schnorr
			};
			let seed = cli::SchemeSeed { seed, scheme };
			match super::execute(ShowAddressArgs {
				network: "testnet".to_string(),
				seed,
				specific_address: SpecificAddressTypeArgs {
					unshielded: true,
					..Default::default()
				},
			}) {
				ShowAddress::SingleAddress(addr) => addr,
				ShowAddress::Addresses(_) => panic!("expected a single unshielded address"),
			}
		};

		let schnorr = unshielded_for(false);
		let ecdsa = unshielded_for(true);

		// Same seed, different scheme → different NIGHT identity, hence a different address.
		assert_ne!(schnorr, ecdsa, "Schnorr and ECDSA must give distinct unshielded addresses");
		assert!(schnorr.starts_with("mn_addr"), "unexpected schnorr unshielded hrp: {schnorr}");
		assert!(ecdsa.starts_with("mn_addr"), "unexpected ecdsa unshielded hrp: {ecdsa}");
	}
}
