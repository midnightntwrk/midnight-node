#runtime
# Add vendored `pallet-batch` to runtime

Added a minimal in-tree `pallet-batch` at pallet index 20 to let governance batch
calls. The pallet exposes only `batch` (best-effort) and `batch_all` (atomic) —
two extrinsics drawn verbatim from upstream `pallet-utility` — and nothing else.

Without batching, two governance actions in the same block can land
independently: one may be voted in while the other is not, or one may succeed
while the other fails, leaving the chain in an inconsistent state. A common
example is updating ledger transaction-cost parameters alongside a runtime
upgrade in a single motion. Batching also reduces the per-action burden on node
operators.

We deliberately did **not** wire in upstream `pallet-utility`. Its
`dispatch_as` call accepts an arbitrary `PalletsOrigin` from a Root caller and
re-dispatches with `dispatch_bypass_filter`. Because federated-authority
motions execute with `RawOrigin::Root`, exposing `dispatch_as` would have let
governance forge `Origin::None` (to inject inherents or call
`send_mn_transaction`) or `Origin::Signed(_)` (to dispatch any signed call as
any victim account). The vendored pallet drops `dispatch_as`, `as_derivative`,
`force_batch`, `with_weight`, and `if_else`, along with the `PalletsOrigin`
associated type that exists solely to support `dispatch_as`. What remains is
only the atomic-batch primitive governance actually needs.

PR: https://github.com/midnightntwrk/midnight-node/pull/463
Issue: https://github.com/midnightntwrk/midnight-node/issues/1143
