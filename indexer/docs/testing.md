# Testing & Node Consistency

How the indexer is kept consistent with the node, and what the test layers cover.

## The guarantee is a runtime guard, not a test

The hard guarantee lives in the chain-indexer, not in a suite. For every block the
chain-indexer applies the node's transactions to its **own** `LedgerState`, recomputes the
merkle roots, and **`bail!`s (halts indexing) on any mismatch** rather than persisting a
divergent state - `chain-indexer/src/application.rs` (~404-420):

- **Zswap merkle tree root - every block.** `ledger_state.zswap_merkle_tree_root()` is
  compared to the node's per-block root; mismatch → `bail!`. This is the check that works
  against every node version.
- **Full ledger-state root - every block the node supplies one.** When `block.ledger_state_root`
  is present (Node ≥ 0.22) the recomputed `ledger_state.root()` is compared to it; mismatch →
  `bail!`. At genesis the state root additionally disambiguates a pre- vs post-block-0 genesis.
  (A `TODO` retires the zswap-only path once Node < 0.22 support is dropped.)

So the model is **fail loud, don't drift**: if the indexer ever computes a root the node
disagrees with, it stops. There is no after-the-fact `assert!(indexer_root == node_root)`
anywhere - the guard is the source of truth.

Per-block **dust roots** are computed, stored and served but not cross-checked against the node
(there is no node dust-root RPC); they are covered transitively by the deterministic re-apply
that the zswap/state-root match guards each block.

## How CI exercises the guard

`indexer-tests/tests/native_e2e.rs` (run by `just test`, on every PR via `ci-cloud.yaml` /
`ci-standalone.yaml`):

- Starts a **real `midnightntwrk/midnight-node` container** (version = last line of
  `NODE_VERSIONS`, currently `2.0.0-rc.3`) whose chain DB is **pre-seeded from fixed data in
  `.node/<version>/`** (bind-mounted, `CFG_PRESET=dev`) so it replays a known, deterministic
  chain, plus postgres + nats via testcontainers, and runs the **actual** chain-indexer /
  wallet-indexer / indexer-api binaries (cloud) or `indexer-standalone` + SQLite (standalone).
  It SIGTERMs and restarts chain-indexer once to exercise reconnect.
- `indexer-tests/src/e2e.rs` then runs the assertions. It **collects the blocks subscription
  (heights 0..=32) as the source of truth**, validating structural invariants as it goes: heights
  increment by one, parent-hash linkage, transactions reference their block and share its protocol
  version, segment results match the transaction status, fees parse, a contract call shares its
  deploy's address, unshielded balances have a valid token type + amount, and zswap/dust ledger
  events are present. It then asserts **every query and every other subscription returns
  byte-identical JSON to that collected data** (`to_json_value()` equality, across all offsets plus
  the unknown/error cases). It also round-trips a wallet through **connect → wallet-indexer → the
  shielded-transactions subscription** (expecting the two relevant test-wallet txs with no index
  gaps), and exercises the input guards (invalid viewing key, unknown/short session id, invalid
  index ranges, nullifier-prefix validation).
- There is **no explicit node-root assertion** - but if the indexer's roots disagreed with the
  real node container the chain-indexer would `bail!`, `/ready` would never reach 200, and the
  e2e would fail. That is how the guard is covered in CI.

`indexer-tests/src/main.rs` (the `e2e` bin) can point the same suite at a **deployed** indexer
(`--host/--port/--network-id`), but no workflow wires it up - it is a manual tool.

## The QA TypeScript suite (`qa/tests/`)

- **smoke** - `/ready`, schema introspection, schema-change / deprecation detection.
- **integration** - GraphQL correctness via zod schemas + internal consistency.
- **e2e** (`tests/e2e/`) - the closest to node-matching: submits real shielded / unshielded txs
  and a contract deploy/call through the **Midnight Node Toolkit**, then asserts the indexer
  reports them over GraphQL (source of truth = what the toolkit pushed to the node). Env config
  in `qa/tests/environment/model.ts`.

## Against deployed environments

- CI runs against an **ephemeral fresh node**, never a deployed env.
- Scheduled monitors (`.github/workflows/qa-test-*`, business hours) hit
  `indexer.<env>.midnight.network` for devnet / qanet / preprod / preview and run
  block-subscription + ledger-event streaming tests - they validate the **API** (streaming
  continuity, shape), **not** an indexer-vs-node root diff.

## See also

- [Creating a release](./releasing.md)
