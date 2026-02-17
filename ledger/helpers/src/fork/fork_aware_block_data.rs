use crate::fork::fork_aware_context::ForkAwareLedgerContext;
use crate::ledger_7::{DB, ProofKind as ProofKind7, SignatureKind as SignatureKind7, Tagged};
use crate::ledger_8::{ProofKind as ProofKind8, SignatureKind as SignatureKind8};

pub enum ForkAwareBlockData<
	S7: SignatureKind7<D> + Tagged,
	P7: ProofKind7<D>,
	S8: SignatureKind8<D> + Tagged,
	P8: ProofKind8<D>,
	D: DB + Clone,
> {
	Ledger7(crate::ledger_7::block_data::BlockData<S7, P7, D>),
	Ledger8(crate::ledger_8::block_data::BlockData<S8, P8, D>),
}

impl<
	S7: SignatureKind7<D> + Tagged,
	P7: ProofKind7<D>,
	S8: SignatureKind8<D> + Tagged,
	P8: ProofKind8<D>,
	D: DB + Clone,
> ForkAwareBlockData<S7, P7, S8, P8, D>
{
	fn new_context(&self, network_id: impl Into<String>) -> ForkAwareLedgerContext<D> {
		match self {
			ForkAwareBlockData::Ledger7(_) => ForkAwareLedgerContext::Ledger7(
				crate::ledger_7::context::LedgerContext::new(network_id),
			),
			ForkAwareBlockData::Ledger8(_) => ForkAwareLedgerContext::Ledger8(
				crate::ledger_8::context::LedgerContext::new(network_id),
			),
		}
	}

	fn new_context_from_wallet_seeds(
		&self,
		network_id: impl Into<String>,
		seeds: &[crate::ledger_7::WalletSeed],
	) -> ForkAwareLedgerContext<D> {
		match self {
			ForkAwareBlockData::Ledger7(_) => ForkAwareLedgerContext::Ledger7(
				crate::ledger_7::context::LedgerContext::new_from_wallet_seeds(network_id, seeds),
			),
			ForkAwareBlockData::Ledger8(_) => {
				// Convert ledger_7 WalletSeeds to ledger_8 WalletSeeds
				let seeds_8: Vec<crate::ledger_8::WalletSeed> = seeds
					.iter()
					.map(|s| {
						crate::ledger_8::WalletSeed::try_from(s.as_bytes())
							.expect("wallet seed conversion failed")
					})
					.collect();
				ForkAwareLedgerContext::Ledger8(
					crate::ledger_8::context::LedgerContext::new_from_wallet_seeds(
						network_id, &seeds_8,
					),
				)
			},
		}
	}
}
