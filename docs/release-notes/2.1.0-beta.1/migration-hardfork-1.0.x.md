<!-- markdownlint-disable MD012 MD013 MD014 MD022 MD031 MD032 MD033 MD034 MD060 -->

# Migration: hard-forking a 1.0.3 chain to 2.1.0-beta.1

This guide covers upgrading a chain **already running the 1.0.3 runtime** to `2.1.0-beta.1`. That
is the supported fork-from baseline; the 1.0.3 release itself is still pending. It is a ledger hard
fork (ledger 8 → ledger 9) enacted by a governance `set_code`, not a rolling binary upgrade — the
boundary block permanently changes how ledger state is encoded, and one on-chain migration runs
across several blocks afterwards.

`2.1.0-beta.1` is a **pre-release**. Run this on a test or pre-production network first; do not
treat this guide as a mainnet runbook until the corresponding final release ships.

CI currently exercises the fork boundary from `midnightntwrk/midnight-node:1.0.1`
(`util/toolkit/tests/hardfork_e2e.rs`, default set in
`util/toolkit/test-images.docker-compose.yml`, overridable with `FORK_FROM_NODE_IMAGE`). That path
crosses the boundary under `system_version: 1` and so exercises the skew-block handling in §3.1,
which does not arise from 1.0.3.

## 1. Before the upgrade

### 1.1 Every validator must be on the new binary first

> **This is the one step that breaks the chain if skipped.**

A runtime can only be instantiated by a node that exports every host function it imports, so a
validator left on an older binary stops importing blocks at the `set_code` and is stuck there.
Roll every validator onto 2.1.0 and confirm they are all importing and finalizing normally
*before* the governance action is submitted. The binary upgrade on its own is a hot swap — the
1.0.3 runtime keeps running until `set_code`.

```shell
docker pull midnightntwrk/midnight-node:2.1.0-beta.1
docker pull midnightntwrk/midnight-node-toolkit:2.1.0-beta.1
```

### 1.2 Remove retired config keys

Nothing to do here if you are coming from 1.0.3 — the key below never existed on the 1.0.x line.
It applies only to an operator moving across from a 2.0.0 pre-release. The config loader ignores
keys it does not recognise, so a stale key is a **silent no-op** rather than a startup error; clean
it out so nobody later reads it as a live setting.

| Key | Why it went | Replacement |
| --- | --- | --- |
| `cnight_observation_window_size` | The sliding-window observation cache was reverted ([#2030](https://github.com/midnightntwrk/midnight-node/pull/2030)). | None — the follower is back on the per-call db-sync path. |

Also note `block_stability_margin` now defaults to **30** (was 10) across every preset
([#1914](https://github.com/midnightntwrk/midnight-node/pull/1914)). The on-chain effective margin
is the minimum across all producers, so this only takes effect when the whole validator set moves;
if you pin it via `BLOCK_STABILITY_MARGIN`, coordinate the change with the rest of the set.

### 1.3 Take a database snapshot

The v8 → v9 state translation is one-way. Snapshot at least one validator's database at a
pre-fork height; that snapshot is the only route back (§6).

## 2. Enacting the upgrade

The runtime is swapped by a governance `set_code`. Using the toolkit's `runtime-upgrade`
command, which submits the proposal, collects collective approvals, and waits for finality past
the applying block:

```shell
midnight-node-toolkit runtime-upgrade \
  --wasm-file midnight_node_runtime.compact.compressed.wasm \
  -c <collective-signer-1> -c <collective-signer-2> \
  -t <technical-signer-1> -t <technical-signer-2> \
  --rpc-url ws://<node>:9944 \
  --signer-key <submitter-key>
```

Substitute your network's actual governance keys for the `-c` / `-t` / `--signer-key` values; the
e2e test uses the dev accounts `//Dave`, `//Eve`, `//Alice`, `//Bob`.

This pre-release publishes **no runtime WASM asset**
([#2061](https://github.com/midnightntwrk/midnight-node/issues/2061)), so extract the blob from
the node image:

```shell
cid=$(docker create midnightntwrk/midnight-node:2.1.0-beta.1)
docker cp "$cid:/artifacts-amd64/midnight_node_runtime.compact.compressed.wasm" .
docker rm -f "$cid"
```

Because there is no deterministic (srtool) build for this tag, the blob's hash is not independently
reproducible. For a governance action that needs a verifiable artefact, wait for a release that
publishes one.

## 3. What happens at the boundary

### 3.1 The `set_code` block is a skew block (only if upgrading from 1.0.2 or earlier)

**Not applicable on the 1.0.3 baseline.** 1.0.3 ships `system_version: 3`, so `frame_system` stages
the new code in `:pending_code` and applies it at the start of the *next* block; `:code` and the
ledger `StateKey` it is read against stay in step across the boundary. Skip to §3.2 unless you are
forking from 1.0.2 or earlier.

On those earlier runtimes `system_version` is 1, so `frame_system` overwrites `:code` *inside* the
`set_code` block, while pallet-midnight's v8 → v9 state translation only runs in the *next*
block's `initialize_block`. That one block's committed state therefore pairs ledger-9 `:code` with
a ledger-8 `StateKey`, permanently.

[#1985](https://github.com/midnightntwrk/midnight-node/pull/1985) makes this readable: the
read-only ledger-9 host accessors inspect the tagged-serialization header of the `state_key` they
are handed and, when it is a ledger-8 arena root, serve the read from the ledger-8 bridge instead.
Because the dispatch lives in the host function, it covers the `midnight_*` JSON-RPCs,
`MidnightRuntimeApi` via `state_call`, and subxt-based tooling such as `chain-indexer` alike.

Behaviour to expect: at the `set_code` block `midnight_ledgerStateRoot` returns a **v8-tagged**
root — correct, because that block's state *is* v8. Consumers walking the fork see the tag flip at
the migration block rather than a hole.

Transaction paths (`get_transaction_cost`, `validate_transaction`, `apply_transaction`) are
deliberately *not* covered; they resolve on their own one block later.

### 3.2 Dust state is wiped

The v8 → v9 translation drops the ledger's dust state and installs the empty one genesis starts
from. Nothing you do avoids this; the replay below puts back the part that can be reconstructed.

### 3.3 cNIGHT dust generation is replayed (multi-block)

`pallet-cnight-observation` migrates storage version 1 → 2
([#2012](https://github.com/midnightntwrk/midnight-node/pull/2012)):

- `on_runtime_upgrade` saves the pre-fork ledger-8 arena root in `PreForkStateKey` — the only
  place the wiped entries' night value and dust owner survive.
- A multi-block migration pages through `UtxoOwners`, reads each nonce's pre-wipe value and owner
  through the `dust_generation_values` host function, and re-applies them in batches of 25 as
  `CNightGeneratesDustUpdate` system transactions.
- **Cardano observations are ignored while it runs.** `NextCardanoPosition` does not advance, so
  the observer re-delivers the same UTXOs once the gate lifts — no cNIGHT events are lost, they
  are only delayed.

Throughput is roughly **175 nonces/block**. Measured live generating sets on 2026-08-06:

| Network | Entries | Approx. blocks of gated observation |
| --- | --- | --- |
| mainnet | 4870 | ~28 |
| preview | 1524 | ~9 |
| preprod | 85 | 1 |

Restored entries are field-for-field identical to the wiped ones except the accrual clock: the
original creation time lived on the destroyed dust UTXO, so the replay stamps
`fork block time - dust.time_to_cap()` (~1 week). Every cNIGHT holder lands back **at their cap**
rather than at zero refilling over a week.

## 4. After the upgrade: what holders must do

### 4.1 Native NIGHT holders must re-register their dust address

Only **cNIGHT's slice** of the generating set is restored. Native NIGHT registers generation
entries too, and nothing in this repo records which of those the wipe took. A wallet holding
native NIGHT crosses the fork still holding its NIGHT but generating no DUST — and with no DUST it
cannot pay a fee.

Re-register to restart generation. The registration funds itself from the retroactive DUST the
now-generationless NIGHT accrued, which is the path a real holder takes after the wipe:

```shell
midnight-node-toolkit generate-txs \
  --fetch-cache inmemory \
  register-dust-address \
  --wallet-seed <seed> \
  -s ws://<node>:9944 -d ws://<node>:9944
```

The re-registered NIGHT starts generating from *that* block's time, so allow a couple of blocks
before attempting a fee-paying transaction.

> `DustReapplyCompleted` must **not** be read as "all dust generation restored". It means cNIGHT's
> slice is restored.

### 4.2 Clients, SDKs, and indexers must re-fetch metadata

`transaction_version` bumps 3 → 4, so extrinsics encoded against the 1.0.x runtime will not decode.
Every client must re-fetch metadata from a block at or after the `set_code`, and any pre-signed or
queued extrinsic must be rebuilt.

Two traps here:

- **subxt resolves metadata at the finalized head.** While finality is still behind the fork, a
  subxt client that connects fresh picks up the *old* metadata and then fails against the new
  runtime ([#1969](https://github.com/midnightntwrk/midnight-node/issues/1969)). Wait for finality
  to pass the `set_code` block, or pin metadata explicitly.
- **`CNightObservation` error indices are renumbered** (see
  [runtime-diff.md](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/runtime-diff.md#cnightobservation-index-13)). Tooling that maps
  `DispatchError::Module { index, error }` through a hardcoded table rather than through metadata
  fetched at the block being decoded will mis-report errors.

## 5. Watching it happen

New events on `CNightObservation`: `DustReapplyStarted`, `DustReapplyBatchFailed { nonces }`,
`DustReapplyCompleted { applied, skipped }`, `DustReapplySkipped { applied, skipped }`,
`ObservationsSkippedForMigration` (once per block while the gate holds).

**Only `DustReapplyStarted` and `ObservationsSkippedForMigration` show up in Polkadot.js Apps.**
The rest are deposited from a multi-block-migration step, whose phase is `ApplyExtrinsic(n)` for
an `n` no extrinsic in the block has, and the explorer drops events it cannot attach to an
extrinsic. Read them via `System::Events`, subxt's `events()`, or the node log — each is logged
under its own name.

Verification checklist, in order:

| Check | How | Expected |
| --- | --- | --- |
| New code applied | `state_getRuntimeVersion` at successive heights; the first height reporting the new spec is the `set_code` block | `specVersion` 2001000 |
| Fork boundary readable | `midnight_zswapStateRoot` / `midnight_ledgerStateRoot` at `applied - 1`, `applied`, `applied + 1` | non-empty root at all three |
| Runtime API readable at the boundary | `state_call` `MidnightRuntimeApi_get_ledger_state_root` and `..._get_ledger_parameters` at `applied` | SCALE result starts `0x00` (`Ok`) |
| Replay wound up | `CNightObservation` storage version | `2` |
| Pre-fork key cleared | `CNightObservation::PreForkStateKey` | unset |
| Replay actually restored something | `DustReapplyCompleted` event fields | `applied > 0` |
| Observation resumed | `ObservationsSkippedForMigration` stops appearing; `NextCardanoPosition` advances | — |

A `DustReapplySkipped`, or a `DustReapplyCompleted` with `applied == 0` on a chain that had cNIGHT
`UtxoOwners`, means the replay fell short — capture the events and the node log before doing
anything else.

## 6. Rollback

There is no supported downgrade path. The v8 → v9 state translation rewrites ledger state in
place, and `transaction_version` has moved, so a `set_code` back to the 1.0.x runtime does not
undo the fork.

Recovering means restoring the pre-fork database snapshot from §1.3 across the validator set and
abandoning every block produced after the boundary. Treat that as an incident, coordinate it with
the node team, and do not attempt it piecemeal on individual validators.

## Related

- [2.1.0-beta.1 release notes](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/release-notes-2.1.0-beta.1.md)
- [Runtime diff 1.0.3 → 2.1.0-beta.1](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/runtime-diff.md)
- [Ledger 9 and the C2M bridge](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.0.0-alpha.1/eng-ledger9-c2m-bridge.md)
- [Storage separation config](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.0.0-alpha.1/config-storage-separation.md)
