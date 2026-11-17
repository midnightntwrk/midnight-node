//! Validation of Cardano Shelley addresses held as UTF-8 Bech32 text (CIP-5, CIP-19).
//!
//! The runtime stores Cardano addresses as raw bytes and the node later reads them back as
//! query strings, so a value that is not a well-formed address must be rejected before it
//! reaches storage.

use bech32::{Bech32, primitives::decode::CheckedHrpstring};

const MAINNET_HRP: &str = "addr";
const TESTNET_HRP: &str = "addr_test";

/// CIP-19 header type of an enterprise address whose payment credential is a script hash.
const ENTERPRISE_SCRIPT_TYPE: u8 = 0b0111;
const MAINNET_NETWORK_ID: u8 = 1;
const TESTNET_NETWORK_ID: u8 = 0;
/// Header byte followed by a 28-byte script hash.
const ENTERPRISE_ADDRESS_LEN: usize = 1 + 28;

/// Cardano network an address belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardanoNetwork {
	/// Network id 1, `addr` prefix
	Mainnet,
	/// Network id 0, `addr_test` prefix
	Testnet,
}

/// Reason a byte string was rejected as a Cardano address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum CardanoAddressError {
	/// The bytes are not valid UTF-8
	#[cfg_attr(feature = "std", error("address is not valid UTF-8"))]
	NotUtf8,
	/// The text is not lowercase Bech32 with a valid checksum and padding
	#[cfg_attr(feature = "std", error("address is not valid lowercase Bech32"))]
	InvalidBech32,
	/// The human-readable prefix is neither `addr` nor `addr_test`
	#[cfg_attr(feature = "std", error("address prefix is neither `addr` nor `addr_test`"))]
	UnsupportedPrefix,
	/// The network id in the header does not match the human-readable prefix
	#[cfg_attr(feature = "std", error("address network id does not match its prefix"))]
	NetworkMismatch,
	/// The address is not an enterprise address with a script payment credential
	#[cfg_attr(feature = "std", error("address is not an enterprise script address"))]
	NotEnterpriseScript,
}

/// Checks that `bytes` are the UTF-8 Bech32 text of a Shelley enterprise address with a script
/// payment credential (CIP-19 header type 7), and returns its network.
pub fn validate_enterprise_script_address(
	bytes: &[u8],
) -> Result<CardanoNetwork, CardanoAddressError> {
	let text = core::str::from_utf8(bytes).map_err(|_| CardanoAddressError::NotUtf8)?;
	// Bech32 also accepts all-uppercase text, but the node matches addresses against
	// lowercase db-sync rows.
	if text.bytes().any(|b| b.is_ascii_uppercase()) {
		return Err(CardanoAddressError::InvalidBech32);
	}
	let checked =
		CheckedHrpstring::new::<Bech32>(text).map_err(|_| CardanoAddressError::InvalidBech32)?;
	checked
		.validate_segwit_padding()
		.map_err(|_| CardanoAddressError::InvalidBech32)?;

	let network = match checked.hrp().as_str() {
		MAINNET_HRP => CardanoNetwork::Mainnet,
		TESTNET_HRP => CardanoNetwork::Testnet,
		_ => return Err(CardanoAddressError::UnsupportedPrefix),
	};

	let mut payload = checked.byte_iter();
	let header = payload.next().ok_or(CardanoAddressError::NotEnterpriseScript)?;
	if header >> 4 != ENTERPRISE_SCRIPT_TYPE || 1 + payload.len() != ENTERPRISE_ADDRESS_LEN {
		return Err(CardanoAddressError::NotEnterpriseScript);
	}
	let expected_network_id = match network {
		CardanoNetwork::Mainnet => MAINNET_NETWORK_ID,
		CardanoNetwork::Testnet => TESTNET_NETWORK_ID,
	};
	if header & 0x0F != expected_network_id {
		return Err(CardanoAddressError::NetworkMismatch);
	}

	Ok(network)
}

#[cfg(test)]
mod tests {
	use super::*;
	use bech32::{Hrp, encode};

	fn encode_address(hrp: &str, payload: &[u8]) -> alloc::string::String {
		encode::<Bech32>(Hrp::parse(hrp).unwrap(), payload).unwrap()
	}

	#[test]
	fn accepts_enterprise_script_addresses() {
		assert_eq!(
			validate_enterprise_script_address(
				b"addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng"
			),
			Ok(CardanoNetwork::Testnet)
		);
		assert_eq!(
			validate_enterprise_script_address(
				b"addr1w9e7ft4rrdd4rkdseguxr9hudfxyytm5ckh2qy0yhz7lfeg9lvhq7"
			),
			Ok(CardanoNetwork::Mainnet)
		);
	}

	// Raw storage bytes of the council, technical committee and mapping validator addresses,
	// read from mainnet block #3281949 and preview block #1565068.
	#[test]
	fn accepts_addresses_stored_on_live_networks() {
		let cases = [
			(
				"6164647231777867336d6d333433366635377234723974366371647678653068776a757361797a34656438756c6d6c656e7474716a3632756c32",
				CardanoNetwork::Mainnet,
			),
			(
				"61646472317738756d6c6773773663666b7870646b326a656b7a776137726a64783774633933376d70616868796e3030343330733037346b3879",
				CardanoNetwork::Mainnet,
			),
			(
				"6164647231773965376674347272646434726b647365677578723968756466787979746d35636b683271793079687a376c666567396c76687137",
				CardanoNetwork::Mainnet,
			),
			(
				"616464725f7465737431777a7934377a64737132327067396c343863357630663833356c6a646a7a6b7a34377361357a613963656863656a637732386b3264",
				CardanoNetwork::Testnet,
			),
			(
				"616464725f74657374317770746379376839726d6b68646e686e336a766d3674756865686371326868687a6e74766e30306e7137397068386334347634336a",
				CardanoNetwork::Testnet,
			),
			(
				"616464725f746573743177706c786a7a72616e72617674703537347332777a30306d6437767a39727a707563753235326a65363875396138717a6a68656e67",
				CardanoNetwork::Testnet,
			),
		];
		for (stored, network) in cases {
			let bytes = hex::decode(stored).unwrap();
			assert_eq!(validate_enterprise_script_address(&bytes), Ok(network));
		}
	}

	#[test]
	fn rejects_invalid_utf8() {
		assert_eq!(validate_enterprise_script_address(&[0xff]), Err(CardanoAddressError::NotUtf8));
	}

	#[test]
	fn rejects_non_bech32_text() {
		for input in [
			&b""[..],
			b"x",
			b"nonsense",
			// Last character changed, so the checksum no longer matches
			b"addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjhenq",
			b"ADDR_TEST1WPLXJZRANRAVTP574S2WZ00MD7VZ9RZPUCU252JE68U9A8QZJHENG",
		] {
			assert_eq!(
				validate_enterprise_script_address(input),
				Err(CardanoAddressError::InvalidBech32)
			);
		}
	}

	#[test]
	fn rejects_unsupported_prefix() {
		let stake = encode_address("stake_test", &[0xf0; 29]);
		assert_eq!(
			validate_enterprise_script_address(stake.as_bytes()),
			Err(CardanoAddressError::UnsupportedPrefix)
		);
	}

	#[test]
	fn rejects_network_mismatch() {
		let mut payload = [0u8; 29];
		payload[0] = 0x71;
		let address = encode_address("addr_test", &payload);
		assert_eq!(
			validate_enterprise_script_address(address.as_bytes()),
			Err(CardanoAddressError::NetworkMismatch)
		);
	}

	#[test]
	fn rejects_other_address_types() {
		// Enterprise address with a key payment credential
		let key = encode_address("addr_test", &[[0x60].as_slice(), &[0; 28]].concat());
		// Base address with script payment and key stake credentials
		let base = encode_address("addr_test", &[[0x10].as_slice(), &[0; 56]].concat());
		// Enterprise script header with a truncated hash
		let short = encode_address("addr_test", &[[0x70].as_slice(), &[0; 27]].concat());
		for address in [key, base, short] {
			assert_eq!(
				validate_enterprise_script_address(address.as_bytes()),
				Err(CardanoAddressError::NotEnterpriseScript)
			);
		}
	}
}
