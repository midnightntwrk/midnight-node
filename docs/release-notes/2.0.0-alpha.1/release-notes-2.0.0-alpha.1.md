<!-- markdownlint-disable MD012 MD013 MD014 MD022 MD031 MD032 MD033 MD034 MD060 -->
# Midnight Node 2.0.0-alpha.1

## Metadata

- **Type of release**: major (alpha pre-release)
- **Date**: 2026-06-03
- **Ships in bundle**: TBD — alpha pre-release (not bundled)
- **Git tag**: [node-2.0.0-alpha.1](https://github.com/midnightntwrk/midnight-node/tree/node-2.0.0-alpha.1)
- **Environment**: All public networks at time of release. For the full compatibility matrix, see the [release notes overview](https://docs.midnight.network/relnotes/overview).
- **Upgrade scope**: binary + runtime (`spec_version` 1_000_000 → 2_000_000)
- **Reset required**: Yes — Ledger 9 currently has no state transition from ledger 8; a fresh chain is required
- **Governance action required**: No for this alpha (deployed as a fresh chain). Later RCs will require an on-chain runtime upgrade once state-migration work is complete; enabling the Cardano→Midnight bridge also requires a governance action (the bridge is inert in all environments here)

## High-level summary

First alpha of the Midnight Node 2.0.0 line. It moves the ledger to **version 9**, lands the (still-inert) **Cardano→Midnight bridge** pallet plus toolkit commands, removes unused partner-chains pallets, and bumps the Rust toolchain to 1.95 — alongside a batch of Least Authority audit-hardening fixes across the node and toolkit. This is a **runtime upgrade** (`spec_version` 1_000_000 → 2_000_000, `transaction_version` 3 → 4) and a binary upgrade. **There is currently no migration from ledger 8 — 2.0.0 chains must start fresh.**

## Audience

These release notes are intended for:

- [ ] Operators who run a node on any network and want to evaluate the 2.0.0 line — a fresh-chain start is mandatory.
- [ ] Developers who build and sign extrinsics — `transaction_version` bumped 3 → 4, so extrinsics signed against the 1.0.0 runtime no longer decode.
- [ ] Toolkit users who generate transactions, manage wallets, or rehearse the Cardano→Midnight bridge flow.
- [ ] Integrators tracking the Cardano→Midnight bridge surface — it ships inert in this alpha.

## Dependencies

- **Ledger**: 9 (`midnight-ledger-v9` 0.1.0). No v8 → v9 migration path; incompatible with ledger-8 chain state.
- **CompactC**: the toolkit defaults to 0.31.0.
- **`transaction_version` 4**: signed extrinsics built against the 1.0.0 runtime (tx version 3) will not decode.

**Downstream impact (cascading effects)**: indexers, wallets, and SDKs that decode runtime metadata or ledger-8 state must be rebuilt against the 2.0.0 metadata / ledger 9 before they can follow a 2.0.0 chain. The C2M bridge is inert, so no downstream bridge integration is active yet.

For all other interop questions, see the bundle dependency matrix.

## Deployment information

- **Upgrade scope**: binary + runtime (`spec_version` 1_000_000 → 2_000_000, `transaction_version` 3 → 4).
- **Reset required**: Yes — Ledger 9 currently has no state transition from ledger 8. An existing ledger-8 chain cannot be upgraded in place; start a fresh chain. Mixed pre-v9/post-v9 handling in block replay and toolkit caching is deliberately relaxed and unsupported in this alpha.
- **Governance action required**: No for this alpha — it deploys as a fresh chain, not an on-chain upgrade. **Later RCs will require an on-chain runtime upgrade** once state-migration (ledger 8 → 9) work is complete, at which point existing chains roll forward instead of resetting. Separately, enabling the C2M bridge requires a governance action (setting the bridge `MainChainScripts` addresses + a data checkpoint); until then the bridge inherent-data provider reports `Inert` everywhere.
- **Downtime / coordination**: Not an in-place upgrade — existing chains cannot roll forward. New networks bootstrap from fresh genesis.

## Artifacts

- **Docker**: `midnightntwrk/midnight-node:2.0.0-alpha.1`
- **Docker**: `midnightntwrk/midnight-node-toolkit:2.0.0-alpha.1`
- **Runtime WASM**: `midnight_node_runtime-2.0.0-alpha.1.compact.compressed.wasm` (release asset)
- **Git tree hash**: `04caf1b8210b4633c22ace70ae8046ba49b2a1a3`

```shell
docker pull midnightntwrk/midnight-node:2.0.0-alpha.1
docker pull midnightntwrk/midnight-node-toolkit:2.0.0-alpha.1
```

## What changed

- **Ledger 9** — new ledger major; no migration from ledger 8, so 2.0.0 chains start fresh (#1604).
- **Cardano→Midnight bridge** (inert) — new `c2m-bridge` pallet, genesis/runtime config, transfer classification, pre-approvals, subminimal-transfer holds, and a toolkit `bridge-transfer` command (#1386, #1333, #1513, #1477, #1393, #1608, #1340).
- **Removed unused partner-chains pallets** and their CLI commands (#1562).
- **`SessionInfoApi`** runtime API exposing the substrate session index (#1534).
- **`storage_separation`** config option — optionally unify Midnight ledger + Substrate storage into one ParityDb instance (#1278).
- **Rust 1.95** toolchain (#1363); toolkit **CompactC default 0.31.0** (#1555).
- Granular ledger error variants surfaced through pallet + RPC error reporting (#1449, #1475, #916, #1359).
- A batch of **Least Authority audit-hardening** fixes across node and toolkit.

| Change | Upgrade Type | PR |
| ------ | ------------ | -- |
| Ledger 9 support | Runtime upgrade | [#1604](https://github.com/midnightntwrk/midnight-node/pull/1604) |
| Added `c2m-bridge` pallet | Runtime upgrade | [#1386](https://github.com/midnightntwrk/midnight-node/pull/1386) |
| C2M bridge: genesis / runtime configuration | Runtime upgrade | [#1333](https://github.com/midnightntwrk/midnight-node/pull/1333) |
| C2M bridge: Reserve Transfer classification | Runtime upgrade | [#1513](https://github.com/midnightntwrk/midnight-node/pull/1513) |
| C2M bridge: pre-approvals filter (→ treasury + `UnapprovedTransfer`) | Runtime upgrade | [#1477](https://github.com/midnightntwrk/midnight-node/pull/1477) |
| C2M bridge: hold + flush subminimal transfers | Runtime upgrade | [#1393](https://github.com/midnightntwrk/midnight-node/pull/1393) |
| C2M bridge: STAR denomination fix | Runtime upgrade | [#1608](https://github.com/midnightntwrk/midnight-node/pull/1608) |
| Remove unused partner-chains pallets + commands | Runtime upgrade | [#1562](https://github.com/midnightntwrk/midnight-node/pull/1562) |
| `SessionInfoApi` runtime API | Runtime upgrade | [#1534](https://github.com/midnightntwrk/midnight-node/pull/1534) |
| cnight-observation: panic → typed error on inherent decode | Runtime upgrade | [#1234](https://github.com/midnightntwrk/midnight-node/pull/1234) |
| federated-authority: keep motion removal on failed dispatch | Runtime upgrade | [#938](https://github.com/midnightntwrk/midnight-node/pull/938) |
| cNIGHT observation UTXO capacity −16× (API v2 gated) | Runtime upgrade | [#1367](https://github.com/midnightntwrk/midnight-node/pull/1367) |
| cNight observation: bounded storage (unbounded-alloc fix) | Runtime upgrade | [#1423](https://github.com/midnightntwrk/midnight-node/pull/1423) |
| Throttle `AccountUsage` storage-migration refactor | Runtime upgrade | [#1526](https://github.com/midnightntwrk/midnight-node/pull/1526) |
| Granular ledger error variants in pallet error reporting | Runtime upgrade | [#1449](https://github.com/midnightntwrk/midnight-node/pull/1449) |
| Session-change error log when D-parameter below permissioned count | Runtime upgrade | [#1506](https://github.com/midnightntwrk/midnight-node/pull/1506) |
| cnight-observation genesis panic diagnostics | Runtime upgrade | [#1466](https://github.com/midnightntwrk/midnight-node/pull/1466) |
| Update Rust toolchain to 1.95 | Runtime upgrade (mixed) | [#1363](https://github.com/midnightntwrk/midnight-node/pull/1363) |
| Bound GRANDPA + BEEFY finality subscription fan-out | Node upgrade | [#1075](https://github.com/midnightntwrk/midnight-node/pull/1075) |
| Enforce TLS cert + hostname validation for DB connections | Node upgrade | [#1104](https://github.com/midnightntwrk/midnight-node/pull/1104) |
| Verify removal of `WalletSeed` Default implementation | Node upgrade | [#1109](https://github.com/midnightntwrk/midnight-node/pull/1109) |
| Return `ContractNotPresent` for missing contracts | Node upgrade | [#916](https://github.com/midnightntwrk/midnight-node/pull/916) |
| Surface `ContractNotPresent` through `midnight_contractState` RPC | Node upgrade | [#1475](https://github.com/midnightntwrk/midnight-node/pull/1475) |
| Return `BeneficiaryNotFound` in `get_unclaimed_amount` | Node upgrade | [#1359](https://github.com/midnightntwrk/midnight-node/pull/1359) |
| Surface nested ledger error variants in flat error enums | Node upgrade | [#1449](https://github.com/midnightntwrk/midnight-node/pull/1449) |
| Per-session validator committee-membership log | Node upgrade | [#1534](https://github.com/midnightntwrk/midnight-node/pull/1534) |
| Reject block headers with duplicate mainchain-ref-hash digests | Node upgrade | [#1617](https://github.com/midnightntwrk/midnight-node/pull/1617) |
| Complete zeroization of secret buffers | Node upgrade | [#1379](https://github.com/midnightntwrk/midnight-node/pull/1379) |
| Default `unsafe_allow_symlinks` to `false` when missing | Node upgrade | [#1600](https://github.com/midnightntwrk/midnight-node/pull/1600) |
| parity-db: Midnight fork (lower flush threshold, ahash) | Node upgrade | [#1478](https://github.com/midnightntwrk/midnight-node/pull/1478) |
| Silence cNIGHT observation noise logs | Node upgrade | [#1324](https://github.com/midnightntwrk/midnight-node/pull/1324) |
| Run hardware benchmarks on node startup | Node upgrade | [#1394](https://github.com/midnightntwrk/midnight-node/pull/1394) |
| Midnight-specific reference hardware profile | Node upgrade | [#1511](https://github.com/midnightntwrk/midnight-node/pull/1511) |
| Log sanitized db-sync startup probe results | Node upgrade | [#1411](https://github.com/midnightntwrk/midnight-node/pull/1411) |
| Tune autovacuum on db-sync hot tables | Node upgrade | [#1434](https://github.com/midnightntwrk/midnight-node/pull/1434) |
| Eliminate deadlock in `LedgerContext::with_wallets_from_seeds` | Node upgrade | [#1471](https://github.com/midnightntwrk/midnight-node/pull/1471) |
| New `storage_separation` config option | Node upgrade | [#1278](https://github.com/midnightntwrk/midnight-node/pull/1278) |
| Toolkit `bridge-transfer` command | Toolkit | [#1340](https://github.com/midnightntwrk/midnight-node/pull/1340) |
| Enforce derivation-path role validation in wallet constructors | Toolkit | [#1076](https://github.com/midnightntwrk/midnight-node/pull/1076) |
| Improve wallet seed / keypair / address code quality | Toolkit | [#1217](https://github.com/midnightntwrk/midnight-node/pull/1217) |
| `--coin-selection` flag on coin-selecting commands | Toolkit | [#1457](https://github.com/midnightntwrk/midnight-node/pull/1457) |
| Batched `dust_balance::execute_many` for cache warmup | Toolkit | [#1603](https://github.com/midnightntwrk/midnight-node/pull/1603) |
| `--input-utxo` pinning for `generate-txs single-tx` | Toolkit | [#1404](https://github.com/midnightntwrk/midnight-node/pull/1404) |
| `--print-system-tx-hex` for `update-ledger-parameters` | Toolkit | [#1473](https://github.com/midnightntwrk/midnight-node/pull/1473) |
| Harden coin-selection arithmetic with checked operations | Toolkit | [#1293](https://github.com/midnightntwrk/midnight-node/pull/1293) |
| Enforce EOF on untagged CLI parser path (ADR-0022) | Toolkit | [#1437](https://github.com/midnightntwrk/midnight-node/pull/1437) |
| Abstract transaction builders over a `BuilderContext` trait | Toolkit | [#1605](https://github.com/midnightntwrk/midnight-node/pull/1605) |
| Fix dust-balance snapshot tagged at `block_height = 0` under `dust_warp` | Toolkit | [#1574](https://github.com/midnightntwrk/midnight-node/pull/1574) |
| Lock redb fetch cache against concurrent toolkit processes | Toolkit | [#1493](https://github.com/midnightntwrk/midnight-node/pull/1493) |
| Terminal-status sender error handling | Toolkit | [#1323](https://github.com/midnightntwrk/midnight-node/pull/1323) |
| Dispatch toolkit-js variants by `compactc` version | Toolkit | [#1555](https://github.com/midnightntwrk/midnight-node/pull/1555) |
| Fix stack overflow in `trusted_deserialize_tagged` | Toolkit | [#1576](https://github.com/midnightntwrk/midnight-node/pull/1576) |
| Update default CompactC version to 0.31.0 | Toolkit | [#1555](https://github.com/midnightntwrk/midnight-node/pull/1555) |
| Initial CI for fork testing | Infrastructure | [#1353](https://github.com/midnightntwrk/midnight-node/pull/1353) |
| local-files secrets mode for mock authorities | Infrastructure | [#1287](https://github.com/midnightntwrk/midnight-node/pull/1287) |
| Remove Kubernetes + AWS coupling from local-environment | Infrastructure | [#1470](https://github.com/midnightntwrk/midnight-node/pull/1470) |
| Remove local-env + e2e-tests in partner-chains | Infrastructure | [#1351](https://github.com/midnightntwrk/midnight-node/pull/1351) |
| e2e regression coverage for `genesis_extrinsics` parsing | Infrastructure | [#1516](https://github.com/midnightntwrk/midnight-node/pull/1516) |
| Per-test tracing logger for the e2e suite | Infrastructure | [#1564](https://github.com/midnightntwrk/midnight-node/pull/1564) |
| Local fork-testing for the 1.0.0 release train | Infrastructure | [#1522](https://github.com/midnightntwrk/midnight-node/pull/1522) |
| Read e2e contract values from runtime-values | Infrastructure | [#1348](https://github.com/midnightntwrk/midnight-node/pull/1348) |
| Split e2e suite into per-topic module files | Infrastructure | [#1565](https://github.com/midnightntwrk/midnight-node/pull/1565) |
| Remove unused/outdated ddosnet network | Infrastructure | [#1343](https://github.com/midnightntwrk/midnight-node/pull/1343) |

## New features

### Ledger 9 support

**Description**: Moves the node, runtime, and toolkit onto ledger version 9 (`midnight-ledger-v9` 0.1.0). New chains can run ledger 9 on the local environment. There is **no state transition from ledger 8** — an existing ledger-8 chain cannot be hard-forked or migrated to ledger 9 in this alpha, so 2.0.0 chains must start fresh; mixed pre-v9/post-v9 handling in block replay and toolkit caching is deliberately relaxed and unsupported. The ledger-v8 `construct_distribute_treasury_system_tx` (only ever invoked, incorrectly, from the inert c2m-bridge pallet) is removed. **Why ledger 9, not 8.1:** 8.1 was a minor (storage stability + wallet WASM bindings); ledger 9 supplies the primitives 2.0.0 is built around — the `UnlockToTreasury` system transaction (locked pool → treasury), ECDSA signatures for the Cardano bridge, and an explicit governed fee price floor — plus split-phase execution and panic/cost-heuristic hardening. See [what the Ledger 9 upgrade unlocks](eng-ledger9-c2m-bridge.md#what-the-ledger-9-upgrade-unlocks-vs-ledger-81) for the itemised list with ledger PR links. _Runtime upgrade._

**PR**: [#1604](https://github.com/midnightntwrk/midnight-node/pull/1604)

**Reference**: ledger [9.0.1.0-alpha.1 changelog](https://github.com/midnightntwrk/midnight-ledger/blob/crate-ledger-9.0.1.0-alpha.1/CHANGELOG.md) · [ledger 8.1.0 release](https://github.com/midnightntwrk/midnight-ledger/releases/tag/ledger-8.1.0)

### Cardano→Midnight bridge (inert)

**Description**: Introduces the `c2m-bridge` pallet, a stateful `TransferHandler` built on the partner-chains bridge pallet that holds the Midnight-specific bridge logic: it classifies Cardano transfers by distinguishing Reserve Validator and ICS Validator inputs (closing a metadata-spoofing attack on the M.R pool), redirects unapproved user transfers to the treasury while emitting `UnapprovedTransfer`, accumulates subminimal transfers and flushes them as one once a configured threshold is met, and treats amounts as STAR end-to-end (no NIGHT denomination conversion). Governance dispatchables `set_subminimal_transfers_config` and `add_approved_mc_tx_hashes` configure it. A toolkit `bridge-transfer` command submits the corresponding Cardano transaction. **The bridge ships inert** — its inherent-data provider reports `Inert` in every environment, and a governance action (setting `MainChainScripts` addresses + a data checkpoint) is required to enable it. Full surface (pallet extrinsics/events/storage, `bridge-transfer` flags, `SessionInfoApi`) is documented in the [engineering notes](eng-ledger9-c2m-bridge.md). _Runtime upgrade (+ Toolkit)._

**PR**: [#1386](https://github.com/midnightntwrk/midnight-node/pull/1386), [#1333](https://github.com/midnightntwrk/midnight-node/pull/1333), [#1513](https://github.com/midnightntwrk/midnight-node/pull/1513), [#1477](https://github.com/midnightntwrk/midnight-node/pull/1477), [#1393](https://github.com/midnightntwrk/midnight-node/pull/1393), [#1608](https://github.com/midnightntwrk/midnight-node/pull/1608), [#1340](https://github.com/midnightntwrk/midnight-node/pull/1340)

### `SessionInfoApi` runtime API

**Description**: A new `midnight-primitives-session-info::SessionInfoApi` runtime API exposes `current_session_index() -> u32`, backed by `pallet_partner_chains_session::Pallet::current_index()`. Node-side code can now read the substrate session index through a typed runtime API instead of reaching into pallet storage directly. Requires a metadata rebuild. _Runtime upgrade._

**PR**: [#1534](https://github.com/midnightntwrk/midnight-node/pull/1534)

### Toolkit transaction-generation controls

**Description**: Adds operator-facing controls to the transaction generator: `--coin-selection <largest-first|smallest-first>` orders coin/UTXO selection (`largest-first` default minimises inputs; `smallest-first` consolidates dust); `--input-utxo <intent_hash>#<n>` pins exact UTXOs as inputs to `generate-txs single-tx`, bypassing greedy selection; `--print-system-tx-hex` builds and prints the `update-ledger-parameters` system-transaction payload without submitting (no council/TC keys needed, useful for manual governance flows); and `dust_balance::execute_many` warms the wallet cache for many seeds in a single shared block replay instead of one replay per seed. _Toolkit._

**PR**: [#1457](https://github.com/midnightntwrk/midnight-node/pull/1457), [#1404](https://github.com/midnightntwrk/midnight-node/pull/1404), [#1473](https://github.com/midnightntwrk/midnight-node/pull/1473), [#1603](https://github.com/midnightntwrk/midnight-node/pull/1603)

## New features requiring configuration updates

### `storage_separation` config option

**Required updates**:

- Set `storage_separation = "separate"` (default) or `"unified"` in node config (TOML key, or the `STORAGE_SEPARATION` environment variable). There is no CLI flag.
- Choose the value **at chain initialisation** — it cannot be changed on an existing database.

**Impact**: `unified` stores Midnight ledger and Substrate items in a single ParityDb instance, reducing the chance of cross-instance data-integrity errors on unexpected process termination. Switching modes against a populated database fails at open with `IncompatibleColumnConfig`; the node prints a clear error and you must delete the chain-data directory and resync. See the [storage_separation operator guide](config-storage-separation.md).

**PR**: [#1278](https://github.com/midnightntwrk/midnight-node/pull/1278)

## Improvements

- **cNIGHT sync performance**: lower the per-transaction UTXO overestimate from 64× to 4× (runtime-API-v2 gated so validators can't diverge mid-rollout) (#1367); switch the workspace `parity-db` to the Midnight fork with a lower background-flush threshold and `ahash` (#1478); tune `autovacuum_analyze_scale_factor` (0.1 → 0.01) on hot db-sync tables to avoid >400s worst-case query plans (#1434).
- **Observability**: per-session log of whether the local AURA key is in the active committee (#1534); error log when the D-parameter is below the permissioned candidate count (#1506); sanitized db-sync startup probe timings (#1411); hardware benchmarks on startup against a Midnight reference profile, with `--no-hardware-benchmarks` to opt out (#1394, #1511); actionable cnight-observation genesis-failure diagnostics naming the offending chain-spec field (#1466).
- **Log hygiene**: demote per-UTXO cNIGHT "no registration" / "no create event" warnings to `trace` and drop no-signal address debug lines (#1324).
- **Error reporting**: granular inner-cause variants for `InvalidError` / `MalformedError` / `SystemTransactionError` (stable codes 212–250, metadata rebuild required) (#1449); `ContractNotPresent` distinguishable from empty state, end-to-end through the `midnight_contractState` RPC (#916, #1475); `BeneficiaryNotFound` vs zero unclaimed reward (#1359).
- **Block validation**: reject headers carrying more than one `mcsh` pre-runtime digest, mirroring `sc-consensus-aura` (#1617).
- **Toolkit robustness**: advisory lock on the redb fetch cache to stop concurrent-process corruption (#1493); `stacker`-based fix for stack overflow in `trusted_deserialize_tagged` on long chains (#1576); terminal-status sender error handling (#1323); `BuilderContext` trait so builders no longer require a full local chain replay (#1605); dispatch toolkit-js by `compactc` version rather than ledger version (#1555).

## Deprecations

None.

## Breaking changes

> ⚠️ This is a major release: `spec_version` 1_000_000 → 2_000_000, `transaction_version` 3 → 4, and ledger 8 → 9 with **no migration path**. Existing chains cannot roll forward — start fresh.

### `transaction_version` bumped 3 → 4

**What changed**: The runtime transaction version moved from 3 (node 1.0.0) to 4.

**What breaks**: Signed extrinsics constructed against the 1.0.0 runtime metadata will no longer decode/validate. Any tooling, SDK, or service that builds and signs extrinsics must refresh its metadata.

**Required actions**:

- Rebuild runtime metadata against 2.0.0-alpha.1 and regenerate any codegen/types derived from it.
- Re-sign or re-construct pending extrinsics with the new metadata.
- Pin integrators to the 2.0.0 metadata before pointing them at a 2.0.0 chain.

### Ledger 9 — no migration from ledger 8

**What changed**: The ledger moves to version 9; this alpha ships **no v8 → v9 state transition or hard-fork**.

**What breaks**: An existing ledger-8 chain cannot be upgraded in place. Block replay and toolkit caching across the v8/v9 boundary are unsupported.

**Required actions**:

- Start a fresh ledger-9 chain from genesis.
- Do not attempt to point ledger-8 state, snapshots, or caches at a 2.0.0 binary.

### Partner-chains pallets removed

**What changed**: Partner-chains pallets not used by Midnight, and their related CLI commands, were removed (#1562).

**What breaks**: Any caller of those pallets' extrinsics or the removed CLI commands. Their storage no longer exists in the runtime.

**Required actions**:

- Drop usage of the removed extrinsics/commands. As this requires a fresh chain anyway, no storage migration is involved.

## Known issues

### No ledger 8 → 9 state transition

**Description**: 2.0.0-alpha.1 runs ledger 9 but ships no migration or hard-fork path from ledger 8. An existing ledger-8 chain cannot be upgraded in place — only a fresh ledger-9 chain can be started. Mixed pre-v9/post-v9 handling in block replay and toolkit caching is deliberately relaxed and unsupported in this alpha; tests requiring `intent[v7]` or a v8 → v9 hard-fork are ignored.

**Issue**: [#1579](https://github.com/midnightntwrk/midnight-node/issues/1579)

**Workaround (if any)**: Start a fresh chain. Migration / hard-fork support is planned for a later 2.0.0 pre-release.

## Links and references

- **PRs**: full set on the [GitHub release page](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.0.0-alpha.1); every change above links its PR inline.
- **Engineering docs**:
  - [Ledger 9 and the Cardano→Midnight bridge](eng-ledger9-c2m-bridge.md)
  - [storage_separation operator guide](config-storage-separation.md)
- **Migration guides**: not produced for this alpha (a fresh chain is required; a partner-chains / `transaction_version` migration guide will accompany a later pre-release).
- **API documentation**: new `SessionInfoApi` (`current_session_index() -> u32`) and the `midnight_contractState` RPC now surfacing `ContractNotPresent` — see the engineering notes.
- **GitHub release**: <https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.0.0-alpha.1>
- **Known issues board**: <https://github.com/midnightntwrk/midnight-node/issues?q=is%3Aissue+is%3Aopen+label%3Abug>

## Fixed defect list

| Defect number | Description |
| ------------- | ----------- |
| [PM-19967](https://shielded.atlassian.net/browse/PM-19967) | Unbounded GRANDPA/BEEFY finality subscription fan-out could exhaust node resources ([#1075](https://github.com/midnightntwrk/midnight-node/pull/1075)) |
| [PM-20015](https://shielded.atlassian.net/browse/PM-20015) | Wallet constructors accepted derivation paths with mismatched roles ([#1076](https://github.com/midnightntwrk/midnight-node/pull/1076)) |
| [PM-22023](https://shielded.atlassian.net/browse/PM-22023) | DB connections allowed insecure SSL modes / unverified TLS ([#1104](https://github.com/midnightntwrk/midnight-node/pull/1104)) |
| [PM-22024](https://shielded.atlassian.net/browse/PM-22024) | Residual all-zero `WalletSeed` Default needed removal verification ([#1109](https://github.com/midnightntwrk/midnight-node/pull/1109)) |
| [PM-21799](https://shielded.atlassian.net/browse/PM-21799) | Malformed cnight-observation inherent data could panic all validators and halt the chain ([#1234](https://github.com/midnightntwrk/midnight-node/pull/1234)) |
| [PM-22085](https://shielded.atlassian.net/browse/PM-22085) | Approved-but-failed federated-authority motions became permanently stuck ([#938](https://github.com/midnightntwrk/midnight-node/pull/938)) |
| [PM-21800](https://shielded.atlassian.net/browse/PM-21800) | Deadlock in `LedgerContext::with_wallets_from_seeds` (reentrant mutex) ([#1471](https://github.com/midnightntwrk/midnight-node/pull/1471)) |
| [PM-21801](https://shielded.atlassian.net/browse/PM-21801) | `get_unclaimed_amount` returned `Ok(0)` for absent beneficiaries ([#1359](https://github.com/midnightntwrk/midnight-node/pull/1359)) |
| [PM-19896](https://shielded.atlassian.net/browse/PM-19896) | cnight-observation genesis panics gave no actionable diagnostics ([#1466](https://github.com/midnightntwrk/midnight-node/pull/1466)) |
| [PM-22034](https://shielded.atlassian.net/browse/PM-22034) | Incomplete zeroization of secret buffers after conversion ([#1379](https://github.com/midnightntwrk/midnight-node/pull/1379)) |
| [PM-22018](https://shielded.atlassian.net/browse/PM-22018) | Unchecked arithmetic in coin selection could overflow/panic ([#1293](https://github.com/midnightntwrk/midnight-node/pull/1293)) |
| [PM-22028](https://shielded.atlassian.net/browse/PM-22028) | Untagged CLI parser accepted trailing bytes (silent-fallback ambiguity) ([#1437](https://github.com/midnightntwrk/midnight-node/pull/1437)) |
| [PM-22038](https://shielded.atlassian.net/browse/PM-22038) | Secret material lacked `Zeroize`/redacted `Debug`; unneeded `Copy`/`Clone` ([#1217](https://github.com/midnightntwrk/midnight-node/pull/1217)) |
| [#116](https://github.com/midnightntwrk/midnight-security/issues/116) | Unbounded allocation storing cNight mappings per user ([#1423](https://github.com/midnightntwrk/midnight-node/pull/1423)) |
| [#1573](https://github.com/midnightntwrk/midnight-node/issues/1573) | dust-balance snapshot saved at `block_height = 0` under `dust_warp`, corrupting later replays ([#1574](https://github.com/midnightntwrk/midnight-node/pull/1574)) |
| [#1401](https://github.com/midnightntwrk/midnight-node/issues/1401) | Concurrent toolkit processes corrupted the shared redb fetch cache ([#1493](https://github.com/midnightntwrk/midnight-node/pull/1493)) |
| [#1575](https://github.com/midnightntwrk/midnight-node/issues/1575) | Stack overflow in `trusted_deserialize_tagged` on long-running chains ([#1576](https://github.com/midnightntwrk/midnight-node/pull/1576)) |
| [#1607](https://github.com/midnightntwrk/midnight-node/issues/1607) | C2M bridge applied an unnecessary NIGHT denomination to STAR amounts ([#1608](https://github.com/midnightntwrk/midnight-node/pull/1608)) |
| [#1599](https://github.com/midnightntwrk/midnight-node/issues/1599) | New binary failed to start against an older `default.toml` missing `unsafe_allow_symlinks` ([#1600](https://github.com/midnightntwrk/midnight-node/pull/1600)) |
