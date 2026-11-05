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
So the arena is chosen once, before the first ledger object is built:

* `TxGenerator::source` (every command that reads a chain) and `fetch` call
  `ledger_arena::init`. With a ledger state cache (`--ledger-state-db` set and a fetch
  cache that is not `inmemory`, the same rule as before) it installs a ParityDB in
  `<ledger-state-db>/ledger-arena`; otherwise the in-memory arena.
* `run_command` installs the in-memory arena for every other command.
* `LedgerContext::new` (both generations) installs the in-memory arena if nothing is
  installed yet, which covers library users and tests.

Installing a second arena fails; the first choice wins for the life of the process.

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
   4M nodes were written since the last one (a count kept in
   `<ledger-state-db>/ledger-arena.gc`);
5. unloads the state from memory (`Sp::unload`): the next blocks reload only what they touch.

So each version-3 snapshot file owns exactly one root reference, and a node stays in the
DB while any remaining snapshot reaches it. GC also keeps every node still in use in
memory. A crash between steps 1 and 2 leaks one root, which keeps one old state alive; it
does not corrupt anything.

Restoring reads the root key and opens it with `Arena::get_lazy`: only the top node is
read; the rest load when touched, through a bounded read cache (200,000 nodes).

During a long replay with the on-disk arena, a checkpoint is saved every 10,000 blocks
(`--replay-checkpoint-interval 0` keeps its meaning for in-memory runs). A checkpoint
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

## Risks

* The ledger state cache now needs exclusive use of its directory for the whole command.
* Memory during a replay is bounded by what a 10,000-block window touches, but nodes a
  command forces stay in memory until the next checkpoint or the end of the command.
* GC is a full mark-and-sweep over the live state (millions of node reads) and runs
  inside a save. The threshold keeps it rare.
* The toolkit's storage layout changed to `layout-v2` for `-p` builds (see above).
