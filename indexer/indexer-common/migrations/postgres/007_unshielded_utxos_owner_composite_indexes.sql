-- Owner-leading composites for queries that filter by owner equality and a
-- tx_id range (get_transactions_by_unshielded_address and
-- get_highest_transaction_id_for_unshielded_address). The existing
-- (link_column, owner) composites have the link column leading and don't fit
-- this access pattern.

--------------------------------------------------------------------------------
-- unshielded_utxos
--------------------------------------------------------------------------------
CREATE INDEX ON unshielded_utxos (owner, creating_transaction_id);
CREATE INDEX ON unshielded_utxos (owner, spending_transaction_id);
