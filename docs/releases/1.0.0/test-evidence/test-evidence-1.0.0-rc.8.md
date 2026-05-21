# Node 1.0.0-rc.8 Release Test Report

Dry-run report against the `node-1.0.0-rc.8` tree (re-cut at the final 1.0.0 tag). Source: [release notes](https://github.com/midnightntwrk/midnight-node/releases/tag/node-1.0.0-rc.8). Headline runtime changes: `spec_version` 22_000 → 1_000_000, `transaction_version` 2 → 3 (`SignedExtension` → `TransactionExtension`), `polkadot-stable2603` SDK alignment, midnight-ledger 8.1.0, throttle `MaxTxs` with storage migration, `motion_close` gains `proposal_weight_bound`, networkId validated on boot, C-to-M bridge handler hooks (bridge itself **not enabled**).

## Smoke tests

Network-agnostic — applies on `local`, `devnet`, `qanet`, `preview`.

Mark each row as ☐ (not yet run), ✅ (pass), ❌ (fail), or ⏭ (skipped / N/A) once executed.

| ID | Check | How | Result |
|----|-------|-----|:------:|
| SMK-01 | Node starts and stays up for ≥ 5 min | `./target/release/midnight-node` runs without panics | ✅ |
| SMK-02 | RPC reachable | `curl -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"sidechain_getStatus","params":[],"id":1}' https://rpc.<env>.midnight.network` | ✅ |
| SMK-03 | Chain producing blocks | Best block height increases over a 6s window | ✅ |
| SMK-04 | Finality advancing | `chain_getFinalizedHead` block number increases | ✅ |
| SMK-05 | Runtime version expected | `state_getRuntimeVersion` returns `specVersion = 1000000`, `transactionVersion = 3` | ✅ |
| SMK-06 | Sync from genesis | Wipe one node's data and let it sync (expect ~0.2 BPS — see [#1298](https://github.com/midnightntwrk/midnight-node/issues/1298)) | ✅ |
| SMK-07 | Peer connectivity | `system_peers` returns ≥ 1 peer on a non-isolated network | ✅ |
| SMK-08 | Submit a trivial signed extrinsic | Accepted into pool, included in a block, status becomes `Finalized` (exercises the new `TransactionExtension` stack) | ✅ |
| SMK-09 | No errors in logs | No `ERROR` / `panic` lines in logs over the smoke window | ✅ |
| SMK-10 | Dust generation | Register new wallet and receive tNIGHT, tDust starts generating | ✅ |
| SMK-11 | Simple governance action — `system.remark` | Federated-authority motion wrapping `system.remark` is proposed, voted, and closed via `motion_close(..., proposal_weight_bound, length_bound)`; the inner call dispatches as Root and emits `system::Remarked` | ✅ |

---

## 1.0.0-rc.8 — concrete test cases

Highest-signal-per-minute checks for this release candidate.

### TC-1 · Node boots cleanly under polkadot-stable2603 + `TransactionExtension`

**Why:** the bulk of this release rebases on a new Substrate SDK line (#1262 → #1299) and migrates the runtime from `SignedExtension` to `TransactionExtension` (#597), bumping `transaction_version` 2 → 3 and `spec_version` 22_000 → 1_000_000. That touches `sc_service::build_network` (now takes `spawn_essential_handle`), `SessionKeys::generate_session_keys` (now takes an owner), `Core::execute_block` / `BlockBuilder::check_inherents` (now `LazyBlock`), BEEFY signature verification, and prepends `AuthorizeCall` / appends `WeightReclaim` to the extension tuple. Both new extensions are zero-sized (`type Implicit = ()`), so encoded tx bytes are unchanged — but `transaction_version` rotated, so live signers must round-trip cleanly.

1. Start a node from the rc.8 image / binary against a target network.
2. Tail logs for the first 5 minutes; assert SMK-01..04 stay green.
3. Submit a trivial signed extrinsic from a fresh polkadot.js / subxt client so the new `AuthorizeCall` + `WeightReclaim` extensions are actually exercised end-to-end.
4. Pre-sign a transaction against the pre-1.0.0 metadata; submit it after the upgrade.

**Expected:**
- `state_getRuntimeVersion` returns `specName = "midnight"`, `specVersion = 1000000`, `transactionVersion = 3`.
- No `ERROR` / `panic`; finalized head advances.
- The fresh-client signed extrinsic reaches `Finalized` without code changes on the client side.
- The pre-signed transaction is rejected with `BadProof` (expected — confirms `transaction_version` is being verified).

**Result:** ✅ &nbsp;&nbsp; **Notes:**

---

### TC-2 · Ledger 8.1.0 — shielded transaction round-trips end-to-end

**Why:** the ledger pin moves from `8.0.2` → `8.1.0` (#1301, #1510), pulling in `storage-core 1.2.0` (incremental GC, shared ParityDB backend) and several correctness fixes upstream (`force_as_arc` race, `Sp` serialization panic, pending-`Updates` memory leak, lock-ordering violation). The `[patch.crates-io]` block pinning the previous git tag has been removed — all midnight-ledger workspace crates now resolve from crates.io at 8.1.0. Also exercises the early-exit weight check added to the midnight pallet's `pre_dispatch` (#1305) and the toolkit's nonce-vs-nullifier serialization fixes (#895, #1074, #1128).

1. Generate a shielded transaction with the rc.8 toolkit (`single-tx` or `batch-single-tx`).
2. Submit it to a rc.8 node and follow its lifecycle: pool → in-block → finalized.
3. Replay the recipient wallet from chain and submit a spend-from-spend follow-up.

**Expected:** both transactions reach `Finalized`; ledger state reflects the spend and the new output; the follow-up spend confirms the DustWallet propagation fix (#877, audit Issue AO); no `WARN` / `ERROR` from the midnight pallet during execution.

**Result:** ✅ &nbsp;&nbsp; **Notes:**

---

### TC-3 · cNight observation parity over a fixed Cardano block range

**Why:** rc.8 carries forward the `multi_asset.id` cache (#934) and tx-id range bounding (#1365) that shipped in 0.22.5, plus new per-query Prometheus timing histograms (#904) and the cleaner address-decoder log level (#905). The contract is identical: query plans change, observation results don't.

1. Pick a fixed Cardano block range with known registrations, deregistrations, asset creates, and asset spends.
2. Run the observation pipeline against that range on a rc.8 node.
3. Verify DUST generation for the registered addresses.
4. Sanity-check `:9615/metrics` for `midnight_data_source_query_time_elapsed` with non-empty `query_name` labels (13 sub-query timers expected).
5. Grep node logs for cNIGHT address-decode lines — they should appear at `DEBUG`, not `ERROR`.

**Expected:** observed event sets equal the pre-change baseline; per-query timing histograms populated; address-decoder noise no longer at error level.

**Result:** ✅ &nbsp;&nbsp; **Notes:**

---

### TC-4 · `networkId` mismatch is rejected on boot (#1265)

**Why:** a new defensive check guards against running with a chainspec whose `networkId` doesn't match the `networkId` baked into the genesis state. Cheap, high-signal, and called out as a required operator action in the release notes — misconfigured environments now fail loud at boot rather than producing a chain that cannot be re-synced.

1. Take a valid rc.8 chainspec and edit `networkId` to a value that does not match the genesis state (or load a genesis built for a different network).
2. Boot the node.
3. As a positive control, boot again with matching `networkId` and confirm normal startup.

**Expected:** mismatched boot exits with an explicit error naming both values (chainspec vs. genesis); no partial database is created. Matching boot proceeds to SMK-01..04 green.

**Result:** ✅ &nbsp;&nbsp; **Notes:**

---

### TC-5 · Throttle pallet `MaxTxs` enforcement + `AccountUsage` migration (#1060)

**Why:** the per-account throttle gains a transaction-count limit alongside the existing byte limit, and `AccountUsage` migrates from a 2-field tuple to a `UsageStats` struct (`txs_used` added) with a runtime migration that clears the old map on upgrade. A storage migration in a 1.0.0 GA release is exactly the thing to verify on the first boot of the new runtime, and the negative path (tripping `MaxTxs`) is observable in a single block.

1. Before the upgrade, capture a snapshot of `AccountUsage` for a busy account.
2. Apply the runtime upgrade.
3. Read `AccountUsage` again — expect the new `UsageStats` shape, with the old map cleared.
4. From a single account, submit transactions back-to-back inside one rolling window to exceed `MaxTxs`.

**Expected:** post-upgrade storage decodes as `UsageStats { bytes_used, txs_used, ... }`; the (`MaxTxs` + 1)-th transaction in the window is rejected with a throttle error; no panic or migration failure logged.

**Result:** ✅ &nbsp;&nbsp; **Notes:**

---

### TC-6 · Governance `system.remark` via federated-authority motion (#1032, SMK-11)

**Why:** `motion_close` now takes a `proposal_weight_bound: Weight` parameter (Substrate can refund weight post-dispatch but never increase it, so the inner Root call's weight must be pre-charged) and is reclassified `DispatchClass::Operational`. Driving a real propose → vote → close → Root-dispatch flow on the smallest possible inner call (`system.remark`) verifies the full collective + federated-authority path without coupling to any other state, and also exercises the generated benchmark weights now wired into the runtime (#1495).

1. As a federated-authority collective member, propose a motion whose inner call is `system.remark(b"smoke-rc.8")`.
2. Cast enough Aye votes to meet threshold.
3. Call `motion_close(proposal_hash, index, proposal_weight_bound, length_bound)` — `proposal_weight_bound` must cover the inner remark's weight (toolkit / upgrader handle this; if calling raw, pull it from `TransactionPaymentApi_query_info` on the inner call). The extrinsic is now `DispatchClass::Operational`.
4. Inspect the resulting block's events.

**Expected:**
- `collective::Closed` and `collective::Approved` events fire.
- `collective::Executed { result: Ok(()) }` — inner call dispatched as Root.
- `system::Remarked { hash: blake2_256(b"smoke-rc.8"), sender: <root origin> }` present.
- Close extrinsic's actual weight ≤ `proposal_weight_bound`; no `ExhaustsResources`.

**Result:** ✅ &nbsp;&nbsp; **Notes:**

---

### TC-7 · Runtime upgrade dry-run on forked preview / pre-prod / mainnet ([#1520](https://github.com/midnightntwrk/midnight-node/issues/1520))

**Why:** before cutting 1.0.0 GA we dry-ran the runtime upgrade on snapshots of the live chains (preview, pre-prod, mainnet) to confirm the `spec_version` 22_000 → 1_000_000 bump, the `SignedExtension` → `TransactionExtension` migration, and the `AccountUsage` storage migration all apply cleanly against real chain state rather than just `dev`/`local`.

1. Fork each target network from a recent snapshot.
2. Apply the rc.8 runtime upgrade.
3. Confirm block production and finality resume post-upgrade and that storage migrations complete without error.

**Expected:** upgrade applies cleanly on every forked network; no panics or migration failures; chain continues producing and finalizing blocks. See [#1520](https://github.com/midnightntwrk/midnight-node/issues/1520).

**Result:** ✅ &nbsp;&nbsp; **Notes:**

---

### TC-8 · `try-runtime` migration check 22_000 → 1_000_000 ([#1554](https://github.com/midnightntwrk/midnight-node/issues/1554))

**Why:** independent verification of the runtime upgrade path using `try-runtime --checks all` against a mainnet-state snapshot. Exercises every pallet's `on_runtime_upgrade`, full state decode, and all `try_state` hooks before touching a live chain.

1. Take a recent `midnight-22000@latest.snap` snapshot.
2. Run `./midnight-node try-runtime --snap <snap> --runtime <1.0.0-rc.8 try-runtime wasm> --checks all`.
3. Inspect the report for migration warnings, decode errors, and weight consumption.

**Expected:** original runtime reported as version 22000, new as 1000000; entire runtime state decodes without error; all per-pallet `try-state` checks pass; migration weight well under max block weight. See [#1554](https://github.com/midnightntwrk/midnight-node/issues/1554) for the captured run.

**Result:** ✅ &nbsp;&nbsp; **Notes:**

---

## Sign-off

| Field | Value |
|-------|-------|
| Network | `local`, `devnet`, `qanet` |
| Node build / image tag | `node-1.0.0-rc.8` |
| Toolkit build / image tag | `toolkit-1.0.0-rc.8` |
| Runtime WASM srtool digest | `sha256:25043b5cd95be20bd0706022826b9ec37eec7a52efce77a3928a676355708526` |
| Tested by | Radosław Sporny |
| Date (UTC) | 2026-05-15 |
| Overall verdict | ✅ Pass &nbsp;&nbsp; ☐ Pass with caveats &nbsp;&nbsp; ☐ Fail |

---

## Out of scope (intentional)

- **Cardano-to-Midnight bridge end-to-end transfer.** The bridge handler hooks ship in 1.0.0 (#1188) but the bridge **is not enabled** at this release. Verify only that the runtime upgrade applies cleanly and that no `bridge::*` transfer events fire on chain; defer transfer-flow testing to the release that enables the bridge. ([release notes — bridge handler](https://github.com/midnightntwrk/midnight-node/releases/tag/node-1.0.0-rc.8))

## Known issues to watch during smoke

- **Initial sync is slow** (~0.2 BPS observed on some operator hardware). Affects SMK-06 wall-clock — the test passes if sync completes, even if it takes longer than expected. Track [#1298](https://github.com/midnightntwrk/midnight-node/issues/1298).
- **Toolkit JSON log format changed** (`structured_logger` → `tracing-subscriber`, #899). If your environment defaults to JSON and parses toolkit logs, re-validate parsers against the new `timestamp` / `level` / `fields` / `target` shape before treating any "no errors in logs" smoke check as authoritative.
