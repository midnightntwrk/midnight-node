<!-- markdownlint-disable MD012 MD013 MD014 MD022 MD031 MD032 MD033 MD034 MD060 -->

# Midnight Node 2.1.0-beta.1

## Metadata

- **Type of release**: major (relative to the previous final release, `node-1.0.1`; relative to the 2.0.0 pre-release line it is a minor)
- **Date**: 2026-08-21
- **Ships in bundle**: TBD — standalone major pre-release (not bundled)
- **Git tag**: [node-2.1.0-beta.1](https://github.com/midnightntwrk/midnight-node/tree/node-2.1.0-beta.1)
- **Environment**: All public networks at time of release. For the full compatibility matrix, see the [release notes overview](https://docs.midnight.network/relnotes/overview).
- **Upgrade scope**: binary + runtime
- **Reset required**: No
- **Governance action required**: Yes
- **Sister-line note**: The 1.0.x maintenance line runs in parallel. **1.0.3 is the supported fork-from baseline for 2.1.0**; that release is still pending. Changes shared with it — the runtime-gated tblock correction and the `system_version` bump to 3 — are part of that baseline and are not restated here.

## High-level summary

`2.1.0-beta.1` is the feature-complete beta of the 2.1.0 line: **its feature set is identical to 2.0.0** ([node-2.0.0-rc.4](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.0.0-rc.4), [2.0.0-alpha.1 release notes](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.0.0-alpha.1/release-notes-2.0.0-alpha.1.md)), which never shipped a final release. On top of that it adds two months of maintenance and security patches from `main`, and the ability to **hard-fork from a running 1.0.x chain** rather than only from a fresh genesis.

This is a **binary + runtime** upgrade: `spec_version` moves 1_000_003 → 2_001_000 and `transaction_version` moves 3 → 4. Enacting it is a ledger 8 → 9 hard fork with an on-chain multi-block migration — follow the [1.0.3 → 2.1.0 hardfork migration guide](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/migration-hardfork-1.0.x.md).

The supported fork-from baseline is the **pending 1.0.3 release**. `system_version` is already 3 there, so the block applying this runtime is not a skew block and the boundary needs no special handling on that account.

The change entries below are the delta against the 2.0.0 pre-release line and against 1.0.3, so anything shared with the 1.0.x maintenance line is not restated. For everything the 2.x line introduced relative to 1.0.x, see the 2.0.0 notes linked above and the [runtime diff](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/runtime-diff.md).

## Audience

- [x] Operators who run validators or RPC nodes on a chain that will hard-fork from 1.0.x
- [x] Operators who maintain node config presets or `.env` overrides
- [x] Developers who build, sign, or decode extrinsics against the node (SDK, indexer, toolkit users)
- [x] Developers who hold or generate DUST from native NIGHT
- [x] Administrators who hold governance keys and enact the runtime upgrade

## Dependencies

- **midnight-ledger 9.1.0.0-rc.4** — pinned as a single workspace tag; the node binary provides it via host calls. Ledger 7 is fully removed, so this node cannot decode or replay chain history predating the ledger 7 → 8 hardfork.
- **The chain must already be running the 1.0.3 runtime** (pending release) — that is the fork-from baseline this release is built and tested against. Forking from 1.0.2 or earlier is not the documented path; see [Other Changes](#other-changes).

**Downstream impact (cascading effects)**: the node's consumers must be rebuilt or re-pointed before they can follow a 2.1.0 chain.

- `transaction_version` 3 → 4: every client, SDK, and indexer must re-fetch metadata at or after the `set_code` block, and any pre-signed or queued extrinsic must be rebuilt.
- `CNightObservation` error indices are renumbered; tooling that maps `DispatchError::Module { index, error }` through a hardcoded table will mis-report errors.
- subxt-based consumers (`chain-indexer`) resolve metadata at the *finalized* head, so a client connecting while finality is still behind the fork picks up the old metadata and then fails against the new runtime — see the migration guide.
- The toolkit's block fetcher now recognises the 1.0.3 runtime (`001_000_003`); older toolkits reject those blocks with `UnsupportedBlockVersion`.

For all other interop questions, see the bundle dependency matrix.

## Deployment information

- **Upgrade scope**: binary + runtime — a coordinated on-chain runtime upgrade, not a new image alone.
- **Reset required**: No. One caveat: a toolkit block cache holding genuinely ledger-7 blocks is now rejected with a "delete the cache and re-fetch" error.
- **Governance action required**: Yes — the runtime is swapped by a governance `set_code`, held by the collective/technical signers. Enabling the C2M bridge additionally requires a governance action to set its `MainChainScripts` and data checkpoint; without it the bridge's inherent data provider stays `Inert`.
- **Downtime / coordination**: Coordinated. Roll every validator onto the new binary first (hot swap, old runtime keeps running), confirm the whole set is importing and finalizing, then enact `set_code`. Cardano observation is gated for the duration of the dust replay — roughly 28 blocks at mainnet scale, 9 at preview scale, 1 at preprod scale.

## Artifacts

- **Docker**: `midnightntwrk/midnight-node:2.1.0-beta.1` and `midnightntwrk/midnight-node-toolkit:2.1.0-beta.1`
- **Binaries**: `midnight-node-2.1.0-beta.1-linux-{amd64,arm64}.tar.gz`, `midnight-node-toolkit-2.1.0-beta.1-linux-{amd64,arm64}.tar.gz`, with `SHA256SUMS-amd64` / `SHA256SUMS-arm64`
- **Git tree hash**: `d87a3992d0d087418f78b456da31e0bb289c2e35`
- **Runtime WASM**: **not published** — the deterministic (srtool) build is blocked by [#2061](https://github.com/midnightntwrk/midnight-node/issues/2061). Extract the blob from the node image at `/artifacts-{amd64,arm64}/midnight_node_runtime.compact.compressed.wasm`; see [Known issues](#known-issues).

```shell
docker pull midnightntwrk/midnight-node:2.1.0-beta.1
docker pull midnightntwrk/midnight-node-toolkit:2.1.0-beta.1
```

## What changed

- cNIGHT dust generation is rebuilt after the ledger 8 → 9 hardfork wipes it, as a multi-block migration that gates Cardano observation while it runs.
- Ledger moves to a single 9.1.0.0-rc.4 workspace pin, and ledger 7 support is removed entirely.
- The cNIGHT observation sliding-window cache is reverted while its security hardening is finished.
- `block_stability_margin` defaults to 30 across every preset.
- Toolkit: contract e2e coverage (tic-tac-toe, welcome), resumable long ledger replays, and a wallet-cache GC retention floor.

| Change | Upgrade Type | PR |
| --- | --- | --- |
| Re-apply cNIGHT dust generation after the ledger 8 → 9 hardfork | Runtime upgrade | [#2012](https://github.com/midnightntwrk/midnight-node/pull/2012) |
| Bump ledger to 9.1.0.0-rc.4 | Node upgrade + Toolkit | [#2022](https://github.com/midnightntwrk/midnight-node/pull/2022) |
| Remove ledger 7 support | Node upgrade + Toolkit | [#1999](https://github.com/midnightntwrk/midnight-node/pull/1999) |
| Revert the cNIGHT observation sliding-window cache | Node upgrade | [#2030](https://github.com/midnightntwrk/midnight-node/pull/2030) |
| Default `block_stability_margin` to 30 across all config presets | Node upgrade | [#1914](https://github.com/midnightntwrk/midnight-node/pull/1914) |
| Toolkit: opt-in wallet-cache checkpoints during long ledger replays, plus snapshot GC retention floor | Toolkit | [#1968](https://github.com/midnightntwrk/midnight-node/pull/1968) |
| Add tic-tac-toe and welcome contract e2e tests | Toolkit | [#1940](https://github.com/midnightntwrk/midnight-node/pull/1940) |
| Rename `StandardTrasactionInfo` to `StandardTransactionInfo` | Toolkit | [#2016](https://github.com/midnightntwrk/midnight-node/pull/2016) |
| Runtime metadata diff (subwasm) — [runtime-diff.md](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/runtime-diff.md) | Runtime upgrade | [#2061](https://github.com/midnightntwrk/midnight-node/issues/2061) |

## New features

### Re-apply cNIGHT dust generation after the ledger 8 → 9 hardfork

**Description**: The v8 → v9 state translation drops the ledger's dust state and installs the empty one genesis starts from, which would silently stop DUST generation for every cNIGHT holder. `pallet-cnight-observation` now rebuilds its own slice of the generating set as a multi-block migration (storage version 1 → 2). `on_runtime_upgrade` saves the pre-fork ledger-8 arena root — the only place the wiped entries' night value and dust owner survive — and the migration pages through `UtxoOwners`, reads each nonce's pre-wipe value and owner through a new `dust_generation_values` host function, and re-applies them in batches of 25 as `CNightGeneratesDustUpdate` system transactions. Cardano observations are ignored while it runs, so `NextCardanoPosition` does not advance and the observer re-delivers the same UTXOs afterwards.

Each batch is priced before it is applied and applied only if it fits the remaining migration weight budget; a batch too large for a whole block's budget is given up on, emitting `DustReapplySkipped` and lifting the gate. Restoration runs at ~175 nonces/block; measured live sets on 2026-08-06 were 4870 (mainnet), 1524 (preview) and 85 (preprod) — about 28, 9 and 1 block of gated observation. Restored entries are field-for-field identical to the wiped ones except the accrual clock: the replay stamps `fork block time - dust.time_to_cap()` (~1 week), so every holder lands back at their cap rather than at zero.

Developers interact with it through five new `CNightObservation` events (`DustReapplyStarted`, `DustReapplyBatchFailed`, `DustReapplyCompleted`, `DustReapplySkipped`, `ObservationsSkippedForMigration`) and four new storage items. Note that only `DustReapplyStarted` and `ObservationsSkippedForMigration` appear in Polkadot.js Apps — the rest are deposited from a migration step whose phase no extrinsic owns, so read them via `System::Events`, subxt, or the node log.

**Only cNIGHT's slice is restored.** Native NIGHT registers generation entries too, and nothing in this repo records which of those the wipe took, so `DustReapplyCompleted` must not be read as "all dust generation restored". Native NIGHT holders must re-register their dust address — see the [migration guide](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/migration-hardfork-1.0.x.md#41-native-night-holders-must-re-register-their-dust-address). *Runtime upgrade.*

**PR**: [#2012](https://github.com/midnightntwrk/midnight-node/pull/2012)

### Resumable long ledger replays in the toolkit

**Description**: `generate-txs` — and every command sharing the `Source` args — accepts `--replay-checkpoint-interval <BLOCKS>` (env `MN_REPLAY_CHECKPOINT_INTERVAL`, default `0` = off). When set, the cached context builder saves a ledger snapshot plus per-wallet cache entries every N replayed blocks, so an interrupted full replay — tens of minutes on a long chain — resumes from the last checkpoint instead of starting over from genesis. Wallets cached beyond a checkpoint boundary are withheld from that replay chunk, and intermediate snapshots are collected by the existing reference-based GC as wallet heights advance past them. *Toolkit.*

**PR**: [#1968](https://github.com/midnightntwrk/midnight-node/pull/1968)

### Contract e2e coverage: tic-tac-toe and welcome

**Description**: Two contracts ported from `midnight-contracts` and driven end to end through the compile → prove → submit → on-chain-verify pipeline. Tic-tac-toe plays a full two-player game: deploy with X and O identities derived from private witness keys, alternate `make_move` (each move proves knowledge of the current player's secret while fees are paid by `FUNDING_SEED`), then assert the outcome via the `verify_game_state` and `verify_winner` circuits — exercising Map- and Counter-backed on-chain state and private-witness turn authorization independent of the public fee-paying wallet. Welcome drives deploy → `add_participant` → `check_in`, and adds a config template plus a `generate_intent_deploy_with_args` helper for constructor arguments.

This also fixes `prerequisites_ready` to match the compact variant directory by major.minor.patch rather than major.minor. Without that fix **all** contract e2e tests silently skipped; when `compact-contract-tests` is enabled, missing prerequisites now fail the test instead of reporting a successful skip. *Toolkit.*

**PR**: [#1940](https://github.com/midnightntwrk/midnight-node/pull/1940)

## New features requiring configuration updates

### Retired node config keys

Only relevant if you are coming from a **2.0.0 pre-release**. Neither key ever existed on the 1.0.x line, so operators on 1.0.3 have nothing to do here.

**Required updates**:

- Remove `cnight_observation_window_size` and `cnight_follower_genesis_from_storage` — the sliding-window observation cache they configured has been reverted ([#2030](https://github.com/midnightntwrk/midnight-node/pull/2030)).

**Impact**: The config loader ignores unrecognised keys, so a stale entry is a silent no-op rather than a startup error. Clean them out so they are not later mistaken for live settings.

### `block_stability_margin` default raised to 30

**Required updates**:

- No action if you rely on the preset default: `res/cfg/default.toml` (inherited by dev/devnet/govnet/guardnet/local/perfnet/preview/qanet/stagenet) and the `preprod` and `mainnet` presets now all specify 30, up from 10.
- If you pin the value via `BLOCK_STABILITY_MARGIN`, decide deliberately and coordinate with the rest of the validator set.

**Impact**: `block_stability_margin` adds to `cardano_security_parameter` when selecting the latest stable Cardano block a producer references (`tip − (k + margin)`). The on-chain effective margin is the minimum across all producers, so a uniform value is required for it to take effect network-wide; the mainnet value should only change via a deliberate, coordinated rollout ([#1914](https://github.com/midnightntwrk/midnight-node/pull/1914)).

## Improvements

### Ledger 9.1.0.0-rc.4

**Description**: Moves the midnight-ledger patch set from the per-crate 9.1.0.0-rc.3 tags to the single 9.1.0.0-rc.4 workspace tag, so every L9 crate is pinned to one commit instead of a mix of per-crate tags straddling several commits. Upstream's fix is dust registration accounting: a registration's initial DUST is now valued at block time rather than the transaction's declared `ctime`, and dust-generation uniqueness is keyed on the initial nonce alone rather than the generating set. `GenerationInfoAlreadyPresent` is renamed to `InitialNonceAlreadyPresent` upstream; the node-side `InvalidError` / `SystemTransactionError` variant names and their error codes (198 / 207) are unchanged, since it is the same condition and they are a stable runtime surface. Runtime metadata is not regenerated — no pallet storage item, extrinsic signature or runtime API changed. See [Breaking changes](#breaking-changes) for the regenerated proof fixtures. *Node upgrade + Toolkit.*

**PR**: [#2022](https://github.com/midnightntwrk/midnight-node/pull/2022)

### Toolkit wallet-cache: GC retention floor and cache observability

**Description**: The file-backend ledger-snapshot GC now always retains the newest two snapshots even when no cached wallet references their height. Previously a save whose per-wallet writes were skipped by `write_wallet_if_newer` left its just-written ledger snapshot unreferenced, and the next GC deleted exactly the snapshot the following warm start needed. `set_wallet_states` now reports skipped writes instead of counting them as saved, and the transaction builder logs a warning when uncached wallet seeds force the replay back to genesis — previously that fallback was silent, so a single stray seed caused a full-chain replay with no indication why. *Toolkit.*

**PR**: [#1968](https://github.com/midnightntwrk/midnight-node/pull/1968)

### `StandardTransactionInfo` spelling fixed

**Description**: Fixes the typo in `StandardTrasactionInfo` in `ledger/helpers` and all of its uses in the toolkit transaction builders. No behaviour change; a deprecated alias under the old spelling keeps existing callers building — see [Deprecations](#deprecations). *Toolkit.*

**PR**: [#2016](https://github.com/midnightntwrk/midnight-node/pull/2016)

## Deprecations

- **Deprecated item**: `StandardTrasactionInfo` (type alias in `ledger/helpers`)
- **Starts**: 2.1.0-beta.1
- **Full removal**: TBD — the next major release
- **Replacement**: `StandardTransactionInfo`
- **Migration steps**: Rename the type at each use site. The alias is a plain `#[deprecated]` re-export, so a compile with warnings enabled lists every occurrence; there is no behavioural or wire-format difference.

## Breaking changes

> **Warning**: this release is a runtime upgrade. `transaction_version` moves 3 → 4 and `spec_version` 1_000_003 → 2_001_000. Signed extrinsics encoded against a 1.0.x runtime will not decode. Review the [runtime metadata diff](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/runtime-diff.md) before enacting the upgrade.

### `transaction_version` bump

**What changed**: `transaction_version` 3 → 4 and `spec_version` 1_000_003 → 2_001_000 (`system_version` stays at 3, the value 1.0.3 already ships). One pallet is added (`C2MBridge`, index 33) and four change their error/event/storage surface; `CNightObservation` renumbers its error variants and renames storage `Mappings` → `Mapping`.

**What breaks**:

- Any extrinsic signed or encoded against the 1.0.x runtime — pre-signed, queued, or built from cached metadata — fails to decode after the `set_code`.
- A fresh subxt client resolves metadata at the *finalized* head, so while finality is still behind the fork it picks up the old metadata and then fails against the new runtime ([#1969](https://github.com/midnightntwrk/midnight-node/issues/1969)).
- Tooling that maps `DispatchError::Module { index, error }` through a hardcoded table rather than through metadata fetched at the block being decoded mis-reports `CNightObservation` errors: `MaxRegistrationsExceeded` is gone and every remaining variant shifted down one index.
- `Bridge`'s `Transfer` event and three `FederatedAuthority` errors (`MotionTooEarlyToClose`, `MotionAlreadyExists`, `MotionExpired`) are removed. `MidnightSystem`'s catch-all `LedgerApiError` is replaced by thirteen granular variants.

**Required actions**:

- Re-fetch metadata from a block at or after the `set_code`, or pin metadata explicitly; wait for finality to pass the boundary before relying on a fresh subxt client.
- Rebuild any pre-signed or queued extrinsic.
- Resolve error names from metadata at the decoded block, not from a compiled-in table.

**Code example**:

```rust
// Before: a client built once at startup, whose metadata is whatever the
// finalized head reported then.
let api = OnlineClient::<Cfg>::from_insecure_url(url).await?;

// After: for reads that straddle the fork, resolve the runtime version from
// the block itself rather than trusting the client's cached metadata.
let rpc = RpcClient::from_insecure_url(url).await?;
let version: serde_json::Value =
    rpc.request("state_getRuntimeVersion", rpc_params![block_hash]).await?;
// ... then build a client whose metadata matches `version["specVersion"]`.
```

### Ledger 7 support removed

**What changed**: Mainnet has always run ledger 8 onwards, so ledger 7 — a pre-mainnet protocol generation — is dead code. The `ledger_7` host bridge, storage, validation, error and system-tx modules are removed from the `ledger` and `ledger/helpers` crates, and all ledger-7 builders/commands from the toolkit. `ForkAwareLedgerContext::dispatch` now takes two closures instead of three, and `LedgerVersion` no longer has a `Ledger7` variant.

**What breaks**: This is a full purge, not a version-support drop. The node can no longer decode or replay chain history that predates the ledger 7 → 8 hardfork. Mainnet is unaffected (it launched post-hardfork), but devnet/testnet archives from before that hardfork are no longer replayable by the toolkit. A cached block that really is ledger 7 is now rejected with an explicit "delete the cache and re-fetch" error rather than silently replaying as ledger 8.

**Required actions**:

- Discard any toolkit block cache (redb or PostgreSQL) that may hold pre-hardfork blocks; existing caches whose contents are ledger 8 or 9 keep decoding, since `LedgerVersion` now carries explicit wire discriminants (`Ledger8 = 1`, `Ledger9 = 2`) with hand-written serde impls so dropping `Ledger7` does not renumber the postcard encoding.
- Keep a 1.0.x toolkit around if you need to replay a pre-hardfork archive.

**PR**: [#1999](https://github.com/midnightntwrk/midnight-node/pull/1999)

### Dust spend proofs built against ledger 9.1.0.0-rc.3 no longer verify

**What changed**: rc.4's dust spend circuit changed, so the `spend.zkir` / `spend.verifier` artefacts are new.

**What breaks**: Every committed `.mn` fixture carrying a dust spend fails `well_formed` with `InvalidDustSpendProof` under rc.4. In-repo fixtures were rebuilt via `earthly -P +rebuild-genesis-state-undeployed`: undeployed genesis, the derived test transactions, and the toolkit's counter/mint contract fixtures. The undeployed contract address is unchanged, and the deployed networks' genesis and chainspecs are unchanged.

**Required actions**: Regenerate any locally held dust-spend proof or `.mn` fixture built against rc.3. Nothing on a deployed chain is affected.

**PR**: [#2022](https://github.com/midnightntwrk/midnight-node/pull/2022)

### Native NIGHT holders lose DUST generation across the fork

**What changed**: The v8 → v9 translation wipes the ledger's dust state, and the replay restores only cNIGHT's slice of the generating set.

**What breaks**: A wallet holding native NIGHT crosses the fork still holding its NIGHT but generating no DUST — and with no DUST it cannot pay a fee.

**Required actions**: Re-register the dust address. The registration funds itself from the retroactive DUST the now-generationless NIGHT accrued, and generation restarts from that block's time, so allow a couple of blocks before attempting a fee-paying transaction.

**Code example**:

```shell
midnight-node-toolkit generate-txs \
  --fetch-cache inmemory \
  register-dust-address \
  --wallet-seed <seed> \
  -s ws://<node>:9944 -d ws://<node>:9944
```

## Known issues

### No runtime WASM asset on this release

**Description**: The deterministic (srtool) runtime build fails, so this release publishes no `*.wasm` asset. The srtool image is pinned to Rust 1.93.0 while the repo builds and tests on Rust 1.95.0, and no published srtool image for 1.95.0 exists — it may have to be built in-house. A deterministic WASM is a requirement for a final release, so this blocks 2.1.0 final.

Workaround: extract the blob from the node image at `/artifacts-{amd64,arm64}/midnight_node_runtime.compact.compressed.wasm`. Its hash is not independently reproducible, so for a governance action that needs a verifiable artefact, wait for a release that publishes one.

**Issue**: [#2061](https://github.com/midnightntwrk/midnight-node/issues/2061)

**Workaround (if any)**: See above — extract from the docker image for testing; do not treat the extracted blob as a reproducible artefact.

### Sync from genesis is slower again

**Description**: Reverting the cNIGHT observation sliding-window cache while its security hardening is finished puts the follower back on the per-call db-sync path, so sync from genesis is slower again. In practice this reopens the sync-performance issue the cache was written to close. Two levers bundled into the same original PR are deliberately kept, since neither touches the cNIGHT query path: the autovacuum tuning on the db-sync hot tables, and the default `storage_cache_size` of 100000.

**Issue**: [#1158](https://github.com/midnightntwrk/midnight-node/issues/1158)

**Workaround (if any)**: None. Restore from a database snapshot rather than syncing from genesis where that option exists.

## Links and references

- **PRs**: [#2012](https://github.com/midnightntwrk/midnight-node/pull/2012), [#1985](https://github.com/midnightntwrk/midnight-node/pull/1985), [#2054](https://github.com/midnightntwrk/midnight-node/pull/2054), [#2022](https://github.com/midnightntwrk/midnight-node/pull/2022), [#1999](https://github.com/midnightntwrk/midnight-node/pull/1999), [#2030](https://github.com/midnightntwrk/midnight-node/pull/2030), [#1914](https://github.com/midnightntwrk/midnight-node/pull/1914), [#1968](https://github.com/midnightntwrk/midnight-node/pull/1968), [#1940](https://github.com/midnightntwrk/midnight-node/pull/1940), [#2016](https://github.com/midnightntwrk/midnight-node/pull/2016)
- **Engineering docs**: [Runtime diff 1.0.3 → 2.1.0-beta.1](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/runtime-diff.md), [Ledger 9 and the C2M bridge](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.0.0-alpha.1/eng-ledger9-c2m-bridge.md), [Storage separation config](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.0.0-alpha.1/config-storage-separation.md), [2.0.0-alpha.1 release notes](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.0.0-alpha.1/release-notes-2.0.0-alpha.1.md)
- **Migration guides**: [Hard-forking a 1.0.x chain to 2.1.0-beta.1](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/migration-hardfork-1.0.x.md)
- **SDK docs**: [Clients, SDKs, and indexers must re-fetch metadata](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/migration-hardfork-1.0.x.md#42-clients-sdks-and-indexers-must-re-fetch-metadata)
- **Known issues board**: [open bugs](https://github.com/midnightntwrk/midnight-node/issues?q=is%3Aissue+is%3Aopen+label%3Abug)
- **GitHub release**: [node-2.1.0-beta.1](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.1.0-beta.1)
- **Prior 2.x line**: [node-2.0.0-rc.4](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.0.0-rc.4)

## Fixed defect list

| Defect number | Description |
| --- | --- |
| [#1970](https://github.com/midnightntwrk/midnight-node/issues/1970) | Interrupted long ledger replays restarted from genesis, and wallet-cache GC deleted the snapshot the next warm start needed |
| [shieldedtech/shielded-security-engineering#548](https://github.com/shieldedtech/shielded-security-engineering/issues/548), [#549](https://github.com/shieldedtech/shielded-security-engineering/issues/549), [#550](https://github.com/shieldedtech/shielded-security-engineering/issues/550) | DUST generation would silently stop for every cNIGHT holder after the ledger 8 → 9 hardfork (internal tracker) |
| [shieldedtech/shielded-sre#424](https://github.com/shieldedtech/shielded-sre/issues/424) | `block_stability_margin` default was too low for a conservative Cardano-reference lag (internal tracker) |

Reopened in practice by [#2030](https://github.com/midnightntwrk/midnight-node/pull/2030): [#1158](https://github.com/midnightntwrk/midnight-node/issues/1158) — see [Known issues](#known-issues).

## Other Changes

- Use the upstream ledger v8 → v9 state translation crate ([#2054](https://github.com/midnightntwrk/midnight-node/pull/2054)) — internal hardfork plumbing. The in-repo copy of the translation table is replaced by a git dependency on the ledger team's own crate, so ledger-side fixes land without a manual re-port and the node stays aligned with the indexer. No behaviour change; the table was never a published surface. Closes [#2049](https://github.com/midnightntwrk/midnight-node/issues/2049).
- Serve ledger state reads at the ledger-hardfork `set_code` block ([#1985](https://github.com/midnightntwrk/midnight-node/pull/1985)) — the ledger-9 read-only host accessors detect a ledger-8 arena root from the `state_key`'s tagged-serialization header and serve the read from the ledger-8 bridge. This only matters for a chain that crosses the fork under `system_version: 1`, i.e. one forking from 1.0.2 or earlier; on the 1.0.3 baseline the applying block is not a skew block and the dispatch never triggers. Fixes [#1959](https://github.com/midnightntwrk/midnight-node/issues/1959).
