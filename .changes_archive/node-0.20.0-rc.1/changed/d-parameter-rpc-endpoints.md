# D Parameter RPC endpoint and pallet-midnight call index update

This change adds D Parameter RPC support and updates pallet call indices:

1. **New RPC endpoint:** Add `systemParameters_getAriadneParameters` to `pallet-system-parameters-rpc`. This returns permissioned candidates from Cardano with D Parameter sourced from on-chain `pallet-system-parameters` storage.

2. **RPC deprecation:** Mark `sidechain_getAriadneParameters` as deprecated (still functional). Integrators should migrate to the new endpoint.

3. **pallet-midnight update:** Renumber `set_tx_size_weight` from `call_index(2)` to `call_index(1)`.

**Breaking changes:**
- `pallet-midnight`: Pre-encoded transactions referencing `call_index(2)` for `set_tx_size_weight` will fail.

Ticket: [PM-20993](https://shielded.atlassian.net/browse/PM-20993)
PR: https://github.com/midnightntwrk/midnight-node/pull/378
