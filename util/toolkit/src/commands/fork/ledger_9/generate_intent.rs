use crate::toolkit_js::EncodedZswapLocalState;
use crate::toolkit_js::encoded_zswap_local_state::{
	EncodedCoinPublic, EncodedOutput, EncodedRecipient, EncodedShieldedCoinInfo,
};
use ledger_helpers_local::{BuilderContext, CoinPublicKey, DefaultDB, WalletSeed};
use midnight_ledger_unsafe_helpers::ledger_9 as ledger_helpers_local;

pub fn fetch_zswap_state_from_context<C: BuilderContext<DefaultDB>>(
	context: &C,
	wallet_seed: WalletSeed,
	coin_public: CoinPublicKey,
) -> EncodedZswapLocalState {
	let zswap_state =
		context.with_wallet_from_seed(wallet_seed, |wallet| wallet.shielded.state.clone());

	// Build EncodedZswapLocalState from raw byte fields.
	// This avoids calling from_zswap_state, which expects the crate-level (latest)
	// WalletState.
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
