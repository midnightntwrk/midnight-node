use crate::ledger_7::{DB, ProofKind as ProofKind7, SignatureKind as SignatureKind7, Tagged};
use crate::ledger_8::{ProofKind as ProofKind8, SignatureKind as SignatureKind8};

pub enum BlockData<
	S7: SignatureKind7<D> + Tagged,
	P7: ProofKind7<D>,
	S8: SignatureKind8<D> + Tagged,
	P8: ProofKind8<D>,
	D: DB,
> {
	Ledger7(crate::ledger_7::block_data::BlockData<S7, P7, D>),
	Ledger8(crate::ledger_8::block_data::BlockData<S8, P8, D>),
}
