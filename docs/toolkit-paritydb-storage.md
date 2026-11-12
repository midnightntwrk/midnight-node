# Toolkit ledger state in an on-disk ParityDB arena

## Problem

With `--ledger-state-db`, the toolkit cached the replayed ledger state as one file per
block height: the whole state, serialized and zstd-compressed. A warm command read that
file and rebuilt every node of the state as an in-memory object before doing anything.
On preprod at block 600,000 (3.37M arena nodes, 111 MB of state) that took about 19 s and
9 GB of memory, on every run, whether the command needed one node or all of them.

## Design

### The arena database type

Ledger objects are generic over a storage DB `D`; the toolkit uses `DefaultDB` everywhere.
The storage crate's `DefaultDB` is `InMemoryDB` and cannot be changed from outside it.

`midnight-ledger-unsafe-helpers` (only the toolkit and test tooling depend on it) now
shadows the glob-imported `DefaultDB` in its `ledger_8` and `ledger_9` modules with its own
type, `arena_db::ToolkitDB`:

```rust
pub enum ToolkitDB {
    Memory(InMemoryDB<DefaultHasher>),
    Disk(ParityDb<DefaultHasher, OwnedDb>, Arc<parity_db::Db>),
}
```

It implements the storage crate's `DB` trait by dispatching to the variant. The variant
is chosen at run time, so there is no cargo feature and no second build of the toolkit;
an in-memory `ToolkitDB` behaves exactly like the old `InMemoryDB`. Both ledger
generations share one storage crate instance, so ledger 8, ledger 9 and the 8->9 fork
translation all live in the same arena, as before.

A generic `D` parameter threaded through the toolkit was rejected: more than 500 uses of
`DefaultDB` in the toolkit and the concrete `Db8`/`Db9` aliases in the fork code would all
have to change, for no gain over a run-time choice.

`unsafe-helpers` now also depends on `midnight-storage-core` directly, which turns on the
workspace's `layout-v2` and `gc-v1` features for the toolkit too. Before, a
`cargo build -p midnight-node-toolkit` used the old layout and a workspace build used
`layout-v2`; now the toolkit always runs the layout and GC the node runs. Arena hashes do
not depend on the layout.

### Installing the default storage

The storage crate keeps one default storage per DB type per process. It creates an
`InMemoryDB` one on first use, but panics for any other type that was not installed.
So `midnight-ledger-unsafe-helpers` installs an in-memory `ToolkitDB` arena when the
process starts (a `ctor`), before `main` and before any test. Every binary that links the
crate, including tests and the e2e suite, can build ledger objects without setup.

`TxGenerator::source` (every command that reads a chain) and `fetch` call
`ledger_arena::init`. With `PARITYDB_MODE=true` (off by default) and a ledger state cache
(`--ledger-state-db` set and a fetch cache that is not `inmemory`, the same rule as
before) it swaps the start-up arena for a ParityDB in `<ledger-state-db>/ledger-arena`;
otherwise the in-memory arena stays, and cached states are saved as snapshot files as
before.

The swap only happens while the in-memory arena is still empty. Ledger objects keep a
handle to the arena they were built in, so an object built before the swap would never
reach the database. If the arena already holds objects, or the on-disk arena is already
open, `init` returns an error instead of silently staying in memory.

The storage crate cannot replace a default storage in one step, so `init` must run
before any other thread touches ledger state. A thread that looks the arena up during
the swap panics; one that is still using the old arena makes the swap fail with
`InstallError::InUse`, so no ledger object is lost without an error.

### Persisting roots and collecting garbage

The per-wallet cache files and the per-height ledger snapshot files stay. A snapshot file
saved with the on-disk arena has format version 3: instead of the state it holds the
state's root key (a few dozen bytes). Saving a snapshot:

1. `persist`s the state's root (root count + 1) and flushes every pending node and count
   change to ParityDB in one write;
2. writes the snapshot file; if it replaces a version-3 snapshot at the same height, that
   snapshot's root is released (`unpersist`);
3. writes the wallet entries and runs the existing snapshot-file GC, which now returns
   the version-3 snapshots it deleted; their roots are released;
4. flushes the root-count changes, and runs a full mark-and-sweep (`gc-v1`) when more than
   1M nodes were written since the last one (a count kept in
   `<ledger-state-db>/ledger-arena.gc`; a full preprod replay to block 600,000 writes about
   770k nodes, and a GC of the result removes about 250k of them in 35 s);
5. unloads the state from memory (`Sp::unload`): the next blocks reload only what they touch.

So each version-3 snapshot file owns exactly one root reference, and a node stays in the
DB while any remaining snapshot reaches it. GC also keeps every node still in use in
memory.

Not every root count comes from a snapshot: the storage crate's `ChildRef` (used inside
ledger state) takes a root reference while it is alive in memory and drops it when it is
dropped. References alive at a process's last flush are never dropped on disk. So when a
process opens the arena (holding the lock, before it loads anything) it reconciles: every
root count is set to the number of version-3 snapshot files that name it, across all
chains in the directory. This also cleans up after a crash between steps 1 and 2. On the
preprod run below it dropped 6 such left-over references.

Restoring reads the root key and opens it with `Arena::get_lazy`: only the top node is
read; the rest load when touched, through a bounded read cache (200,000 nodes).

During a long replay with the on-disk arena, a checkpoint is saved every
`--replay-checkpoint-interval` blocks, or every 10,000 blocks when it is 0 (in memory, 0
still means no mid-replay checkpoint). A checkpoint
writes only the nodes changed since the previous one and then drops the state from
memory, so the replay's memory no longer grows with the chain, and an interrupted replay
resumes from the last checkpoint.

### Concurrent toolkit processes

ParityDB allows one process per directory. The toolkit takes an exclusive lock on
`<ledger-state-db>/ledger-arena.lock` (with its pid inside) before opening the DB and
holds it until it exits. A second process on the same `--ledger-state-db` waits, logging
which process holds the lock once a minute. `MN_LEDGER_STATE_DB_LOCK_TIMEOUT=<seconds>`
makes it give up with a clear error instead. Waiting was chosen over failing because test
suites run several toolkit commands against the default `toolkit_cache/ledger_cache_db`
and were correct, only slower, before. Use separate `--ledger-state-db` directories to
run in parallel.

### Existing snapshot files

Version-2 (inline) snapshots are still read. When the on-disk arena restores one, it saves
the restored state back as a version-3 snapshot at the same height, even if no block is
replayed: the file is converted once, in place. Version-2 files at other heights are
removed by the normal snapshot GC. Without the on-disk arena (an in-memory run on a cache
written with it), a version-3 snapshot cannot be opened: the run logs a warning and
replays from genesis instead of failing.

## Measurements

`show-wallet` on preprod blocks 0..599,999 (ledger 8, chain
`d64fcd69…af01b`) from a local Postgres fetch cache, `-v`, `/usr/bin/time -v`, release
build, on a shared 32-core box (other jobs were running, so wall times are approximate).
The old toolkit is the parent commit of this change.

| Run | Wall | Max RSS | Ledger state restore |
| --- | --- | --- | --- |
| Old, cold (full replay) | 20:40 | 6.3 GiB | - |
| Old, warm | 14.8 s / 18.0 s | 6.5 GiB | 10.1 s (`trusted_deserialize`) |
| New, cold (full replay) | 20:45 | 3.7 GiB | - |
| New, warm | 4.9 s, 2.7 s, 2.6 s, 2.5 s | 0.53-0.57 GiB | 0.2-8 ms (open root) + 0.23-2.3 s DB open |
| New, first run on an old cache (import) | 15.9 s | 6.6 GiB | 10.3 s, then 0.7 s to persist |
| New, runs after the import | 2.5 s | 0.52 GiB | 0.3 ms |

Every run printed the same wallet (coins, UTXOs, dust UTXOs) as the old cold replay.
During the new cold replay the arena held 7k-33k nodes in memory; the DB directory
ended at 975 MB (60 checkpoints, nothing collected yet). A GC of that DB took 34 s and
1.1 GiB and removed 246k nodes; a warm run afterwards gave the same wallet. An imported
cache is 134 MB.

## Risks

* The ledger state cache now needs exclusive use of its directory for the whole command.
* Memory during a replay is bounded by what a 10,000-block window touches, but nodes a
  command forces stay in memory until the next checkpoint or the end of the command.
* GC is a full mark-and-sweep over the live state (millions of node reads) and runs
  inside a save. The threshold keeps it rare.
* The toolkit's storage layout changed to `layout-v2` for `-p` builds (see above).
