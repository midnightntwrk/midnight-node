# Proposal 0002: Incremental wallet cache for the indexer-backed toolkit

Status: implemented. Raised in review of the indexer `show-wallet` PR (issue #1186).

This document records both the original proposal and what reading the indexer's source changed
about it — four findings, marked **Finding** below, that moved the design materially. The most
consequential: the "blocking prerequisite" turned out not to block anything, and one of the
proposed resume calls would have fed a value into the wrong index space.

## Problem statement

`IndexerContext::init_wallets` re-drains every subscription from the origin on each invocation.
All three drains hardcode their start cursor:

| stream | call site | cursor before this change |
| --- | --- | --- |
| shielded | `client.connect(&viewing_key, None)` + `client.shielded_transactions(sid, 0)` | from genesis |
| unshielded | `client.unshielded_transactions(&address, 0)` | from genesis |
| dust | `client.dust_ledger_events(1)` | from genesis |

On a short-lived dev chain this is cheap, which is why the first PR shipped without a cache. On a
long-lived network the cost grows with chain length rather than with wallet activity: the dust
stream in particular is a *global* ledger-event log, so its drain is O(chain), not O(wallet).

## What already exists

Both halves of the mechanism are in the tree; nothing new has to be invented.

**Server side.** Every operation in `ledger/helpers/unsafe/graphql/indexer.graphql` already accepts a
resume cursor — `ConnectOptions.startIndex`, `shieldedTransactions(index:)`,
`unshieldedTransactions(transactionId:)`, `dustLedgerEvents(id:)` — and `IndexerClient` already
plumbs each one through as a parameter. The work is entirely on the caller's side.

**Client side.** The replay path already persists wallet state between runs:
`util/toolkit/src/fetcher/wallet_state_cache.rs` defines `CachedWalletState` behind the
`WalletStateCaching` trait, with a `FileBackend` implementation wired up by
`create_file_wallet_cache` and already reachable from `show-wallet`. The serialization of the two
stateful sub-wallets is therefore solved.

## The gap

`CachedWalletState` is keyed and versioned by **block height**, because the replay path resumes by
replaying blocks. The indexer path resumes by three independent per-stream cursors that do not map
onto a block height. Reusing the struct means adding those cursors and a cache-validity key.

## Findings from the indexer source

**Finding 1 — chain identity is not blocked on an indexer change.** The original proposal called a
chain identifier "the blocking prerequisite", on the basis that `LatestBlock` exposes no chain or
genesis field. It does not need to: `Query.block(offset: BlockOffset)` returns `Block.hash`, so
`block(offset: {height: 1}).hash` yields exactly the `H256` the replay path already uses as its
`chain_id` (`util/toolkit/src/serde_def/transactions.rs`). This is a one-query client-side
addition, and an indexer that has not yet indexed block 1 returns `None`, which disables caching —
mirroring `SourceTransactions::chain_id()`.

**Finding 2 — `ConnectOptions.startIndex` is a different index space.** It lands in
`wallets.wanted_start_index` and is compared against `first_indexed_transaction_id`
(`indexer/wallet-indexer/src/application.rs`) — *transaction ids*, not zswap indices. The
proposal's "`connect(vk, Some(n))` plus `shielded_transactions(sid, n)`" would have fed a zswap
index into a transaction-id field. It is also upserted as a MIN, so it can never narrow an existing
server-side scan, making it useless for this purpose even with the right units. **`connect(vk,
None)` stays.**

**Finding 3 — cursors are inclusive.** Every backing query selects `... >= $2`
(`indexer/indexer-api/src/infra/storage/{ledger_events,transaction}.rs`), and the shielded stream
advances `index = transaction.zswap_end_index`, an exclusive end. So: resume at `last + 1` for
unshielded, and at the stored zswap index for shielded. Dust is inclusive too, but deliberately
resumes *at* `last` rather than past it — that stream has no progress heartbeat, so a caught-up
wallet subscribed past its cursor receives nothing and stalls for the full idle timeout; the
re-delivered event carries `maxId` and ends the drain in one round trip. It is skipped, not
replayed.

**Finding 4 — shielded resume is unsound on a `PartialSuccess` tail.** `relevant_offers` applies
only the guaranteed offer there, so `state.first_free` ends *below* that transaction's
`zswap_end_index`. The live drain tolerates it (the next collapsed update realigns the tree), but
no single cursor is then both correct and safe: resume at `first_free` and the server re-delivers
that transaction with `collapsed_update: None` — it omits the update whenever `index >=
zswap_start_index` — double-applying its outputs onto the merkle tree. Handled by a guard, below,
rather than by an assumption.

## Design

Cursor and state are stored as one unit per (chain, seed, ledger generation), and the shielded
cursor is **derived from the persisted state** (`WalletState::first_free`) rather than stored
alongside it, so the pair cannot drift.

| stream | cursor | resume call | safety |
| --- | --- | --- | --- |
| shielded | `state.first_free` | `shielded_transactions(sid, first_free)` | persist only when aligned |
| unshielded | highest applied `transactionId` | `unshielded_transactions(addr, id + 1)` | order-independent fold |
| dust | last applied event `id` | `dust_ledger_events(id)`, re-delivered event skipped | `replay_events` errors `NonLinearInsertion` on a bad resume |

**Shielded alignment guard.** `drain_shielded` tracks `last_end_index` (initialised to the resume
index, set to `zswap_end_index` on each `Relevant`). After the drain, the shielded half is persisted
only when `state.first_free == last_end_index`. Any case that leaves them apart — `PartialSuccess`,
a `Failure` transaction that still consumed indices, a stalled stream — degrades to "don't cache the
shielded half", never to a corrupt merkle tree. It is self-healing: the next run's tail is usually
aligned.

**Dust pre-TTL split.** `sync_dust` ends with `process_ttls(tip_time)`, a projection against the
current tip, not part of the state. The state is captured *between* `replay_events` and
`process_ttls` (an `Sp` clone — a refcount bump) and that is what is persisted; `process_ttls` is
re-applied on every load against the new tip.

**Cache invalidation.** A separate key namespace from the replay path, folding in the ledger
generation — the indexer path writes no `LedgerSnapshot`, so it loses the
`restore_context_from_ledger_snapshot` version check the replay path relies on:

```rust
/// Indexer-path wallet entries. The 0xFF source byte sits where `wallet_cache_key` puts
/// `scheme_discriminant` (0/1), so replay and indexer entries can never collide.
pub fn indexer_wallet_cache_key(seed: &WalletSeed, ledger_version: LedgerVersion) -> H256
```

The chain is the existing `chain_id` directory namespace (block 1's hash, per Finding 1). The
network id used to derive the viewing key is not folded in: a wrong `--network` produces a bech32
HRP the indexer rejects outright, so it cannot silently populate an entry.

## Test plan

The existing e2e (`util/toolkit/tests/indexer_show_wallet_e2e.rs`) gives the oracle for free: a
cached run must produce byte-identical `WalletInfoJson` to a cold run against the same chain tip.
It now runs an uncached baseline, two cached runs (cold-into-cache, then resumed), and a chain-id
mismatch case that must fall back to a full drain rather than serve another chain's state.

## Not done here

- The `zswapMerkleTreeCollapsedUpdate(first_free, last_end_index - 1)` realign query. The alignment
  guard makes it an optimisation, not a correctness requirement; add it if the shielded half stops
  persisting often.
- `init_wallets` drains the chain-wide dust log **once per seed**. Irrelevant for single-seed
  `show-wallet`, a large win for any multi-seed caller. Worth its own change.
- Caching for the `todo!()` transaction-building `BuilderContext` methods.
