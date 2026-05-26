use std::pin::Pin;

use super::super::{
	ArenaKey, BlockContext, DB, DUST_EXPECTED_FILES, DustResolver, Event, FetchMode,
	LedgerParameters, LedgerState, Loader, MidnightDataProvider, Offer, OutputMode, PUBLIC_PARAMS,
	ProofKind, PureGeneratorPedersen, Resolver, SerdeTransaction, SignatureKind, Sp, Storable,
	SyntheticCost, Tagged, Timestamp, Transaction, TransactionContext, TransactionResult, Utxo,
	VerifiedTransaction, Wallet, WalletAddress, WalletSeed, WellFormedStrictness,
	clamp_and_normalize, compute_overall_fullness, default_storage, deserialize,
	mn_ledger_serialize as serialize, mn_ledger_storage as storage, types::StorableSyntheticCost,
};

pub trait BuilderContext<D: DB + Clone>: Send + Sync {
	/// Helper to get or create a wallet for a seed within an existing lock.
	fn wallet_from_seed(&self, seed: WalletSeed) -> Wallet<D>;

	/// Operate on a single wallet identified by seed.
	fn with_wallet_from_seed<F, R>(&self, seed: WalletSeed, f: F) -> R
	where
		F: FnOnce(&mut Wallet<D>) -> R;

	/// Operate on two wallets identified by origin and destination seeds.
	fn with_wallets_from_seeds<F, R>(
		&self,
		origin_seed: WalletSeed,
		destination_seed: WalletSeed,
		f: F,
	) -> R
	where
		F: FnOnce(&mut Wallet<D>, &mut Wallet<D>) -> R;

	fn tx_context(&self, block_context: BlockContext) -> TransactionContext<D>;

	fn latest_block_context(&self) -> BlockContext;

	fn ledger_parameters(&self) -> LedgerParameters;

	fn update_resolver(
		&self,
		resolver: &'static Resolver,
	) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}
