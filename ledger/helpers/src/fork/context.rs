use crate::ledger_7::DB;

pub enum LedgerContext<D: DB + Clone> {
	Ledger7(crate::ledger_7::context::LedgerContext<D>),
	Ledger8(crate::ledger_8::context::LedgerContext<D>),
}
