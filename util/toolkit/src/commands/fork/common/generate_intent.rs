use super::ledger_helpers_local::{self, CoinPublicKey, DefaultDB, WalletSeed};
use crate::toolkit_js::EncodedZswapLocalState;
use crate::toolkit_js::encoded_zswap_local_state::{
	EncodedCoinPublic, EncodedOutput, EncodedRecipient, EncodedShieldedCoinInfo,
};

pub fn fetch_zswap_state_from_context(
	context: &ledger_helpers_local::context::LedgerContext<DefaultDB>,
	wallet_seed: WalletSeed,
	coin_public: CoinPublicKey,
) -> EncodedZswapLocalState {
	let wallet = context.wallet_from_seed(wallet_seed);
	let zswap_state = wallet.shielded.state;

	// Build EncodedZswapLocalState from raw byte fields.
	// This avoids calling from_zswap_state which expects ledger_8 WalletState.
	let coin_public_bytes = coin_public.0.0;

	EncodedZswapLocalState {
		coin_public_key: EncodedCoinPublic::from_raw_bytes(coin_public_bytes),
		current_index: zswap_state.first_free,
		inputs: vec![],
		outputs: zswap_state
			.coins
			.iter()
			.map(|(_nullifier, c)| {
				EncodedOutput::new(
					EncodedShieldedCoinInfo::new(c.nonce.0.0, c.type_.0.0, c.value),
					EncodedRecipient::user(EncodedCoinPublic::from_raw_bytes(coin_public_bytes)),
				)
			})
			.collect(),
	}
}
