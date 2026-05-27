use std::pin::Pin;

use super::super::{
	BindingKind, BlockContext, ContractAddress, ContractState, DB, LedgerParameters,
	PedersenDowngradeable, ProofKind, Resolver, Serializable, SignatureKind, Storable, Tagged,
	Timestamp, Transaction, Utxo, Wallet, WalletSeed, ZswapChainState,
};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

type BuildResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Abstraction over the ledger context that transaction builders interact with.
///
/// The backend is either a local [`super::super::LedgerContext`] (which owns a
/// [`super::super::LedgerState`]) or, in the future, an indexer-backed client that answers the
/// same queries without replaying every block locally (see issue #1186).
pub trait BuilderContext<D: DB + Clone>: Send + Sync {
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

	/// The most recent block context (block time etc.).
	fn latest_block_context(&self) -> BoxFuture<'_, BlockContext>;

	/// Current ledger parameters (fee/cost model, dust params, TTLs).
	fn ledger_parameters(&self) -> BoxFuture<'_, LedgerParameters>;

	/// The chain's network identifier.
	fn network_id(&self) -> BoxFuture<'_, String>;

	/// All unshielded UTXOs owned by the wallet for `seed`, with their creation time.
	fn unshielded_utxos(&self, seed: WalletSeed) -> BoxFuture<'_, Vec<(Utxo, Timestamp)>>;

	/// The global shielded (zswap) chain state.
	fn zswap_state(&self) -> BoxFuture<'_, ZswapChainState<D>>;

	/// The on-chain state of the contract at `address`, if it exists.
	fn contract_state(&self, address: ContractAddress) -> BoxFuture<'_, Option<ContractState<D>>>;

	/// The resolver currently used for proving.
	fn resolver(&self) -> BoxFuture<'_, &'static Resolver>;

	/// Replace the resolver used for proving.
	fn update_resolver(&self, resolver: &'static Resolver) -> BoxFuture<'_, ()>;

	/// Validate that `tx` is well-formed against the current ledger state.
	///
	/// The local backend checks against its [`super::super::LedgerState`]; an indexer-backed
	/// backend has no full state to check against and relies on the node validating on submission.
	fn well_formed<S, P, B>(&self, tx: &Transaction<S, P, B, D>, now: Timestamp) -> BuildResult<()>
	where
		S: SignatureKind<D>,
		P: ProofKind<D> + Storable<D>,
		B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D> + Tagged;
}
